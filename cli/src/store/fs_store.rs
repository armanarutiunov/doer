use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use doer_core::store::{Loaded, Problem, ProjectFile, Store, StoreError, StoreSnapshot, Target};
use doer_core::{ProjectId, Todo};
use serde::Serialize;
use serde_json::Value;

use super::atomic::write_atomic;

const TRASH_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// The `~/.doer` directory. `DOER_HOME` overrides the location so tests and throwaway
/// runs never touch the real one.
#[derive(Debug)]
pub struct FsStore {
    root: PathBuf,
    read_only: Mutex<HashSet<Target>>,
    /// Array entries that are valid JSON but not todos, kept verbatim with the position
    /// they held so a save puts them back untouched. Without this a single unreadable
    /// entry would either be dropped on the next save or lock the file for the session.
    unparsed: Mutex<HashMap<Target, Unparsed>>,
}

/// Entries we could not read, each with its index in the array it came from.
type Unparsed = Vec<(usize, Value)>;

impl FsStore {
    pub fn new() -> Result<Self, StoreError> {
        let root = resolve_root(
            std::env::var_os("DOER_HOME").map(PathBuf::from),
            dirs::home_dir(),
        )?;
        Ok(Self::with_root(root))
    }

    #[must_use]
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            read_only: Mutex::new(HashSet::new()),
            unparsed: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn all_todos_path(&self) -> PathBuf {
        self.root.join("all-todos.json")
    }

    #[must_use]
    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    #[must_use]
    pub fn trash_dir(&self) -> PathBuf {
        self.root.join(".trash")
    }

    #[must_use]
    pub fn project_path(&self, id: &ProjectId) -> PathBuf {
        self.projects_dir().join(format!("{}.json", file_stem(id)))
    }

    /// Creates the directories, renames a legacy `todos.json`, and drops expired trash.
    fn init(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        for dir in [self.root.clone(), self.projects_dir()] {
            if let Err(err) = fs::create_dir_all(&dir) {
                problems.push(Problem::Unreadable {
                    path: dir,
                    detail: err.to_string(),
                });
            }
        }
        problems.extend(self.migrate_legacy());
        problems.extend(self.prune_trash(now_secs()));
        problems
    }

    fn migrate_legacy(&self) -> Option<Problem> {
        let legacy = self.root.join("todos.json");
        let target = self.all_todos_path();
        if !legacy.is_file() || target.exists() {
            return None;
        }
        match fs::rename(&legacy, &target) {
            Ok(()) => Some(Problem::Migrated {
                from: legacy,
                to: target,
            }),
            Err(err) => Some(Problem::Unreadable {
                path: legacy,
                detail: err.to_string(),
            }),
        }
    }

    fn prune_trash(&self, now: u64) -> Vec<Problem> {
        let Ok(entries) = fs::read_dir(self.trash_dir()) else {
            return Vec::new();
        };
        let mut problems = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let stamp = path
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(|n| n.parse::<u64>().ok());
            // An unparseable name is not ours; leave it rather than guessing its age.
            let Some(stamp) = stamp else { continue };
            if now.saturating_sub(stamp) <= TRASH_RETENTION_SECS {
                continue;
            }
            if let Err(err) = fs::remove_dir_all(&path) {
                problems.push(Problem::TrashPruneFailed {
                    path,
                    detail: err.to_string(),
                });
            }
        }
        problems
    }

    fn load_all_todos(&self, problems: &mut Vec<Problem>) -> Vec<Todo> {
        let path = self.all_todos_path();
        if !path.exists() {
            return Vec::new();
        }
        let Some(raw) = read_json::<Vec<Value>>(&path, problems) else {
            self.mark_read_only(Target::AllTodos);
            return Vec::new();
        };
        let (todos, unparsed) = split_entries(&path, raw, problems);
        self.remember_unparsed(Target::AllTodos, unparsed);
        todos
    }

    fn load_projects(&self, problems: &mut Vec<Problem>) -> Vec<ProjectFile> {
        let Ok(entries) = fs::read_dir(self.projects_dir()) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension() == Some(OsStr::new("json")))
            .collect();
        paths.sort();

        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(raw) = read_json::<RawProjectFile>(&path, problems) else {
                // The project's own fields are unreadable, so fall back to the filename to
                // refuse a later save — otherwise a project restored by undo could
                // overwrite a file we never managed to read.
                if let Some(id) = id_from_path(&path) {
                    self.mark_read_only(Target::Project(id));
                }
                continue;
            };
            let (todos, unparsed) = split_entries(&path, raw.todos, problems);
            let file = ProjectFile {
                id: raw.id,
                index: raw.index,
                name: raw.name,
                parent_id: raw.parent_id,
                todos,
            };
            self.remember_unparsed(Target::Project(file.id.clone()), unparsed);
            if !file.id.is_canonical() {
                problems.push(Problem::NonCanonicalId {
                    id: file.id.to_string(),
                    saved_as: self.project_path(&file.id),
                });
            }
            files.push(file);
        }
        files
    }

    fn remember_unparsed(&self, target: Target, unparsed: Unparsed) {
        if let Ok(mut map) = self.unparsed.lock() {
            if unparsed.is_empty() {
                map.remove(&target);
            } else {
                map.insert(target, unparsed);
            }
        }
    }

    fn unparsed_for(&self, target: &Target) -> Unparsed {
        self.unparsed
            .lock()
            .ok()
            .and_then(|map| map.get(target).cloned())
            .unwrap_or_default()
    }

    fn mark_read_only(&self, target: Target) {
        if let Ok(mut set) = self.read_only.lock() {
            set.insert(target);
        }
    }

    fn refuse_if_read_only(&self, target: &Target) -> Result<(), StoreError> {
        let refused = self.read_only.lock().is_ok_and(|set| set.contains(target));
        if refused {
            return Err(StoreError::RefusedReadOnly {
                target: target.clone(),
            });
        }
        Ok(())
    }
}

fn write_json<T: serde::Serialize>(
    path: &Path,
    target: &Target,
    value: &T,
) -> Result<(), StoreError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| StoreError::Encode {
        target: target.clone(),
        source,
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| StoreError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    write_atomic(path, &bytes).map_err(|source| StoreError::Write {
        path: path.to_path_buf(),
        source,
    })
}

impl Store for FsStore {
    fn load(&self) -> Loaded<StoreSnapshot> {
        if let Ok(mut set) = self.read_only.lock() {
            set.clear();
        }
        if let Ok(mut map) = self.unparsed.lock() {
            map.clear();
        }
        let mut problems = self.init();
        let all_todos = self.load_all_todos(&mut problems);
        let projects = self.load_projects(&mut problems);
        let read_only = self
            .read_only
            .lock()
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();

        Loaded::new(
            StoreSnapshot {
                all_todos,
                projects,
                read_only,
            },
            problems,
        )
    }

    fn save_all_todos(&self, todos: &[Todo]) -> Result<(), StoreError> {
        let target = Target::AllTodos;
        self.refuse_if_read_only(&target)?;
        let path = self.all_todos_path();
        let unparsed = self.unparsed_for(&target);
        if unparsed.is_empty() {
            write_json(&path, &target, &todos)
        } else {
            write_json(&path, &target, &splice(todos, &unparsed, &target)?)
        }
    }

    fn save_project(&self, file: &ProjectFile) -> Result<(), StoreError> {
        let target = Target::Project(file.id.clone());
        self.refuse_if_read_only(&target)?;
        let path = self.project_path(&file.id);
        let unparsed = self.unparsed_for(&target);
        if unparsed.is_empty() {
            return write_json(&path, &target, file);
        }
        let out = ProjectFileOut {
            id: &file.id,
            index: file.index,
            name: &file.name,
            parent_id: &file.parent_id,
            todos: splice(&file.todos, &unparsed, &target)?,
        };
        write_json(&path, &target, &out)
    }

    /// Deletes by moving the file into `.trash/<unix>/`, which is crash insurance for an
    /// undo: undo restores the domain snapshot and the writer recreates the file, so the
    /// trash copy only matters if the process dies in between.
    fn delete_project(&self, id: &ProjectId) -> Result<(), StoreError> {
        let path = self.project_path(id);
        if !path.exists() {
            return Ok(());
        }
        let bin = self.trash_dir().join(now_secs().to_string());
        fs::create_dir_all(&bin).map_err(|source| StoreError::Write {
            path: bin.clone(),
            source,
        })?;
        let name = path.file_name().unwrap_or(OsStr::new("project.json"));
        fs::rename(&path, bin.join(name)).map_err(|source| StoreError::Write { path, source })
    }
}

fn resolve_root(
    env_override: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, StoreError> {
    match env_override {
        Some(path) if !path.as_os_str().is_empty() => Ok(path),
        _ => home.map(|h| h.join(".doer")).ok_or(StoreError::NoHome),
    }
}

/// A filename that cannot escape the projects directory. Ids the app generates are already
/// safe; anything else is hex-encoded, which is reversible, collision-free, and cannot
/// contain a separator.
fn file_stem(id: &ProjectId) -> String {
    if id.is_canonical() {
        return id.to_string();
    }
    let encoded: String = id
        .as_str()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .concat();
    if encoded.is_empty() {
        "unnamed".to_string()
    } else {
        encoded
    }
}

/// Only a filename the app itself would have written identifies a project; anything else
/// is some other tool's file and we make no claim about which project it belongs to.
fn id_from_path(path: &Path) -> Option<ProjectId> {
    let stem = path.file_stem().and_then(OsStr::to_str)?;
    let id = ProjectId::from(stem);
    id.is_canonical().then_some(id)
}

fn read_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    problems: &mut Vec<Problem>,
) -> Option<T> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            problems.push(Problem::Unreadable {
                path: path.to_path_buf(),
                detail: err.to_string(),
            });
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(err) => {
            problems.push(Problem::Corrupt {
                path: path.to_path_buf(),
                detail: err.to_string(),
            });
            None
        }
    }
}

/// The project's own fields decoded, with its todos still raw so they can be split into
/// what we understood and what we are only carrying.
#[derive(serde::Deserialize)]
struct RawProjectFile {
    id: ProjectId,
    #[serde(default)]
    index: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    parent_id: Option<ProjectId>,
    #[serde(default)]
    todos: Vec<Value>,
}

/// Field order here is `ProjectFile`'s field order, which is the on-disk key order.
#[derive(Serialize)]
struct ProjectFileOut<'a> {
    id: &'a ProjectId,
    index: i64,
    name: &'a str,
    parent_id: &'a Option<ProjectId>,
    todos: Vec<Value>,
}

/// Splits an array into the todos we understood and the entries we did not, keeping the
/// latter verbatim at the position they held.
fn split_entries(
    path: &Path,
    raw: Vec<Value>,
    problems: &mut Vec<Problem>,
) -> (Vec<Todo>, Unparsed) {
    let mut todos = Vec::with_capacity(raw.len());
    let mut unparsed = Unparsed::new();
    for (index, entry) in raw.into_iter().enumerate() {
        match serde_json::from_value(entry.clone()) {
            Ok(todo) => todos.push(todo),
            Err(err) => {
                problems.push(Problem::SkippedEntry {
                    path: path.to_path_buf(),
                    detail: err.to_string(),
                });
                unparsed.push((index, entry));
            }
        }
    }
    (todos, unparsed)
}

/// Rebuilds the array the file will hold: the current todos with the entries we could not
/// read put back. An entry whose index no longer exists lands at the end rather than being
/// dropped. `preserve_order` on `serde_json` is what keeps a carried entry's keys in their
/// original order — the default `BTreeMap` backing would silently re-sort them.
fn splice(todos: &[Todo], unparsed: &Unparsed, target: &Target) -> Result<Vec<Value>, StoreError> {
    let mut out: Vec<Value> = Vec::with_capacity(todos.len() + unparsed.len());
    for todo in todos {
        out.push(
            serde_json::to_value(todo).map_err(|source| StoreError::Encode {
                target: target.clone(),
                source,
            })?,
        );
    }
    for (index, entry) in unparsed {
        let at = (*index).min(out.len());
        out.insert(at, entry.clone());
    }
    Ok(out)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_override_wins_over_the_home_directory() {
        let root = resolve_root(
            Some(PathBuf::from("/tmp/doer-test")),
            Some(PathBuf::from("/home/someone")),
        );
        assert_eq!(root.expect("root"), PathBuf::from("/tmp/doer-test"));
    }

    #[test]
    fn an_empty_env_override_falls_back_to_the_home_directory() {
        let root = resolve_root(Some(PathBuf::new()), Some(PathBuf::from("/home/someone")));
        assert_eq!(root.expect("root"), PathBuf::from("/home/someone/.doer"));
    }

    #[test]
    fn without_a_home_directory_there_is_no_root() {
        assert!(matches!(resolve_root(None, None), Err(StoreError::NoHome)));
    }

    #[test]
    fn a_canonical_id_is_used_as_the_filename_verbatim() {
        assert_eq!(
            file_stem(&ProjectId::from("8257339108cf0e12")),
            "8257339108cf0e12"
        );
    }

    #[test]
    fn an_id_with_path_separators_cannot_escape_the_projects_directory() {
        let stem = file_stem(&ProjectId::from("../../etc/passwd"));
        assert!(!stem.contains('/'));
        assert!(!stem.contains('.'));
        assert_eq!(stem, "2e2e2f2e2e2f6574632f706173737764");
    }

    #[test]
    fn an_empty_id_still_produces_a_filename() {
        assert_eq!(file_stem(&ProjectId::from("")), "unnamed");
    }
}
