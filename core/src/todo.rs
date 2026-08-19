use serde::{Deserialize, Serialize};

use crate::id::TodoId;

pub const SECONDS_PER_DAY: i64 = 86_400;

/// Field order is the on-disk key order and matches what the Elixir build wrote.
/// Reordering these fields is a data-format break.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    pub id: TodoId,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
}

impl Todo {
    #[must_use]
    pub fn new(text: impl Into<String>, now: i64) -> Self {
        Self {
            id: TodoId::generate(),
            text: text.into(),
            done: false,
            created_at: now,
            completed_at: None,
        }
    }

    pub fn toggle(&mut self, now: i64) {
        if self.done {
            self.done = false;
            self.completed_at = None;
        } else {
            self.done = true;
            self.completed_at = Some(now);
        }
    }

    /// Whole days since creation, floored, never negative.
    #[must_use]
    pub fn age_days(&self, now: i64) -> i64 {
        days_between(self.created_at, now)
    }

    /// Whole days since completion, or `None` for an active todo.
    #[must_use]
    pub fn completed_days(&self, now: i64) -> Option<i64> {
        self.completed_at.map(|at| days_between(at, now))
    }
}

fn days_between(from: i64, to: i64) -> i64 {
    to.saturating_sub(from).max(0) / SECONDS_PER_DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000;

    #[test]
    fn toggle_sets_and_clears_completed_at() {
        let mut todo = Todo::new("write tests", T0);
        assert!(!todo.done);
        assert_eq!(todo.completed_at, None);

        todo.toggle(T0 + 10);
        assert!(todo.done);
        assert_eq!(todo.completed_at, Some(T0 + 10));

        todo.toggle(T0 + 20);
        assert!(!todo.done);
        assert_eq!(todo.completed_at, None);
    }

    #[test]
    fn age_days_floors_at_day_boundaries() {
        let todo = Todo::new("x", T0);
        assert_eq!(todo.age_days(T0), 0);
        assert_eq!(todo.age_days(T0 + SECONDS_PER_DAY - 1), 0);
        assert_eq!(todo.age_days(T0 + SECONDS_PER_DAY), 1);
        assert_eq!(todo.age_days(T0 + 2 * SECONDS_PER_DAY + 5), 2);
    }

    #[test]
    fn a_clock_that_went_backwards_reads_as_zero_days() {
        let todo = Todo::new("x", T0);
        assert_eq!(todo.age_days(T0 - SECONDS_PER_DAY), 0);
    }

    #[test]
    fn completed_days_is_none_while_active() {
        let mut todo = Todo::new("x", T0);
        assert_eq!(todo.completed_days(T0), None);
        todo.toggle(T0);
        assert_eq!(todo.completed_days(T0 + 3 * SECONDS_PER_DAY), Some(3));
    }

    #[test]
    fn json_key_order_and_shape_match_the_elixir_format() {
        let todo = Todo {
            id: TodoId::from("8257339108cf0e12"),
            text: "buy milk".into(),
            done: false,
            created_at: 1_755_000_000,
            completed_at: None,
        };
        let json = serde_json::to_string_pretty(&todo).expect("serialize");
        assert_eq!(
            json,
            "{\n  \"id\": \"8257339108cf0e12\",\n  \"text\": \"buy milk\",\n  \"done\": false,\n  \"created_at\": 1755000000,\n  \"completed_at\": null\n}"
        );
    }

    #[test]
    fn missing_fields_default_rather_than_failing_the_file() {
        let todo: Todo = serde_json::from_str(r#"{"id":"abc"}"#).expect("lenient decode");
        assert_eq!(todo.text, "");
        assert!(!todo.done);
        assert_eq!(todo.created_at, 0);
        assert_eq!(todo.completed_at, None);
    }
}
