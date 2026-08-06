//! Citation liveness for the coverage ledger's containment evidence: checks that every hand-curated `tests/<file>.rs::<test_fn>` citation names a file and function that still exist, since a renamed or deleted test would otherwise leave a dangling pointer the ledger keeps reporting as evidence it does not have. Does NOT check that the cited test actually proves its `note` claim -- that stays a human review obligation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pg_foma::capability::default_registry;
use pg_foma::coverage_ledger::build_ledger;

/// Crate root, from `CARGO_MANIFEST_DIR` -- never a path relative to the process CWD, which differs between `cargo test` and a bare test-binary invocation.
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `tests/<name>.rs` (or `src/<name>.rs`) path token appearing in `citation`, found by scanning for the `.rs` suffix and walking backwards over the path token rather than imposing a grammar on the free prose.
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

/// Every test-function identifier referenced by `citation`, in either of the two forms `containment_evidence_for` uses: `tests/<file>.rs::<ident>`, or a `(+ <ident>, ...)` continuation listing sibling tests, filtered to snake_case so prose words aren't mistaken for identifiers.
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
            // Require a `_` and a reasonable length, so prose fragments in the continuation aren't mistaken for identifiers.
            if ident.contains('_') && ident.len() >= 8 {
                out.insert(ident);
            }
        }
        j = start + close_rel;
    }

    out
}

/// Read every `.rs` file under `dir` (one level; this crate keeps integration tests flat) and concatenate them, for a cheap `fn <ident>` containment scan.
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

/// Every ledger citation's file paths resolve to a file that exists -- the half that catches a deleted or renamed test FILE.
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

/// Every ledger citation's `::<test_fn>` references resolve to a real `fn` -- the half that catches a RENAMED test inside a file that still exists, which a file-existence check alone cannot see.
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
