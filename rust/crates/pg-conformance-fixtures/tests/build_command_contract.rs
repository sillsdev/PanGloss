//! Regression lock, not a style check: fails if a future `CLAUDE.md` edit drops the bare-Cargo prohibition or the pointer to `rust/tools/pg.ps1`, rather than letting the guidance silently revert. Reads only a tracked markdown file, so it needs no private corpus.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn claude_md_mandates_the_managed_build_entry_point() {
    let path = repo_root().join("CLAUDE.md");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    assert!(
        text.contains("rust/tools/pg.ps1"),
        "CLAUDE.md must point agent workflows at rust/tools/pg.ps1, the managed build entry point"
    );

    // Checked individually, not just "cargo build" alone, so a rewrite keeping some but dropping others is still caught.
    for banned in ["cargo build", "cargo test", "cargo check", "cargo run"] {
        assert!(
            text.contains(banned),
            "CLAUDE.md must still name `{banned}` among the prohibited bare Cargo invocations"
        );
    }

    let lower = text.to_ascii_lowercase();
    assert!(
        lower.contains("prohibit"),
        "CLAUDE.md must state the bare-Cargo prohibition outright, not just mention pg.ps1 in passing"
    );
}

/// The managed scripts must declare `PositionalBinding = $false`: PowerShell otherwise binds a stray cargo flag to whichever positional parameter is free instead of passing it through, silently discarding an argument that changes what runs. Text pin only — no real cargo invocation is run here.
#[test]
fn managed_build_scripts_refuse_positional_binding() {
    for script in ["pg.ps1", "build.ps1", "test.ps1"] {
        let path = repo_root().join("rust/tools").join(script);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let normalized = text.replace(' ', "");
        assert!(
            normalized.contains("[CmdletBinding(PositionalBinding=$false)]"),
            "{script} must declare [CmdletBinding(PositionalBinding = $false)] so an unrecognized \
             flag reaches cargo or fails loudly, instead of being absorbed as some other \
             parameter's value"
        );
    }
}
