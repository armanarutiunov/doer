use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use doer_core::action::Action;
use doer_core::app::{AppState, Effect};
use doer_core::input::{self, KeyCode, KeyPress, Mods};
use doer_core::layout::Geometry;
use doer_core::mode::{Focus, MainMode, SidebarMode};
use doer_core::store::{ProjectFile, Store, Target};
use doer_core::{ProjectId, Todo};
use ratatui::crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyModifiers};

use doer::event::{Events, Input};
use doer::store::FsStore;
use doer::term::{self, TerminalGuard};
use doer::ui;
use doer::ui::theme::{ColorDepth, Theme};
use doer::writer::{Job, Report, Saver};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bounded so a wedged filesystem cannot hold the terminal in raw mode.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

const USAGE: &str = "\
doer — a vim-flavoured terminal todo app

Usage: doer [options]

Options:
  -h, --help       Print this help
  -V, --version    Print the version

Todos live in ~/.doer, or in $DOER_HOME if it is set.
Press ? inside the app for the keybindings.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(arg) = args.first() {
        match arg.as_str() {
            "-h" | "--help" => println!("{USAGE}"),
            "-V" | "--version" => println!("doer {VERSION}"),
            other => {
                eprintln!("doer: unrecognised argument '{other}'\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // After the guard has dropped, so the message is not swallowed by the
            // alternate screen.
            eprintln!("doer: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let store = FsStore::new()?;
    let loaded = store.load();

    let mut guard = TerminalGuard::new()?;
    let size = guard.tty.size()?;
    let geo = Geometry::new(size.width, size.height, true);
    let (mut app, startup) = AppState::from_loaded(loaded, geo);

    let mut saver = Saver::start(Box::new(store));
    let _ = term::SAVER.set(Box::new(saver.flush_handle()));

    let events = Events::start(now);
    let theme = Theme::for_depth(ColorDepth::detect());

    for effect in startup {
        apply(&effect, &app, &saver, &events);
    }

    let mut dirty = true;
    let mut saving_broken = false;
    loop {
        if dirty {
            let moment = now();
            guard
                .tty
                .draw(|frame| ui::draw(frame, &app, moment, &theme))?;
            dirty = false;
        }

        let Ok(first) = events.next() else { break };
        let batch: Vec<Input> = std::iter::once(first).chain(events.drain()).collect();

        // One reading of the clock for the whole batch, so every todo created by a key
        // burst shares a timestamp and the age labels cannot disagree within a frame.
        let moment = now();
        for input in batch {
            if matches!(input, Input::Closed) {
                return finish(&mut saver, FLUSH_TIMEOUT);
            }
            for action in actions_for(input, &app) {
                // reduce puts its saves and deletes before Quit, which is what makes
                // returning from the middle of this loop safe. Anything emitted after a
                // Quit would be dropped.
                for effect in doer_core::app::reduce(&mut app, &action, moment) {
                    if matches!(effect, Effect::Quit) {
                        return finish(&mut saver, FLUSH_TIMEOUT);
                    }
                    apply(&effect, &app, &saver, &events);
                }
                dirty = true;
            }
        }

        // Reports are drained before the death check, so a success still queued from
        // before a panic cannot arrive afterwards and clear the warning about it.
        // A write that failed has to say so; the domain state is untouched, so the file
        // stays dirty and the next edit retries it. A standing failure is cleared only by
        // a write to that same file getting through.
        for report in saver.reports().collect::<Vec<_>>() {
            match report {
                Report::Wrote(target) => app.save_succeeded(&target),
                Report::Failed(target, error) => {
                    let effect = app.save_failed(Some(target), &error);
                    apply(&effect, &app, &saver, &events);
                }
            }
            dirty = true;
        }

        // A panic on the save thread would otherwise be invisible: the app keeps taking
        // edits, every write fails silently, and the reports channel is gone so nothing
        // ever reports it.
        if saver.has_died() && !saving_broken {
            saving_broken = true;
            let effect = app.saving_stopped();
            apply(&effect, &app, &saver, &events);
            dirty = true;
        }
    }

    finish(&mut saver, FLUSH_TIMEOUT)
}

/// Flushes, then says what could not be written. Exiting quietly would tell the user
/// their last edits were saved when they were not, so both a wait that ran out and a
/// write that failed during the final flush are reported.
fn finish(saver: &mut Saver, timeout: Duration) -> anyhow::Result<()> {
    let flushed = saver.shutdown(timeout);
    let failures: Vec<String> = saver
        .reports()
        .filter_map(|report| match report {
            Report::Failed(target, error) => Some(format!("{target}: {error}")),
            Report::Wrote(_) => None,
        })
        .collect();

    if !failures.is_empty() {
        anyhow::bail!("could not save {}", failures.join(", "));
    }
    if !flushed {
        anyhow::bail!(
            "gave up waiting for the disk after {timeout:?}; some edits may not be saved"
        );
    }
    Ok(())
}

fn apply(effect: &Effect, app: &AppState, saver: &Saver, events: &Events) {
    match effect {
        Effect::Save(Target::AllTodos) => {
            saver.send(Job::AllTodos(all_todos(app)));
        }
        Effect::Save(Target::Project(id)) => match project_file(app, id) {
            Some(file) => saver.send(Job::Project(Box::new(file))),
            // The dirty set cancels a save for a project that has been deleted, so
            // reaching here means the two disagree and a write has been lost.
            None => debug_assert!(false, "save requested for a project that is gone: {id}"),
        },
        Effect::DeleteProject(id) => saver.send(Job::Delete(id.clone())),
        Effect::Toast(toast) => {
            if let Some(ttl) = toast.ttl_ms {
                events.arm_toast(toast.seq, Duration::from_millis(ttl));
            }
        }
        Effect::Quit => {}
    }
}

fn all_todos(app: &AppState) -> Vec<Todo> {
    app.ws.todos(&doer_core::Bucket::All).to_vec()
}

fn project_file(app: &AppState, id: &ProjectId) -> Option<ProjectFile> {
    let (project, todos) = app.ws.project_file(id)?;
    Some(ProjectFile {
        id: project.id.clone(),
        index: project.index,
        name: project.name.clone(),
        parent_id: project.parent_id.clone(),
        todos: todos.to_vec(),
    })
}

fn actions_for(input: Input, app: &AppState) -> Vec<Action> {
    match input {
        Input::Key(key) => convert(key)
            .and_then(|press| input::map(app.input_context(), press))
            .into_iter()
            .collect(),
        // Only a text field accepts a paste. Anywhere else every character would be read
        // as a keybinding, so pasting "add docs" into the list would delete a todo, undo
        // an edit and quit -- from one stray key combination.
        Input::Paste(text) if accepts_text(app) => paste_text(&text)
            .chars()
            .filter_map(|ch| {
                input::map(
                    app.input_context(),
                    KeyPress {
                        code: KeyCode::Char(ch),
                        mods: Mods::NONE,
                    },
                )
            })
            .collect(),
        // A paste outside a text field is dropped rather than read as keybindings, and
        // `Closed` is handled by the event loop, which has to leave rather than reduce.
        Input::Paste(_) | Input::Closed => Vec::new(),
        Input::Resize(width, height) => vec![Action::Resize(width, height)],
        Input::DayChanged => vec![Action::DayChanged],
        Input::ToastExpire(seq) => vec![Action::ToastExpire(seq)],
    }
}

/// Whether a paste would be read as text rather than as a run of keybindings. The help
/// overlay swallows keys, so it swallows a paste too.
fn accepts_text(app: &AppState) -> bool {
    !app.help
        && matches!(
            app.focus(),
            Focus::Main(MainMode::Insert | MainMode::Search) | Focus::Sidebar(SidebarMode::Insert)
        )
}

/// A todo is one line, so the line breaks in a multi-line paste become spaces. Dropping
/// them instead would run the last word of one line into the first of the next, and a
/// terminal may deliver one paste as several chunks, so there is no reliable "first line"
/// to keep.
fn paste_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

/// Shift is dropped deliberately: the character already carries the case, and terminals
/// disagree about whether `J` also reports SHIFT.
fn convert(key: KeyEvent) -> Option<KeyPress> {
    let code = match key.code {
        CtKeyCode::Char(ch) => KeyCode::Char(ch),
        CtKeyCode::Enter => KeyCode::Enter,
        CtKeyCode::Esc => KeyCode::Escape,
        CtKeyCode::Backspace => KeyCode::Backspace,
        CtKeyCode::Delete => KeyCode::Delete,
        CtKeyCode::Tab | CtKeyCode::BackTab => KeyCode::Tab,
        CtKeyCode::Left => KeyCode::Left,
        CtKeyCode::Right => KeyCode::Right,
        CtKeyCode::Up => KeyCode::Up,
        CtKeyCode::Down => KeyCode::Down,
        CtKeyCode::Home => KeyCode::Home,
        CtKeyCode::End => KeyCode::End,
        _ => return None,
    };
    Some(KeyPress {
        code,
        mods: Mods {
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
        },
    })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
