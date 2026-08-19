use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::process;

/// Writes `bytes` to `path` so that a crash leaves either the old file or the new one,
/// never a half-written one: write a sibling temp file, flush it to the disk, then rename
/// over the target. The rename is atomic because both paths are in the same directory.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_path(path);
    if let Err(err) = write_and_sync(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    sync_dir(path);
    Ok(())
}

/// The file's contents are durable once `write_and_sync` returns, but the rename that
/// publishes them is an edit to the *directory*, and that needs its own sync or a power
/// cut can leave the pre-save file behind — after the app has already told the user the
/// save succeeded. Ignored on failure: the write itself is sound either way, and no
/// filesystem worth supporting fails this while accepting the rename.
fn sync_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = File::open(parent).and_then(|dir| dir.sync_all());
    }
}

fn write_and_sync(tmp: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn temp_path(path: &Path) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| "doer".to_string(), |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!(".{name}.tmp-{}", process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writing_replaces_the_previous_contents() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("f.json");
        write_atomic(&path, b"one").expect("first write");
        write_atomic(&path, b"two").expect("second write");
        assert_eq!(fs::read(&path).expect("read"), b"two");
    }

    #[test]
    fn a_successful_write_leaves_no_temp_file_behind() {
        let dir = tempdir().expect("tempdir");
        write_atomic(&dir.path().join("f.json"), b"x").expect("write");
        let names: Vec<String> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["f.json"]);
    }

    #[test]
    fn a_failed_write_leaves_no_temp_file_behind() {
        let dir = tempdir().expect("tempdir");
        // A directory where the target file should be makes the rename fail after the
        // temp file has already been written.
        let path = dir.path().join("f.json");
        fs::create_dir(&path).expect("mkdir");
        fs::write(path.join("occupied"), b"x").expect("write");

        assert!(write_atomic(&path, b"x").is_err());
        let names: Vec<String> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["f.json"]);
    }
}
