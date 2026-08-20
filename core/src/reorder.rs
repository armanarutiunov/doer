//! The one reorder rule, for both `J`/`K` on a single todo and on a visual range.
//!
//! Planning is separated from applying so a test can assert *what* a keypress would
//! do without a workspace mutation, and so the reducer can turn a `Blocked` outcome
//! into a status message instead of a silent no-op.

use std::ops::Range;

use crate::action::Dir;
use crate::display::{DisplayList, ViewId};
use crate::id::TodoId;
use crate::workspace::{Bucket, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// Already at the top or bottom of the active list.
    Edge,
    /// Completed todos sit in a block of their own and have no order to change.
    /// The Elixir version computed over the active list regardless and silently
    /// reordered unrelated todos.
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reorder {
    /// Travel within one bucket. `at` is the insertion point *after* the run has
    /// been lifted out, which is what `Workspace::move_bucket` expects.
    Move {
        bucket: Bucket,
        ids: Vec<TodoId>,
        at: usize,
    },
    /// Crossing a section boundary in the All view reassigns the owner rather than
    /// swapping with the neighbour. Landing on the near edge of the section being
    /// entered keeps the run in the same place on screen, so only its heading changes.
    Reassign {
        to: Bucket,
        ids: Vec<TodoId>,
        at: usize,
    },
    Blocked(Blocked),
}

/// `selection` indexes the display order (active, then completed), so it is the same
/// range the cursor and the visual highlight use.
#[must_use]
pub fn plan(
    ws: &Workspace,
    view: &ViewId,
    dl: &DisplayList<'_>,
    selection: Range<usize>,
    dir: Dir,
) -> Reorder {
    if selection.is_empty() {
        return Reorder::Blocked(Blocked::Edge);
    }
    if selection.end > dl.active.len() {
        return Reorder::Blocked(Blocked::Completed);
    }

    let neighbour_index = match dir {
        Dir::Down if selection.end < dl.active.len() => selection.end,
        Dir::Up if selection.start > 0 => selection.start - 1,
        _ => return Reorder::Blocked(Blocked::Edge),
    };
    let (Some(neighbour), Some(leading)) = (
        dl.active.get(neighbour_index),
        dl.active.get(match dir {
            Dir::Down => selection.end - 1,
            Dir::Up => selection.start,
        }),
    ) else {
        return Reorder::Blocked(Blocked::Edge);
    };

    let ids: Vec<TodoId> = selection
        .filter_map(|i| dl.active.get(i))
        .map(|entry| entry.todo.id.clone())
        .collect();

    // The bucket that matters is the one at the edge of the run that is about to
    // touch the neighbour; a visual selection spanning two sections travels as a
    // whole and lands wherever its leading edge lands.
    let leading_bucket = leading.bucket.clone();

    if view.is_all() && neighbour.bucket != leading_bucket {
        let to = neighbour.bucket.clone();
        let at = match dir {
            Dir::Down => 0,
            Dir::Up => ws.todos(&to).len(),
        };
        return Reorder::Reassign { to, ids, at };
    }

    let Some((_, anchor)) = ws.find(&neighbour.todo.id) else {
        return Reorder::Blocked(Blocked::Edge);
    };
    // Positions shift by however much of the run is lifted out ahead of the anchor.
    let lifted_before = ids
        .iter()
        .filter_map(|id| ws.find(id))
        .filter(|(bucket, index)| *bucket == leading_bucket && *index < anchor)
        .count();
    let adjusted = anchor.saturating_sub(lifted_before);

    Reorder::Move {
        bucket: leading_bucket,
        ids,
        at: match dir {
            Dir::Down => adjusted.saturating_add(1),
            Dir::Up => adjusted,
        },
    }
}

pub fn apply(ws: &mut Workspace, plan: &Reorder) {
    match plan {
        Reorder::Move { bucket, ids, at }
        | Reorder::Reassign {
            to: bucket,
            ids,
            at,
        } => {
            ws.move_bucket(ids, bucket, *at);
        }
        Reorder::Blocked(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ProjectId;
    use crate::todo::Todo;

    const T0: i64 = 1_700_000_000;

    struct Fixture {
        ws: Workspace,
        project: ProjectId,
    }

    fn fixture(all: &[&str], project_todos: &[&str]) -> Fixture {
        let mut ws = Workspace::default();
        let project = ws.add_project("Work".into(), None);
        for text in all {
            ws.push_todo(&Bucket::All, Todo::new(*text, T0));
        }
        for text in project_todos {
            ws.push_todo(&Bucket::Project(project.clone()), Todo::new(*text, T0));
        }
        Fixture { ws, project }
    }

    fn order(ws: &Workspace, view: &ViewId) -> Vec<String> {
        DisplayList::build(ws, view, None)
            .order()
            .iter()
            .map(|t| t.text.clone())
            .collect()
    }

    fn run(fx: &mut Fixture, view: &ViewId, selection: Range<usize>, dir: Dir) -> Reorder {
        let dl = DisplayList::build(&fx.ws, view, None);
        let plan = plan(&fx.ws, view, &dl, selection, dir);
        drop(dl);
        apply(&mut fx.ws, &plan);
        plan
    }

    #[test]
    fn a_single_todo_swaps_with_its_neighbour_below() {
        let mut fx = fixture(&["a", "b", "c"], &[]);
        run(&mut fx, &ViewId::All, 0..1, Dir::Down);
        assert_eq!(order(&fx.ws, &ViewId::All), ["b", "a", "c"]);
    }

    #[test]
    fn a_single_todo_swaps_with_its_neighbour_above() {
        let mut fx = fixture(&["a", "b", "c"], &[]);
        run(&mut fx, &ViewId::All, 2..3, Dir::Up);
        assert_eq!(order(&fx.ws, &ViewId::All), ["a", "c", "b"]);
    }

    #[test]
    fn a_visual_run_travels_together_and_keeps_its_internal_order() {
        let mut fx = fixture(&["a", "b", "c", "d"], &[]);
        run(&mut fx, &ViewId::All, 0..2, Dir::Down);
        assert_eq!(order(&fx.ws, &ViewId::All), ["c", "a", "b", "d"]);
    }

    #[test]
    fn reordering_stops_at_the_edges_instead_of_wrapping() {
        let mut fx = fixture(&["a", "b"], &[]);
        assert_eq!(
            run(&mut fx, &ViewId::All, 0..1, Dir::Up),
            Reorder::Blocked(Blocked::Edge)
        );
        assert_eq!(
            run(&mut fx, &ViewId::All, 1..2, Dir::Down),
            Reorder::Blocked(Blocked::Edge)
        );
        assert_eq!(order(&fx.ws, &ViewId::All), ["a", "b"]);
    }

    #[test]
    fn a_completed_todo_cannot_be_reordered_and_leaves_the_active_list_alone() {
        let mut fx = fixture(&["a", "b"], &[]);
        let done_id = fx.ws.todos(&Bucket::All)[1].id.clone();
        fx.ws.toggle(&done_id, T0);

        // Display order is now ["a", "b"] with "b" in the completed block at index 1.
        assert_eq!(
            run(&mut fx, &ViewId::All, 1..2, Dir::Up),
            Reorder::Blocked(Blocked::Completed)
        );
        assert_eq!(order(&fx.ws, &ViewId::All), ["a", "b"]);
    }

    #[test]
    fn crossing_a_section_boundary_downwards_reassigns_instead_of_swapping() {
        let mut fx = fixture(&["ungrouped"], &["project work"]);
        let plan = run(&mut fx, &ViewId::All, 0..1, Dir::Down);

        assert!(matches!(plan, Reorder::Reassign { .. }));
        assert!(fx.ws.todos(&Bucket::All).is_empty());
        assert_eq!(
            order(&fx.ws, &ViewId::All),
            ["ungrouped", "project work"],
            "the todo keeps its place on screen; only its section changes"
        );
        assert_eq!(
            fx.ws.find(
                &fx.ws.todos(&Bucket::Project(fx.project.clone()))[0]
                    .id
                    .clone()
            ),
            Some((Bucket::Project(fx.project.clone()), 0))
        );
    }

    #[test]
    fn crossing_a_section_boundary_upwards_reassigns_to_the_section_above() {
        let mut fx = fixture(&["ungrouped"], &["project work"]);
        let plan = run(&mut fx, &ViewId::All, 1..2, Dir::Up);

        assert!(matches!(plan, Reorder::Reassign { .. }));
        assert!(fx.ws.todos(&Bucket::Project(fx.project.clone())).is_empty());
        assert_eq!(order(&fx.ws, &ViewId::All), ["ungrouped", "project work"]);
    }

    #[test]
    fn a_reassigned_run_keeps_its_internal_order() {
        let mut fx = fixture(&["a", "b"], &["p"]);
        run(&mut fx, &ViewId::All, 0..2, Dir::Down);

        let texts: Vec<String> = fx
            .ws
            .todos(&Bucket::Project(fx.project.clone()))
            .iter()
            .map(|t| t.text.clone())
            .collect();
        assert_eq!(texts, ["a", "b", "p"]);
    }

    #[test]
    fn a_project_view_swaps_across_sections_because_there_are_none() {
        let mut fx = fixture(&[], &["a", "b"]);
        let view = ViewId::Project(fx.project.clone());
        let plan = run(&mut fx, &view, 0..1, Dir::Down);

        assert!(matches!(plan, Reorder::Move { .. }));
        assert_eq!(order(&fx.ws, &view), ["b", "a"]);
    }

    #[test]
    fn a_completed_todo_between_two_active_ones_does_not_absorb_the_move() {
        let mut fx = fixture(&["a", "middle", "b"], &[]);
        let middle = fx.ws.todos(&Bucket::All)[1].id.clone();
        fx.ws.toggle(&middle, T0);

        // Active display order is ["a", "b"]; moving "a" down must land it after "b"
        // in the stored list, not merely past the completed todo sitting between them.
        run(&mut fx, &ViewId::All, 0..1, Dir::Down);

        let active: Vec<String> = fx
            .ws
            .todos(&Bucket::All)
            .iter()
            .filter(|t| !t.done)
            .map(|t| t.text.clone())
            .collect();
        assert_eq!(active, ["b", "a"]);
    }
}
