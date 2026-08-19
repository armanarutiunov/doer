use std::collections::HashMap;

use crate::id::{ProjectId, TodoId};
use crate::project::{Project, Projects};
use crate::todo::Todo;

/// Which list a todo lives in. This replaces the old runtime-only `source` field:
/// ownership is structural, so there is nothing to re-stamp on load or view switch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Bucket {
    All,
    Project(ProjectId),
}

impl Bucket {
    #[must_use]
    pub fn project(&self) -> Option<&ProjectId> {
        match self {
            Self::All => None,
            Self::Project(id) => Some(id),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Workspace {
    all: Vec<Todo>,
    projects: Projects,
    project_todos: HashMap<ProjectId, Vec<Todo>>,
    /// Buckets whose file failed to load. They are never written back, so a damaged
    /// file is not destroyed by the next save.
    read_only: Vec<Bucket>,
}

impl Workspace {
    #[must_use]
    pub fn new(
        all: Vec<Todo>,
        projects: Projects,
        mut project_todos: HashMap<ProjectId, Vec<Todo>>,
    ) -> Self {
        for project in projects.as_slice() {
            project_todos.entry(project.id.clone()).or_default();
        }
        let known: Vec<ProjectId> = projects.as_slice().iter().map(|p| p.id.clone()).collect();
        project_todos.retain(|id, _| known.contains(id));

        let mut this = Self {
            all,
            projects,
            project_todos,
            read_only: Vec::new(),
        };
        this.drop_duplicate_ids();
        this
    }

    /// Todo ids must be globally unique: they address the cursor, undo and every
    /// bulk operation. A duplicate arriving from a hand-edited file is re-issued
    /// rather than dropped, so no text is lost.
    fn drop_duplicate_ids(&mut self) {
        let mut seen: Vec<TodoId> = Vec::new();
        let mut fix = |todos: &mut Vec<Todo>| {
            for todo in todos.iter_mut() {
                if seen.contains(&todo.id) {
                    todo.id = TodoId::generate();
                }
                seen.push(todo.id.clone());
            }
        };
        fix(&mut self.all);
        for project in self
            .projects
            .as_slice()
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()
        {
            if let Some(todos) = self.project_todos.get_mut(&project) {
                fix(todos);
            }
        }
    }

    #[must_use]
    pub fn projects(&self) -> &Projects {
        &self.projects
    }

    #[must_use]
    pub fn buckets_in_display_order(&self) -> Vec<Bucket> {
        let mut out = vec![Bucket::All];
        out.extend(
            self.projects
                .flat_ordered()
                .iter()
                .map(|p| Bucket::Project(p.id.clone())),
        );
        out
    }

    #[must_use]
    pub fn todos(&self, bucket: &Bucket) -> &[Todo] {
        match bucket {
            Bucket::All => &self.all,
            Bucket::Project(id) => self.project_todos.get(id).map_or(&[], Vec::as_slice),
        }
    }

    fn todos_mut(&mut self, bucket: &Bucket) -> Option<&mut Vec<Todo>> {
        match bucket {
            Bucket::All => Some(&mut self.all),
            Bucket::Project(id) => self.project_todos.get_mut(id),
        }
    }

    #[must_use]
    pub fn find(&self, id: &TodoId) -> Option<(Bucket, usize)> {
        for bucket in self.buckets_in_display_order() {
            if let Some(pos) = self.todos(&bucket).iter().position(|t| &t.id == id) {
                return Some((bucket, pos));
            }
        }
        None
    }

    #[must_use]
    pub fn get(&self, id: &TodoId) -> Option<&Todo> {
        self.find(id)
            .and_then(|(bucket, pos)| self.todos(&bucket).get(pos))
    }

    #[must_use]
    pub fn is_read_only(&self, bucket: &Bucket) -> bool {
        self.read_only.contains(bucket)
    }

    pub fn mark_read_only(&mut self, bucket: Bucket) {
        if !self.read_only.contains(&bucket) {
            self.read_only.push(bucket);
        }
    }

    pub fn insert_todo(&mut self, bucket: &Bucket, at: usize, todo: Todo) {
        if let Some(todos) = self.todos_mut(bucket) {
            let at = at.min(todos.len());
            todos.insert(at, todo);
        }
    }

    pub fn push_todo(&mut self, bucket: &Bucket, todo: Todo) {
        if let Some(todos) = self.todos_mut(bucket) {
            todos.push(todo);
        }
    }

    pub fn remove_todo(&mut self, id: &TodoId) -> Option<(Bucket, usize, Todo)> {
        let (bucket, pos) = self.find(id)?;
        let todos = self.todos_mut(&bucket)?;
        if pos >= todos.len() {
            return None;
        }
        let todo = todos.remove(pos);
        Some((bucket, pos, todo))
    }

    pub fn set_text(&mut self, id: &TodoId, text: String) {
        if let Some((bucket, pos)) = self.find(id)
            && let Some(todo) = self.todos_mut(&bucket).and_then(|t| t.get_mut(pos))
        {
            todo.text = text;
        }
    }

    pub fn toggle(&mut self, id: &TodoId, now: i64) {
        if let Some((bucket, pos)) = self.find(id)
            && let Some(todo) = self.todos_mut(&bucket).and_then(|t| t.get_mut(pos))
        {
            todo.toggle(now);
        }
    }

    /// Moves the contiguous run `from` so that it starts at `to`, preserving the
    /// order within the run. This is the only list move in the app: single-item
    /// `J`/`K` and a visual-mode range both come through here.
    pub fn move_run(&mut self, bucket: &Bucket, from: std::ops::Range<usize>, to: usize) {
        let Some(todos) = self.todos_mut(bucket) else {
            return;
        };
        if from.is_empty() || from.end > todos.len() {
            return;
        }
        let len = from.len();
        let to = to.min(todos.len().saturating_sub(len));
        if to == from.start {
            return;
        }
        let run: Vec<Todo> = todos.drain(from).collect();
        let at = to.min(todos.len());
        for (offset, todo) in run.into_iter().enumerate() {
            todos.insert(at.saturating_add(offset), todo);
        }
    }

    /// Moves `ids` into `to`, inserting at `at` and preserving their relative order.
    /// This is what a reorder crossing a section boundary does instead of swapping.
    pub fn move_bucket(&mut self, ids: &[TodoId], to: &Bucket, at: usize) {
        let mut moved = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some((_, _, todo)) = self.remove_todo(id) {
                moved.push(todo);
            }
        }
        let Some(todos) = self.todos_mut(to) else {
            return;
        };
        let at = at.min(todos.len());
        for (offset, todo) in moved.into_iter().enumerate() {
            todos.insert(at.saturating_add(offset), todo);
        }
    }

    pub fn add_project(&mut self, name: String, parent: Option<ProjectId>) -> ProjectId {
        let index = self.projects.next_index(parent.as_ref());
        let project = Project::new(name, index, parent);
        let id = project.id.clone();
        self.projects.push(project);
        self.project_todos.entry(id.clone()).or_default();
        id
    }

    pub fn rename_project(&mut self, id: &ProjectId, name: String) {
        self.projects.rename(id, name);
    }

    /// Deletes a project and its children. Returns every id removed so the caller
    /// can delete the matching files.
    pub fn delete_project(&mut self, id: &ProjectId) -> Vec<ProjectId> {
        let subtree = self.projects.subtree(id);
        self.projects.remove_all(&subtree);
        for gone in &subtree {
            self.project_todos.remove(gone);
            self.read_only.retain(|b| b.project() != Some(gone));
        }
        subtree
    }

    pub fn reorder_project(&mut self, id: &ProjectId, down: bool) -> bool {
        self.projects.swap_within_level(id, down)
    }

    /// Drives the delete confirmation: only a subtree holding unfinished work asks.
    #[must_use]
    pub fn has_open_todos(&self, id: &ProjectId) -> bool {
        self.projects.subtree(id).iter().any(|p| {
            self.todos(&Bucket::Project(p.clone()))
                .iter()
                .any(|t| !t.done)
        })
    }

    #[must_use]
    pub fn project_file(&self, id: &ProjectId) -> Option<(&Project, &[Todo])> {
        let project = self.projects.get(id)?;
        Some((project, self.todos(&Bucket::Project(id.clone()))))
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        let mut ids: Vec<&TodoId> = self.all.iter().map(|t| &t.id).collect();
        for project in self.projects.as_slice() {
            let Some(todos) = self.project_todos.get(&project.id) else {
                return false;
            };
            ids.extend(todos.iter().map(|t| &t.id));
        }
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        sorted.len() == ids.len() && self.project_todos.len() == self.projects.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000;

    fn todo(text: &str) -> Todo {
        Todo::new(text, T0)
    }

    fn texts(ws: &Workspace, bucket: &Bucket) -> Vec<String> {
        ws.todos(bucket).iter().map(|t| t.text.clone()).collect()
    }

    fn workspace_with_one_project() -> (Workspace, ProjectId) {
        let mut ws = Workspace::default();
        let id = ws.add_project("Work".into(), None);
        (ws, id)
    }

    #[test]
    fn a_new_project_starts_with_an_empty_todo_list() {
        let (ws, id) = workspace_with_one_project();
        assert!(ws.todos(&Bucket::Project(id)).is_empty());
        assert!(ws.is_valid());
    }

    #[test]
    fn find_reports_the_bucket_and_position_of_a_todo() {
        let (mut ws, project) = workspace_with_one_project();
        ws.push_todo(&Bucket::All, todo("ungrouped"));
        let tracked = todo("in project");
        let tracked_id = tracked.id.clone();
        ws.push_todo(&Bucket::Project(project.clone()), tracked);

        assert_eq!(ws.find(&tracked_id), Some((Bucket::Project(project), 0)));
        assert_eq!(
            ws.get(&tracked_id).map(|t| t.text.as_str()),
            Some("in project")
        );
    }

    #[test]
    fn removing_a_todo_returns_where_it_came_from() {
        let mut ws = Workspace::default();
        ws.push_todo(&Bucket::All, todo("a"));
        let second = todo("b");
        let id = second.id.clone();
        ws.push_todo(&Bucket::All, second);

        let (bucket, pos, removed) = ws.remove_todo(&id).expect("removed");
        assert_eq!(bucket, Bucket::All);
        assert_eq!(pos, 1);
        assert_eq!(removed.text, "b");
        assert_eq!(texts(&ws, &Bucket::All), ["a"]);
    }

    #[test]
    fn move_run_moves_a_single_todo_down() {
        let mut ws = Workspace::default();
        for text in ["a", "b", "c"] {
            ws.push_todo(&Bucket::All, todo(text));
        }
        ws.move_run(&Bucket::All, 0..1, 1);
        assert_eq!(texts(&ws, &Bucket::All), ["b", "a", "c"]);
    }

    #[test]
    fn move_run_keeps_the_order_within_a_multi_todo_run() {
        let mut ws = Workspace::default();
        for text in ["a", "b", "c", "d"] {
            ws.push_todo(&Bucket::All, todo(text));
        }
        ws.move_run(&Bucket::All, 0..2, 1);
        assert_eq!(texts(&ws, &Bucket::All), ["c", "a", "b", "d"]);
    }

    #[test]
    fn move_run_clamps_rather_than_falling_off_the_end() {
        let mut ws = Workspace::default();
        for text in ["a", "b", "c"] {
            ws.push_todo(&Bucket::All, todo(text));
        }
        ws.move_run(&Bucket::All, 1..2, 99);
        assert_eq!(texts(&ws, &Bucket::All), ["a", "c", "b"]);
    }

    #[test]
    fn move_bucket_carries_todos_across_and_keeps_their_order() {
        let (mut ws, project) = workspace_with_one_project();
        let mut ids = Vec::new();
        for text in ["a", "b"] {
            let t = todo(text);
            ids.push(t.id.clone());
            ws.push_todo(&Bucket::All, t);
        }
        ws.push_todo(&Bucket::Project(project.clone()), todo("existing"));

        ws.move_bucket(&ids, &Bucket::Project(project.clone()), 0);

        assert!(texts(&ws, &Bucket::All).is_empty());
        assert_eq!(
            texts(&ws, &Bucket::Project(project)),
            ["a", "b", "existing"]
        );
        assert!(ws.is_valid());
    }

    #[test]
    fn deleting_a_project_takes_its_children_and_their_todos() {
        let mut ws = Workspace::default();
        let parent = ws.add_project("Parent".into(), None);
        let child = ws.add_project("Child".into(), Some(parent.clone()));
        ws.push_todo(&Bucket::Project(child.clone()), todo("child work"));

        let removed = ws.delete_project(&parent);

        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&child));
        assert_eq!(ws.projects().len(), 0);
        assert!(ws.todos(&Bucket::Project(child)).is_empty());
        assert!(ws.is_valid());
    }

    #[test]
    fn has_open_todos_looks_through_the_whole_subtree() {
        let mut ws = Workspace::default();
        let parent = ws.add_project("Parent".into(), None);
        let child = ws.add_project("Child".into(), Some(parent.clone()));
        assert!(!ws.has_open_todos(&parent));

        let mut done = todo("finished");
        done.toggle(T0);
        ws.push_todo(&Bucket::Project(child.clone()), done);
        assert!(!ws.has_open_todos(&parent));

        ws.push_todo(&Bucket::Project(child), todo("still open"));
        assert!(ws.has_open_todos(&parent));
    }

    #[test]
    fn duplicate_ids_from_a_hand_edited_file_are_re_issued_not_dropped() {
        let shared = todo("original");
        let mut clone = shared.clone();
        clone.text = "duplicate".into();

        let ws = Workspace::new(vec![shared, clone], Projects::default(), HashMap::new());

        assert_eq!(texts(&ws, &Bucket::All), ["original", "duplicate"]);
        assert!(ws.is_valid());
    }

    #[test]
    fn todos_for_a_project_that_no_longer_exists_are_discarded_on_load() {
        let mut orphaned = HashMap::new();
        orphaned.insert(ProjectId::from("gone"), vec![todo("stale")]);

        let ws = Workspace::new(Vec::new(), Projects::default(), orphaned);

        assert!(ws.is_valid());
        assert!(
            ws.todos(&Bucket::Project(ProjectId::from("gone")))
                .is_empty()
        );
    }

    #[test]
    fn a_read_only_bucket_stays_marked_until_its_project_is_deleted() {
        let (mut ws, project) = workspace_with_one_project();
        ws.mark_read_only(Bucket::Project(project.clone()));
        assert!(ws.is_read_only(&Bucket::Project(project.clone())));

        ws.delete_project(&project);
        assert!(!ws.is_read_only(&Bucket::Project(project)));
    }

    #[test]
    fn buckets_in_display_order_follow_the_sidebar() {
        let mut ws = Workspace::default();
        let a = ws.add_project("A".into(), None);
        let b = ws.add_project("B".into(), None);
        let a1 = ws.add_project("A1".into(), Some(a.clone()));

        assert_eq!(
            ws.buckets_in_display_order(),
            [
                Bucket::All,
                Bucket::Project(a),
                Bucket::Project(a1),
                Bucket::Project(b)
            ]
        );
    }
}
