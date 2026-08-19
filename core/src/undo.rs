//! Snapshot-based undo.
//!
//! Snapshots rather than inverse commands: a source-reassigning reorder has no
//! obvious inverse, and at this size (a few thousand small records) cloning the
//! workspace is measured in kilobytes. Correctness is worth more than the bytes.

use std::collections::VecDeque;

use crate::app::SidebarCursor;
use crate::display::ViewId;
use crate::id::TodoId;
use crate::workspace::Workspace;

pub const CAPACITY: usize = 50;

/// Roughly how large a snapshot may get before the cloning approach deserves a
/// second look. Checked only in debug builds.
const SNAPSHOT_WARN_BYTES: usize = 1_000_000;

/// Everything undo restores. Cursor and view travel with the data so undoing a
/// reorder puts you back where you were looking at it.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub ws: Workspace,
    pub view: ViewId,
    pub cursor: Option<TodoId>,
    pub cursor_hint: usize,
    pub sidebar_cursor: SidebarCursor,
}

impl Snapshot {
    #[must_use]
    pub fn approx_bytes(&self) -> usize {
        let per_todo = size_of::<crate::todo::Todo>();
        self.ws
            .buckets_in_display_order()
            .iter()
            .flat_map(|bucket| self.ws.todos(bucket))
            .map(|todo| per_todo + todo.text.len() + todo.id.as_str().len())
            .sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct UndoStack {
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}

impl UndoStack {
    /// Records the state *before* a mutation. Any new mutation invalidates the redo
    /// branch, as in every editor.
    pub fn push(&mut self, snapshot: Snapshot) {
        debug_assert!(
            snapshot.approx_bytes() < SNAPSHOT_WARN_BYTES,
            "undo snapshots have outgrown plain cloning"
        );
        self.redo.clear();
        self.undo.push_back(snapshot);
        while self.undo.len() > CAPACITY {
            self.undo.pop_front();
        }
    }

    /// Discards the most recent snapshot without restoring anything to the redo
    /// branch. Used to drop an edit session that was cancelled.
    pub fn pop(&mut self) -> Option<Snapshot> {
        self.undo.pop_back()
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop_back()?;
        self.redo.push(current);
        Some(previous)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push_back(current);
        Some(next)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::Todo;
    use crate::workspace::Bucket;

    const T0: i64 = 1_700_000_000;

    fn snapshot(texts: &[&str]) -> Snapshot {
        let mut ws = Workspace::default();
        for text in texts {
            ws.push_todo(&Bucket::All, Todo::new(*text, T0));
        }
        Snapshot {
            ws,
            view: ViewId::All,
            cursor: None,
            cursor_hint: 0,
            sidebar_cursor: SidebarCursor::All,
        }
    }

    fn texts(snapshot: &Snapshot) -> Vec<String> {
        snapshot
            .ws
            .todos(&Bucket::All)
            .iter()
            .map(|t| t.text.clone())
            .collect()
    }

    #[test]
    fn undo_returns_the_state_recorded_before_the_mutation() {
        let mut stack = UndoStack::default();
        stack.push(snapshot(&["a"]));
        let restored = stack.undo(snapshot(&["a", "b"])).expect("undo");
        assert_eq!(texts(&restored), ["a"]);
    }

    #[test]
    fn redo_returns_the_state_undo_replaced() {
        let mut stack = UndoStack::default();
        stack.push(snapshot(&["a"]));
        let undone = stack.undo(snapshot(&["a", "b"])).expect("undo");
        let redone = stack.redo(undone).expect("redo");
        assert_eq!(texts(&redone), ["a", "b"]);
    }

    #[test]
    fn a_new_mutation_discards_the_redo_branch() {
        let mut stack = UndoStack::default();
        stack.push(snapshot(&["a"]));
        let undone = stack.undo(snapshot(&["a", "b"])).expect("undo");
        assert!(stack.can_redo());

        stack.push(undone);
        assert!(!stack.can_redo());
    }

    #[test]
    fn the_stack_forgets_the_oldest_snapshot_once_it_is_full() {
        let mut stack = UndoStack::default();
        for i in 0..CAPACITY + 10 {
            stack.push(snapshot(&[Box::leak(format!("{i}").into_boxed_str())]));
        }
        assert_eq!(stack.depth(), CAPACITY);

        let mut last = snapshot(&["current"]);
        while let Some(previous) = stack.undo(last.clone()) {
            last = previous;
        }
        assert_eq!(texts(&last), ["10"]);
    }

    #[test]
    fn popping_an_abandoned_session_leaves_no_redo_entry() {
        let mut stack = UndoStack::default();
        stack.push(snapshot(&["a"]));
        let popped = stack.pop().expect("pop");

        assert_eq!(texts(&popped), ["a"]);
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
    }
}
