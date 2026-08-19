pub mod help;
pub mod layout;
pub mod main_list;
pub mod sidebar;
pub mod statusline;
pub mod theme;

use ratatui::Frame;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use doer_core::MainMode;
use doer_core::app::{AppState, MainState, Pane, SidebarCursor, SidebarState, ToastLevel};
use doer_core::display::ViewId;
use doer_core::layout::Layout;

use crate::ui::layout::frames;
use crate::ui::main_list::ListView;
use crate::ui::sidebar::SidebarView;
use crate::ui::statusline::StatusView;
use crate::ui::theme::Theme;

/// Everything one frame draws, resolved from the app state before rendering starts.
///
/// Keeping the resolution in one place is what lets every render function below be a
/// plain function of borrowed data: nothing here reaches back into the app, and no
/// widget holds state between frames.
pub struct Screen<'a> {
    pub list: ListView<'a>,
    pub status: StatusView<'a>,
    /// `None` when the sidebar is closed. A sidebar that does not fit is hidden by
    /// the layout instead, without disturbing the app's own flag.
    pub sidebar: Option<SidebarView<'a>>,
    pub show_help: bool,
}

const MIN_WIDTH: u16 = 10;
const MIN_HEIGHT: u16 = 3;

impl<'a> Screen<'a> {
    /// Reads the app once, so the render functions below never reach back into it.
    #[must_use]
    pub fn resolve(app: &'a AppState, layout: &'a Layout, theme: &Theme) -> Self {
        let (done, total) = app.counts();
        let main_focused = app.pane == Pane::Main;

        let list = ListView {
            layout,
            scroll: app.scroll,
            cursor: app.cursor.as_ref(),
            selection: match &app.main {
                MainState::Visual { .. } => app
                    .selection()
                    .map(|range| range.start..=range.end.saturating_sub(1)),
                _ => None,
            },
            editing: match &app.main {
                MainState::Insert(editing) => Some(editing.id()),
                _ => None,
            },
            main_focused,
        };

        let status = StatusView {
            mode: app.main.mode(),
            search: match &app.main {
                MainState::Search(input) => Some((input.text(), input.caret_col())),
                MainState::SearchNav { query } => Some((query.as_str(), query.chars().count())),
                _ => None,
            },
            // A query being typed must stay on screen, so only a settled view lets a
            // toast borrow the row.
            toast: app
                .toast
                .as_ref()
                .filter(|_| app.main.mode() != MainMode::Search)
                .map(|toast| {
                    let style = match toast.level {
                        ToastLevel::Info => theme.toast_info,
                        ToastLevel::Warning => theme.toast_warning,
                        ToastLevel::Error => theme.toast_error,
                    };
                    (toast.text.as_str(), style)
                }),
            done,
            total,
            show_help: app.help,
        };

        let sidebar = app.sidebar_open.then(|| SidebarView {
            projects: app.ws.projects(),
            cursor: match &app.sidebar_cursor {
                SidebarCursor::All => None,
                SidebarCursor::Project(id) => Some(id),
            },
            current: match &app.view {
                ViewId::All => None,
                ViewId::Project(id) => Some(id),
            },
            focused: !main_focused,
            editing: match &app.sidebar {
                SidebarState::Insert { target, input } => {
                    Some((target, input.text(), input.caret_col()))
                }
                _ => None,
            },
            confirm_delete: match &app.sidebar {
                SidebarState::ConfirmDelete(id) => Some(id),
                _ => None,
            },
        });

        Self {
            list,
            status,
            sidebar,
            show_help: app.help,
        }
    }
}

pub fn draw(frame: &mut Frame, app: &AppState, now: i64, theme: &Theme) {
    let layout = app.layout(now);
    let screen = Screen::resolve(app, &layout, theme);
    render(frame, &screen, theme);
}

pub fn render(frame: &mut Frame, screen: &Screen<'_>, theme: &Theme) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(Paragraph::new(Line::styled("too small", theme.dim)), area);
        return;
    }

    let frames = frames(area, screen.sidebar.is_some());

    if let (Some(view), Some(area)) = (&screen.sidebar, frames.sidebar) {
        sidebar::render(frame, area, view, theme);
    }
    if let Some(area) = frames.border {
        let focused = screen.sidebar.as_ref().is_some_and(|s| s.focused);
        let style = if focused {
            theme.border_focused
        } else {
            theme.border_idle
        };
        frame.render_widget(
            Block::new().borders(Borders::RIGHT).border_style(style),
            area,
        );
    }

    main_list::render(frame, frames.content, &screen.list, theme);
    statusline::render_search(frame, frames.search, &screen.status, theme);
    statusline::render_modebar(frame, frames.modebar, &screen.status, theme);

    if screen.show_help {
        help::render(frame, area, theme);
    }
}
