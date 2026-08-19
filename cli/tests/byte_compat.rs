#![allow(clippy::expect_used)]
//! The user must be able to switch between the Elixir build and this one at will, so a
//! load followed by a save has to reproduce the file byte for byte. These fixtures carry
//! the shapes that would break it: CJK, emoji, embedded quotes, a set `parent_id`, an
//! empty todo list and an empty top-level array.

use std::fs;
use std::path::{Path, PathBuf};

use doer::store::FsStore;
use doer_core::Todo;
use doer_core::store::{ProjectFile, Store};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/elixir")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create destination");
    for entry in fs::read_dir(from).expect("read fixtures").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture");
        }
    }
}

fn todo_list_fixtures() -> Vec<PathBuf> {
    vec![
        fixtures().join("all-todos.json"),
        fixtures().join("empty-all-todos.json"),
    ]
}

/// Fixtures holding an entry this build cannot decode. They are excluded from the plain
/// re-encode tests, which decode strictly, and covered by the round-trip tests instead.
fn carried_fixtures() -> Vec<PathBuf> {
    vec![
        fixtures().join("all-todos-with-unreadable-entry.json"),
        fixtures().join("damaged-project/c0ffee0123456789.json"),
    ]
}

fn project_fixtures() -> Vec<PathBuf> {
    let dir = fixtures().join("projects");
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read projects fixtures")
        .flatten()
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}

#[test]
fn a_todo_list_file_re_encodes_to_the_bytes_it_was_read_from() {
    for path in todo_list_fixtures() {
        let original = fs::read(&path).expect("read fixture");
        let todos: Vec<Todo> = serde_json::from_slice(&original).expect("decode fixture");
        let encoded = serde_json::to_vec_pretty(&todos).expect("encode");
        assert_eq!(
            String::from_utf8_lossy(&encoded),
            String::from_utf8_lossy(&original),
            "{}",
            path.display()
        );
    }
}

#[test]
fn a_project_file_re_encodes_to_the_bytes_it_was_read_from() {
    for path in project_fixtures() {
        let original = fs::read(&path).expect("read fixture");
        let file: ProjectFile = serde_json::from_slice(&original).expect("decode fixture");
        let encoded = serde_json::to_vec_pretty(&file).expect("encode");
        assert_eq!(
            String::from_utf8_lossy(&encoded),
            String::from_utf8_lossy(&original),
            "{}",
            path.display()
        );
    }
}

#[test]
fn no_fixture_ends_in_a_newline() {
    for path in todo_list_fixtures()
        .into_iter()
        .chain(project_fixtures())
        .chain(carried_fixtures())
    {
        let bytes = fs::read(&path).expect("read fixture");
        assert_ne!(
            bytes.last(),
            Some(&b'\n'),
            "{} gained a trailing newline; the Elixir build writes none",
            path.display()
        );
    }
}

#[test]
fn non_ascii_text_is_stored_raw_rather_than_escaped() {
    let bytes = fs::read(fixtures().join("all-todos.json")).expect("read fixture");
    let text = String::from_utf8(bytes).expect("utf-8");
    assert!(text.contains('买'), "CJK should survive as itself");
    assert!(text.contains('🚀'), "emoji should survive as itself");
    assert!(!text.contains("\\u"), "nothing should be \\u-escaped");
    assert!(text.contains(r#"ask \"why\" before \"how\""#));
}

#[test]
fn loading_and_saving_a_whole_doer_home_changes_no_byte() {
    let home = tempfile::tempdir().expect("tempdir");
    copy_tree(&fixtures(), home.path());
    fs::remove_file(home.path().join("empty-all-todos.json")).expect("drop the spare fixture");

    let before = snapshot_bytes(home.path());

    let store = FsStore::with_root(home.path());
    let loaded = store.load();
    assert!(
        !loaded.is_degraded(),
        "clean fixtures should load clean: {:?}",
        loaded.problems
    );

    store
        .save_all_todos(&loaded.value.all_todos)
        .expect("save all todos");
    for file in &loaded.value.projects {
        store.save_project(file).expect("save project");
    }

    assert_eq!(snapshot_bytes(home.path()), before);
}

fn snapshot_bytes(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    collect(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push((name, fs::read(&path).expect("read file")));
        }
    }
}

/// A file holding an entry this build cannot read — a newer doer's todo, or a hand edit —
/// must survive a load and save with every byte intact, including that entry.
#[test]
fn a_file_with_an_unreadable_entry_round_trips_byte_for_byte() {
    let home = tempfile::tempdir().expect("tempdir");
    let original =
        fs::read(fixtures().join("all-todos-with-unreadable-entry.json")).expect("read fixture");
    fs::write(home.path().join("all-todos.json"), &original).expect("seed");

    let store = FsStore::with_root(home.path());
    let loaded = store.load();
    assert_eq!(loaded.value.all_todos.len(), 2, "two of the three decode");
    assert!(
        !loaded.is_degraded(),
        "a carried entry is not damage: {:?}",
        loaded.problems
    );

    store
        .save_all_todos(&loaded.value.all_todos)
        .expect("the file is still writable");

    assert_eq!(
        String::from_utf8_lossy(&fs::read(home.path().join("all-todos.json")).expect("read back")),
        String::from_utf8_lossy(&original)
    );
}

#[test]
fn a_todo_added_to_a_file_with_an_unreadable_entry_saves_alongside_it() {
    let home = tempfile::tempdir().expect("tempdir");
    let original =
        fs::read(fixtures().join("all-todos-with-unreadable-entry.json")).expect("read fixture");
    fs::write(home.path().join("all-todos.json"), &original).expect("seed");

    let store = FsStore::with_root(home.path());
    let mut todos = store.load().value.all_todos;
    todos.push(Todo {
        id: doer_core::TodoId::from("ffffffffffffffff"),
        text: "added by this build".into(),
        done: false,
        created_at: 1_786_500_000,
        completed_at: None,
    });
    store.save_all_todos(&todos).expect("save");

    let after = fs::read(home.path().join("all-todos.json")).expect("read back");
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&after).expect("valid json");
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[1]["done"], "sometimes", "carried, at its own index");
    assert_eq!(
        entries[1]["priority"], "high",
        "a field this build knows nothing about is still there"
    );
    assert_eq!(entries[3]["text"], "added by this build");

    let reloaded = FsStore::with_root(home.path()).load();
    assert_eq!(reloaded.value.all_todos.len(), 3);
}

/// The nested case: a todos array inside a project file sits two levels deeper, so this is
/// what catches a re-indentation bug the flat file would hide. The fixture carries both
/// shapes at once — a todo with fields this build does not know, and an entry it cannot
/// decode at all.
#[test]
fn a_project_file_with_an_unreadable_entry_round_trips_byte_for_byte() {
    let home = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(home.path().join("projects")).expect("projects dir");
    let original =
        fs::read(fixtures().join("damaged-project/c0ffee0123456789.json")).expect("read fixture");
    fs::write(
        home.path().join("projects/c0ffee0123456789.json"),
        &original,
    )
    .expect("seed");

    let store = FsStore::with_root(home.path());
    let loaded = store.load();
    assert_eq!(loaded.value.projects.len(), 1);
    assert_eq!(
        loaded.value.projects[0].todos.len(),
        2,
        "unknown fields do not stop a todo decoding; only the undecodable entry is set aside"
    );
    assert!(!loaded.is_degraded(), "{:?}", loaded.problems);

    store
        .save_project(&loaded.value.projects[0])
        .expect("still writable");

    assert_eq!(
        String::from_utf8_lossy(
            &fs::read(home.path().join("projects/c0ffee0123456789.json")).expect("read back")
        ),
        String::from_utf8_lossy(&original)
    );
}

#[test]
fn fields_from_a_newer_doer_survive_an_edit_to_the_same_todo() {
    let home = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(home.path().join("projects")).expect("projects dir");
    fs::write(
        home.path().join("projects/c0ffee0123456789.json"),
        fs::read(fixtures().join("damaged-project/c0ffee0123456789.json")).expect("read fixture"),
    )
    .expect("seed");

    let store = FsStore::with_root(home.path());
    let mut file = store.load().value.projects[0].clone();
    file.todos[1].text = "edited by this build".into();
    file.todos[1].done = true;
    store.save_project(&file).expect("save");

    let raw: serde_json::Value = serde_json::from_slice(
        &fs::read(home.path().join("projects/c0ffee0123456789.json")).expect("read back"),
    )
    .expect("valid json");
    let todo = &raw["todos"][1];

    assert_eq!(todo["text"], "edited by this build");
    assert_eq!(todo["done"], true);
    assert_eq!(
        todo["tags"],
        serde_json::json!(["later", "maybe"]),
        "a field this build knows nothing about is not collateral damage from an edit"
    );
    assert_eq!(todo["recurrence"]["every"], "week");

    let keys: Vec<&String> = todo.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        [
            "id",
            "text",
            "done",
            "created_at",
            "completed_at",
            "tags",
            "recurrence"
        ],
        "unknown keys keep the position they held"
    );
}
