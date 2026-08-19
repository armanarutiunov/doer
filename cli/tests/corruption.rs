#![allow(clippy::expect_used)]
//! Damaged data must never crash the app, and — more importantly — must never be
//! overwritten by the partial reading we managed to salvage from it. Every case here
//! asserts both: the load reports a problem and stays usable, and the save that follows
//! leaves the damaged bytes exactly as they were.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use doer::store::FsStore;
use doer_core::store::{Problem, ProjectFile, Severity, Store, StoreError, Target};
use doer_core::{ProjectId, Todo};

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("projects")).expect("projects dir");
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn write_all_todos(&self, bytes: &[u8]) {
        fs::write(self.path().join("all-todos.json"), bytes).expect("write fixture");
    }

    fn store(&self) -> FsStore {
        FsStore::with_root(self.path())
    }

    fn all_todos_bytes(&self) -> Vec<u8> {
        fs::read(self.path().join("all-todos.json")).expect("read back")
    }
}

fn a_todo() -> Todo {
    Todo {
        id: doer_core::TodoId::from("0123456789abcdef"),
        text: "replacement".into(),
        done: false,
        created_at: 1,
        completed_at: None,
    }
}

fn assert_refuses_to_save_all_todos(store: &FsStore) {
    let err = store
        .save_all_todos(&[a_todo()])
        .expect_err("saving over an unreadable file must be refused");
    assert!(
        matches!(
            err,
            StoreError::RefusedReadOnly {
                target: Target::AllTodos
            }
        ),
        "{err:?}"
    );
}

#[test]
fn truncated_json_is_reported_and_the_file_is_left_alone() {
    let home = Home::new();
    let damaged = br#"[{"id":"05dd062149b1f9a6","text":"half a "#;
    home.write_all_todos(damaged);

    let store = home.store();
    let loaded = store.load();

    assert!(loaded.value.all_todos.is_empty());
    assert!(loaded.is_degraded());
    assert!(matches!(
        loaded.problems.as_slice(),
        [Problem::Corrupt { .. }]
    ));
    assert_eq!(loaded.value.read_only, [Target::AllTodos]);

    assert_refuses_to_save_all_todos(&store);
    assert_eq!(home.all_todos_bytes(), damaged);
}

#[test]
fn a_wrongly_typed_entry_is_carried_rather_than_dropped() {
    let home = Home::new();
    let damaged = br#"[{"id":"05dd062149b1f9a6","text":"x","done":"yes","created_at":1}]"#;
    home.write_all_todos(damaged);

    let store = home.store();
    let loaded = store.load();

    assert!(loaded.value.all_todos.is_empty());
    assert!(matches!(
        loaded.problems.as_slice(),
        [Problem::SkippedEntry { .. }]
    ));
    assert!(
        !loaded.is_degraded(),
        "nothing was lost, so this is a note rather than damage"
    );
    assert!(loaded.value.read_only.is_empty(), "the file stays writable");

    store.save_all_todos(&[a_todo()]).expect("save is allowed");

    let after = String::from_utf8(home.all_todos_bytes()).expect("utf-8");
    assert!(
        after.contains(r#""done": "yes""#),
        "the entry we could not read comes back verbatim: {after}"
    );
    assert!(
        after.contains("replacement"),
        "and the new todo is there too"
    );
}

#[test]
fn an_entry_with_a_null_id_is_kept_at_its_position_across_a_save() {
    let home = Home::new();
    let damaged = br#"[{"id":null,"text":"nameless"},{"id":"05dd062149b1f9a6","text":"fine"}]"#;
    home.write_all_todos(damaged);

    let store = home.store();
    let loaded = store.load();

    assert_eq!(loaded.value.all_todos.len(), 1, "the good entry loads");
    assert_eq!(loaded.value.all_todos[0].text, "fine");
    assert!(!loaded.is_degraded());

    store
        .save_all_todos(&loaded.value.all_todos)
        .expect("save is allowed");

    let reloaded: Vec<serde_json::Value> =
        serde_json::from_slice(&home.all_todos_bytes()).expect("valid json");
    assert_eq!(reloaded.len(), 2, "both entries are on disk");
    assert_eq!(reloaded[0]["id"], serde_json::Value::Null);
    assert_eq!(
        reloaded[0]["text"], "nameless",
        "the unreadable entry is back where it was"
    );
    assert_eq!(reloaded[1]["text"], "fine");
}

#[test]
fn non_utf8_bytes_are_reported_and_the_file_is_left_alone() {
    let home = Home::new();
    let damaged: Vec<u8> = vec![b'[', b'{', b'"', 0xff, 0xfe, b'"', b'}', b']'];
    home.write_all_todos(&damaged);

    let store = home.store();
    let loaded = store.load();

    assert!(loaded.is_degraded());
    assert_refuses_to_save_all_todos(&store);
    assert_eq!(home.all_todos_bytes(), damaged);
}

#[test]
fn a_directory_where_a_file_belongs_is_reported_rather_than_fatal() {
    let home = Home::new();
    fs::create_dir(home.path().join("all-todos.json")).expect("mkdir in place of the file");

    let store = home.store();
    let loaded = store.load();

    assert!(loaded.value.all_todos.is_empty());
    assert!(matches!(
        loaded.problems.as_slice(),
        [Problem::Unreadable { .. }]
    ));
    assert_refuses_to_save_all_todos(&store);
    assert!(home.path().join("all-todos.json").is_dir());
}

#[test]
fn a_corrupt_project_file_does_not_stop_its_siblings_from_loading() {
    let home = Home::new();
    let projects = home.path().join("projects");
    fs::write(
        projects.join("8257339108cf0e12.json"),
        b"{ this is not json",
    )
    .expect("write damaged project");
    fs::write(
        projects.join("a1b2c3d4e5f60718.json"),
        br#"{"id":"a1b2c3d4e5f60718","index":0,"name":"fine","parent_id":null,"todos":[]}"#,
    )
    .expect("write healthy project");

    let store = home.store();
    let loaded = store.load();

    assert_eq!(loaded.value.projects.len(), 1);
    assert_eq!(loaded.value.projects[0].name, "fine");
    assert!(loaded.is_degraded());

    let damaged_id = ProjectId::from("8257339108cf0e12");
    assert!(
        loaded
            .value
            .read_only
            .contains(&Target::Project(damaged_id.clone()))
    );

    let before = fs::read(projects.join("8257339108cf0e12.json")).expect("read back");
    let err = store
        .save_project(&ProjectFile {
            id: damaged_id,
            index: 0,
            name: "overwrite me".into(),
            parent_id: None,
            todos: vec![a_todo()],
        })
        .expect_err("must be refused");
    assert!(matches!(err, StoreError::RefusedReadOnly { .. }), "{err:?}");
    assert_eq!(
        fs::read(projects.join("8257339108cf0e12.json")).expect("read back"),
        before
    );

    // The healthy sibling is still writable.
    store
        .save_project(&loaded.value.projects[0].clone())
        .expect("healthy project still saves");
}

#[test]
fn a_read_only_directory_surfaces_a_write_error_rather_than_a_panic() {
    let home = Home::new();
    home.write_all_todos(b"[]");
    let store = home.store();
    assert!(!store.load().is_degraded());

    let mut perms = fs::metadata(home.path()).expect("metadata").permissions();
    perms.set_readonly(true);
    fs::set_permissions(home.path(), perms).expect("make read-only");

    let result = store.save_all_todos(&[a_todo()]);

    let mut perms = fs::metadata(home.path()).expect("metadata").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(home.path(), perms).expect("restore permissions");

    // Running as root ignores the mode bits entirely, so only assert when the OS enforced it.
    if let Err(err) = result {
        assert!(matches!(err, StoreError::Write { .. }), "{err:?}");
        assert_eq!(home.all_todos_bytes(), b"[]");
    }
}

#[test]
fn an_empty_doer_home_loads_clean() {
    let home = Home::new();
    let loaded = home.store().load();

    assert!(loaded.value.all_todos.is_empty());
    assert!(loaded.value.projects.is_empty());
    assert!(loaded.problems.is_empty());
}

#[test]
fn a_load_after_the_damage_is_repaired_clears_the_refusal() {
    let home = Home::new();
    home.write_all_todos(b"{ not an array");
    let store = home.store();
    assert!(store.load().is_degraded());
    assert!(store.save_all_todos(&[a_todo()]).is_err());

    home.write_all_todos(b"[]");
    let loaded = store.load();
    assert!(!loaded.is_degraded());
    assert!(loaded.problems.is_empty());
    store
        .save_all_todos(&[a_todo()])
        .expect("a repaired file is writable again");
}

#[test]
fn a_carried_entry_keeps_its_own_key_order() {
    let home = Home::new();
    home.write_all_todos(br#"[{"zebra":1,"apple":2,"id":null}]"#);

    let store = home.store();
    store.load();
    store.save_all_todos(&[]).expect("save is allowed");

    let after = String::from_utf8(home.all_todos_bytes()).expect("utf-8");
    let zebra = after.find("zebra").expect("zebra");
    let apple = after.find("apple").expect("apple");
    assert!(
        zebra < apple,
        "re-sorting a carried entry's keys would rewrite a file we do not understand: {after}"
    );
}

#[test]
fn an_entry_we_cannot_read_survives_being_reloaded_and_saved_again() {
    let home = Home::new();
    home.write_all_todos(br#"[{"id":null,"text":"nameless"}]"#);

    for _ in 0..3 {
        let store = home.store();
        let loaded = store.load();
        store
            .save_all_todos(&loaded.value.all_todos)
            .expect("save is allowed");
    }

    let entries: Vec<serde_json::Value> =
        serde_json::from_slice(&home.all_todos_bytes()).expect("valid json");
    assert_eq!(
        entries.len(),
        1,
        "the entry is neither dropped nor duplicated"
    );
    assert_eq!(entries[0]["text"], "nameless");
}

#[test]
fn every_reported_problem_renders_a_message_naming_the_file() {
    let home = Home::new();
    home.write_all_todos(b"{ not an array");
    let loaded = home.store().load();

    for problem in &loaded.problems {
        let text = problem.to_string();
        assert!(text.contains("all-todos.json"), "{text}");
        assert_eq!(problem.severity(), Severity::Error);
    }
}

/// `init` creates the directory only if it is missing, so a `projects/` that exists but
/// cannot be listed would otherwise make every project vanish with no warning at all.
#[test]
fn a_projects_directory_that_cannot_be_listed_is_reported_not_swallowed() {
    let home = Home::new();
    let projects = home.path().join("projects");
    fs::write(
        projects.join("0123456789abcdef.json"),
        br#"{"id":"0123456789abcdef","index":0,"name":"work","parent_id":null,"todos":[]}"#,
    )
    .expect("seed a project");

    let mut perms = fs::metadata(&projects).expect("metadata").permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&projects, perms).expect("make unlistable");

    let loaded = home.store().load();

    let mut perms = fs::metadata(&projects).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&projects, perms).expect("restore permissions");

    // Running as root ignores the mode bits, so only assert when the OS enforced it.
    if loaded.value.projects.is_empty() {
        assert!(
            loaded
                .problems
                .iter()
                .any(|p| matches!(p, Problem::Unreadable { .. })),
            "the projects went missing without a word: {:?}",
            loaded.problems
        );
        assert!(
            loaded.is_degraded(),
            "an empty sidebar with the data still on disk is data loss, not a warning"
        );
    }
}
