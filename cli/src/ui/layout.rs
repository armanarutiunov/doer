//! Where each region of the screen goes. Purely geometric — nothing here reads state
//! beyond the sidebar flag.

use ratatui::layout::{Constraint, Layout, Rect};

pub const SIDEBAR_WIDTH: u16 = 35;
pub const BORDER_WIDTH: u16 = 1;
const CONTENT_PERCENT: u32 = 60;
const CONTENT_MIN_WIDTH: u16 = 20;
const PAD_TOP: u16 = 1;
/// blank, search line, blank, mode bar, blank

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frames {
    pub sidebar: Option<Rect>,
    pub border: Option<Rect>,
    /// The scrolling list viewport. Its height is the `vh` the scroll maths needs.
    pub content: Rect,
    pub search: Rect,
    pub modebar: Rect,
    /// False when the sidebar was hidden because it would not fit, so the caller can
    /// keep its own `sidebar_open` flag untouched and restore it on a widening.
    pub sidebar_fits: bool,
}

/// Smallest width that still leaves the content column its minimum beside a sidebar.
const SIDEBAR_MIN_TOTAL: u16 = SIDEBAR_WIDTH + BORDER_WIDTH + CONTENT_MIN_WIDTH;

#[must_use]
pub fn frames(area: Rect, sidebar_open: bool) -> Frames {
    let sidebar_fits = area.width >= SIDEBAR_MIN_TOTAL;
    let (sidebar, border, body) = if sidebar_open && sidebar_fits {
        let [sidebar, border, body] = Layout::horizontal([
            Constraint::Length(SIDEBAR_WIDTH),
            Constraint::Length(BORDER_WIDTH),
            Constraint::Min(0),
        ])
        .areas(area);
        (Some(sidebar), Some(border), body)
    } else {
        (None, None, area)
    };

    let column = centred_column(body);
    // Rows start at the content column but may run past its right edge, as they did
    // in the Elixir build: an over-long section header or mode bar overflows into the
    // padding rather than being clipped. Row content is padded to the content width,
    // so nothing else reaches into it.
    let body_rows = Rect {
        width: body.right().saturating_sub(column.x),
        ..column
    };
    let [_, content, _, search, _, modebar, _] = Layout::vertical([
        Constraint::Length(PAD_TOP),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(body_rows);

    Frames {
        sidebar,
        border,
        content,
        search,
        modebar,
        sidebar_fits,
    }
}

/// `Flex::Center` rounds the odd remainder column to the left; the Elixir build floored
/// it to the right. That one column is visible, so the padding is explicit instead.
fn centred_column(body: Rect) -> Rect {
    let width = content_width(body.width);
    let [_, column, _] = Layout::horizontal([
        Constraint::Length((body.width - width) / 2),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .areas(body);
    column
}

/// Integer form of the original `trunc(available * 0.6)`; the two agree for every width
/// a terminal can have.
#[must_use]
pub fn content_width(available: u16) -> u16 {
    let scaled = u16::try_from(u32::from(available) * CONTENT_PERCENT / 100).unwrap_or(u16::MAX);
    scaled.max(CONTENT_MIN_WIDTH).min(available)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    fn content_left_pad(available: u16) -> u16 {
        (available - content_width(available)) / 2
    }

    #[test]
    fn the_odd_remainder_column_stays_on_the_right() {
        // 81 wide: content 48, so 33 spare — 16 left, 17 right, as the Elixir div/2 did.
        let f = frames(screen(81, 24), false);
        assert_eq!(f.content.x, 16);
        assert_eq!(content_width(81), 48);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn content_width_matches_the_original_float_maths() {
        for available in 1..=1000u16 {
            let expected = ((f64::from(available) * 0.6) as u16).max(20).min(available);
            assert_eq!(content_width(available), expected, "width {available}");
        }
    }

    #[test]
    fn the_sidebar_takes_its_width_plus_a_border() {
        let f = frames(screen(100, 24), true);
        assert_eq!(f.sidebar.map(|r| r.width), Some(35));
        assert_eq!(f.border.map(|r| (r.x, r.width)), Some((35, 1)));
        // Rows may overflow the content column, so the frame runs to the right edge.
        assert_eq!(f.content.x, 36 + content_left_pad(100 - 36));
        assert_eq!(f.content.width, 100 - f.content.x);
    }

    #[test]
    fn a_narrow_screen_drops_the_sidebar_without_changing_the_users_setting() {
        let f = frames(screen(55, 24), true);
        assert!(f.sidebar.is_none());
        assert!(!f.sidebar_fits);

        let f = frames(screen(56, 24), true);
        assert!(f.sidebar.is_some());
        assert!(f.sidebar_fits);
    }

    #[test]
    fn a_tiny_screen_still_yields_a_usable_content_row() {
        let f = frames(screen(40, 10), true);
        assert!(f.sidebar.is_none());
        assert_eq!(content_width(40), 24);
        assert_eq!(f.content.height, 4);
    }

    #[test]
    fn the_reserved_rows_collapse_before_the_list_does() {
        let f = frames(screen(80, 3), false);
        assert_eq!(f.content.height, 1);
        assert_eq!(f.modebar.height, 0);
    }

    #[test]
    fn a_degenerate_area_produces_no_panic_and_no_negative_widths() {
        for (w, h) in [(0, 0), (1, 1), (0, 24), (80, 0), (3, 2)] {
            let f = frames(screen(w, h), true);
            assert!(f.content.width <= w);
            assert!(f.content.height <= h);
        }
    }
}
