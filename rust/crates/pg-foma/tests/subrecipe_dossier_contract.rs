//! Mechanical contract for the six maintained executable-subrecipe dossiers.
//!
//! This test checks the parts of the dossier contract that are easy to lose during later
//! implementation work. It deliberately does not try to prove the linguistic claims; those remain
//! source-and-review obligations.

use std::path::{Path, PathBuf};

const DOSSIERS: [&str; 6] = [
    "morphotactics.md",
    "static-partition.md",
    "ordered-phonology.md",
    "structural-allomorph.md",
    "copy-process.md",
    "boundary-cleanup.md",
];

const REQUIRED_HEADINGS: [&str; 14] = [
    "Scope",
    "Languages and families in mind",
    "Primary sources",
    "Grammar facts",
    "Formal model and regularity",
    "Chosen architecture",
    "Rejected architectures",
    "Interfaces and interactions",
    "Complexity and resource bounds",
    "Conformance fixtures",
    "Implementation status",
    "Known gaps and split triggers",
    "Research log",
    "Evidence decisions",
];

fn dossier_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/fst-plan/subrecipes")
}

fn read_dossiers() -> Vec<(&'static str, String)> {
    DOSSIERS
        .iter()
        .map(|name| {
            let path = dossier_root().join(name);
            (
                *name,
                std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("cannot read dossier {}: {error}", path.display())
                }),
            )
        })
        .collect()
}

fn section(text: &str, heading: &str) -> String {
    let marker = format!("## {heading}");
    let start = text
        .find(&marker)
        .unwrap_or_else(|| panic!("missing section {heading:?}"));
    let body = &text[start + marker.len()..];
    body.split_once("\n## ")
        .map_or_else(|| body.to_owned(), |(body, _)| body.to_owned())
}

#[test]
fn subrecipe_dossier_files_exist_and_have_exact_required_headings() {
    for (name, text) in read_dossiers() {
        assert!(!text.trim().is_empty(), "{name} is empty");
        for heading in REQUIRED_HEADINGS {
            assert!(
                text.lines().any(|line| line == format!("## {heading}")),
                "{name} is missing the exact `## {heading}` heading"
            );
        }
    }
}

#[test]
fn subrecipe_dossier_scope_and_invariants_are_explicit() {
    for (name, text) in read_dossiers() {
        let scope = section(&text, "Scope").to_ascii_lowercase();
        assert!(
            scope.contains("non-scope"),
            "{name} must state an explicit non-scope"
        );
        assert!(
            text.to_ascii_lowercase().contains("invariant"),
            "{name} must state invariants"
        );
        assert!(
            text.to_ascii_lowercase().contains("confidence")
                && text.to_ascii_lowercase().contains("uncertainty"),
            "{name} must distinguish confidence from source uncertainty"
        );
    }
}

#[test]
fn subrecipe_dossier_language_anchors_name_families_and_construct_roles() {
    for (name, text) in read_dossiers() {
        let anchors = section(&text, "Languages and families in mind");
        let qualified = anchors
            .split("\n- **")
            .filter(|entry| entry.contains("Anchor"))
            .filter(|entry| entry.contains("Family:") && entry.contains("Construct:"))
            .count();
        assert!(
            qualified >= 2,
            "{name} needs at least two anchors with explicit `Family:` and `Construct:` fields"
        );
    }
}

#[test]
fn subrecipe_dossier_architecture_correctness_and_complexity_are_explicit() {
    for (name, text) in read_dossiers() {
        for required in [
            "Chosen architecture",
            "Rejected architectures",
            "Correctness obligations",
            "Failure modes",
            "Big-O variables",
            "Time",
            "Space",
        ] {
            assert!(
                text.contains(required),
                "{name} must contain an explicit {required} statement"
            );
        }
        assert!(
            text.contains("O("),
            "{name} must include a Big-O expression"
        );
        assert!(
            section(&text, "Conformance fixtures")
                .matches("Exercise ")
                .count()
                >= 2,
            "{name} needs two independent conformance exercises where possible"
        );
    }
}

#[test]
fn subrecipe_dossier_logs_links_and_decision_triggers_are_dated() {
    for (name, text) in read_dossiers() {
        let log = section(&text, "Research log");
        assert!(
            log.contains("| 2026-08-01 |"),
            "{name} needs a dated research-log row"
        );
        assert!(
            log.contains("https://") || log.contains("]("),
            "{name} research log needs a direct source or repository link"
        );

        let decisions = section(&text, "Evidence decisions");
        for decision in ["fits", "refines", "splits/adds"] {
            assert!(
                decisions.contains(&format!("| {decision} |")),
                "{name} must record the {decision} evidence decision and consequence"
            );
        }
        assert!(
            decisions.to_ascii_lowercase().contains("trigger"),
            "{name} evidence decisions must state when a decision changes"
        );
    }
}
