use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Display columns, not bytes and not chars: CJK and emoji occupy two.
#[must_use]
pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// One wrapped line: the text, and the display column it starts at within the source.
///
/// The start column is what lets a caret be placed exactly. Wrapping drops the space at
/// each break, so summing the widths of the lines drifts one column per break -- which is
/// visible as a caret sitting to the right of the character it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedLine {
    pub text: String,
    pub start_col: usize,
}

/// Greedy word wrap measured in display columns. A word longer than the line is
/// hard-broken on grapheme boundaries, and a double-width grapheme that would
/// straddle the edge moves to the next line rather than being split.
#[must_use]
pub fn wrap(text: &str, max_width: usize) -> Vec<String> {
    wrap_lines(text, max_width)
        .into_iter()
        .map(|line| line.text)
        .collect()
}

#[must_use]
pub fn wrap_lines(text: &str, max_width: usize) -> Vec<WrappedLine> {
    if text.is_empty() || max_width == 0 {
        return vec![WrappedLine {
            text: String::new(),
            start_col: 0,
        }];
    }

    let mut lines: Vec<WrappedLine> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;
    // Where the line being built starts, and where the source has been consumed to.
    let mut current_start = 0;
    let mut source_col = 0;

    let mut push = |current: &mut String, start: usize| {
        lines.push(WrappedLine {
            text: std::mem::take(current),
            start_col: start,
        });
    };

    for (index, word) in text.split(' ').enumerate() {
        let word_width = width(word);
        // Every split consumed one space from the source except before the first word.
        if index > 0 {
            source_col += 1;
        }
        let space = usize::from(!current.is_empty());

        if !current.is_empty() && current_width + space + word_width <= max_width {
            current.push(' ');
            current.push_str(word);
            current_width += space + word_width;
            source_col += word_width;
            continue;
        }

        if word_width <= max_width {
            if !current.is_empty() {
                push(&mut current, current_start);
            }
            current = word.to_string();
            current_width = word_width;
            current_start = source_col;
            source_col += word_width;
            continue;
        }

        for chunk in hard_break(word, max_width) {
            if !current.is_empty() {
                push(&mut current, current_start);
            }
            current_width = width(&chunk);
            current_start = source_col;
            source_col += current_width;
            current = chunk;
        }
    }

    push(&mut current, current_start);
    lines
}

fn hard_break(word: &str, max_width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut chunk_width = 0;

    for grapheme in word.graphemes(true) {
        let g_width = width(grapheme).max(1);
        if chunk_width + g_width > max_width && !chunk.is_empty() {
            chunks.push(std::mem::take(&mut chunk));
            chunk_width = 0;
        }
        chunk.push_str(grapheme);
        chunk_width += g_width;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

/// Shortens to `max_width` columns, marking the cut with an ellipsis.
#[must_use]
pub fn truncate(text: &str, max_width: usize) -> String {
    if width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }

    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let g_width = width(grapheme).max(1);
        if used + g_width > budget {
            break;
        }
        out.push_str(grapheme);
        used += g_width;
    }
    out.push('…');
    out
}

#[must_use]
pub fn pad_end(text: &str, max_width: usize) -> String {
    let mut out = text.to_string();
    out.push_str(&" ".repeat(max_width.saturating_sub(width(text))));
    out
}

#[must_use]
pub fn pad_start(text: &str, max_width: usize) -> String {
    let mut out = " ".repeat(max_width.saturating_sub(width(text)));
    out.push_str(text);
    out
}

/// A single-line editable buffer. The caret is a byte offset that is always on a
/// grapheme boundary, so combining marks and ZWJ emoji move and delete as one unit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextInput {
    text: String,
    caret: usize,
}

impl TextInput {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let caret = text.len();
        Self { text, caret }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn into_text(self) -> String {
        self.text
    }

    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Byte offset of the caret. Exposed so tests can assert the grapheme-boundary
    /// invariant that every other method here relies on.
    #[must_use]
    pub fn caret_byte(&self) -> usize {
        self.caret
    }

    /// Column the caret sits at, for positioning the terminal cursor.
    #[must_use]
    pub fn caret_col(&self) -> usize {
        width(self.before_caret())
    }

    fn before_caret(&self) -> &str {
        self.text.get(..self.caret).unwrap_or(&self.text)
    }

    fn boundaries(&self) -> Vec<usize> {
        let mut out: Vec<usize> = self.text.grapheme_indices(true).map(|(i, _)| i).collect();
        out.push(self.text.len());
        out
    }

    /// Control characters would break the column arithmetic, so a tab or newline
    /// arriving from a paste becomes a space.
    pub fn insert_char(&mut self, ch: char) {
        let ch = if ch.is_control() { ' ' } else { ch };
        self.text.insert(self.caret, ch);
        self.caret = self.caret.saturating_add(ch.len_utf8());
    }

    pub fn insert_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert_char(ch);
        }
    }

    pub fn backspace(&mut self) {
        let boundaries = self.boundaries();
        let Some(pos) = boundaries.iter().position(|b| *b == self.caret) else {
            return;
        };
        let Some(prev) = pos.checked_sub(1).and_then(|p| boundaries.get(p)).copied() else {
            return;
        };
        self.text.replace_range(prev..self.caret, "");
        self.caret = prev;
    }

    pub fn delete(&mut self) {
        let boundaries = self.boundaries();
        let Some(pos) = boundaries.iter().position(|b| *b == self.caret) else {
            return;
        };
        let Some(next) = boundaries.get(pos.saturating_add(1)).copied() else {
            return;
        };
        self.text.replace_range(self.caret..next, "");
    }

    pub fn move_left(&mut self) {
        let boundaries = self.boundaries();
        if let Some(pos) = boundaries.iter().position(|b| *b == self.caret)
            && let Some(prev) = pos.checked_sub(1).and_then(|p| boundaries.get(p))
        {
            self.caret = *prev;
        }
    }

    pub fn move_right(&mut self) {
        let boundaries = self.boundaries();
        if let Some(pos) = boundaries.iter().position(|b| *b == self.caret)
            && let Some(next) = boundaries.get(pos.saturating_add(1))
        {
            self.caret = *next;
        }
    }

    pub fn move_home(&mut self) {
        self.caret = 0;
    }

    pub fn move_end(&mut self) {
        self.caret = self.text.len();
    }

    pub fn move_word_left(&mut self) {
        self.caret = self.word_start();
    }

    pub fn move_word_right(&mut self) {
        self.caret = self.word_end();
    }

    pub fn delete_word_before(&mut self) {
        let start = self.word_start();
        self.text.replace_range(start..self.caret, "");
        self.caret = start;
    }

    pub fn delete_to_start(&mut self) {
        self.text.replace_range(..self.caret, "");
        self.caret = 0;
    }

    fn word_start(&self) -> usize {
        let before = self.before_caret();
        let mut start = 0;
        let mut seen_word = false;
        for (offset, word) in before.split_word_bound_indices() {
            if !word.trim().is_empty() {
                start = offset;
                seen_word = true;
            }
        }
        if seen_word { start } else { 0 }
    }

    fn word_end(&self) -> usize {
        let after = self.text.get(self.caret..).unwrap_or("");
        for (offset, word) in after.split_word_bound_indices() {
            if !word.trim().is_empty() {
                return self.caret.saturating_add(offset).saturating_add(word.len());
            }
        }
        self.text.len()
    }
}

/// Named casts at the u16/usize boundary, so the conversions are not sprinkled
/// through the layout and render code.
#[must_use]
pub fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

#[must_use]
pub fn to_usize(n: u16) -> usize {
    usize::from(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_counts_display_columns_not_characters() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("世界"), 4);
        assert_eq!(width(""), 0);
    }

    #[test]
    fn an_empty_string_still_occupies_one_line() {
        assert_eq!(wrap("", 10), [""]);
    }

    #[test]
    fn wrap_breaks_between_words() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
    }

    #[test]
    fn wrap_never_exceeds_the_width_for_wide_characters() {
        for line in wrap("世界世界世界", 5) {
            assert!(width(&line) <= 5, "{line:?} is too wide");
        }
    }

    #[test]
    fn a_word_longer_than_the_line_is_hard_broken() {
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
    }

    #[test]
    fn a_long_word_after_a_short_one_starts_on_its_own_line() {
        assert_eq!(wrap("hi abcdefgh", 3), ["hi", "abc", "def", "gh"]);
    }

    #[test]
    fn wrapping_preserves_every_grapheme() {
        let text = "a👍🏽b 世界 ok";
        let joined: String = wrap(text, 4).join("");
        let original: String = text.chars().filter(|c| *c != ' ').collect();
        let produced: String = joined.chars().filter(|c| *c != ' ').collect();
        assert_eq!(produced, original);
    }

    #[test]
    fn a_wrapped_lines_start_column_accounts_for_the_spaces_wrapping_dropped() {
        let text = "aaa bbb ccc ddd";
        let lines = wrap_lines(text, 7);
        assert_eq!(
            lines,
            [
                WrappedLine {
                    text: "aaa bbb".into(),
                    start_col: 0
                },
                WrappedLine {
                    text: "ccc ddd".into(),
                    start_col: 8
                },
            ]
        );
    }

    #[test]
    fn every_start_column_matches_the_position_of_the_line_in_the_source() {
        let text = "one two three four five six seven eight";
        for width_limit in 3..20 {
            for line in wrap_lines(text, width_limit) {
                if line.text.is_empty() {
                    continue;
                }
                let at = width(&text[..line.start_col.min(text.len())]);
                assert_eq!(
                    at, line.start_col,
                    "line {:?} claims column {} at width {width_limit}",
                    line.text, line.start_col
                );
            }
        }
    }

    #[test]
    fn a_hard_broken_word_has_no_gap_between_its_lines() {
        let lines = wrap_lines("abcdefgh", 3);
        let columns: Vec<usize> = lines.iter().map(|l| l.start_col).collect();
        assert_eq!(columns, [0, 3, 6]);
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn truncate_marks_the_cut_and_respects_width() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert!(width(&truncate("世界世界", 5)) <= 5);
    }

    #[test]
    fn padding_is_measured_in_columns() {
        assert_eq!(pad_end("世", 4), "世  ");
        assert_eq!(pad_start("1d", 4), "  1d");
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut input = TextInput::new("ac");
        input.move_left();
        input.insert_char('b');
        assert_eq!(input.text(), "abc");
        assert_eq!(input.caret_col(), 2);
    }

    #[test]
    fn backspace_removes_a_whole_grapheme_cluster() {
        let mut input = TextInput::new("a👍🏽");
        input.backspace();
        assert_eq!(input.text(), "a");

        let mut flag = TextInput::new("x🇬🇧");
        flag.backspace();
        assert_eq!(flag.text(), "x");
    }

    #[test]
    fn backspace_on_an_empty_buffer_does_nothing() {
        let mut input = TextInput::default();
        input.backspace();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn delete_removes_the_grapheme_under_the_caret() {
        let mut input = TextInput::new("abc");
        input.move_home();
        input.delete();
        assert_eq!(input.text(), "bc");
    }

    #[test]
    fn the_caret_stops_at_both_ends() {
        let mut input = TextInput::new("ab");
        input.move_home();
        input.move_left();
        assert_eq!(input.caret_col(), 0);
        input.move_end();
        input.move_right();
        assert_eq!(input.caret_col(), 2);
    }

    #[test]
    fn caret_column_accounts_for_wide_characters() {
        let mut input = TextInput::new("世界");
        input.move_home();
        input.move_right();
        assert_eq!(input.caret_col(), 2);
    }

    #[test]
    fn ctrl_w_deletes_the_word_before_the_caret() {
        let mut input = TextInput::new("buy some milk");
        input.delete_word_before();
        assert_eq!(input.text(), "buy some ");
    }

    #[test]
    fn ctrl_u_deletes_back_to_the_start() {
        let mut input = TextInput::new("buy some milk");
        input.move_word_left();
        input.delete_to_start();
        assert_eq!(input.text(), "milk");
    }

    #[test]
    fn pasted_control_characters_become_spaces() {
        let mut input = TextInput::default();
        input.insert_str("two\tlines\nhere");
        assert_eq!(input.text(), "two lines here");
    }

    #[test]
    fn a_caret_position_is_always_a_grapheme_boundary() {
        let mut input = TextInput::new("é👍🏽世x");
        input.move_home();
        for _ in 0..6 {
            input.move_right();
            assert!(input.text().is_char_boundary(input.caret));
        }
    }
}
