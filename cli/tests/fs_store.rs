#![allow(clippy::expect_used)]
//! Directory-level behaviour: the legacy migration, deletes going to the trash, pruning
//! that trash, and ids that cannot be used as filenames.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use doer::store::FsStore;
use doer_core::ProjectId;
use doer_core::store::{Problem, ProjectFile, Store};

const DAY: u64 = 24 * 60 * 60;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn project(id: &str) -> ProjectFile {
    ProjectFile {
        id: ProjectId::from(id),
        index: 0,
        name: "work".into(),
        parent_id: None,
        todos: Vec::new(),
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn a_first_run_creates_the_directories_it_needs() {
    let dir = home();
    let root = dir.path().join("fresh");
    let store = FsStore::with_root(&root);
    store.load();

    assert!(root.is_dir());
    assert!(root.join("projects").is_dir());
}

#[test]
fn a_legacy_todos_file_is_renamed_once_and_reported() {
    let dir = home();
    let body = br#"[{"id":"05dd062149b1f9a6","text":"legacy","done":false,"created_at":1,"completed_at":null}]"#;
    fs::write(dir.path().join("todos.json"), body).expect("write legacy file");

    let store = FsStore::with_root(dir.path());
    let loaded = store.load();

    assert_eq!(loaded.value.all_todos.len(), 1);
    assert_eq!(loaded.value.all_todos[0].text, "legacy");
    assert!(matches!(
        loaded.problems.as_slice(),
        [Problem::Migrated { .. }]
    ));
    assert!(!dir.path().join("todos.json").exists());
    assert_eq!(
        fs::read(dir.path().join("all-todos.json")).expect("read migrated"),
        body
    );
}

#[test]
fn a_legacy_file_is_left_alone_when_the_new_one_already_exists() {
    let dir = home();
    fs::write(dir.path().join("todos.json"), b"[]").expect("write legacy");
    fs::write(dir.path().join("all-todos.json"), b"[]").expect("write current");

    let loaded = FsStore::with_root(dir.path()).load();

    assert!(loaded.problems.is_empty());
    assert!(
        dir.path().join("todos.json").exists(),
        "the legacy file stays put rather than being silently discarded"
    );
}

#[test]
fn deleting_a_project_moves_its_file_to_the_trash() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();
    store
        .save_project(&project("8257339108cf0e12"))
        .expect("save");

    let path = store.project_path(&ProjectId::from("8257339108cf0e12"));
    let bytes = fs::read(&path).expect("read before delete");

    store
        .delete_project(&ProjectId::from("8257339108cf0e12"))
        .expect("delete");

    assert!(!path.exists());
    let bins = names_in(&store.trash_dir());
    assert_eq!(bins.len(), 1, "one dated bin: {bins:?}");
    let recovered = store
        .trash_dir()
        .join(&bins[0])
        .join("8257339108cf0e12.json");
    assert_eq!(
        fs::read(recovered).expect("read from trash"),
        bytes,
        "the trashed copy is the file verbatim, so a crash before undo loses nothing"
    );
}

#[test]
fn deleting_a_project_that_was_never_written_is_not_an_error() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();
    store
        .delete_project(&ProjectId::from("0123456789abcdef"))
        .expect("deleting nothing succeeds");
}

#[test]
fn trash_older_than_a_week_is_pruned_at_startup_and_newer_trash_is_kept() {
    let dir = home();
    let trash = dir.path().join(".trash");
    let stale = (now_secs() - 8 * DAY).to_string();
    let fresh = (now_secs() - DAY).to_string();
    for bin in [&stale, &fresh, &"not-a-timestamp".to_string()] {
        fs::create_dir_all(trash.join(bin)).expect("create bin");
        fs::write(trash.join(bin).join("x.json"), b"{}").expect("write bin file");
    }

    FsStore::with_root(dir.path()).load();

    let remaining = names_in(&trash);
    assert!(!remaining.contains(&stale), "{remaining:?}");
    assert!(remaining.contains(&fresh), "{remaining:?}");
    assert!(
        remaining.contains(&"not-a-timestamp".to_string()),
        "a bin we did not name is not ours to delete: {remaining:?}"
    );
}

#[test]
fn an_id_that_is_not_a_safe_filename_is_written_under_an_encoded_name() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();

    let hostile = ProjectId::from("../../../etc/passwd");
    let path = store.new_project_path(&hostile);
    store
        .save_project(&project("../../../etc/passwd"))
        .expect("save");

    assert!(path.is_file());
    assert_eq!(
        path.parent(),
        Some(store.projects_dir().as_path()),
        "the write stays inside the projects directory"
    );
    assert_eq!(names_in(&store.projects_dir()).len(), 1);
}

#[test]
fn a_project_with_an_unsafe_id_still_round_trips_with_its_id_intact() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();
    store.save_project(&project("not hex")).expect("save");

    let loaded = FsStore::with_root(dir.path()).load();

    assert_eq!(loaded.value.projects.len(), 1);
    assert_eq!(loaded.value.projects[0].id, ProjectId::from("not hex"));
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
}

/// Renaming another tool's file would leave the original behind, and the next load would
/// read both and show the project twice.
#[test]
fn a_project_read_from_an_oddly_named_file_is_written_back_to_that_file() {
    let dir = home();
    fs::create_dir_all(dir.path().join("projects")).expect("projects dir");
    fs::write(
        dir.path().join("projects/not hex.json"),
        br#"{"id":"not hex","index":0,"name":"odd id","parent_id":null,"todos":[]}"#,
    )
    .expect("seed");

    let store = FsStore::with_root(dir.path());
    let mut file = store.load().value.projects[0].clone();
    file.todos.push(doer_core::Todo {
        id: doer_core::TodoId::from("0123456789abcdef"),
        text: "added this session".into(),
        done: false,
        created_at: 1,
        completed_at: None,
    });
    store.save_project(&file).expect("save");

    assert_eq!(
        names_in(&store.projects_dir()),
        ["not hex.json"],
        "no second file appears alongside the original"
    );

    let reloaded = FsStore::with_root(dir.path()).load();
    assert_eq!(reloaded.value.projects.len(), 1, "and no duplicate project");
    assert_eq!(reloaded.value.projects[0].todos.len(), 1);
    assert_eq!(
        reloaded.value.projects[0].todos[0].text,
        "added this session"
    );
}

#[test]
fn two_files_claiming_one_project_load_once_and_neither_is_overwritten() {
    let dir = home();
    fs::create_dir_all(dir.path().join("projects")).expect("projects dir");
    let body = br#"{"id":"not hex","index":0,"name":"odd id","parent_id":null,"todos":[]}"#;
    fs::write(dir.path().join("projects/aaa.json"), body).expect("seed");
    fs::write(dir.path().join("projects/not hex.json"), body).expect("seed");

    let store = FsStore::with_root(dir.path());
    let loaded = store.load();

    assert_eq!(loaded.value.projects.len(), 1, "the project loads once");
    assert!(
        loaded
            .problems
            .iter()
            .any(|p| matches!(p, Problem::DuplicateProject { .. })),
        "{:?}",
        loaded.problems
    );
    assert!(
        !loaded.is_degraded(),
        "nothing was lost, so this is a warning"
    );

    let before = fs::read(dir.path().join("projects/not hex.json")).expect("read");
    store
        .save_project(&loaded.value.projects[0].clone())
        .expect("save");
    assert_eq!(
        fs::read(dir.path().join("projects/not hex.json")).expect("read"),
        before,
        "the file we did not adopt is left exactly as it was"
    );
}

#[test]
fn a_saved_project_reloads_with_its_todos() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();

    let mut file = project("8257339108cf0e12");
    file.parent_id = Some(ProjectId::from("a1b2c3d4e5f60718"));
    file.todos = vec![doer_core::Todo {
        id: doer_core::TodoId::from("05dd062149b1f9a6"),
        text: "write it down".into(),
        done: true,
        created_at: 10,
        completed_at: Some(20),
    }];
    store.save_project(&file).expect("save");

    let loaded = FsStore::with_root(dir.path()).load();
    assert_eq!(loaded.value.projects, vec![file]);
}

#[test]
fn projects_load_in_a_stable_order_regardless_of_directory_order() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();
    for id in ["ffffffffffffffff", "0000000000000000", "8257339108cf0e12"] {
        store.save_project(&project(id)).expect("save");
    }

    let ids: Vec<String> = FsStore::with_root(dir.path())
        .load()
        .value
        .projects
        .iter()
        .map(|p| p.id.to_string())
        .collect();

    assert_eq!(
        ids,
        ["0000000000000000", "8257339108cf0e12", "ffffffffffffffff"]
    );
}

#[test]
fn a_non_json_file_in_the_projects_directory_is_ignored() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();
    fs::write(store.projects_dir().join("README.txt"), b"not mine").expect("write");

    let loaded = FsStore::with_root(dir.path()).load();
    assert!(loaded.value.projects.is_empty());
    assert!(loaded.problems.is_empty());
}

#[test]
fn a_save_leaves_no_temp_files_in_the_doer_home() {
    let dir = home();
    let store = FsStore::with_root(dir.path());
    store.load();
    store.save_all_todos(&[]).expect("save");
    store
        .save_project(&project("8257339108cf0e12"))
        .expect("save");

    let stray: Vec<PathBuf> = walk(dir.path())
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn an_unreadable_todo_inside_a_project_does_not_lock_the_project() {
    let dir = home();
    fs::create_dir_all(dir.path().join("projects")).expect("projects dir");
    fs::write(
        dir.path().join("projects/8257339108cf0e12.json"),
        br#"{"id":"8257339108cf0e12","index":0,"name":"work","parent_id":null,"todos":[{"id":null},{"id":"05dd062149b1f9a6","text":"real","done":false,"created_at":1,"completed_at":null}]}"#,
    )
    .expect("write project");

    let store = FsStore::with_root(dir.path());
    let loaded = store.load();

    assert!(!loaded.is_degraded(), "{:?}", loaded.problems);
    assert!(loaded.value.read_only.is_empty());
    let mut file = loaded.value.projects[0].clone();
    assert_eq!(file.todos.len(), 1);

    file.todos.push(doer_core::Todo {
        id: doer_core::TodoId::from("ffffffffffffffff"),
        text: "added later".into(),
        done: false,
        created_at: 2,
        completed_at: None,
    });
    store.save_project(&file).expect("save is allowed");

    let reloaded = FsStore::with_root(dir.path()).load();
    assert_eq!(
        reloaded.value.projects[0].todos.len(),
        2,
        "the new todo persisted rather than being lost to a locked file"
    );
    let raw: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.path().join("projects/8257339108cf0e12.json")).expect("read"),
    )
    .expect("valid json");
    assert_eq!(
        raw["todos"].as_array().map(Vec::len),
        Some(3),
        "the entry we could not read is still on disk alongside both todos"
    );
}
