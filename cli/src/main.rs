use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use doer_core::action::Action;
use doer_core::app::{AppState, Effect};
use doer_core::input::{self, KeyCode, KeyPress, Mods};
use doer_core::layout::Geometry;
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

    let saver = Saver::start(Box::new(store));
    let _ = term::SAVER.set(Box::new(saver.flush_handle()));

    let events = Events::start(now());
    let theme = Theme::for_depth(ColorDepth::detect());

    for effect in startup {
        apply(&effect, &app, &saver, &events);
    }

    let mut dirty = true;
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

        for input in batch {
            let moment = now();
            for action in actions_for(input, &app) {
                for effect in doer_core::app::reduce(&mut app, &action, moment) {
                    if matches!(effect, Effect::Quit) {
                        saver.shutdown(FLUSH_TIMEOUT);
                        return Ok(());
                    }
                    apply(&effect, &app, &saver, &events);
                }
                dirty = true;
            }
        }

        // A write that failed has to say so; the domain state is untouched, so the file
        // stays dirty and the next edit retries it. A standing failure is cleared only by
        // a write that actually got through -- not by the next keystroke, or the user
        // could quit on a stale reassurance.
        for report in saver.reports().collect::<Vec<_>>() {
            match report {
                Report::Wrote => app.save_succeeded(),
                Report::Failed(error) => {
                    let effect = app.save_failed(&error);
                    apply(&effect, &app, &saver, &events);
                }
            }
            dirty = true;
        }
    }

    saver.shutdown(FLUSH_TIMEOUT);
    Ok(())
}

fn apply(effect: &Effect, app: &AppState, saver: &Saver, events: &Events) {
    match effect {
        Effect::Save(Target::AllTodos) => {
            saver.send(Job::AllTodos(all_todos(app)));
        }
        Effect::Save(Target::Project(id)) => {
            if let Some(file) = project_file(app, id) {
                saver.send(Job::Project(Box::new(file)));
            }
        }
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
        // A paste is one edit, not one keystroke per character, so it arrives as the
        // text it is and the editor decides what to do with each char.
        Input::Paste(text) => text
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
        Input::Resize(width, height) => vec![Action::Resize(width, height)],
        Input::DayChanged => vec![Action::DayChanged],
        Input::ToastExpire(seq) => vec![Action::ToastExpire(seq)],
    }
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
