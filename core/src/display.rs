use crate::id::ProjectId;
use crate::todo::Todo;
use crate::workspace::{Bucket, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewId {
    All,
    Project(ProjectId),
}

impl ViewId {
    #[must_use]
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
}

#[derive(Clone, Debug)]
pub struct Entry<'a> {
    pub todo: &'a Todo,
    pub bucket: Bucket,
}

/// The todos a view shows, split the way they are drawn: active on top,
/// completed below. Derived fresh from the workspace, never stored as truth.
#[derive(Clone, Debug)]
pub struct DisplayList<'a> {
    pub active: Vec<Entry<'a>>,
    pub completed: Vec<Entry<'a>>,
    /// Section headers are drawn only in the All view, only when a project todo is
    /// present, and never while searching.
    pub sectioned: bool,
}

impl<'a> DisplayList<'a> {
    #[must_use]
    pub fn build(ws: &'a Workspace, view: &ViewId, filter: Option<&str>) -> Self {
        let buckets: Vec<Bucket> = match view {
            ViewId::All => ws.buckets_in_display_order(),
            ViewId::Project(id) => vec![Bucket::Project(id.clone())],
        };

        let needle = filter.map(str::to_lowercase).filter(|f| !f.is_empty());
        let mut active = Vec::new();
        let mut completed = Vec::new();

        for bucket in &buckets {
            for todo in ws.todos(bucket) {
                if let Some(needle) = &needle
                    && !todo.text.to_lowercase().contains(needle)
                {
                    continue;
                }
                let entry = Entry {
                    todo,
                    bucket: bucket.clone(),
                };
                if todo.done {
                    completed.push(entry);
                } else {
                    active.push(entry);
                }
            }
        }

        let sectioned = view.is_all()
            && needle.is_none()
            && active.iter().any(|e| e.bucket.project().is_some());

        Self {
            active,
            completed,
            sectioned,
        }
    }

    /// Display order: active first, then completed. This is the order the cursor
    /// and visual selection index into.
    #[must_use]
    pub fn order(&self) -> Vec<&'a Todo> {
        self.active
            .iter()
            .chain(self.completed.iter())
            .map(|e| e.todo)
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.active.len() + self.completed.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Projects;
    use std::collections::HashMap;

    const T0: i64 = 1_700_000_000;

    fn ws_with_project_todos() -> (Workspace, ProjectId) {
        let mut ws = Workspace::new(Vec::new(), Projects::default(), HashMap::new());
        let project = ws.add_project("Work".into(), None);
        ws.push_todo(&Bucket::All, Todo::new("ungrouped", T0));
        ws.push_todo(
            &Bucket::Project(project.clone()),
            Todo::new("project work", T0),
        );
        (ws, project)
    }

    #[test]
    fn the_all_view_lists_ungrouped_todos_before_project_todos() {
        let (ws, _) = ws_with_project_todos();
        let dl = DisplayList::build(&ws, &ViewId::All, None);
        let texts: Vec<&str> = dl.order().iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["ungrouped", "project work"]);
    }

    #[test]
    fn a_project_view_shows_only_that_project() {
        let (ws, project) = ws_with_project_todos();
        let dl = DisplayList::build(&ws, &ViewId::Project(project), None);
        let texts: Vec<&str> = dl.order().iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["project work"]);
    }

    #[test]
    fn completed_todos_sort_below_active_ones_regardless_of_bucket() {
        let (mut ws, project) = ws_with_project_todos();
        let mut done = Todo::new("finished", T0);
        done.toggle(T0);
        ws.push_todo(&Bucket::All, done);
        ws.push_todo(&Bucket::Project(project), Todo::new("later", T0));

        let dl = DisplayList::build(&ws, &ViewId::All, None);
        let texts: Vec<&str> = dl.order().iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["ungrouped", "project work", "later", "finished"]);
    }

    #[test]
    fn sections_appear_only_when_a_project_todo_is_visible() {
        let (ws, _) = ws_with_project_todos();
        assert!(DisplayList::build(&ws, &ViewId::All, None).sectioned);

        let mut plain = Workspace::new(Vec::new(), Projects::default(), HashMap::new());
        plain.push_todo(&Bucket::All, Todo::new("only ungrouped", T0));
        assert!(!DisplayList::build(&plain, &ViewId::All, None).sectioned);
    }

    #[test]
    fn searching_suppresses_sections() {
        let (ws, _) = ws_with_project_todos();
        let dl = DisplayList::build(&ws, &ViewId::All, Some("work"));
        assert!(!dl.sectioned);
    }

    #[test]
    fn the_filter_is_case_insensitive_and_matches_anywhere() {
        let (ws, _) = ws_with_project_todos();
        let dl = DisplayList::build(&ws, &ViewId::All, Some("GROUP"));
        let texts: Vec<&str> = dl.order().iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["ungrouped"]);
    }

    #[test]
    fn an_empty_query_filters_nothing() {
        let (ws, _) = ws_with_project_todos();
        assert_eq!(DisplayList::build(&ws, &ViewId::All, Some("")).len(), 2);
    }

    #[test]
    fn the_filter_also_matches_completed_todos() {
        let (mut ws, _) = ws_with_project_todos();
        let mut done = Todo::new("archived work", T0);
        done.toggle(T0);
        ws.push_todo(&Bucket::All, done);

        let dl = DisplayList::build(&ws, &ViewId::All, Some("work"));
        assert_eq!(dl.active.len(), 1);
        assert_eq!(dl.completed.len(), 1);
    }
}
