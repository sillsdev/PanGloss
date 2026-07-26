//! Citation liveness for the coverage ledger's containment/refusal evidence.
//!
//! # Why this gate exists
//! `crate::coverage_ledger::containment_evidence_for` returns hand-curated
//! [`ContainmentEvidence`] citations of the form `tests/<file>.rs::<test_fn>`, and
//! `build_ledger` grades a [`Disposition::FailClosed`] row `Covered` **iff** such a citation
//! exists with `kind == RefusalWitness` (G8: a refused construct can never have a passing
//! analysis fixture, so the refusal witness is its only admissible evidence). That makes the
//! ledger's own `Covered` verdict depend on a **string** — and nothing, before this file,
//! checked that the string pointed at anything real.
//!
//! A citation that names a deleted file, or a `#[test]` that was renamed or removed, would keep
//! reporting `Covered` forever. That is precisely the shape of overclaim ADR 0001 exists to
//! forbid: the ledger would assert evidence it does not have. Renaming a test is routine; this
//! gate is what makes that routine change fail loudly instead of silently downgrading the
//! ledger's honesty.
//!
//! # What it checks
//! For every citation string on every ledger row:
//! 1. every `tests/<name>.rs` path token names a file that EXISTS in this crate's `tests/`
//!    directory (or, for the handful of citations that point at a `src/` unit-test module, that
//!    `src/` file), and
//! 2. every `<file>.rs::<ident>` test-function reference — including the ones listed in a
//!    `(+ a, b, c)` continuation, which is this crate's own established citation style — names an
//!    identifier that actually appears as `fn <ident>` somewhere under `tests/` or `src/`.
//!
//! # What it deliberately does NOT check
//! That the cited test actually *proves* what its `note` claims. That is a human review
//! obligation and no test can discharge it. This gate closes the mechanical half only: the
//! citation cannot be a dangling pointer. The `note` field remains hand-reviewed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pg_foma::capability::default_registry;
use pg_foma::coverage_ledger::build_ledger;

/// Crate root (`rust/crates/pg-foma`), from `CARGO_MANIFEST_DIR` — never a path relative to the
/// process CWD, which differs between `cargo test` and a bare test-binary invocation.
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `tests/<name>.rs` (or `src/<name>.rs`) path token appearing in `citation`.
///
/// Citations are free prose with embedded paths, multi-file lists, and parenthetical
/// continuations, so this scans for the `.rs` suffix and walks backwards over the path token
/// rather than trying to impose a grammar on the prose.
fn cited_paths(citation: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = citation.as_bytes();
    let mut i = 0usize;
    while let Some(rel) = citation[i..].find(".rs") {
        let end = i + rel + 3;
        // Walk backwards over path-legal characters to find the token start.
        let mut start = i + rel;
        while start > 0 {
            let c = bytes[start - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'/' || c == b'-' || c == b'.' {
                start -= 1;
            } else {
                break;
            }
        }
        let token = &citation[start..end];
        if token.starts_with("tests/") || token.starts_with("src/") {
            out.insert(token.to_string());
        }
        i = end;
    }
    out
}

/// Every test-function identifier referenced by `citation`.
///
/// Two forms, both already in use in `containment_evidence_for`:
/// - `tests/<file>.rs::<ident>` — the primary reference.
/// - `(+ <ident>, <ident>, ...)` — a continuation listing sibling tests in the same file.
///
/// A `(+ ...)` group is only scanned for snake_case identifiers; prose words inside it (`for`,
/// `the`, `split`) are filtered by requiring at least one `_`, which every test name in this
/// crate has and no bare English word in these notes does.
fn cited_test_fns(citation: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();

    // Form 1: `::<ident>`
    let mut i = 0usize;
    while let Some(rel) = citation[i..].find("::") {
        let start = i + rel + 2;
        let ident: String = citation[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if ident.contains('_') {
            out.insert(ident);
        }
        i = start.max(i + rel + 2);
        if i >= citation.len() {
            break;
        }
    }

    // Form 2: `(+ a, b, c)` continuations.
    let mut j = 0usize;
    while let Some(rel) = citation[j..].find("(+") {
        let start = j + rel + 2;
        let Some(close_rel) = citation[start..].find(')') else {
            break;
        };
        let group = &citation[start..start + close_rel];
        for raw in group.split([',', ';']) {
            let ident: String = raw
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            // Require a `_` (every test fn in this crate is snake_case with at least one) and a
            // reasonable length, so prose fragments in the continuation are not mistaken for
            // identifiers.
            if ident.contains('_') && ident.len() >= 8 {
                out.insert(ident);
            }
        }
        j = start + close_rel;
    }

    out
}

/// Read every `.rs` file under `dir` (one level; this crate keeps integration tests flat) and
/// concatenate them, for a cheap `fn <ident>` containment scan.
fn concat_rs_sources(dir: &Path) -> String {
    let mut all = String::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return all;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                all.push_str(&text);
                all.push('\n');
            }
        }
    }
    all
}

/// Every ledger citation's file paths resolve to a file that exists.
///
/// This is the half that catches a deleted or renamed test FILE. A `FailClosed` row whose
/// refusal-witness file no longer exists would otherwise keep reporting `Covered`.
#[test]
fn every_ledger_citation_names_a_file_that_exists() {
    let ledger = build_ledger(&default_registry(), &std::collections::HashSet::new());
    let root = crate_root();
    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for row in &ledger.rows {
        let Some(ev) = &row.containment else { continue };
        for rel in cited_paths(&ev.citation) {
            checked += 1;
            if !root.join(&rel).is_file() {
                missing.push(format!("{:?} cites missing file {rel}", row.kind));
            }
        }
    }

    assert!(
        checked > 0,
        "the scan found zero cited paths — the citation format changed and this gate went \
         vacuous, which is worse than a failure (it would silently stop protecting the ledger)"
    );
    assert!(
        missing.is_empty(),
        "ledger citations point at files that do not exist ({} of {checked} cited paths). A \
         dangling citation keeps its row reporting Covered on evidence that is gone — fix the \
         citation in `coverage_ledger::containment_evidence_for`, or restore the test:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

/// Every ledger citation's `::<test_fn>` references resolve to a real `fn`.
///
/// This is the half that catches a RENAMED test inside a file that still exists — the more
/// likely of the two failure modes, and the one a file-existence check alone cannot see.
#[test]
fn every_ledger_citation_names_a_test_fn_that_exists() {
    let ledger = build_ledger(&default_registry(), &std::collections::HashSet::new());
    let root = crate_root();
    let haystack = {
        let mut s = concat_rs_sources(&root.join("tests"));
        s.push_str(&concat_rs_sources(&root.join("src")));
        s
    };
    assert!(
        !haystack.is_empty(),
        "read no Rust sources at all — this gate cannot be trusted in that state"
    );

    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for row in &ledger.rows {
        let Some(ev) = &row.containment else { continue };
        for ident in cited_test_fns(&ev.citation) {
            checked += 1;
            if !haystack.contains(&format!("fn {ident}")) {
                missing.push(format!("{:?} cites missing test fn `{ident}`", row.kind));
            }
        }
    }

    assert!(
        checked > 0,
        "the scan found zero cited test functions — the citation format changed and this gate \
         went vacuous; see the sibling test's own note on why that is the worse outcome"
    );
    assert!(
        missing.is_empty(),
        "ledger citations name test functions that do not exist ({} of {checked} references). \
         A renamed test leaves its row claiming evidence it no longer has:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

/// The specific row whose `Covered` verdict depends ENTIRELY on a citation string.
///
/// Every other disposition is graded against the passing-fixture set, which is computed from real
/// fixture runs; `FailClosed` is the one disposition where the citation IS the evidence (G8,
/// `build_ledger`'s own doc). So this pins the strongest form of the check for exactly that row:
/// not merely that the citation resolves, but that it resolves to a `#[test]`.
#[test]
fn fail_closed_refusal_witness_resolves_to_an_actual_test() {
    use pg_foma::capability::Disposition;

    let ledger = build_ledger(&default_registry(), &std::collections::HashSet::new());
    let root = crate_root();
    let mut fail_closed_rows = 0usize;

    for row in &ledger.rows {
        if row.disposition != Disposition::FailClosed {
            continue;
        }
        fail_closed_rows += 1;
        let ev = row.containment.as_ref().unwrap_or_else(|| {
            panic!(
                "{:?} is FailClosed, so its Covered verdict can ONLY come from a refusal-witness \
                 citation — it has none, which means build_ledger must be reporting it Uncovered; \
                 if that changed, this gate needs updating deliberately",
                row.kind
            )
        });

        // Read every cited file up front so an ident may legitimately live in any ONE of them,
        // while still being REQUIRED to live in at least one — a `continue`-on-not-found loop
        // would silently pass a citation whose ident exists nowhere, which is the exact failure
        // this test is here to catch.
        let mut cited_sources: Vec<(String, String)> = Vec::new();
        for rel in cited_paths(&ev.citation) {
            let path = root.join(&rel);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{:?} cites {rel}, which cannot be read: {e}", row.kind));
            cited_sources.push((rel, text));
        }
        assert!(
            !cited_sources.is_empty(),
            "{:?} is FailClosed and its citation names no readable file at all",
            row.kind
        );

        let idents = cited_test_fns(&ev.citation);
        assert!(
            !idents.is_empty(),
            "{:?} is FailClosed but its citation names no test function — the Covered verdict \
             would rest on a file path alone, which cannot witness a refusal",
            row.kind
        );

        for ident in idents {
            let needle = format!("fn {ident}");
            let hit = cited_sources
                .iter()
                .find_map(|(rel, text)| text.find(&needle).map(|at| (rel, text, at)));
            let (rel, text, at) = hit.unwrap_or_else(|| {
                panic!(
                    "{:?}'s refusal witness `{ident}` appears in NONE of its own cited files \
                     ({}) — a FailClosed row's entire Covered verdict rests on this citation",
                    row.kind,
                    cited_sources
                        .iter()
                        .map(|(r, _)| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
            let preamble = &text[at.saturating_sub(200)..at];
            assert!(
                preamble.contains("#[test]"),
                "{:?}'s refusal witness `{ident}` in {rel} is not a #[test] — it must name a \
                 test that actually runs, not a helper",
                row.kind
            );
        }
    }

    assert!(
        fail_closed_rows > 0,
        "no FailClosed rows found — either the registry changed or this gate is vacuous"
    );
}
