//! The background saver: keystrokes must never wait on a disk.
//!
//! One thread owns the store. Work arrives as whole payloads, so the thread never
//! borrows app state, and a newer payload for the same file replaces a queued one —
//! holding `J` for a second is one write, not thirty.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle, ThreadId};
use std::time::{Duration, Instant};

use doer_core::store::{ProjectFile, Store, StoreError, Target};
use doer_core::{ProjectId, Todo};

use crate::term::PanicFlush;

/// How long a file stays queued while more edits arrive.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Bound for the unwind path, where nobody chose a timeout. The queue has usually been
/// flushed already; this only covers a write still in progress.
const DROP_STOP_TIMEOUT: Duration = Duration::from_secs(2);

pub enum Job {
    AllTodos(Vec<Todo>),
    Project(Box<ProjectFile>),
    Delete(ProjectId),
}

enum Message {
    Work(Job),
    Flush(SyncSender<()>),
    /// Explicit, rather than relying on the channel disconnecting: the panic hook's
    /// handle lives in a static and holds a sender that is never dropped, so waiting
    /// for a disconnect would wait forever.
    Shutdown,
}

/// What the thread reports back about a write it attempted. Successes matter as much
/// as failures: a standing "save failed" message must only be cleared by a write that
/// actually got through, not by the next keystroke.
pub enum Report {
    Wrote(Target),
    Failed(Target, StoreError),
}

pub struct Saver {
    tx: Sender<Message>,
    reports: Receiver<Report>,
    /// Kept only to know whether the thread has already been told to stop, and to detach
    /// it deliberately when the bounded wait gives up rather than joining it.
    handle: Option<JoinHandle<()>>,
    alive: Arc<AtomicBool>,
    /// Closes when the thread returns. Waiting on this rather than joining is what keeps
    /// the wait bounded: `JoinHandle::join` cannot time out.
    stopped: Receiver<()>,
}

/// Cloneable handle for the panic hook, which cannot reach the `Saver` itself.
pub struct FlushHandle {
    tx: Sender<Message>,
    /// The thread that would have to answer the flush. If the panic being handled is
    /// on that very thread there is nobody left to answer, so asking would stall for
    /// the whole timeout and then restore the terminal under a still-running app.
    worker: ThreadId,
}

impl PanicFlush for FlushHandle {
    fn flush_blocking(&self, timeout: Duration) {
        if thread::current().id() == self.worker {
            return;
        }
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
        let (stopped_tx, stopped) = mpsc::channel::<()>();
        let alive = Arc::new(AtomicBool::new(true));
        let handle = thread::spawn({
            let alive = Arc::clone(&alive);
            move || {
                let _stopped = stopped_tx;
                let worker = Worker {
                    store,
                    reports: report_tx,
                };
                worker.run(&rx);
                // Reached on a clean shutdown; a panic skips it, which is exactly the
                // signal the event loop needs.
                alive.store(false, Ordering::Release);
            }
        });
        Self {
            tx,
            reports,
            handle: Some(handle),
            alive,
            stopped,
        }
    }

    #[must_use]
    pub fn flush_handle(&self) -> FlushHandle {
        FlushHandle {
            tx: self.tx.clone(),
            worker: self
                .handle
                .as_ref()
                .map_or_else(|| thread::current().id(), |h| h.thread().id()),
        }
    }

    /// False once the thread has died on a panic. Saving is then permanently broken, so
    /// the event loop has to say so rather than accepting edits that go nowhere.
    #[must_use]
    pub fn has_died(&self) -> bool {
        self.handle.as_ref().is_some_and(JoinHandle::is_finished)
            && self.alive.load(Ordering::Acquire)
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
    /// hold the terminal in raw mode forever. False means the wait ran out with writes
    /// still outstanding.
    #[must_use]
    pub fn flush(&self, timeout: Duration) -> bool {
        let (ack, done) = mpsc::sync_channel(0);
        if self.tx.send(Message::Flush(ack)).is_err() {
            return false;
        }
        // The worker drops the ack once it has written everything, so a disconnect is
        // the success signal and a timeout is the failure.
        matches!(
            done.recv_timeout(timeout),
            Err(mpsc::RecvTimeoutError::Disconnected)
        )
    }

    /// Writes everything queued, then waits for the thread to finish. False means the
    /// bound expired with writes outstanding, which the caller has to report: exiting
    /// quietly would tell the user their last edits were saved.
    /// Borrows rather than consumes, so the caller can still drain the reports the final
    /// flush produced: a write that failed there is the last chance to say so.
    #[must_use]
    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        let flushed = self.flush(timeout);
        self.stop(timeout);
        flushed
    }

    /// Asks the thread to finish and waits, bounded, for it to do so.
    ///
    /// The wait is on the thread's own channel rather than `JoinHandle::join`, which
    /// cannot time out: a write wedged on an unresponsive filesystem would otherwise hold
    /// the process open forever, which is the failure this whole timeout exists to avoid.
    /// Abandoning a thread mid-write is safe because every write goes to a temp file and
    /// is renamed into place, so an interrupted one leaves the original file untouched.
    fn stop(&mut self, timeout: Duration) {
        if self.handle.take().is_none() {
            return;
        }
        let _ = self.tx.send(Message::Shutdown);
        let _ = self.stopped.recv_timeout(timeout);
    }
}

impl Drop for Saver {
    /// Only reached when `shutdown` was not called -- an early return on the way up
    /// from an error, or a panic unwinding. Tell the thread to finish rather than waiting
    /// for a disconnect that the panic hook's static handle prevents.
    fn drop(&mut self) {
        self.stop(DROP_STOP_TIMEOUT);
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
                Ok(Message::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    self.write_all(&mut pending, &mut deletes);
                    return;
                }
            }
        }
    }

    fn report(&self, target: Target, result: Result<(), StoreError>) {
        let _ = self.reports.send(match result {
            Ok(()) => Report::Wrote(target),
            Err(error) => Report::Failed(target, error),
        });
    }

    fn write_all(&self, pending: &mut HashMap<Target, Job>, deletes: &mut Vec<ProjectId>) {
        // Deletes go first, so undoing a delete -- which queues a write for the same
        // file -- ends with the file present rather than removed.
        for id in deletes.drain(..) {
            let result = self.store.delete_project(&id);
            self.report(Target::Project(id), result);
        }
        for (target, job) in pending.drain() {
            let result = match job {
                Job::AllTodos(todos) => self.store.save_all_todos(&todos),
                Job::Project(file) => self.store.save_project(&file),
                // A delete never reaches `pending`: it is taken out of the queue above so
                // that it can be applied before the writes.
                Job::Delete(id) => self.store.delete_project(&id),
            };
            self.report(target, result);
        }
    }
}
