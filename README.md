# doer

A vim-flavoured terminal todo app, built with Rust and ratatui.

## Install

### Homebrew

```sh
brew install armanarutiunov/doer/doer
```

That taps and installs in one step, and ships a prebuilt binary rather than compiling
on your machine. Once tapped, `brew upgrade doer` is enough.

### Cargo

```sh
cargo install doer-tui
```

The crate is `doer-tui` because `doer` on crates.io belongs to an unrelated tool. The
binary it installs is still called `doer`.

### From source

Requires a Rust toolchain (1.88 or newer).

```sh
git clone https://github.com/armanarutiunov/doer.git
cd doer
cargo build --release
mv target/release/doer /usr/local/bin/
```

## Keybindings

Every binding below is generated from `core/src/input.rs`, which is the whole keymap
in one function. If a key is not listed here it does nothing.

### Global

Available whenever neither pane is mid-edit.

| Key | Action |
|-----|--------|
| `\` | Toggle sidebar |
| `Tab` | Switch focus (sidebar must be open) |
| `?` | Toggle help |
| `q` | Quit |
| `ctrl+c` | Quit, from any mode |

While the help overlay is open, only `?` and `escape` do anything — both close it.

`q` saves before it exits. If that save fails it says so and stays open; pressing `q`
again then quits anyway, losing the unsaved edits.

### Normal mode

| Key | Action |
|-----|--------|
| `j` / `k` / `↓` / `↑` | Navigate |
| `g` / `gg` | Jump to start |
| `G` | Jump to end |
| `ctrl+d` / `ctrl+u` | Half page down / up |
| `a` | Add todo below the cursor |
| `e` / `i` | Edit todo |
| `d` | Delete todo |
| `space` | Toggle done |
| `J` / `K` | Reorder todo |
| `v` | Enter visual mode |
| `/` | Search |
| `u` / `ctrl+r` | Undo / redo |
| `h` / `←` | Focus sidebar |

### Insert mode

Entered by `a`, `e` or `i`. Any character is accepted, including non-ASCII.

Pasting inserts the text as text. A todo is one line, so line breaks and tabs in a
pasted block become spaces. Outside a text field a paste is ignored entirely — pasted
text is never read as a run of keybindings.

| Key | Action |
|-----|--------|
| `enter` | Confirm |
| `escape` | Cancel — a new todo is discarded, an edited one reverts |
| `backspace` / `delete` | Delete before / at the caret |
| `←` / `→` | Move caret |
| `home` / `end` or `ctrl+a` / `ctrl+e` | Caret to start / end |
| `ctrl+w` | Delete word before the caret |
| `ctrl+u` | Delete to start of line |

### Visual mode

| Key | Action |
|-----|--------|
| `j` / `k` / `↓` / `↑` | Extend selection |
| `J` / `K` or `ctrl+j` / `ctrl+k` / `ctrl+↓` / `ctrl+↑` | Reorder selected |
| `d` | Delete selected |
| `space` | Toggle selected |
| `escape` | Exit visual mode |

### Search

`/` opens the search line and filters as you type. `enter` commits the query and moves
to search-nav, where `j` / `k` walk the matches, `/` edits the query again, and
`escape` clears the search.

Search accepts the same editing keys as insert mode.

### Sidebar

| Key | Action |
|-----|--------|
| `j` / `k` / `↓` / `↑` | Navigate — the main pane follows immediately |
| `enter` / `l` / `→` | Select project and return focus to the todos |
| `a` | Add project |
| `s` | Add subproject (top-level projects only; nesting stops at two levels) |
| `e` / `i` | Rename project |
| `d` | Delete project |
| `J` / `K` | Reorder projects within their own level |
| `u` / `ctrl+r` | Undo / redo |

Renaming uses the same editing keys as insert mode. Deleting a project also deletes its
subprojects; if any of them still holds an uncompleted todo, the row asks `Delete? y/n`
first. Deletes are undoable.

## Display

A row shows a checkbox, the todo text, and the age of the todo in whole days. Completed
todos move to their own section below the active ones, struck through, with a second
column for how long ago they were completed. In the All Todos view, todos are grouped
under a header per project.

The layout adapts to the terminal, so a narrow window shows less than a wide one:

- The date columns shrink to their natural width, then disappear, before the todo text is
  allowed to get narrower than about twelve columns. The text is the point of the row.
- The mode bar shortens the completed count and drops the `? for help` hint rather than
  letting them be cut mid-word.
- The sidebar hides itself when the main column could not otherwise keep its minimum
  width. Toggling it with `\` still works; it reappears when there is room.

Long todos wrap. A project name too long for the sidebar is truncated, but while you are
renaming it the row scrolls horizontally so the text stays visible as you type.

## Data

```
~/.doer/
  all-todos.json          # ungrouped todos
  projects/
    <id>.json             # project, with its todos embedded
  .trash/
    <timestamp>/          # deleted project files, pruned after 7 days
```

On first launch, `todos.json` is migrated to `all-todos.json`.

Writes are atomic (write to a temporary file, then rename) and touch only the file that
changed.

The format is treated as a contract rather than doer's private state: unknown fields and
entries doer cannot parse are carried through and written back unchanged. So a file with
one damaged entry stays fully editable and nothing is lost. A file doer cannot parse at
all is a different matter — it is never overwritten, and doer says so instead of saving
over it.
