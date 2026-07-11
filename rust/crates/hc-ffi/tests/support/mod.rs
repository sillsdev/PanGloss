//! Shared helpers for hc-ffi's integration tests. A `tests/support/mod.rs` (not a top-level
//! `tests/*.rs` file) so Cargo treats it as a module, not its own test binary — the same pattern
//! `hc-grammar`'s tests use for locating the untracked sample corpora (plan §8: "corpora stay
//! untracked local files with self-skipping tests").

use std::path::{Path, PathBuf};

/// Locate a sample corpus file on disk. `None` if absent, so callers self-skip rather than fail
/// on a machine/CI without the untracked `samples/data` corpora.
pub fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/hc-ffi ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

pub fn load_words(name: &str) -> Option<Vec<String>> {
    let path = sample_path(name)?;
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
}

pub fn load_xml(name: &str) -> Option<String> {
    let path = sample_path(name)?;
    std::fs::read_to_string(path).ok()
}
