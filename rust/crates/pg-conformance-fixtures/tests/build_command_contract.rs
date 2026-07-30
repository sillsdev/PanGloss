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

/// The managed scripts MUST refuse positional binding on their own parameters.
///
/// PowerShell makes every `[string]$Foo = ''` parameter implicitly positional, so without
/// `PositionalBinding = $false` a stray cargo flag is bound as the VALUE of whichever positional
/// slot is free rather than passed through. Measured before the fix: `pg.ps1 -Mode test -Package
/// pg-foma --no-capture` bound `--no-capture` to `-Filter`, and nextest duly reported `0 tests run`
/// for a filter no test name can match -- a run that executed nothing while looking like a
/// successful filtered run. `-Mode build --example foo` was absorbed into `-Path`/`-Base` just as
/// quietly. Since the hook now makes these scripts the ONLY way to build, an entry point that can
/// silently discard an argument that changes what runs is the same self-concealing class of failure
/// the corpus-required gate exists to prevent.
///
/// This is a TEXT pin on the declaration, not an execution test: running PowerShell from a Rust test
/// would need a real cargo invocation to observe the passthrough, and this suite deliberately runs
/// without a corpus or a build. It catches removal of the attribute, which is the regression that
/// matters; it does not re-derive the binding semantics.
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
