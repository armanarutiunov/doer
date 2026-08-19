//! Terminal lifecycle: raw mode, alternate screen, bracketed paste, and the restore
//! paths that have to survive both a clean exit and a panic.

use std::io::{self, Stdout};
use std::sync::OnceLock;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;

pub type Tty = Terminal<CrosstermBackend<Stdout>>;

/// Seam for the store's flush-on-panic. The writer registers itself here at startup;
/// the panic hook is the only reader.
pub trait PanicFlush: Send + Sync {
    /// Must not panic and must not block past `timeout` — it runs inside a panic hook.
    fn flush_blocking(&self, timeout: Duration);
}

pub static SAVER: OnceLock<Box<dyn PanicFlush>> = OnceLock::new();

const PANIC_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// Owns the terminal's mode changes and undoes them on drop.
pub struct TerminalGuard {
    pub tty: Tty,
}

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        // Before `try_init`, so ratatui's own restore hook wraps ours and still runs
        // even if the flush itself panics.
        install_panic_hook();

        // The guard is constructed the moment `try_init` succeeds, so everything after
        // it is covered by `Drop`. Enabling paste and hiding the cursor afterwards would
        // otherwise be able to fail with raw mode on and the alternate screen entered
        // but no guard in existence to undo either, dropping the user back into a shell
        // with no echo and printing the error into a screen they can no longer see.
        let guard = Self {
            tty: ratatui::try_init()?,
        };
        execute!(io::stdout(), EnableBracketedPaste, Hide)?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// `ratatui::restore` undoes raw mode and the alternate screen but knows nothing about
/// the cursor or bracketed paste. Leaving paste enabled hands the user a shell that
/// garbles every subsequent paste, so those two are undone here.
fn restore_terminal() {
    let _ = execute!(io::stdout(), Show, DisableBracketedPaste);
    let _ = ratatui::try_restore();
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(saver) = SAVER.get() {
            saver.flush_blocking(PANIC_FLUSH_TIMEOUT);
        }
        restore_terminal();
        previous(info);
    }));
}
