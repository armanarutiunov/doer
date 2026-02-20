# Doer - Terminal Todo App

## Context
Build a vim-inspired terminal todo app using Elixir + TermUI (Elm Architecture). Todos persist to `~/.doer/todos.json`. Three modes: normal, visual, insert.

## State Shape
```elixir
%{
  mode: :normal | :visual | :insert | :search | :search_nav,
  todos: [%Todo{id, text, done, created_at, completed_at}],
  cursor: integer,          # index into combined list (todos ++ completed)
  visual_anchor: integer,   # where visual selection started
  editing_text: string,     # buffer for insert mode
  editing_id: id | nil,     # which todo is being edited (nil = new todo via 'a')
  editing_original: string,  # original text before edit (for revert on esc)
  search_text: string,      # current search query
  search_matches: [id],     # ids of matching todos (filtered view)
  show_help: boolean,
  terminal_height: integer  # for half-page jumps
}
```

Todos split into two derived lists for display: active (not done) on top, completed (done) below.

## Files to Create/Modify

### `lib/doer/todo.ex` - Todo struct + helpers
- `%Todo{id, text, done, created_at, completed_at}`
- Helpers: `new/1`, `toggle/1`, `age_label/1`, `completed_label/1`

### `lib/doer/store.ex` - File persistence
- Read/write `~/.doer/todos.json` using Jason
- `load/0`, `save/1`

### `lib/doer/home.ex` - Main TermUI Elm component (rewrite existing)

#### Modes
- **Normal**: navigate with arrows/hjkl, `a` to add, `e`/`i` to edit, `d` to delete, `space` to toggle, `v` to enter visual, `?` for help, `q` to quit, `ctrl+d`/`ctrl+u` half-page jump
- **Visual**: enter with `v`. Arrow/hjkl extends selection (without ctrl). `ctrl+j`/`ctrl+k` or `ctrl+arrows` move selected todos. `escape` back to normal. `d` deletes selected. `space` toggles selected
- **Insert**: enter with `a` (new todo, auto-enters insert mode) or `e`/`i` (edit existing). Type text. `enter` confirms. `escape` cancels:
  - If adding new (`a`): empty text on escape → remove the todo entirely
  - If editing existing (`e`/`i`): empty text on escape → revert to original text (preserve todo)
- **Search** (`/`): shows search input at bottom of screen. As user types, list filters live to matching todos (case-insensitive substring). `enter` confirms and enters search_nav mode (navigate filtered list with arrows/hjkl). `escape` clears search and shows all todos again. `/` while in search_nav re-enters search mode with existing text for continued editing

#### Layout (vertical stack)
```
 ◯ Buy groceries                          1d
 ◯ Write tests                            0d
 ◯ Fix bug                                2d
─── completed ───
 ◉ ̶D̶e̶p̶l̶o̶y̶ ̶a̶p̶p̶                              3d  1d

                              -- NORMAL --

Search mode:
 ◯ Buy groceries                          1d
─── completed ───
/gro█
```

- Active todos: `◯` (U+25EF) hollow circle, cursor line highlighted
- Completed: `◉` (U+25C9) fisheye, strikethrough + dim/grey text, created + completed columns
- Visual selection: pink left indicator bar `▎` on selected lines
- Mode indicator: bottom-left, dark text on colored bg (light blue=normal, pink=visual, green=insert)
- Help popup: `?` shows overlay with all keybinds via `Dialog` widget

#### Cursor behavior
- Cursor spans the combined list (active todos, then completed)
- On tick: cursor stays at same position, item moves to completed section
- On untick: item moves back to active section, cursor stays

#### Half-page jumps
- `ctrl+d`: jump cursor down by terminal_height/2
- `ctrl+u`: jump cursor up by terminal_height/2
- Track terminal height via `Event.Resize`

### `lib/doer.ex` - Entry point (minor tweak)

### `mix.exs` - Add `jason` dependency for JSON persistence

## Key Implementation Details

- Use `TermUI.Elm` callbacks: `init`, `event_to_msg`, `update`, `view`
- `event_to_msg` pattern-matches on mode + key combo to produce messages
- `view` renders: todo list section, separator, completed section, mode bar, optional help overlay
- Persistence: save on every mutation (add/edit/delete/toggle/reorder)
- `stack(:horizontal, ...)` for each row: indicator | checkbox | text | age columns
- Use `Style.new(attrs: [:strikethrough, :dim])` for completed todos
- Help popup uses the overlay pattern with `%{type: :overlay, ...}`
- `a`: insert new todo below cursor, auto-enter insert mode. Escape with empty text removes it. Enter with empty text also removes it.
- `e`/`i`: edit existing todo text. Escape reverts to original. Enter with empty text also reverts.
- `/`: search mode. Bottom bar shows `/` + search text. Filters todos live. Enter → navigate filtered results. Escape → clear and restore full list. `/` in search_nav → resume editing search text.

## Dependencies to Add
- `{:jason, "~> 1.4"}` for JSON encoding/decoding

## Implementation Steps

### Step 1: Add jason dependency
- Add `{:jason, "~> 1.4"}` to `mix.exs` deps
- Run `mix deps.get`

### Step 2: Create Todo struct (`lib/doer/todo.ex`)
- Define `%Todo{}` struct with id, text, done, created_at, completed_at
- `new/1` — create todo with generated id and timestamp
- `toggle/1` — flip done, set/clear completed_at
- `age_label/1` — "0d", "1d", etc from created_at to now
- `completed_label/1` — same but from completed_at
- Jason encoder derivation for persistence

### Step 3: Create Store module (`lib/doer/store.ex`)
- `load/0` — read `~/.doer/todos.json`, decode JSON into `[%Todo{}]`, create dir if missing
- `save/1` — encode `[%Todo{}]` to JSON, write to file

### Step 4: Build Home component — normal mode basics (`lib/doer/home.ex`)
- `init/1` — load todos from store, set initial state
- `view/1` — render active todos list, separator, completed list, mode indicator bar
- Row rendering: `stack(:horizontal, [indicator, checkbox, text, age_cols])`
- Cursor highlight styling on current row
- Navigation: arrows + hjkl move cursor, clamp to bounds
- `q` to quit (save on quit)
- `Event.Resize` handler for terminal_height

### Step 5: Add todo CRUD in normal mode
- `a` — insert new empty todo below cursor, switch to insert mode
- `e`/`i` — switch to insert mode on current todo, store original text
- `d` — delete todo at cursor
- `space` — toggle done/undone, cursor stays in place
- Save after every mutation

### Step 6: Implement insert mode
- Capture typed characters into editing_text buffer
- Render text input inline (replace todo text with editable text + cursor)
- `enter` — confirm: if text empty and new todo, remove it; if text empty and editing, revert
- `escape` — cancel: if new todo, remove it; if editing, revert to original
- `backspace` — delete last char
- Return to normal mode after confirm/cancel

### Step 7: Implement visual mode
- `v` in normal mode — enter visual, set anchor at cursor
- Arrow/hjkl without ctrl — extend selection (cursor moves, selection = anchor..cursor range)
- `ctrl+j`/`ctrl+k` or `ctrl+arrows` — move all selected todos up/down in the list
- `d` — delete all selected todos
- `space` — toggle all selected todos
- `escape` — back to normal mode
- Pink `▎` indicator on left of selected rows

### Step 8: Half-page jumps
- `ctrl+d` — cursor += terminal_height / 2, clamp
- `ctrl+u` — cursor -= terminal_height / 2, clamp

### Step 9: Search mode
- `/` in normal mode — enter search mode, show search bar at bottom (`/` + text + cursor)
- Typed chars update search_text, filter todos live (case-insensitive substring)
- `enter` — enter search_nav mode (navigate filtered results with arrows/hjkl)
- `escape` — clear search, restore full list, back to normal
- `/` in search_nav — re-enter search mode with existing text

### Step 10: Help popup
- `?` in normal mode — toggle show_help
- Render overlay with all keybinds listed
- `escape` or `?` to close

### Step 11: Polish
- Mode indicator bar: bottom-left, dark text, colored bg (blue=normal, pink=visual, green=insert)
- Completed todos: strikethrough + dim style
- Ensure edge cases: empty list, cursor bounds, visual mode on single item

## Verification
1. `mix deps.get` to fetch jason
2. `mix run run.exs` to launch
3. Test: `a` creates todo below cursor, type text, `enter` confirms
4. Test: navigate with `j`/`k` and arrows
5. Test: `space` toggles todo, moves to/from completed section
6. Test: `d` deletes, `e` edits
7. Test: `v` enters visual, select multiple, `ctrl+j`/`ctrl+k` moves them
8. Test: `ctrl+d`/`ctrl+u` half-page jumps
9. Test: `?` shows help popup, `escape` closes
10. Test: quit with `q`, relaunch, todos persisted
11. Test: `/` opens search, type to filter, `enter` to navigate results, `esc` to clear
