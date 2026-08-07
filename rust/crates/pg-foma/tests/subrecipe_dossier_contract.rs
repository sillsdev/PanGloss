//! Mechanical contract for the six maintained executable-subrecipe dossiers, checking only the parts easy to lose during later implementation work; linguistic claims remain source-and-review obligations.

use std::path::{Path, PathBuf};

const DOSSIERS: [&str; 6] = [
    "morphotactics.md",
    "static-partition.md",
    "ordered-phonology.md",
    "structural-allomorph.md",
    "copy-process.md",
    "boundary-cleanup.md",
];

const REQUIRED_HEADINGS: [&str; 15] = [
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
    "Task 6 evidence status",
];

const NOT_MEASURED_STATUS: &str = "Not measured — blocks implementation claim";
const MEASURED_STATUS_PREFIX: &str = "Measured — ";

const REQUIRED_EVIDENCE_FIELDS: [&str; 8] = [
    "Source ModelLocation/model-ID evidence",
    "Resource caps",
    "Measured stage counters",
    "Positive cases",
    "Negative cases",
    "Identity/multiplicity cases",
    "Mutations",
    "Exact normalized expected multisets/tuples",
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

/// One `YYYY-MM-DD` cell, shared by both date checks so they cannot disagree; a SHAPE, never a literal.
fn is_iso_date(cell: &str) -> bool {
    let b = cell.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn has_dated_row(log: &str) -> bool {
    log.split('|').map(str::trim).any(is_iso_date)
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

fn has_dated_evidence_decision(decisions: &str) -> bool {
    decisions.lines().any(|line| {
        let cells: Vec<_> = line.split('|').map(str::trim).collect();
        cells.len() >= 5
            && is_iso_date(cells[1])
            && matches!(cells[2], "fits" | "refines" | "splits/adds")
            && !cells[3].is_empty()
            && !cells[4].is_empty()
    })
}

fn evidence_field_block<'a>(text: &'a str, field: &str) -> &'a str {
    let start = text
        .find(field)
        .unwrap_or_else(|| panic!("missing evidence field {field:?}"));
    let body = &text[start..];
    body.split_once("\n- **").map_or(body, |(body, _)| body)
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
fn subrecipe_dossier_evidence_contract_is_explicit_and_honest() {
    for (name, text) in read_dossiers() {
        for required in REQUIRED_EVIDENCE_FIELDS {
            assert!(
                text.contains(required),
                "{name} must explicitly account for `{required}`"
            );
        }

        let evidence_status = section(&text, "Task 6 evidence status");
        for required in [
            "Source ModelLocation/model-ID evidence",
            "Resource caps",
            "Measured stage counters",
        ] {
            assert!(
                evidence_status.contains(required),
                "{name} must place `{required}` in its Task 6 evidence status section"
            );
            let field_block = evidence_field_block(&evidence_status, required);
            let has_unmeasured = field_block.matches(NOT_MEASURED_STATUS).count() == 1;
            let has_measured = field_block.contains(MEASURED_STATUS_PREFIX);
            assert!(
                has_unmeasured ^ has_measured,
                "{name} must bind exactly one canonical measured or unmeasured status to `{required}`"
            );
        }

        let fixtures = section(&text, "Conformance fixtures");
        for required in [
            "Positive cases:",
            "Negative cases:",
            "Identity/multiplicity cases:",
            "Mutations:",
            "Exact normalized expected multisets/tuples:",
        ] {
            assert!(
                fixtures.contains(required),
                "{name} conformance fixtures must contain `{required}`"
            );
        }
        let exact = fixtures
            .split("Exact normalized expected multisets/tuples:")
            .nth(1)
            .unwrap_or_else(|| panic!("{name} is missing exact normalized expected tuples"));
        assert!(
            exact.contains("source_model_id="),
            "{name} normalized expected tuples must retain source model identity"
        );
        assert!(
            exact.contains("multiplicity="),
            "{name} normalized expected tuples must retain multiplicity"
        );
    }
}

#[test]
fn subrecipe_dossier_logs_links_and_decision_triggers_are_dated() {
    for (name, text) in read_dossiers() {
        let log = section(&text, "Research log");
        assert!(
            has_dated_row(&log),
            "{name} needs a dated research-log row (a `| YYYY-MM-DD |` cell)"
        );
        assert!(
            log.contains("https://") || log.contains("]("),
            "{name} research log needs a direct source or repository link"
        );

        let decisions = section(&text, "Evidence decisions");
        assert!(
            has_dated_evidence_decision(&decisions),
            "{name} must record at least one actual dated fits/refines/splits-adds evidence decision with evidence and consequence"
        );
        assert!(
            decisions.to_ascii_lowercase().contains("trigger"),
            "{name} evidence decisions must state when a decision changes"
        );
    }
}
