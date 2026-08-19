//! Keymap parity harness.
//!
//! Every binding in the Elixir `event_mapping.ex` / `sidebar_event_mapping.ex`, plus the
//! ones the README omits, asserted as (context, key) -> Action. If a key moves, this
//! file is where it has to be argued for.

use doer_core::action::{Action, Dir, EditKey, Motion, SidebarAction};
use doer_core::input::{InputContext, KeyCode, KeyPress, Mods, map};
use doer_core::mode::{Focus, MainMode, SidebarMode};

use Action as A;
use KeyCode as C;

fn main_ctx(mode: MainMode) -> InputContext {
    InputContext {
        focus: Focus::Main(mode),
        sidebar_open: true,
        help: false,
    }
}

fn sidebar_ctx(mode: SidebarMode) -> InputContext {
    InputContext {
        focus: Focus::Sidebar(mode),
        sidebar_open: true,
        help: false,
    }
}

fn k(c: char) -> KeyPress {
    KeyPress::char(c)
}

fn p(code: KeyCode) -> KeyPress {
    KeyPress::plain(code)
}

fn ctrl(c: char) -> KeyPress {
    KeyPress::ctrl(c)
}

#[track_caller]
fn assert_map(ctx: InputContext, key: KeyPress, want: Option<&Action>) {
    assert_eq!(map(ctx, key).as_ref(), want, "key {key:?} in {ctx:?}");
}

#[track_caller]
fn expect(ctx: InputContext, table: &[(KeyPress, Action)]) {
    for (key, want) in table {
        assert_map(ctx, *key, Some(want));
    }
}

#[track_caller]
fn expect_ignored(ctx: InputContext, keys: &[KeyPress]) {
    for key in keys {
        assert_map(ctx, *key, None);
    }
}

// --- Global ---------------------------------------------------------------

#[test]
fn global_keys_work_from_either_idle_pane() {
    for ctx in [main_ctx(MainMode::Normal), sidebar_ctx(SidebarMode::Normal)] {
        expect(
            ctx,
            &[
                (k('?'), A::ToggleHelp),
                (k('q'), A::Quit),
                (k('\\'), A::ToggleSidebar),
                (p(C::Tab), A::SwitchFocus),
            ],
        );
    }
}

#[test]
fn tab_needs_an_open_sidebar() {
    let ctx = InputContext {
        sidebar_open: false,
        ..main_ctx(MainMode::Normal)
    };
    assert_map(ctx, p(C::Tab), None);
    // The other global keys do not care whether the sidebar is showing.
    expect(
        ctx,
        &[
            (k('?'), A::ToggleHelp),
            (k('q'), A::Quit),
            (k('\\'), A::ToggleSidebar),
        ],
    );
}

#[test]
fn global_keys_are_literal_text_while_a_pane_is_editing() {
    for ctx in [
        main_ctx(MainMode::Insert),
        main_ctx(MainMode::Search),
        sidebar_ctx(SidebarMode::Insert),
    ] {
        assert_map(ctx, k('q'), Some(&char_action(ctx, 'q')));
        assert_map(ctx, k('?'), Some(&char_action(ctx, '?')));
        assert_map(ctx, k('\\'), Some(&char_action(ctx, '\\')));
        assert_map(ctx, p(C::Tab), None);
    }
}

/// Load-bearing for arch: focus can only leave a pane that is idle, so no half-typed
/// editor is ever discarded by a focus change and `MainState` can be reset to Normal
/// on entering the sidebar without losing anything.
#[test]
fn focus_never_moves_out_of_a_pane_that_is_mid_edit() {
    let movers = [
        k('h'),
        p(C::Left),
        p(C::Tab),
        k('l'),
        p(C::Right),
        p(C::Enter),
    ];

    for mode in [
        MainMode::Insert,
        MainMode::Visual,
        MainMode::Search,
        MainMode::SearchNav,
    ] {
        for key in movers {
            assert_ne!(
                map(main_ctx(mode), key),
                Some(A::SwitchFocus),
                "{mode:?} {key:?}"
            );
        }
    }
    for mode in [SidebarMode::Insert, SidebarMode::ConfirmDelete] {
        for key in movers {
            let got = map(sidebar_ctx(mode), key);
            assert_ne!(got, Some(A::SwitchFocus), "{mode:?} {key:?}");
            assert_ne!(
                got,
                Some(A::Sidebar(SidebarAction::Select)),
                "{mode:?} {key:?}"
            );
        }
    }
}

fn char_action(ctx: InputContext, c: char) -> Action {
    match ctx.focus {
        Focus::Main(MainMode::Search) => A::Search(EditKey::Char(c)),
        Focus::Sidebar(_) => A::Sidebar(SidebarAction::Edit(EditKey::Char(c))),
        Focus::Main(_) => A::Edit(EditKey::Char(c)),
    }
}

#[test]
fn ctrl_c_outranks_every_mode_including_help() {
    for ctx in [
        main_ctx(MainMode::Normal),
        main_ctx(MainMode::Insert),
        main_ctx(MainMode::Visual),
        main_ctx(MainMode::Search),
        main_ctx(MainMode::SearchNav),
        sidebar_ctx(SidebarMode::Insert),
        sidebar_ctx(SidebarMode::ConfirmDelete),
        InputContext {
            help: true,
            ..main_ctx(MainMode::Normal)
        },
    ] {
        assert_map(ctx, ctrl('c'), Some(&A::ForceQuit));
    }
}

#[test]
fn help_swallows_everything_but_question_mark_and_escape() {
    let ctx = InputContext {
        help: true,
        ..main_ctx(MainMode::Normal)
    };
    expect(
        ctx,
        &[(k('?'), A::ToggleHelp), (p(C::Escape), A::ToggleHelp)],
    );
    expect_ignored(
        ctx,
        &[
            k('q'),
            k('j'),
            k('a'),
            k(' '),
            k('\\'),
            p(C::Tab),
            p(C::Enter),
            p(C::Down),
            ctrl('d'),
        ],
    );
}

// --- Main / Normal --------------------------------------------------------

#[test]
fn main_normal() {
    let ctx = main_ctx(MainMode::Normal);
    expect(
        ctx,
        &[
            (k('j'), A::Cursor(Motion::Down)),
            (p(C::Down), A::Cursor(Motion::Down)),
            (k('k'), A::Cursor(Motion::Up)),
            (p(C::Up), A::Cursor(Motion::Up)),
            (k('g'), A::Cursor(Motion::Start)),
            (k('G'), A::Cursor(Motion::End)),
            (ctrl('d'), A::Cursor(Motion::HalfDown)),
            (ctrl('u'), A::Cursor(Motion::HalfUp)),
            (k('a'), A::AddTodo),
            (k('e'), A::EditTodo),
            (k('i'), A::EditTodo),
            (k('d'), A::DeleteTodo),
            (k(' '), A::ToggleTodo),
            (k('J'), A::Move(Dir::Down)),
            (k('K'), A::Move(Dir::Up)),
            (k('v'), A::EnterVisual),
            (k('/'), A::EnterSearch),
            (k('u'), A::Undo),
            (ctrl('r'), A::Redo),
            (k('h'), A::SwitchFocus),
            (p(C::Left), A::SwitchFocus),
        ],
    );
}

#[test]
fn gg_and_g_both_jump_to_the_top() {
    let ctx = main_ctx(MainMode::Normal);
    // No pending-`g` state: a top jump is idempotent, so pressing it twice is `gg`.
    assert_map(ctx, k('g'), Some(&A::Cursor(Motion::Start)));
    assert_map(ctx, k('g'), Some(&A::Cursor(Motion::Start)));
}

#[test]
fn focus_sidebar_keys_need_an_open_sidebar() {
    let ctx = InputContext {
        sidebar_open: false,
        ..main_ctx(MainMode::Normal)
    };
    expect_ignored(ctx, &[k('h'), p(C::Left)]);
}

#[test]
fn main_normal_ignores_unbound_keys() {
    let ctx = main_ctx(MainMode::Normal);
    expect_ignored(
        ctx,
        &[
            k('l'),
            p(C::Right),
            p(C::Enter),
            p(C::Escape),
            k('y'),
            k('p'),
            k('n'),
            k('o'),
            k('x'),
            k('1'),
            ctrl('j'),
            ctrl('w'),
        ],
    );
}

// --- Main / Insert --------------------------------------------------------

#[test]
fn main_insert() {
    let ctx = main_ctx(MainMode::Insert);
    expect(
        ctx,
        &[
            (p(C::Enter), A::ConfirmEdit),
            (p(C::Escape), A::CancelEdit),
            (p(C::Backspace), A::Edit(EditKey::Backspace)),
            (p(C::Delete), A::Edit(EditKey::Delete)),
            (p(C::Left), A::Edit(EditKey::Left)),
            (p(C::Right), A::Edit(EditKey::Right)),
            (p(C::Home), A::Edit(EditKey::Home)),
            (p(C::End), A::Edit(EditKey::End)),
            (ctrl('a'), A::Edit(EditKey::Home)),
            (ctrl('e'), A::Edit(EditKey::End)),
            (ctrl('w'), A::Edit(EditKey::DeleteWordBefore)),
            (ctrl('u'), A::Edit(EditKey::DeleteToStart)),
        ],
    );
    expect_ignored(ctx, &[p(C::Tab), p(C::Up), p(C::Down), ctrl('z')]);
}

#[test]
fn insert_accepts_any_character() {
    let ctx = main_ctx(MainMode::Insert);
    // The Elixir `byte_size(key) == 1` guard dropped every one of these.
    for c in ['a', 'Z', ' ', 'ä', 'ż', '日', 'é', '—', '🙂'] {
        assert_map(ctx, k(c), Some(&A::Edit(EditKey::Char(c))));
    }
}

// --- Main / Visual --------------------------------------------------------

#[test]
fn main_visual() {
    let ctx = main_ctx(MainMode::Visual);
    expect(
        ctx,
        &[
            (k('j'), A::ExtendVisual(Motion::Down)),
            (p(C::Down), A::ExtendVisual(Motion::Down)),
            (k('k'), A::ExtendVisual(Motion::Up)),
            (p(C::Up), A::ExtendVisual(Motion::Up)),
            (k('J'), A::Move(Dir::Down)),
            (k('K'), A::Move(Dir::Up)),
            // Undocumented in the README, present in event_mapping.ex.
            (ctrl('j'), A::Move(Dir::Down)),
            (ctrl('k'), A::Move(Dir::Up)),
            (
                KeyPress {
                    code: C::Down,
                    mods: Mods::CTRL,
                },
                A::Move(Dir::Down),
            ),
            (
                KeyPress {
                    code: C::Up,
                    mods: Mods::CTRL,
                },
                A::Move(Dir::Up),
            ),
            (k('d'), A::DeleteSelected),
            (k(' '), A::ToggleSelected),
            (p(C::Escape), A::ExitVisual),
        ],
    );
    expect_ignored(ctx, &[k('a'), k('e'), k('i'), k('/'), p(C::Enter), k('v')]);
}

// --- Main / Search and SearchNav -----------------------------------------

#[test]
fn main_search() {
    let ctx = main_ctx(MainMode::Search);
    expect(
        ctx,
        &[
            (k('x'), A::Search(EditKey::Char('x'))),
            (k('ż'), A::Search(EditKey::Char('ż'))),
            (p(C::Backspace), A::Search(EditKey::Backspace)),
            (p(C::Left), A::Search(EditKey::Left)),
            (ctrl('w'), A::Search(EditKey::DeleteWordBefore)),
            (ctrl('u'), A::Search(EditKey::DeleteToStart)),
            (p(C::Enter), A::ConfirmSearch),
            (p(C::Escape), A::CancelSearch),
        ],
    );
}

#[test]
fn main_search_nav() {
    let ctx = main_ctx(MainMode::SearchNav);
    expect(
        ctx,
        &[
            (k('j'), A::Cursor(Motion::Down)),
            (p(C::Down), A::Cursor(Motion::Down)),
            (k('k'), A::Cursor(Motion::Up)),
            (p(C::Up), A::Cursor(Motion::Up)),
            (k('/'), A::EnterSearch),
            (p(C::Escape), A::CancelSearch),
        ],
    );
    // Everything else is inert, exactly as today. `Enter` gaining a meaning here is a
    // follow-up commit, not part of the port.
    expect_ignored(
        ctx,
        &[
            p(C::Enter),
            k('a'),
            k('d'),
            k(' '),
            k('v'),
            k('J'),
            k('G'),
            ctrl('d'),
        ],
    );
}

// --- Sidebar --------------------------------------------------------------

#[test]
fn sidebar_normal() {
    let ctx = sidebar_ctx(SidebarMode::Normal);
    expect(
        ctx,
        &[
            (k('j'), A::Sidebar(SidebarAction::Down)),
            (p(C::Down), A::Sidebar(SidebarAction::Down)),
            (k('k'), A::Sidebar(SidebarAction::Up)),
            (p(C::Up), A::Sidebar(SidebarAction::Up)),
            (k('a'), A::Sidebar(SidebarAction::AddProject)),
            (k('s'), A::Sidebar(SidebarAction::AddSubproject)),
            (k('e'), A::Sidebar(SidebarAction::Rename)),
            (k('i'), A::Sidebar(SidebarAction::Rename)),
            (k('d'), A::Sidebar(SidebarAction::Delete)),
            (k('J'), A::Sidebar(SidebarAction::Move(Dir::Down))),
            (k('K'), A::Sidebar(SidebarAction::Move(Dir::Up))),
            (p(C::Enter), A::Sidebar(SidebarAction::Select)),
            (k('l'), A::Sidebar(SidebarAction::Select)),
            // Undocumented in the README, present in sidebar_event_mapping.ex.
            (p(C::Right), A::Sidebar(SidebarAction::Select)),
            (k('u'), A::Undo),
            (ctrl('r'), A::Redo),
        ],
    );
    expect_ignored(
        ctx,
        &[k('h'), p(C::Left), k('v'), k('/'), k('G'), ctrl('d')],
    );
}

#[test]
fn sidebar_dispatch_wins_over_the_main_map() {
    let ctx = sidebar_ctx(SidebarMode::Normal);
    // `a`, `d`, `e`/`i` and `J`/`K` all exist in both panes; focus decides.
    assert_map(ctx, k('a'), Some(&A::Sidebar(SidebarAction::AddProject)));
    assert_map(ctx, k('d'), Some(&A::Sidebar(SidebarAction::Delete)));
    assert_map(ctx, k(' '), None);
}

#[test]
fn sidebar_insert() {
    let ctx = sidebar_ctx(SidebarMode::Insert);
    expect(
        ctx,
        &[
            (p(C::Enter), A::Sidebar(SidebarAction::ConfirmEdit)),
            (p(C::Escape), A::Sidebar(SidebarAction::CancelEdit)),
            (
                p(C::Backspace),
                A::Sidebar(SidebarAction::Edit(EditKey::Backspace)),
            ),
            (k('ä'), A::Sidebar(SidebarAction::Edit(EditKey::Char('ä')))),
            (p(C::Home), A::Sidebar(SidebarAction::Edit(EditKey::Home))),
            (
                ctrl('w'),
                A::Sidebar(SidebarAction::Edit(EditKey::DeleteWordBefore)),
            ),
            (
                ctrl('u'),
                A::Sidebar(SidebarAction::Edit(EditKey::DeleteToStart)),
            ),
        ],
    );
    // `u` is text here, not undo.
    assert_map(
        ctx,
        k('u'),
        Some(&A::Sidebar(SidebarAction::Edit(EditKey::Char('u')))),
    );
    expect_ignored(ctx, &[p(C::Tab), p(C::Up)]);
}

#[test]
fn sidebar_confirm_delete_only_answers_yes_or_no() {
    let ctx = sidebar_ctx(SidebarMode::ConfirmDelete);
    expect(
        ctx,
        &[
            (k('y'), A::Sidebar(SidebarAction::ConfirmDelete)),
            (k('n'), A::Sidebar(SidebarAction::CancelDelete)),
            (p(C::Escape), A::Sidebar(SidebarAction::CancelDelete)),
        ],
    );
    expect_ignored(
        ctx,
        &[
            k('Y'),
            k('d'),
            k('j'),
            k('q'),
            p(C::Enter),
            p(C::Backspace),
            ctrl('r'),
        ],
    );
}

// --- Cursor resolution ----------------------------------------------------

#[test]
fn only_the_triage_keys_keep_the_cursor_at_its_display_index() {
    use doer_core::action::ResolveCursor::{ById, ByIndex};

    for a in [
        A::ToggleTodo,
        A::DeleteTodo,
        A::ToggleSelected,
        A::DeleteSelected,
    ] {
        assert_eq!(a.resolve_cursor(), ByIndex, "{a:?}");
    }
    for a in [
        A::Move(Dir::Down),
        A::Move(Dir::Up),
        A::Undo,
        A::Redo,
        A::ConfirmEdit,
        A::AddTodo,
    ] {
        assert_eq!(a.resolve_cursor(), ById, "{a:?}");
    }
}
