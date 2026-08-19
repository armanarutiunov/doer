#![allow(clippy::unwrap_used)]

//! The save thread is the one piece of concurrency in the app, so its exit paths are
//! tested rather than reasoned about: a shutdown that never returns leaves the terminal
//! in raw mode with no way out but killing it.

use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use doer::term;
use doer::writer::{Job, Saver};
use doer_core::store::{Loaded, ProjectFile, Store, StoreError, StoreSnapshot, Target};
use doer_core::{ProjectId, Todo, TodoId};

/// Records what reached the store, from a handle the test keeps after the `Saver` has
/// taken ownership of the store itself.
#[derive(Clone, Default)]
struct Recorder {
    writes: Arc<Mutex<Vec<Target>>>,
}

impl Store for Recorder {
    fn load(&self) -> Loaded<StoreSnapshot> {
        Loaded::new(StoreSnapshot::default(), Vec::new())
    }

    fn save_all_todos(&self, _todos: &[Todo]) -> Result<(), StoreError> {
        self.writes.lock().unwrap().push(Target::AllTodos);
        Ok(())
    }

    fn save_project(&self, file: &ProjectFile) -> Result<(), StoreError> {
        self.writes
            .lock()
            .unwrap()
            .push(Target::Project(file.id.clone()));
        Ok(())
    }

    fn delete_project(&self, _id: &ProjectId) -> Result<(), StoreError> {
        Ok(())
    }
}

fn todo(text: &str) -> Todo {
    Todo {
        id: TodoId::from("0123456789abcdef"),
        text: text.into(),
        done: false,
        created_at: 0,
        completed_at: None,
    }
}

/// Runs `body` on another thread and fails if it has not finished in time, so a hang
/// is a test failure rather than a test that never ends.
fn within<F: FnOnce() + Send + 'static>(timeout: Duration, what: &str, body: F) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        body();
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(timeout).is_ok(),
        "{what} did not finish within {timeout:?}"
    );
}

#[test]
fn shutdown_returns_even_though_the_panic_hook_holds_a_sender_forever() {
    within(Duration::from_secs(5), "shutdown", || {
        let mut saver = Saver::start(Box::new(Recorder::default()));
        // The real app parks this in a static that is never dropped, which is what
        // made an earlier version wait for a disconnect that could never happen.
        let handle = saver.flush_handle();
        saver.send(Job::AllTodos(vec![todo("keep me")]));
        let _ = saver.shutdown(Duration::from_secs(2));
        drop(handle);
    });
}

#[test]
fn dropping_the_saver_without_shutting_it_down_also_returns() {
    within(Duration::from_secs(5), "drop", || {
        let saver = Saver::start(Box::new(Recorder::default()));
        let handle = saver.flush_handle();
        drop(saver);
        drop(handle);
    });
}

#[test]
fn a_flush_writes_what_was_queued() {
    let recorder = Recorder::default();
    let mut saver = Saver::start(Box::new(recorder.clone()));
    saver.send(Job::AllTodos(vec![todo("persisted")]));
    let _ = saver.shutdown(Duration::from_secs(2));

    assert_eq!(*recorder.writes.lock().unwrap(), vec![Target::AllTodos]);
}

#[test]
fn the_panic_flush_seam_accepts_a_handle() {
    let mut saver = Saver::start(Box::new(Recorder::default()));
    assert!(term::SAVER.set(Box::new(saver.flush_handle())).is_ok() || term::SAVER.get().is_some());
    let _ = saver.shutdown(Duration::from_secs(2));
}

/// A store whose write never returns, standing in for an unresponsive filesystem.
struct WedgedStore;

impl Store for WedgedStore {
    fn load(&self) -> Loaded<StoreSnapshot> {
        Loaded::ok(StoreSnapshot::default())
    }

    fn save_all_todos(&self, _todos: &[Todo]) -> Result<(), StoreError> {
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    fn save_project(&self, _file: &ProjectFile) -> Result<(), StoreError> {
        Ok(())
    }

    fn delete_project(&self, _id: &ProjectId) -> Result<(), StoreError> {
        Ok(())
    }
}

#[test]
fn shutdown_gives_up_on_a_wedged_write_instead_of_holding_the_process_open() {
    let mut saver = Saver::start(Box::new(WedgedStore));
    saver.send(Job::AllTodos(Vec::new()));

    let start = Instant::now();
    let flushed = saver.shutdown(Duration::from_millis(300));

    assert!(!flushed, "a write that never returns has not been flushed");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "shutdown waited {:?}; it must be bounded, because joining the thread is not",
        start.elapsed()
    );
}
