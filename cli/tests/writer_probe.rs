#![allow(clippy::expect_used)]
use std::time::Duration;

use doer::store::FsStore;
use doer::writer::{Job, Saver};
use doer_core::store::Store;
use doer_core::{Todo, TodoId};

fn todo(id: &str) -> Todo {
    Todo {
        id: TodoId::from(id),
        text: "written by the writer".into(),
        done: false,
        created_at: 1,
        completed_at: None,
    }
}

#[test]
fn a_job_sent_then_shutdown_reaches_the_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsStore::with_root(dir.path());
    store.load();
    let saver = Saver::start(Box::new(store));
    saver.send(Job::AllTodos(vec![todo("0123456789abcdef")]));
    saver.flush(Duration::from_secs(2));
    let body = std::fs::read_to_string(dir.path().join("all-todos.json")).expect("file exists");
    assert!(body.contains("written by the writer"), "{body}");
    std::mem::forget(saver);
}

#[test]
fn a_debounced_job_lands_without_any_flush() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsStore::with_root(dir.path());
    store.load();
    let saver = Saver::start(Box::new(store));
    saver.send(Job::AllTodos(vec![todo("0123456789abcdef")]));
    std::thread::sleep(Duration::from_millis(600));
    let body = std::fs::read_to_string(dir.path().join("all-todos.json")).expect("file exists");
    assert!(body.contains("written by the writer"), "{body}");
    std::mem::forget(saver);
}
