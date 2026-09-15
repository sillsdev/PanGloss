//! The divergence catalogue broke inside the session that created it: three files numbered `001`,
//! two numbered `002`, and the extras were evidence dumps rather than entries. A hand-maintained
//! index cannot notice that, so this checks the properties the convention depends on.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn divergences_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/divergences")
}

/// `NNN-slug.md` entries only: `README.md` and `by-module.md` are the index, not entries, and
/// anything under `evidence/` is supporting material that deliberately carries no id.
fn entries() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(divergences_dir())
        .expect("docs/divergences exists")
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some((id, _)) = name.split_once('-') else {
            continue;
        };
        if id.len() == 3 && id.chars().all(|c| c.is_ascii_digit()) {
            out.push((id.to_string(), name));
        }
    }
    out
}

#[test]
fn every_entry_id_is_unique() {
    let mut by_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, name) in entries() {
        by_id.entry(id).or_default().push(name);
    }
    let dupes: Vec<String> = by_id
        .iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(id, files)| format!("{id}: {}", files.join(", ")))
        .collect();
    assert!(
        dupes.is_empty(),
        "divergence entry ids must be unique — two files sharing one id means the status table in \
         README.md can only describe one of them, and the other is invisible. Renumber, or move \
         supporting material under evidence/:\n  {}",
        dupes.join("\n  ")
    );
}

#[test]
fn the_index_lists_every_entry_and_invents_none() {
    let readme = fs::read_to_string(divergences_dir().join("README.md")).expect("README.md");
    let mut missing = Vec::new();
    for (_, name) in entries() {
        if !readme.contains(&name) {
            missing.push(name);
        }
    }
    missing.sort();
    assert!(
        missing.is_empty(),
        "entry file(s) absent from README.md's status table, so a reader scanning the table would \
         never learn they exist: {missing:?}"
    );
}

#[test]
fn the_gate_can_actually_see_entries() {
    let found = entries();
    assert!(
        found.len() >= 20,
        "read only {} entries out of docs/divergences — the naming convention changed and both \
         checks above would now pass vacuously",
        found.len()
    );
}
