//! The help overlay's content, as data.
//!
//! Kept next to the keymap rather than in the renderer so the two are read together and
//! a binding that changes in `input.rs` has an obvious second place to change.

pub struct Section {
    pub title: &'static str,
    pub bindings: &'static [(&'static str, &'static str)],
}

/// Widest key column across every section, so both columns align on one gutter.
#[must_use]
pub fn key_width() -> usize {
    columns()
        .iter()
        .flat_map(|sections| sections.iter())
        .flat_map(|section| section.bindings.iter())
        .map(|(keys, _)| keys.chars().count())
        .max()
        .unwrap_or(0)
}

#[must_use]
pub fn columns() -> [&'static [Section]; 2] {
    [LEFT, RIGHT]
}

/// Rows in a column, counting the blank line and title each section draws above itself.
#[must_use]
pub fn column_height(sections: &[Section]) -> usize {
    sections
        .iter()
        .map(|s| s.bindings.len() + 3)
        .sum::<usize>()
        .saturating_sub(1)
}

static LEFT: &[Section] = &[
    Section {
        title: "Normal",
        bindings: &[
            ("j k ↓ ↑", "navigate"),
            ("g gg G", "start / end"),
            ("ctrl+d/u", "half page down/up"),
            ("a", "add todo"),
            ("e i", "edit todo"),
            ("d", "delete todo"),
            ("space", "toggle done"),
            ("J K", "reorder todo"),
            ("v", "visual mode"),
            ("/", "search"),
            ("u ctrl+r", "undo / redo"),
            ("h ←", "focus sidebar"),
        ],
    },
    Section {
        title: "Visual",
        bindings: &[
            ("j k ↓ ↑", "extend selection"),
            ("J K", "reorder selected"),
            ("d", "delete selected"),
            ("space", "toggle selected"),
            ("esc", "exit visual"),
        ],
    },
    Section {
        title: "Global",
        bindings: &[
            ("\\", "toggle sidebar"),
            ("tab", "switch focus"),
            ("?", "help"),
            ("q", "quit"),
        ],
    },
];

static RIGHT: &[Section] = &[
    Section {
        title: "Editing",
        bindings: &[
            ("enter", "confirm"),
            ("esc", "cancel"),
            ("← →", "move caret"),
            ("home end", "start/end of line"),
            ("ctrl+w", "delete word back"),
            ("ctrl+u", "delete to start"),
        ],
    },
    Section {
        title: "Search",
        bindings: &[
            ("type", "filter todos"),
            ("enter", "browse matches"),
            ("j k", "next/previous match"),
            ("esc", "cancel search"),
        ],
    },
    Section {
        title: "Sidebar",
        bindings: &[
            ("j k ↓ ↑", "navigate"),
            ("enter l →", "select project"),
            ("a", "add project"),
            ("s", "add subproject"),
            ("e i", "rename project"),
            ("d", "delete project"),
            ("J K", "reorder projects"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_stay_roughly_balanced() {
        let [left, right] = columns();
        let (l, r) = (column_height(left), column_height(right));
        assert!(l.abs_diff(r) <= 8, "left {l} rows, right {r} rows");
    }

    #[test]
    fn descriptions_fit_beside_the_key_gutter() {
        // The overlay's 30-column half has to hold the widest key plus its description.
        let gutter = key_width() + 2;
        for sections in columns() {
            for section in sections {
                for (keys, desc) in section.bindings {
                    let w = gutter + desc.chars().count();
                    assert!(w <= 30, "{keys} / {desc} needs {w} columns");
                }
            }
        }
    }
}
