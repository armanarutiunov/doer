//! Named styles for every painted thing.
//!
//! The theme stores `Style` values rather than colours so that a degraded terminal can
//! swap a background fill for `REVERSED` without any render function asking which theme
//! is in play.

use std::env;

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi16,
}

impl ColorDepth {
    /// crossterm writes `Color::Rgb` as a literal `38;2;r;g;b` with no capability check
    /// and no downsampling, so a 16-colour terminal has to be detected here or the
    /// palette lands wrong.
    #[must_use]
    pub fn detect() -> Self {
        match env::var("DOER_COLOR").as_deref() {
            Ok("16") => return Self::Ansi16,
            Ok("truecolor" | "24bit") => return Self::TrueColor,
            _ => {}
        }
        if env::var_os("NO_COLOR").is_some() {
            return Self::Ansi16;
        }
        match env::var("COLORTERM").as_deref() {
            Ok("truecolor" | "24bit") => Self::TrueColor,
            _ => Self::Ansi16,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub text: Style,
    /// Section headers, date columns, counters, hints.
    pub dim: Style,
    /// The right-hand "Xd" columns, one notch brighter than `dim`.
    pub meta: Style,
    pub cursor_row: Style,
    pub cursor_row_unfocused: Style,
    pub cursor_text: Style,
    pub done_text: Style,
    pub editing: Style,
    /// The `▎` bar marking a visual-mode selection.
    pub selection_bar: Style,
    pub sidebar_item: Style,
    pub sidebar_current: Style,
    pub sidebar_hint: Style,
    pub confirm_delete: Style,
    pub border_focused: Style,
    pub border_idle: Style,
    pub help_surface: Style,
    pub help_text: Style,
    pub help_heading: Style,
    pub search: Style,
    pub toast_info: Style,
    pub toast_warning: Style,
    pub toast_error: Style,
    pub mode_normal: Style,
    pub mode_visual: Style,
    pub mode_insert: Style,
    pub mode_search: Style,
    /// True when `cursor_row` highlights by inverting rather than by filling. Reverse
    /// video does not nest — a reversed span inside a reversed row cancels back to
    /// normal — so the renderer must not invert anything else on the cursor row.
    pub cursor_row_inverts: bool,
}

impl Theme {
    #[must_use]
    pub fn for_depth(depth: ColorDepth) -> Self {
        match depth {
            ColorDepth::TrueColor => Self::purple(),
            ColorDepth::Ansi16 => Self::ansi(),
        }
    }

    /// The palette the Elixir build shipped, to the exact channel value.
    #[must_use]
    pub fn purple() -> Self {
        let dim_grey = Color::Rgb(100, 100, 100);
        Self {
            text: Style::new(),
            dim: Style::new().fg(dim_grey),
            meta: Style::new().fg(Color::Rgb(140, 140, 140)),
            cursor_row: Style::new().bg(Color::Rgb(55, 51, 84)),
            cursor_row_unfocused: Style::new().bg(Color::Rgb(50, 48, 60)),
            cursor_text: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            done_text: Style::new()
                .fg(Color::Rgb(80, 80, 80))
                .add_modifier(Modifier::CROSSED_OUT),
            editing: Style::new().fg(Color::Green),
            selection_bar: Style::new().fg(Color::Rgb(255, 122, 178)),
            sidebar_item: Style::new().fg(Color::Rgb(200, 200, 200)),
            sidebar_current: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            sidebar_hint: Style::new().fg(Color::Rgb(80, 80, 80)),
            confirm_delete: Style::new().fg(Color::Red),
            border_focused: Style::new().fg(Color::Rgb(100, 100, 110)),
            border_idle: Style::new().fg(Color::Rgb(50, 50, 55)),
            help_surface: Style::new().bg(Color::Rgb(65, 65, 72)),
            help_text: Style::new().fg(Color::White).bg(Color::Rgb(65, 65, 72)),
            help_heading: Style::new().fg(dim_grey).bg(Color::Rgb(65, 65, 72)),
            search: Style::new().fg(Color::White),
            toast_info: Style::new().fg(Color::Rgb(140, 140, 140)),
            toast_warning: Style::new().fg(Color::Yellow),
            toast_error: Style::new().fg(Color::Red),
            mode_normal: mode(Color::Blue),
            mode_visual: mode(Color::Magenta),
            mode_insert: mode(Color::Green),
            mode_search: mode(Color::Yellow),
            cursor_row_inverts: false,
        }
    }

    /// Degraded palette: the terminal's own 16 colours, so it stays legible on a light
    /// background where the purple fills would not be.
    #[must_use]
    pub fn ansi() -> Self {
        Self {
            text: Style::new(),
            dim: Style::new().fg(Color::DarkGray),
            meta: Style::new().fg(Color::DarkGray),
            cursor_row: Style::new().add_modifier(Modifier::REVERSED),
            cursor_row_unfocused: Style::new().add_modifier(Modifier::DIM | Modifier::REVERSED),
            cursor_text: Style::new().add_modifier(Modifier::BOLD),
            done_text: Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT | Modifier::DIM),
            editing: Style::new().fg(Color::Green),
            selection_bar: Style::new().fg(Color::Magenta),
            sidebar_item: Style::new(),
            sidebar_current: Style::new().add_modifier(Modifier::BOLD),
            sidebar_hint: Style::new().fg(Color::DarkGray),
            confirm_delete: Style::new().fg(Color::Red),
            border_focused: Style::new(),
            border_idle: Style::new().fg(Color::DarkGray),
            help_surface: Style::new().add_modifier(Modifier::REVERSED),
            help_text: Style::new().add_modifier(Modifier::REVERSED),
            help_heading: Style::new().add_modifier(Modifier::REVERSED | Modifier::DIM),
            search: Style::new(),
            toast_info: Style::new().fg(Color::DarkGray),
            toast_warning: Style::new().fg(Color::Yellow),
            toast_error: Style::new().fg(Color::Red),
            mode_normal: mode(Color::Blue),
            mode_visual: mode(Color::Magenta),
            mode_insert: mode(Color::Green),
            mode_search: mode(Color::Yellow),
            cursor_row_inverts: true,
        }
    }
}

fn mode(bg: Color) -> Style {
    Style::new().fg(Color::Black).bg(bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_degraded_theme_highlights_by_inverting() {
        assert!(!Theme::purple().cursor_row_inverts);
        assert!(Theme::ansi().cursor_row_inverts);
    }

    #[test]
    fn the_default_palette_keeps_the_exact_shipped_colours() {
        let theme = Theme::purple();
        assert_eq!(theme.cursor_row.bg, Some(Color::Rgb(55, 51, 84)));
        assert_eq!(theme.cursor_row_unfocused.bg, Some(Color::Rgb(50, 48, 60)));
        assert_eq!(theme.selection_bar.fg, Some(Color::Rgb(255, 122, 178)));
        assert_eq!(theme.done_text.fg, Some(Color::Rgb(80, 80, 80)));
        assert!(theme.done_text.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn nothing_in_the_degraded_theme_stacks_a_second_reverse() {
        let theme = Theme::ansi();
        for style in [
            theme.cursor_text,
            theme.done_text,
            theme.selection_bar,
            theme.editing,
        ] {
            assert!(!style.add_modifier.contains(Modifier::REVERSED));
        }
    }
}
