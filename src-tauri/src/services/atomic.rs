//! Crash-safe file writes: tmp file, fsync, rename (retried: AV/indexers briefly lock the target on Windows).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, thread};

const RENAME_RETRY_DELAYS_MS: [u64; 2] = [15, 45];

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let res = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        rename_with_retry(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    for delay in RENAME_RETRY_DELAYS_MS {
        match fs::rename(from, to) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                thread::sleep(Duration::from_millis(delay))
            }
            r => return r,
        }
    }
    fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::tests::tmp_dir;

    #[test]
    fn creates_and_replaces_without_leftovers() {
        let dir = tmp_dir("atomic");
        let file = dir.join("data.json");
        write_atomic(&file, b"old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_dir_fails_cleanly() {
        let dir = tmp_dir("atomic-missing");
        assert!(write_atomic(&dir.join("nope").join("data.json"), b"x").is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
