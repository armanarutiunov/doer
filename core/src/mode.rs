/// Which pane has the keyboard, and what that pane is doing.
///
/// The mode lives *inside* the focus variant because the two are not independent:
/// every sidebar edit is entered from the sidebar and nothing moves focus while one
/// is open, so "main is mid-insert while the sidebar is focused" was only ever
/// reachable as a bug. Making it unrepresentable is why the keymap needs no
/// cross-pane guards beyond `sidebar_open`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Main(MainMode),
    Sidebar(SidebarMode),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainMode {
    #[default]
    Normal,
    Insert,
    Visual,
    Search,
    SearchNav,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarMode {
    #[default]
    Normal,
    Insert,
    ConfirmDelete,
}

impl Default for Focus {
    fn default() -> Self {
        Self::Main(MainMode::Normal)
    }
}

impl Focus {
    /// True when neither pane is mid-edit, which is the guard the global keys
    /// (`?`, `q`, `\`, Tab) require — in the Elixir original, both `mode` and
    /// `sidebar_mode` had to be `:normal`.
    #[must_use]
    pub fn is_idle(self) -> bool {
        matches!(
            self,
            Self::Main(MainMode::Normal) | Self::Sidebar(SidebarMode::Normal)
        )
    }

    #[must_use]
    pub fn main_mode(self) -> Option<MainMode> {
        match self {
            Self::Main(m) => Some(m),
            Self::Sidebar(_) => None,
        }
    }

    #[must_use]
    pub fn sidebar_mode(self) -> Option<SidebarMode> {
        match self {
            Self::Sidebar(m) => Some(m),
            Self::Main(_) => None,
        }
    }
}
