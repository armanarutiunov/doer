use serde::{Deserialize, Serialize};

use crate::id::ProjectId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub parent_id: Option<ProjectId>,
}

impl Project {
    #[must_use]
    pub fn new(name: impl Into<String>, index: i64, parent_id: Option<ProjectId>) -> Self {
        Self {
            id: ProjectId::generate(),
            name: name.into(),
            index,
            parent_id,
        }
    }

    #[must_use]
    pub fn is_top_level(&self) -> bool {
        self.parent_id.is_none()
    }
}

/// Projects are two levels deep: top-level parents, each with children.
/// Ordering is by `index` within a level, with the id as a tiebreak so a duplicated
/// or corrupt index degrades to a stable order instead of flickering between frames.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Projects {
    items: Vec<Project>,
}

impl Projects {
    #[must_use]
    pub fn new(items: Vec<Project>) -> Self {
        let mut this = Self { items };
        this.repair();
        this
    }

    /// Enforces the two-level invariant: a child whose parent is missing, or whose
    /// parent is itself a child, is promoted to top level rather than dropped.
    fn repair(&mut self) {
        self.drop_duplicate_ids();
        let top: Vec<ProjectId> = self
            .items
            .iter()
            .filter(|p| p.is_top_level())
            .map(|p| p.id.clone())
            .collect();
        for project in &mut self.items {
            if let Some(parent) = &project.parent_id
                && !top.contains(parent)
            {
                project.parent_id = None;
            }
        }
    }

    /// A project id addresses its file and its todo list, so two projects cannot share
    /// one. Unlike a duplicate todo id, which `Workspace` re-issues because the text is
    /// what matters, a re-issued project id would address no file and no todos — a
    /// phantom project that then writes itself to disk. Keeping the first and dropping
    /// the rest loses strictly less.
    fn drop_duplicate_ids(&mut self) {
        let mut seen: Vec<ProjectId> = Vec::with_capacity(self.items.len());
        self.items.retain(|project| {
            if seen.contains(&project.id) {
                return false;
            }
            seen.push(project.id.clone());
            true
        });
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Project] {
        &self.items
    }

    #[must_use]
    pub fn get(&self, id: &ProjectId) -> Option<&Project> {
        self.items.iter().find(|p| &p.id == id)
    }

    fn get_mut(&mut self, id: &ProjectId) -> Option<&mut Project> {
        self.items.iter_mut().find(|p| &p.id == id)
    }

    /// Top-level projects in display order.
    #[must_use]
    pub fn top_level(&self) -> Vec<&Project> {
        let mut parents: Vec<&Project> = self.items.iter().filter(|p| p.is_top_level()).collect();
        parents.sort_by(|a, b| a.index.cmp(&b.index).then_with(|| a.id.cmp(&b.id)));
        parents
    }

    /// Children of `parent` in display order.
    #[must_use]
    pub fn children(&self, parent: &ProjectId) -> Vec<&Project> {
        let mut kids: Vec<&Project> = self
            .items
            .iter()
            .filter(|p| p.parent_id.as_ref() == Some(parent))
            .collect();
        kids.sort_by(|a, b| a.index.cmp(&b.index).then_with(|| a.id.cmp(&b.id)));
        kids
    }

    /// Every project in sidebar order: each parent followed by its children.
    #[must_use]
    pub fn flat_ordered(&self) -> Vec<&Project> {
        let mut out = Vec::with_capacity(self.items.len());
        for parent in self.top_level() {
            out.push(parent);
            out.extend(self.children(&parent.id));
        }
        out
    }

    /// `id` plus every descendant, deepest last.
    #[must_use]
    pub fn subtree(&self, id: &ProjectId) -> Vec<ProjectId> {
        let mut out = vec![id.clone()];
        out.extend(self.children(id).into_iter().map(|c| c.id.clone()));
        out
    }

    /// Depth in the sidebar: 0 for a parent, 1 for a child.
    #[must_use]
    pub fn depth(&self, id: &ProjectId) -> usize {
        usize::from(self.get(id).is_some_and(|p| p.parent_id.is_some()))
    }

    pub fn push(&mut self, project: Project) {
        self.items.push(project);
        self.repair();
    }

    pub fn remove_all(&mut self, ids: &[ProjectId]) {
        self.items.retain(|p| !ids.contains(&p.id));
        self.repair();
    }

    pub fn rename(&mut self, id: &ProjectId, name: String) {
        if let Some(project) = self.get_mut(id) {
            project.name = name;
        }
    }

    /// Next free index within `parent`'s level.
    #[must_use]
    pub fn next_index(&self, parent: Option<&ProjectId>) -> i64 {
        let count = match parent {
            None => self.top_level().len(),
            Some(p) => self.children(p).len(),
        };
        i64::try_from(count).unwrap_or(i64::MAX)
    }

    /// Swaps `id` with its neighbour within its own level. Returns false at an edge.
    pub fn swap_within_level(&mut self, id: &ProjectId, down: bool) -> bool {
        let Some(project) = self.get(id) else {
            return false;
        };
        let siblings: Vec<ProjectId> = match &project.parent_id {
            None => self.top_level().into_iter().map(|p| p.id.clone()).collect(),
            Some(parent) => self
                .children(parent)
                .into_iter()
                .map(|p| p.id.clone())
                .collect(),
        };
        let Some(pos) = siblings.iter().position(|s| s == id) else {
            return false;
        };
        let target = if down {
            pos.checked_add(1)
        } else {
            pos.checked_sub(1)
        };
        let Some(target) = target.filter(|t| *t < siblings.len()) else {
            return false;
        };
        let Some(other) = siblings.get(target) else {
            return false;
        };

        // Indices are not guaranteed dense, so swap the values rather than assuming pos == index.
        let Some(a) = self.get(id).map(|p| p.index) else {
            return false;
        };
        let Some(b) = self.get(other).map(|p| p.index) else {
            return false;
        };
        if a == b {
            // Degenerate data: force an ordering so the swap is observable.
            if let Some(p) = self.get_mut(id) {
                p.index = if down {
                    b.saturating_add(1)
                } else {
                    b.saturating_sub(1)
                };
            }
            return true;
        }
        let other = other.clone();
        if let Some(p) = self.get_mut(id) {
            p.index = b;
        }
        if let Some(p) = self.get_mut(&other) {
            p.index = a;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, index: i64, parent: Option<&str>) -> Project {
        Project {
            id: ProjectId::from(id),
            name: id.to_uppercase(),
            index,
            parent_id: parent.map(ProjectId::from),
        }
    }

    #[test]
    fn flat_ordered_puts_children_under_their_parent() {
        let projects = Projects::new(vec![
            p("b", 1, None),
            p("a", 0, None),
            p("a2", 1, Some("a")),
            p("a1", 0, Some("a")),
            p("b1", 0, Some("b")),
        ]);
        let order: Vec<&str> = projects
            .flat_ordered()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(order, ["a", "a1", "a2", "b", "b1"]);
    }

    #[test]
    fn duplicate_indices_order_stably_by_id() {
        let projects = Projects::new(vec![p("z", 0, None), p("a", 0, None)]);
        let order: Vec<&str> = projects
            .flat_ordered()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(order, ["a", "z"]);
    }

    #[test]
    fn an_orphaned_child_is_promoted_not_lost() {
        let projects = Projects::new(vec![p("kid", 0, Some("gone"))]);
        assert_eq!(projects.len(), 1);
        assert!(
            projects
                .get(&ProjectId::from("kid"))
                .is_some_and(Project::is_top_level)
        );
        let order: Vec<&str> = projects
            .flat_ordered()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(order, ["kid"]);
    }

    #[test]
    fn a_grandchild_is_flattened_to_top_level() {
        let projects = Projects::new(vec![
            p("a", 0, None),
            p("b", 0, Some("a")),
            p("c", 0, Some("b")),
        ]);
        assert!(
            projects
                .get(&ProjectId::from("c"))
                .is_some_and(Project::is_top_level)
        );
    }

    #[test]
    fn subtree_includes_self_and_children() {
        let projects = Projects::new(vec![
            p("a", 0, None),
            p("a1", 0, Some("a")),
            p("b", 1, None),
        ]);
        let ids = projects.subtree(&ProjectId::from("a"));
        assert_eq!(ids, [ProjectId::from("a"), ProjectId::from("a1")]);
    }

    #[test]
    fn removing_a_parent_promotes_nothing_because_children_go_with_it() {
        let mut projects = Projects::new(vec![p("a", 0, None), p("a1", 0, Some("a"))]);
        let subtree = projects.subtree(&ProjectId::from("a"));
        projects.remove_all(&subtree);
        assert!(projects.is_empty());
    }

    #[test]
    fn swap_within_level_moves_parents_and_stops_at_the_edges() {
        let mut projects = Projects::new(vec![p("a", 0, None), p("b", 1, None), p("c", 2, None)]);
        assert!(projects.swap_within_level(&ProjectId::from("b"), true));
        let order: Vec<&str> = projects
            .flat_ordered()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(order, ["a", "c", "b"]);

        assert!(!projects.swap_within_level(&ProjectId::from("b"), true));
        assert!(!projects.swap_within_level(&ProjectId::from("a"), false));
    }

    #[test]
    fn swap_within_level_keeps_children_inside_their_parent() {
        let mut projects = Projects::new(vec![
            p("a", 0, None),
            p("a1", 0, Some("a")),
            p("a2", 1, Some("a")),
            p("b", 1, None),
        ]);
        assert!(projects.swap_within_level(&ProjectId::from("a1"), true));
        let order: Vec<&str> = projects
            .flat_ordered()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(order, ["a", "a2", "a1", "b"]);
    }

    #[test]
    fn swap_with_identical_indices_still_reorders() {
        let mut projects = Projects::new(vec![p("a", 0, None), p("b", 0, None)]);
        assert!(projects.swap_within_level(&ProjectId::from("b"), false));
        let order: Vec<&str> = projects
            .flat_ordered()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(order, ["b", "a"]);
    }

    #[test]
    fn two_projects_claiming_one_id_collapse_to_the_first() {
        let mut first = p("dup", 0, None);
        first.name = "kept".into();
        let mut second = p("dup", 1, None);
        second.name = "dropped".into();

        let projects = Projects::new(vec![first, second, p("other", 2, None)]);

        assert_eq!(projects.len(), 2);
        assert_eq!(
            projects
                .get(&ProjectId::from("dup"))
                .map(|p| p.name.as_str()),
            Some("kept")
        );
    }

    #[test]
    fn a_duplicate_id_added_later_is_not_admitted() {
        let mut projects = Projects::new(vec![p("a", 0, None)]);
        projects.push(p("a", 1, None));
        assert_eq!(projects.len(), 1);
    }

    #[test]
    fn next_index_counts_the_target_level() {
        let projects = Projects::new(vec![p("a", 0, None), p("a1", 0, Some("a"))]);
        assert_eq!(projects.next_index(None), 1);
        assert_eq!(projects.next_index(Some(&ProjectId::from("a"))), 1);
    }
}
