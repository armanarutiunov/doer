use doer_core::layout::{SCROLL_MARGIN, adjust_scroll, clamp_scroll};
use doer_core::text::{self, TextInput};
use proptest::prelude::*;

proptest! {
    #[test]
    fn scrolling_always_lands_inside_the_list(
        offset in 0usize..500,
        start in 0usize..500,
        len in 1usize..40,
        rows in 1usize..500,
        viewport in 1usize..80,
    ) {
        let span = start..start + len;
        let result = adjust_scroll(offset, &span, rows, viewport);
        prop_assert!(result <= rows.saturating_sub(viewport));
    }

    #[test]
    fn a_cursor_that_fits_is_always_brought_fully_into_view(
        offset in 0usize..200,
        start in 0usize..150,
        len in 1usize..4,
        viewport in 20usize..60,
    ) {
        let rows = 300;
        let span = start..start + len;
        let margin = SCROLL_MARGIN.min(viewport / 2);
        prop_assume!(span.len() <= viewport.saturating_sub(margin * 2).max(1));
        prop_assume!(span.end + margin <= rows);

        let result = adjust_scroll(offset, &span, rows, viewport);
        prop_assert!(span.start >= result, "cursor above the viewport");
        prop_assert!(span.end <= result + viewport, "cursor below the viewport");
    }

    #[test]
    fn clamping_is_idempotent(offset in 0usize..500, rows in 0usize..500, viewport in 1usize..80) {
        let once = clamp_scroll(offset, rows, viewport);
        prop_assert_eq!(once, clamp_scroll(once, rows, viewport));
    }

    #[test]
    fn wrapped_lines_never_exceed_the_width(text in ".{0,200}", width in 2usize..40) {
        for line in text::wrap(&text, width) {
            prop_assert!(text::width(&line) <= width, "line {:?} exceeds {}", line, width);
        }
    }

    #[test]
    fn wrapping_never_loses_a_character(text in "[^ ]{0,80}", width in 2usize..20) {
        let joined: String = text::wrap(&text, width).join("");
        prop_assert_eq!(joined, text);
    }

    #[test]
    fn wrapping_always_produces_at_least_one_line(text in ".{0,100}", width in 0usize..30) {
        prop_assert!(!text::wrap(&text, width).is_empty());
    }

    #[test]
    fn truncation_respects_its_budget(text in ".{0,100}", width in 0usize..30) {
        prop_assert!(text::width(&text::truncate(&text, width)) <= width.max(1));
    }

    #[test]
    fn editing_leaves_the_caret_on_a_grapheme_boundary(ops in prop::collection::vec(0u8..8, 0..40)) {
        let mut input = TextInput::new("héllo 世界 👍🏽");
        for op in ops {
            match op {
                0 => input.insert_char('x'),
                1 => input.insert_char('世'),
                2 => input.backspace(),
                3 => input.delete(),
                4 => input.move_left(),
                5 => input.move_right(),
                6 => input.move_word_left(),
                _ => input.move_word_right(),
            }
            prop_assert!(input.text().is_char_boundary(input.caret_byte()));
        }
    }

    #[test]
    fn typing_then_deleting_a_character_is_the_identity(text in "[a-z ]{0,30}") {
        let mut input = TextInput::new(text.clone());
        input.insert_char('q');
        input.backspace();
        prop_assert_eq!(input.text(), text.as_str());
    }
}
