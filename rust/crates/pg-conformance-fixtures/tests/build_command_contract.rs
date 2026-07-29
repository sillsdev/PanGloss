//! Verifies `CLAUDE.md` still states the managed-build-command contract from
//! `docs/superpowers/specs/2026-07-29-categorical-build-hardening-design.md` (Definition of done:
//! "A direct agent-workflow Cargo command is absent from maintained PanGloss instructions"). Runs
//! in CI with no private corpus: it only reads a tracked markdown file, nothing under
//! `samples/data/`.
//!
//! This is a REGRESSION LOCK, not a style check. CLAUDE.md is free to reword the surrounding
//! prose, reorder sections, or add more guidance -- but if a future edit drops the prohibition on
//! bare `cargo build`/`cargo test`/`cargo check`/`cargo run` or the pointer to the managed
//! `rust/tools/pg.ps1` entry point, this test fails instead of the guidance silently reverting to
//! the state the design doc's "Problem" section describes (an agent invoking Cargo directly,
//! bypassing target redirection and the shared compiler cache).

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

    // The specific bare invocations the design doc names as prohibited, checked individually (not
    // just "cargo build" alone) so a rewrite that keeps some but drops others is still caught.
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
