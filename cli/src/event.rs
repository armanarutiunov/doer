//! Input plumbing: one channel carrying keys, resizes and timer wakeups.
//!
//! There is no periodic tick. The terminal size arrives as `Resize` (crossterm turns
//! SIGWINCH into an event), age labels change only at a UTC day boundary, and a toast
//! expires on its own one-shot deadline — so an idle app wakes up at most once a day.

use std::sync::mpsc::{self, Receiver, RecvError, Sender};
use std::thread;
use std::time::Duration;

use doer_core::todo::SECONDS_PER_DAY;
use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};

#[derive(Clone, Debug)]
pub(crate) enum Input {
    Key(KeyEvent),
    Paste(String),
    Resize(u16, u16),
    /// A UTC day boundary passed: the "Xd" age labels need recomputing.
    DayChanged,
    /// The toast with this sequence number reached its TTL. A newer toast supersedes an
    /// older one by having a different seq, so no timer ever needs cancelling.
    ToastExpire(u64),
}

pub(crate) struct Events {
    rx: Receiver<Input>,
    tx: Sender<Input>,
}

impl Events {
    /// Spawns the reader and the day-boundary timer. Both are detached: `event::read`
    /// is uninterruptible, so joining it would hang until the user happened to press a
    /// key. They exit on their own once the receiver drops.
    pub(crate) fn start(now: i64) -> Self {
        let (tx, rx) = mpsc::channel();
        spawn_reader(tx.clone());
        spawn_day_timer(tx.clone(), now);
        Self { rx, tx }
    }

    /// Blocks until something happens. An `Err` means every sender is gone, which the
    /// caller must treat as quit — otherwise the app sits in a dead loop holding a live
    /// terminal, which looks exactly like a hang.
    pub(crate) fn next(&self) -> Result<Input, RecvError> {
        self.rx.recv()
    }

    /// Everything already queued behind the last `next`. Draining a burst — held `j`,
    /// a dragged window corner — into one batch collapses it into a single redraw.
    pub(crate) fn drain(&self) -> impl Iterator<Item = Input> + '_ {
        self.rx.try_iter()
    }

    pub(crate) fn arm_toast(&self, seq: u64, ttl: Duration) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            thread::sleep(ttl);
            let _ = tx.send(Input::ToastExpire(seq));
        });
    }
}

fn spawn_reader(tx: Sender<Input>) {
    thread::spawn(move || {
        loop {
            let input = match event::read() {
                // Release and Repeat would double every keystroke on terminals that
                // report them.
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => Input::Key(key),
                Ok(Event::Paste(text)) => Input::Paste(text),
                Ok(Event::Resize(w, h)) => Input::Resize(w, h),
                Ok(_) => continue,
                Err(_) => return,
            };
            if tx.send(input).is_err() {
                return;
            }
        }
    });
}

/// Sleeps to the next boundary rather than ticking, and recomputes the target after
/// every wake so a suspended laptop resumes on the right schedule.
fn spawn_day_timer(tx: Sender<Input>, start: i64) {
    thread::spawn(move || {
        let mut now = start;
        loop {
            let wait = seconds_until_next_day(now);
            thread::sleep(Duration::from_secs(wait));
            if tx.send(Input::DayChanged).is_err() {
                return;
            }
            now += i64::try_from(wait).unwrap_or(SECONDS_PER_DAY);
        }
    });
}

/// Age labels are `unix / 86_400`, so the boundary that matters is UTC midnight.
fn seconds_until_next_day(now: i64) -> u64 {
    let since_midnight = now.rem_euclid(SECONDS_PER_DAY);
    u64::try_from(SECONDS_PER_DAY - since_midnight).unwrap_or(86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_timer_waits_to_the_next_utc_midnight() {
        assert_eq!(seconds_until_next_day(0), 86_400);
        assert_eq!(seconds_until_next_day(1), 86_399);
        assert_eq!(seconds_until_next_day(86_399), 1);
        assert_eq!(seconds_until_next_day(86_400), 86_400);
    }

    #[test]
    fn a_pre_epoch_clock_still_yields_a_positive_wait() {
        assert_eq!(seconds_until_next_day(-1), 1);
    }
}
