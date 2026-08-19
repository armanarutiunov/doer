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
    for path in todo_list_fixtures().into_iter().chain(project_fixtures()) {
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
