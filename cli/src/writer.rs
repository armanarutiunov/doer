//! The background saver: keystrokes must never wait on a disk.
//!
//! One thread owns the store. Work arrives as whole payloads, so the thread never
//! borrows app state, and a newer payload for the same file replaces a queued one —
//! holding `J` for a second is one write, not thirty.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use doer_core::store::{ProjectFile, Store, StoreError, Target};
use doer_core::{ProjectId, Todo};

use crate::term::PanicFlush;

/// How long a file stays queued while more edits arrive.
const DEBOUNCE: Duration = Duration::from_millis(150);

pub enum Job {
    AllTodos(Vec<Todo>),
    Project(Box<ProjectFile>),
    Delete(ProjectId),
}

enum Message {
    Work(Job),
    Flush(SyncSender<()>),
}

/// What the thread reports back about a write it attempted. Successes matter as much
/// as failures: a standing "save failed" message must only be cleared by a write that
/// actually got through, not by the next keystroke.
pub enum Report {
    Wrote,
    Failed(StoreError),
}

pub struct Saver {
    tx: Sender<Message>,
    reports: Receiver<Report>,
    handle: Option<JoinHandle<()>>,
}

/// Cloneable handle for the panic hook, which cannot reach the `Saver` itself.
pub struct FlushHandle {
    tx: Sender<Message>,
}

impl PanicFlush for FlushHandle {
    fn flush_blocking(&self, timeout: Duration) {
        let (ack, done) = mpsc::sync_channel(0);
        if self.tx.send(Message::Flush(ack)).is_ok() {
            let _ = done.recv_timeout(timeout);
        }
    }
}

impl Saver {
    #[must_use]
    pub fn start(store: Box<dyn Store>) -> Self {
        let (tx, rx) = mpsc::channel();
        let (report_tx, reports) = mpsc::channel();
        let handle = thread::spawn(move || {
            let worker = Worker {
                store,
                reports: report_tx,
            };
            worker.run(&rx);
        });
        Self {
            tx,
            reports,
            handle: Some(handle),
        }
    }

    #[must_use]
    pub fn flush_handle(&self) -> FlushHandle {
        FlushHandle {
            tx: self.tx.clone(),
        }
    }

    pub fn send(&self, job: Job) {
        let _ = self.tx.send(Message::Work(job));
    }

    /// What the thread has reported since the last check. Draining rather than blocking
    /// keeps this callable from the event loop.
    pub fn reports(&self) -> impl Iterator<Item = Report> + '_ {
        self.reports.try_iter()
    }

    /// Writes everything still queued. Bounded, because a wedged filesystem must not
    /// hold the terminal in raw mode forever.
    pub fn flush(&self, timeout: Duration) {
        let (ack, done) = mpsc::sync_channel(0);
        if self.tx.send(Message::Flush(ack)).is_ok() {
            let _ = done.recv_timeout(timeout);
        }
    }

    /// Writes everything queued, then waits for the thread to finish. Dropping the
    /// `Saver` is what closes the channel and lets the thread return.
    pub fn shutdown(self, timeout: Duration) {
        self.flush(timeout);
    }
}

impl Drop for Saver {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct Worker {
    store: Box<dyn Store>,
    reports: Sender<Report>,
}

impl Worker {
    fn run(self, rx: &Receiver<Message>) {
        let mut pending: HashMap<Target, Job> = HashMap::new();
        let mut deletes: Vec<ProjectId> = Vec::new();
        let mut due: Option<Instant> = None;

        loop {
            let message = match due {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
            };

            match message {
                Ok(Message::Work(job)) => {
                    match job {
                        Job::Delete(id) => {
                            // A deleted project must not be recreated by a write that was
                            // still queued when it went away.
                            pending.remove(&Target::Project(id.clone()));
                            deletes.push(id);
                        }
                        Job::AllTodos(todos) => {
                            pending.insert(Target::AllTodos, Job::AllTodos(todos));
                        }
                        Job::Project(file) => {
                            pending.insert(Target::Project(file.id.clone()), Job::Project(file));
                        }
                    }
                    due = Some(Instant::now() + DEBOUNCE);
                }
                Ok(Message::Flush(ack)) => {
                    self.write_all(&mut pending, &mut deletes);
                    due = None;
                    drop(ack);
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.write_all(&mut pending, &mut deletes);
                    due = None;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.write_all(&mut pending, &mut deletes);
                    return;
                }
            }
        }
    }

    fn report(&self, result: Result<(), StoreError>) {
        let _ = self.reports.send(match result {
            Ok(()) => Report::Wrote,
            Err(error) => Report::Failed(error),
        });
    }

    fn write_all(&self, pending: &mut HashMap<Target, Job>, deletes: &mut Vec<ProjectId>) {
        // Deletes go first, so undoing a delete -- which queues a write for the same
        // file -- ends with the file present rather than removed.
        for id in deletes.drain(..) {
            self.report(self.store.delete_project(&id));
        }
        for (_, job) in pending.drain() {
            let result = match job {
                Job::AllTodos(todos) => self.store.save_all_todos(&todos),
                Job::Project(file) => self.store.save_project(&file),
                Job::Delete(id) => self.store.delete_project(&id),
            };
            self.report(result);
        }
    }
}
