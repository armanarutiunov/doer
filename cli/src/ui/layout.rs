//! Where each region of the screen goes. Purely geometric — nothing here reads state
//! beyond the sidebar flag.

use doer_core::layout::{BORDER_WIDTH, Geometry, PAD_Y_TOP, SIDEBAR_WIDTH};
use ratatui::layout::{Constraint, Layout, Rect};

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

#[must_use]
pub fn frames(area: Rect, sidebar_open: bool) -> Frames {
    // Both the geometry the scroll maths uses and the rectangles drawn here have to
    // agree about the sidebar and the column width, so both come from `Geometry`. Two
    // copies of this arithmetic is exactly the class of bug this port removes.
    let geo = Geometry::new(area.width, area.height, sidebar_open);
    let sidebar_fits = geo.sidebar_visible() || !sidebar_open;
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

    // Every row is confined to the centred column, which is what centres the mode bar
    // under the list. Rows are built to fit it: the date columns shrink or drop
    // (`DateColumns::fit`) and the mode bar shortens its counter, so nothing needs to
    // overflow into the padding to stay readable.
    let column = centred_column(body, geo);
    let [_, content, _, search, _, modebar, _] = Layout::vertical([
        Constraint::Length(PAD_Y_TOP),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(column);

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
fn centred_column(body: Rect, geo: Geometry) -> Rect {
    let width = geo.content_width().min(body.width);
    let [_, column, _] = Layout::horizontal([
        Constraint::Length((body.width - width) / 2),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .areas(body);
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    fn content_width(available: u16) -> u16 {
        Geometry::new(available, 24, false).content_width()
    }

    fn content_left_pad(available: u16) -> u16 {
        (available - content_width(available)) / 2
    }

    #[test]
    fn the_odd_remainder_column_stays_on_the_right() {
        // 81 wide: content 48, so 33 spare — 16 left, 17 right, as the Elixir div/2 did.
        let f = frames(screen(81, 24), false);
        assert_eq!(f.content.x, 16);
        assert_eq!(f.content.width, 48);
    }

    /// The scroll maths derives the viewport height from `PAD_Y_TOP + BOTTOM_RESERVED`
    /// while the rows here are laid out one constraint at a time. If those two ever
    /// disagree, the list scrolls against a height it is not drawn at.
    #[test]
    fn the_drawn_viewport_is_the_height_the_scroll_maths_assumes() {
        for height in 8..60u16 {
            for open in [false, true] {
                let f = frames(screen(100, height), open);
                let geo = Geometry::new(100, height, open);
                assert_eq!(
                    usize::from(f.content.height),
                    geo.viewport_height(),
                    "height {height}, sidebar {open}"
                );
            }
        }
    }

    #[test]
    fn the_sidebar_takes_its_width_plus_a_border() {
        let f = frames(screen(100, 24), true);
        assert_eq!(f.sidebar.map(|r| r.width), Some(35));
        assert_eq!(f.border.map(|r| (r.x, r.width)), Some((35, 1)));
        assert_eq!(f.content.x, 36 + content_left_pad(100 - 36));
        assert_eq!(f.content.width, content_width(100 - 36));
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
        assert_eq!(f.content.width, 24);
        assert_eq!(f.content.height, 4);
    }

    #[test]
    fn the_bottom_rows_share_the_lists_column_so_the_mode_bar_centres_under_it() {
        let f = frames(screen(100, 30), true);
        assert_eq!(
            (f.modebar.x, f.modebar.width),
            (f.content.x, f.content.width)
        );
        assert_eq!((f.search.x, f.search.width), (f.content.x, f.content.width));
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
