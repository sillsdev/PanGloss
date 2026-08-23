//! Default cache location: user-data directory, never next to the `.fwdata` file.
//!
//! `ConfigurationSettings/PanGloss/` is reserved for small project data Chorus/FLExBridge
//! synchronizes; a stats cache can reach tens of megabytes and must never be swept into that sync.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::StatsError;

/// First 16 hex chars (64 bits) of the SHA-256 over the canonicalized path's lossy bytes.
fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        write!(&mut hex, "{byte:02x}").expect("write to String cannot fail");
    }
    hex
}

fn user_data_root() -> Result<PathBuf, StatsError> {
    #[cfg(windows)]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            if !local_app_data.is_empty() {
                return Ok(PathBuf::from(local_app_data));
            }
        }
        Err(StatsError::NoUserDataDir)
    }
    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg));
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home).join(".local").join("share"));
            }
        }
        Err(StatsError::NoUserDataDir)
    }
}

/// The directory this fwdata path's cache lives in: `<user-data>/PanGloss/stats/<digest>/`.
///
/// The path must exist on disk — canonicalization is what makes two different-looking paths to
/// the same file collapse onto the same cache, and it needs a real file to resolve `..`/symlinks
/// against.
pub fn default_cache_dir(fwdata_path: impl AsRef<Path>) -> Result<PathBuf, StatsError> {
    cache_dir_under(fwdata_path, user_data_root()?)
}

/// Split out of `default_cache_dir` so tests supply a root instead of mutating process-global `LOCALAPPDATA`.
fn cache_dir_under(fwdata_path: impl AsRef<Path>, root: PathBuf) -> Result<PathBuf, StatsError> {
    let fwdata_path = fwdata_path.as_ref();
    let canonical =
        fwdata_path
            .canonicalize()
            .map_err(|source| StatsError::CanonicalizeFailed {
                path: fwdata_path.to_path_buf(),
                source,
            })?;
    let digest = hex_digest(canonical.to_string_lossy().as_bytes());
    Ok(root.join("PanGloss").join("stats").join(digest))
}

/// The default cache file path for a given `.fwdata` path.
///
/// Callers that want to manage their own cache lifetime (Motif) pass an explicit path to
/// `crate::cache::StatsCache::open` instead of calling this at all.
pub fn default_cache_path(fwdata_path: impl AsRef<Path>) -> Result<PathBuf, StatsError> {
    Ok(default_cache_dir(fwdata_path)?.join("cache.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_path_dependent() {
        assert_eq!(hex_digest(b"a"), hex_digest(b"a"));
        assert_ne!(hex_digest(b"a"), hex_digest(b"b"));
    }

    #[test]
    fn default_cache_dir_is_not_beside_the_fwdata_file() {
        let dir = crate::test_support::TempDir::new("pg-stats-path");
        let fwdata = dir.path().join("project.fwdata");
        std::fs::write(&fwdata, b"stub").unwrap();
        let root = dir.path().join("localappdata");

        let cache_dir = cache_dir_under(&fwdata, root.clone()).unwrap();
        assert!(cache_dir.starts_with(&root));
        assert_ne!(cache_dir.parent().unwrap(), fwdata.parent().unwrap());
        assert!(cache_dir.to_string_lossy().contains("PanGloss"));
        assert!(cache_dir.to_string_lossy().contains("stats"));
    }

    #[test]
    fn same_path_produces_same_dir_twice() {
        let dir = crate::test_support::TempDir::new("pg-stats-path-2");
        let fwdata = dir.path().join("project.fwdata");
        std::fs::write(&fwdata, b"stub").unwrap();
        let root = dir.path().join("localappdata");

        let a = cache_dir_under(&fwdata, root.clone()).unwrap();
        let b = cache_dir_under(&fwdata, root).unwrap();
        assert_eq!(a, b);
    }
}
