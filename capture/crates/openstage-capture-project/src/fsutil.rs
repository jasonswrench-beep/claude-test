// SPDX-License-Identifier: Apache-2.0
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(unix)]
fn sync_dir(dir: Option<&Path>) {
    if let Some(dir) = dir {
        if let Ok(file) = File::open(dir) {
            let _ = file.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: Option<&Path>) {}

/// Write a file so that a crash leaves either the old file or the new one, never half of each.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_dir(path.parent());
    Ok(())
}

/// The current time in UTC, such as `2026-10-13T12:00:00Z`.
pub fn utc_now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// A short identifier such as `cap-3fa9c01b22d4e7a1`. Unique enough to tell projects apart; not secret.
pub fn new_id(prefix: &str) -> String {
    let mut hasher = DefaultHasher::new();
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    COUNTER.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert!(!tmp_path(&path).exists());
    }

    #[test]
    fn ids_differ() {
        assert_ne!(new_id("x"), new_id("x"));
        assert!(new_id("cap").starts_with("cap-"));
    }

    #[test]
    fn the_time_looks_like_a_time() {
        let now = utc_now_iso();
        assert!(now.ends_with('Z') && now.contains('T'), "got {now}");
    }
}
