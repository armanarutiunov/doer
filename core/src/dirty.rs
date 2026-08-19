//! What still needs writing.
//!
//! The Elixir version wrote every project file on every mutation while in the All
//! view. Recording which files a change actually touched is the whole of the fix:
//! the reducer drains this once per action and emits one save per touched file.

use crate::id::ProjectId;
use crate::store::Target;
use crate::workspace::Bucket;

#[must_use]
pub fn target_of(bucket: &Bucket) -> Target {
    match bucket {
        Bucket::All => Target::AllTodos,
        Bucket::Project(id) => Target::Project(id.clone()),
    }
}

#[derive(Clone, Debug, Default)]
pub struct DirtySet {
    saves: Vec<Target>,
    deletes: Vec<ProjectId>,
}

impl DirtySet {
    pub fn mark(&mut self, target: Target) {
        if !self.saves.contains(&target) {
            self.saves.push(target);
        }
    }

    pub fn mark_bucket(&mut self, bucket: &Bucket) {
        self.mark(target_of(bucket));
    }

    pub fn mark_deleted(&mut self, id: ProjectId) {
        // A file that is about to be deleted has nothing worth writing first.
        self.saves.retain(|t| t != &Target::Project(id.clone()));
        if !self.deletes.contains(&id) {
            self.deletes.push(id);
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.saves.is_empty() && self.deletes.is_empty()
    }

    /// Deletes come out first so a stale save can never recreate a file that this
    /// same action removed.
    pub fn drain(&mut self) -> (Vec<ProjectId>, Vec<Target>) {
        (
            std::mem::take(&mut self.deletes),
            std::mem::take(&mut self.saves),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_only_recorded_once_however_often_it_is_touched() {
        let mut dirty = DirtySet::default();
        dirty.mark(Target::AllTodos);
        dirty.mark(Target::AllTodos);

        let (_, saves) = dirty.drain();
        assert_eq!(saves, [Target::AllTodos]);
    }

    #[test]
    fn deleting_a_project_cancels_a_pending_save_of_the_same_file() {
        let id = ProjectId::from("0123456789abcdef");
        let mut dirty = DirtySet::default();
        dirty.mark(Target::Project(id.clone()));
        dirty.mark_deleted(id.clone());

        let (deletes, saves) = dirty.drain();
        assert_eq!(deletes, [id]);
        assert!(saves.is_empty());
    }

    #[test]
    fn draining_leaves_the_set_empty() {
        let mut dirty = DirtySet::default();
        dirty.mark_bucket(&Bucket::All);
        let _ = dirty.drain();
        assert!(dirty.is_empty());
    }
}
