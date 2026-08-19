use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::id::ProjectId;
use crate::project::Project;
use crate::todo::Todo;

/// One project file on disk: the project's own fields with its todos embedded.
///
/// The field order below is the on-disk key order the Elixir build wrote. It is
/// alphabetical only because Erlang sorts small-map binary keys — the order of the map
/// literal in `store.ex` is a red herring, and `Todo`'s order is different again (it comes
/// from Jason's `@derive only:` list). Reordering either list rewrites every byte of the
/// user's files; `tests/byte_compat.rs` is what actually holds the line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub id: ProjectId,
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<ProjectId>,
    #[serde(default)]
    pub todos: Vec<Todo>,
}

impl ProjectFile {
    #[must_use]
    pub fn new(project: &Project, todos: Vec<Todo>) -> Self {
        Self {
            id: project.id.clone(),
            index: project.index,
            name: project.name.clone(),
            parent_id: project.parent_id.clone(),
            todos,
        }
    }

    #[must_use]
    pub fn project(&self) -> Project {
        Project {
            id: self.id.clone(),
            name: self.name.clone(),
            index: self.index,
            parent_id: self.parent_id.clone(),
        }
    }
}

/// Which file a save is aimed at.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    AllTodos,
    Project(ProjectId),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllTodos => f.write_str("all todos"),
            Self::Project(id) => write!(f, "project {id}"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("cannot determine the doer home directory")]
    NoHome,
    #[error("could not read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not encode {target}")]
    Encode {
        target: Target,
        #[source]
        source: serde_json::Error,
    },
    /// The load of this target failed, so writing would destroy data we could not read.
    #[error("not saving {target}: its file failed to load")]
    RefusedReadOnly { target: Target },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// Something that went wrong during a load. A load never fails as a whole; it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    Unreadable {
        path: PathBuf,
        detail: String,
    },
    Corrupt {
        path: PathBuf,
        detail: String,
    },
    SkippedEntry {
        path: PathBuf,
        detail: String,
    },
    Migrated {
        from: PathBuf,
        to: PathBuf,
    },
    /// An id that cannot be used as a filename; the file is written under a safe name.
    NonCanonicalId {
        id: String,
        saved_as: PathBuf,
    },
    TrashPruneFailed {
        path: PathBuf,
        detail: String,
    },
}

impl Problem {
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Self::Unreadable { .. } | Self::Corrupt { .. } | Self::SkippedEntry { .. } => {
                Severity::Error
            }
            Self::Migrated { .. } | Self::NonCanonicalId { .. } | Self::TrashPruneFailed { .. } => {
                Severity::Warning
            }
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, detail } => {
                write!(f, "{} could not be read: {detail}", name_of(path))
            }
            Self::Corrupt { path, detail } => {
                write!(f, "{} is not valid doer json: {detail}", name_of(path))
            }
            Self::SkippedEntry { path, detail } => {
                write!(f, "{} has an unreadable entry: {detail}", name_of(path))
            }
            Self::Migrated { from, to } => {
                write!(f, "migrated {} to {}", name_of(from), name_of(to))
            }
            Self::NonCanonicalId { id, saved_as } => {
                write!(
                    f,
                    "id {id:?} is not a safe filename; saving as {}",
                    name_of(saved_as)
                )
            }
            Self::TrashPruneFailed { path, detail } => {
                write!(f, "could not prune {}: {detail}", name_of(path))
            }
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    )
}

/// A value plus everything that went wrong producing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded<T> {
    pub value: T,
    pub problems: Vec<Problem>,
}

impl<T> Loaded<T> {
    pub fn ok(value: T) -> Self {
        Self {
            value,
            problems: Vec::new(),
        }
    }

    pub fn new(value: T, problems: Vec<Problem>) -> Self {
        Self { value, problems }
    }

    /// True when something was lost, as opposed to merely noted.
    pub fn is_degraded(&self) -> bool {
        self.problems
            .iter()
            .any(|p| p.severity() == Severity::Error)
    }

    pub fn into_parts(self) -> (T, Vec<Problem>) {
        (self.value, self.problems)
    }
}

/// Everything the store holds, in the shape the files hold it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreSnapshot {
    pub all_todos: Vec<Todo>,
    pub projects: Vec<ProjectFile>,
    /// Targets whose file could not be fully understood. Saving these is refused so a
    /// damaged file is never overwritten with our partial reading of it.
    pub read_only: Vec<Target>,
}

pub trait Store: Send {
    fn load(&self) -> Loaded<StoreSnapshot>;
    fn save_all_todos(&self, todos: &[Todo]) -> Result<(), StoreError>;
    fn save_project(&self, file: &ProjectFile) -> Result<(), StoreError>;
    fn delete_project(&self, id: &ProjectId) -> Result<(), StoreError>;
}

/// Store double for tests that need no disk. Records writes so a test can assert what a
/// mutation actually persisted.
#[derive(Debug, Default)]
pub struct MemoryStore {
    state: Mutex<MemoryState>,
}

#[derive(Debug, Default)]
struct MemoryState {
    snapshot: StoreSnapshot,
    problems: Vec<Problem>,
    writes: Vec<Target>,
    deletes: Vec<ProjectId>,
    fail_next: Option<Target>,
}

impl MemoryStore {
    #[must_use]
    pub fn new(snapshot: StoreSnapshot) -> Self {
        Self {
            state: Mutex::new(MemoryState {
                snapshot,
                ..MemoryState::default()
            }),
        }
    }

    #[must_use]
    pub fn with_problems(self, problems: Vec<Problem>) -> Self {
        if let Ok(mut state) = self.state.lock() {
            state.problems = problems;
        }
        self
    }

    /// Makes the next save of `target` fail, so a caller's error path can be exercised.
    pub fn fail_next(&self, target: Target) {
        if let Ok(mut state) = self.state.lock() {
            state.fail_next = Some(target);
        }
    }

    #[must_use]
    pub fn writes(&self) -> Vec<Target> {
        self.state
            .lock()
            .map(|s| s.writes.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn deletes(&self) -> Vec<ProjectId> {
        self.state
            .lock()
            .map(|s| s.deletes.clone())
            .unwrap_or_default()
    }

    fn record(&self, target: Target) -> Result<(), StoreError> {
        let Ok(mut state) = self.state.lock() else {
            return Ok(());
        };
        if state.fail_next.as_ref() == Some(&target) {
            state.fail_next = None;
            return Err(StoreError::Write {
                path: PathBuf::from(target.to_string()),
                source: io::Error::other("injected failure"),
            });
        }
        state.writes.push(target);
        Ok(())
    }
}

impl Store for MemoryStore {
    fn load(&self) -> Loaded<StoreSnapshot> {
        self.state.lock().map_or_else(
            |_| Loaded::ok(StoreSnapshot::default()),
            |state| Loaded::new(state.snapshot.clone(), state.problems.clone()),
        )
    }

    fn save_all_todos(&self, todos: &[Todo]) -> Result<(), StoreError> {
        self.record(Target::AllTodos)?;
        if let Ok(mut state) = self.state.lock() {
            state.snapshot.all_todos = todos.to_vec();
        }
        Ok(())
    }

    fn save_project(&self, file: &ProjectFile) -> Result<(), StoreError> {
        self.record(Target::Project(file.id.clone()))?;
        if let Ok(mut state) = self.state.lock() {
            match state.snapshot.projects.iter_mut().find(|p| p.id == file.id) {
                Some(existing) => existing.clone_from(file),
                None => state.snapshot.projects.push(file.clone()),
            }
        }
        Ok(())
    }

    fn delete_project(&self, id: &ProjectId) -> Result<(), StoreError> {
        if let Ok(mut state) = self.state.lock() {
            state.snapshot.projects.retain(|p| &p.id != id);
            state.deletes.push(id.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::TodoId;

    fn todo(id: &str) -> Todo {
        Todo {
            id: TodoId::from(id),
            text: "x".into(),
            done: false,
            created_at: 0,
            completed_at: None,
        }
    }

    #[test]
    fn project_file_key_order_matches_the_elixir_format() {
        let file = ProjectFile {
            id: ProjectId::from("8257339108cf0e12"),
            index: 0,
            name: "reviews".into(),
            parent_id: None,
            todos: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&file).expect("serialize");
        assert_eq!(
            json,
            "{\n  \"id\": \"8257339108cf0e12\",\n  \"index\": 0,\n  \"name\": \"reviews\",\n  \"parent_id\": null,\n  \"todos\": []\n}"
        );
    }

    #[test]
    fn a_project_file_round_trips_through_its_project() {
        let project = Project {
            id: ProjectId::from("abc"),
            name: "work".into(),
            index: 3,
            parent_id: Some(ProjectId::from("parent")),
        };
        let file = ProjectFile::new(&project, vec![todo("t1")]);
        assert_eq!(file.project(), project);
        assert_eq!(file.todos.len(), 1);
    }

    #[test]
    fn a_load_with_only_warnings_is_not_degraded() {
        let loaded = Loaded::new(
            StoreSnapshot::default(),
            vec![Problem::Migrated {
                from: PathBuf::from("todos.json"),
                to: PathBuf::from("all-todos.json"),
            }],
        );
        assert!(!loaded.is_degraded());
    }

    #[test]
    fn a_load_with_a_corrupt_file_is_degraded() {
        let loaded = Loaded::new(
            StoreSnapshot::default(),
            vec![Problem::Corrupt {
                path: PathBuf::from("all-todos.json"),
                detail: "eof".into(),
            }],
        );
        assert!(loaded.is_degraded());
    }

    #[test]
    fn memory_store_records_which_targets_were_written() {
        let store = MemoryStore::new(StoreSnapshot::default());
        store.save_all_todos(&[todo("a")]).expect("save");
        store
            .save_project(&ProjectFile {
                id: ProjectId::from("p1"),
                index: 0,
                name: "p".into(),
                parent_id: None,
                todos: Vec::new(),
            })
            .expect("save");
        assert_eq!(
            store.writes(),
            [Target::AllTodos, Target::Project(ProjectId::from("p1"))]
        );
        assert_eq!(store.load().value.all_todos.len(), 1);
    }

    #[test]
    fn memory_store_can_inject_a_single_save_failure() {
        let store = MemoryStore::new(StoreSnapshot::default());
        store.fail_next(Target::AllTodos);
        assert!(store.save_all_todos(&[]).is_err());
        assert!(store.save_all_todos(&[]).is_ok());
    }
}
