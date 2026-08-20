# Agents

Things about this codebase that are not obvious from reading it, and that you will
break if you do not know them.

## The crate split is a boundary, not a folder layout

`core/` (`doer-core`) is pure: no `ratatui`, no `crossterm`, no `std::fs`, no
`SystemTime::now()`. Time arrives as an `i64` parameter, persistence arrives through the
`Store` trait. `cli/` (`doer`) owns the terminal, the filesystem and the threads.

The point is that `cargo test -p doer-core` needs no terminal and no home directory, so
the domain, the keymap, the layout and the reducer are all testable as data. Adding a
terminal dependency to `core` deletes that property, which is why Cargo is set up to
refuse it rather than a convention asking you not to.

## The rendered-row list is the only scroll coordinate system

`layout::build` produces the exact list of rows that will be drawn, including wrapped
continuation lines, section headers and blank spacers. Scrolling, the cursor's visual
position and the viewport all index into that list and nothing else.

The Elixir version instead recomputed a visual row by summing section-header overheads
and ignoring wrapping, so scrolling drifted as soon as a todo wrapped. Do not reintroduce
a second way to answer "which screen row is the cursor on". If you need that number, ask
the layout.

## The files are a shared contract, not this binary's private state

Treat `~/.doer` as a format other programs and other builds of doer also read and write,
because one already did: the Elixir version. That is why the rules below exist, and why
they are worth keeping even when nothing is currently on the other end of them.

Field order in `Todo` and `ProjectFile` is the on-disk key order. Reordering a struct
field is a data-format break, not a refactor. Two-space indent, no trailing newline.
`serde_json`'s `preserve_order` feature is load-bearing for this, not an unused flag.

Ids are stored exactly as they were read, so a file written by any other version
round-trips verbatim. Only ids that are 16 hex characters are used as filenames; anything
else is written under a safe name instead.

Writes are per-file and touch only what changed, so a write never rewrites a file whose
contents we had no reason to touch. Anything in a file we do not understand — an unknown
field, an entry we cannot parse — is carried through and written back unchanged rather
than dropped.

The long form is the module doc of `core/src/store.rs`; `cli/tests/byte_compat.rs`
enforces it. Deliberately absent, and deliberately not precluded: schema versions, file
locking, any notion of sync.

Keeping all of that true is what would let `~/.doer` be synced between machines, or read
by something else, without this binary being the one that destroys the data. It is not a
feature today. Do not build one, and do not weaken these rules on the grounds that only
one program reads the files.

## Damaged files are never overwritten with our reading of them

A file that cannot be parsed at all is marked read-only for the session: saving that
target is refused rather than replacing it with whatever we managed to salvage. The user
is told, with a toast that does not expire.

A file with one unreadable *entry* is a different case and is **not** read-only. The
entry is kept verbatim and written back at its original position, so the file stays
editable and nothing is lost in either direction. Do not "simplify" this by dropping the
entry — that is silent data loss, and it is the failure mode the whole rule exists for.

Because nothing is lost, that case is also not worth telling the user about: it would be
an alarm for a non-event, repeated on every load, with no action for them to take. The
whole-file failure is the only one that gets a message, because there the user's edits
really are not being saved.

## Modes

`Focus` is derived, never stored: the mode lives inside the state that owns its payload,
so "insert mode with nothing being edited" is unrepresentable. Focus can only move
between panes when the pane being left is idle, which is what makes resetting a pane's
state on a focus change safe. `core/tests/keymap.rs` pins that; it is load-bearing, not
incidental.

## Comments

Comment only what the code cannot say for itself: why a non-obvious choice was made, an
invariant the reader must preserve, an edge case that would otherwise be lost. Never
restate the next line. Never write a doc comment that repeats the signature. No
section-divider banners. Prefer no comment to an obvious one.
