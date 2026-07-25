//! `pangloss coverage [--json] [--grammar=<path>] [<out.json>]` — capability-coverage VISIBILITY:
//! answers "do we have full HC coverage?" / "so many of this and so many of that?" by rendering
//! `pg_foma`'s own coverage/capability primitives, never inventing a parallel data source.
//!
//! # What this composes (read/reuse only — no `pg_foma` coverage/ledger/capability semantics touched)
//! - [`pg_foma::coverage_ledger::build_ledger`]: one row per [`pg_foma::capability::
//!   CharacteristicKind`] — disposition, discharging predicates, conformance-construct mapping/
//!   status, and curated containment evidence. This is the single source of truth every count below
//!   is tallied FROM (never a second, independently-computed number that could drift).
//! - [`pg_foma::capability::default_registry`]: the same twelve real predicates every production
//!   compile-time gate uses.
//! - The "supported (Proven) constructs vs. conformance-covered" ADR 0001 cross-check
//!   ([`pg_foma::conformance_coverage`]'s own contract) is rendered as a FILTER over the ledger's own
//!   `disposition == Proven` rows — not a second call to `supported_coverage_report` with its own
//!   passing-set — so it is provably impossible for this section to disagree with the ledger.
//! - [`pg_foma::plan_interaction_coverage`] (node-kind adjacency-tuple coverage): OPTIONAL, only when
//!   `--grammar=<path>` is given (it needs a compiled `Plan`, which needs a grammar) — omitted
//!   entirely, and said so in both the human summary and the JSON (`plan_interaction: null`), when no
//!   grammar is supplied.
//!
//! # The "passing conformance construct" set
//! `build_ledger` (like `pg_foma::conformance_coverage::supported_coverage_report`) needs a
//! caller-supplied "which `constructs.txt` ids are exercised by a CURRENTLY-PASSING fixture" set.
//! [`passing_covered_constructs`] builds it by replaying every discoverable IN-REPO SYNTHETIC
//! conformance fixture (`pg_conformance_fixtures::discover()` — `machine/conformance/**` +
//! `conformance-staging/**`; never real-language data, per this repo's own hard rule) against
//! `pg_parse::Morpher`, mirroring `pg-foma/tests/conformance_coverage_gate.rs`'s own
//! `passing_covered_constructs` helper exactly (same oracle, same "only a currently-matching word's
//! `exercises:` tags count toward coverage" rule) — restated here (not imported) because that helper
//! is a private fn in a dev-only test file, not a library export this binary can call.
//!
//! # Tests
//! The command must run and emit valid JSON that round-trips, and every count in the human/JSON
//! summary must match a direct recount over the SAME [`pg_foma::coverage_ledger::CoverageLedger`]
//! this module built — see this file's own `tests` module.

use std::collections::HashSet;
use std::fs;

use pg_conformance_fixtures::discover;
use pg_foma::capability::{default_registry, CharacteristicKind, Disposition};
use pg_foma::conformance_coverage::CoverageStatus;
use pg_foma::coverage_ledger::{build_ledger, CoverageLedger};
use pg_foma::plan_interaction_coverage::{compute_interaction_coverage, plan_and_profile, TupleStatus};
use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use serde::Serialize;

/// This CLI report's own schema version — independent of [`pg_foma::coverage_ledger::
/// COVERAGE_LEDGER_SCHEMA_VERSION`], which the embedded `ledger` field carries in its own right.
pub const COVERAGE_CLI_SCHEMA_VERSION: u32 = 1;

/// Mirrors `pg-foma/tests/conformance_coverage_gate.rs::passing_covered_constructs` exactly (see
/// this module's own top-doc for why it is restated rather than imported). Replays every discovered
/// SYNTHETIC fixture against `pg_parse::Morpher` and collects the `exercises:` construct ids named
/// by every word/parse whose engine output CURRENTLY MATCHES the fixture's declared ground truth —
/// a currently-failing word's `exercises:` tags do not count.
fn passing_covered_constructs() -> HashSet<String> {
    let mut covered = HashSet::new();

    for f in discover() {
        let words_yaml = f.load_words_yaml();
        if words_yaml.skip_in_generic_replay().is_some() {
            continue;
        }

        let xml = f.load_grammar_xml();
        let Ok(grammar) = pg_grammar::load(&xml) else {
            continue;
        };
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

        for w in &words_yaml.words {
            if !w.adapter_visible() {
                continue;
            }
            let outcome = morpher.parse_word(&w.word);
            if w.expect_skip {
                continue;
            }
            if outcome.invalid_shape {
                continue;
            }
            if outcome.signature() != w.expected_signature() {
                continue;
            }
            for c in &w.exercises {
                covered.insert(c.clone());
            }
            for p in &w.parses {
                for c in &p.exercises {
                    covered.insert(c.clone());
                }
            }
        }
    }

    covered
}

#[derive(Serialize)]
struct DispositionCounts {
    proven: usize,
    confirm_only: usize,
    config_predicate: usize,
    fail_closed: usize,
    total: usize,
}

fn compute_disposition_counts(ledger: &CoverageLedger) -> DispositionCounts {
    let mut c = DispositionCounts {
        proven: 0,
        confirm_only: 0,
        config_predicate: 0,
        fail_closed: 0,
        total: ledger.rows.len(),
    };
    for row in &ledger.rows {
        match row.disposition {
            Disposition::Proven => c.proven += 1,
            Disposition::ConfirmOnly => c.confirm_only += 1,
            Disposition::ConfigPredicate => c.config_predicate += 1,
            Disposition::FailClosed => c.fail_closed += 1,
        }
    }
    c
}

#[derive(Serialize)]
struct EvidenceCounts {
    rows_with_discharging_predicate: usize,
    rows_with_containment_evidence: usize,
    rows_mapped_to_conformance_construct: usize,
    rows_conformance_covered: usize,
    rows_unmappable: usize,
    total_rows: usize,
}

fn compute_evidence_counts(ledger: &CoverageLedger) -> EvidenceCounts {
    EvidenceCounts {
        rows_with_discharging_predicate: ledger
            .rows
            .iter()
            .filter(|r| !r.discharging_predicates.is_empty())
            .count(),
        rows_with_containment_evidence: ledger.rows.iter().filter(|r| r.containment.is_some()).count(),
        rows_mapped_to_conformance_construct: ledger
            .rows
            .iter()
            .filter(|r| !r.construct_ids.is_empty())
            .count(),
        rows_conformance_covered: ledger
            .rows
            .iter()
            .filter(|r| r.conformance_status == CoverageStatus::Covered)
            .count(),
        rows_unmappable: ledger
            .rows
            .iter()
            .filter(|r| r.conformance_status == CoverageStatus::Unmappable)
            .count(),
        total_rows: ledger.rows.len(),
    }
}

/// One row of the ADR 0001 "supported (Proven) vs. conformance-covered" cross-check — a pure FILTER
/// over the ledger's own `disposition == Proven` rows (this module's top-doc: provably no drift
/// against the ledger, since it recomputes nothing).
#[derive(Serialize)]
struct SupportedConformanceRow {
    kind: CharacteristicKind,
    status: CoverageStatus,
    construct_ids: Vec<String>,
}

fn supported_conformance_cross_check(ledger: &CoverageLedger) -> Vec<SupportedConformanceRow> {
    ledger
        .rows
        .iter()
        .filter(|r| r.disposition == Disposition::Proven)
        .map(|r| SupportedConformanceRow {
            kind: r.kind,
            status: r.conformance_status,
            construct_ids: r.construct_ids.clone(),
        })
        .collect()
}

#[derive(Serialize)]
struct PlanInteractionRow {
    tuple: String,
    status: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct PlanInteractionSummary {
    grammar_path: String,
    required_total: usize,
    covered: usize,
    uncovered: usize,
    contains_unsupported: usize,
    retired: usize,
    unexpected_tuples: usize,
    rows: Vec<PlanInteractionRow>,
}

/// Node-kind adjacency-tuple coverage (`pg_foma::plan_interaction_coverage`) for exactly ONE
/// grammar's own compiled plan — a single-fixture corpus. Most tuples will legitimately read
/// `Uncovered` here (one grammar rarely exercises every legal adjacency shape); this is honest, not
/// a bug — the full-corpus picture is `tests/plan_interaction_coverage_gate.rs`'s own job, not this
/// command's.
fn plan_interaction_summary(grammar_path: &str, g: &Grammar) -> PlanInteractionSummary {
    let (plan, profile) = plan_and_profile(g);
    let refs = vec![(grammar_path, &plan, &profile)];
    let report = compute_interaction_coverage(&refs);

    let rows: Vec<PlanInteractionRow> = report
        .required
        .iter()
        .map(|r| PlanInteractionRow {
            tuple: format!("{:?}", r.tuple),
            status: format!("{:?}", r.status),
            tags: r.tags.iter().map(|k| format!("{k:?}")).collect(),
        })
        .collect();

    let covered = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::Covered)
        .count();
    let uncovered = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::Uncovered)
        .count();
    let contains_unsupported = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::ContainsUnsupported)
        .count();

    PlanInteractionSummary {
        grammar_path: grammar_path.to_string(),
        required_total: report.required.len(),
        covered,
        uncovered,
        contains_unsupported,
        retired: report.retired.len(),
        unexpected_tuples: report.unexpected_tuples.len(),
        rows,
    }
}

fn build_headline(ledger: &CoverageLedger, disp: &DispositionCounts) -> String {
    let mappable = ledger
        .rows
        .iter()
        .filter(|r| r.conformance_status != CoverageStatus::Unmappable)
        .count();
    let covered = ledger
        .rows
        .iter()
        .filter(|r| r.conformance_status == CoverageStatus::Covered)
        .count();
    let unmappable = disp.total - mappable;

    if disp.fail_closed == 0 && disp.config_predicate == 0 && covered == mappable && unmappable == 0 {
        format!(
            "FULL HC coverage: all {} constructs are Proven/ConfirmOnly (no FailClosed/ConfigPredicate \
             gap), and every construct maps to a conformance construct id covered by a passing fixture.",
            disp.total
        )
    } else {
        format!(
            "NOT full HC coverage: {}/{} constructs Proven, {} ConfirmOnly (recall-preserving via \
             confirm, not admission-proven), {} ConfigPredicate (compiles only when a registered \
             predicate proves the specific configuration observed), {} FailClosed (refused by default; \
             ADR 0005 override only). Conformance mapping: {}/{} constructs Covered by a passing \
             fixture, {} constructs Unmappable (no constructs.txt id exists for them at all).",
            disp.proven,
            disp.total,
            disp.confirm_only,
            disp.config_predicate,
            disp.fail_closed,
            covered,
            mappable,
            unmappable,
        )
    }
}

#[derive(Serialize)]
struct CoverageSummary {
    schema_version: u32,
    headline: String,
    disposition_counts: DispositionCounts,
    evidence_counts: EvidenceCounts,
    supported_conformance_cross_check: Vec<SupportedConformanceRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_interaction: Option<PlanInteractionSummary>,
    ledger: CoverageLedger,
}

fn build_summary(grammar: Option<(&str, &Grammar)>) -> CoverageSummary {
    let registry = default_registry();
    let covered = passing_covered_constructs();
    let covered_refs: HashSet<&str> = covered.iter().map(String::as_str).collect();
    let ledger = build_ledger(&registry, &covered_refs);

    let disposition_counts = compute_disposition_counts(&ledger);
    let evidence_counts = compute_evidence_counts(&ledger);
    let supported_conformance_cross_check = supported_conformance_cross_check(&ledger);
    let headline = build_headline(&ledger, &disposition_counts);
    let plan_interaction = grammar.map(|(path, g)| plan_interaction_summary(path, g));

    CoverageSummary {
        schema_version: COVERAGE_CLI_SCHEMA_VERSION,
        headline,
        disposition_counts,
        evidence_counts,
        supported_conformance_cross_check,
        plan_interaction,
        ledger,
    }
}

fn render_human(summary: &CoverageSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "pangloss coverage (schema v{})\n\n{}\n\n",
        summary.schema_version, summary.headline
    ));

    let d = &summary.disposition_counts;
    out.push_str("Disposition counts:\n");
    out.push_str(&format!("  Proven:          {}\n", d.proven));
    out.push_str(&format!("  ConfirmOnly:     {}\n", d.confirm_only));
    out.push_str(&format!("  ConfigPredicate: {}\n", d.config_predicate));
    out.push_str(&format!("  FailClosed:      {}\n", d.fail_closed));
    out.push_str(&format!("  Total:           {}\n\n", d.total));

    let e = &summary.evidence_counts;
    out.push_str("Evidence counts:\n");
    out.push_str(&format!(
        "  Rows with a discharging predicate:           {}/{}\n",
        e.rows_with_discharging_predicate, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows with curated containment-test evidence: {}/{}\n",
        e.rows_with_containment_evidence, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows mapped to a conformance construct id:   {}/{}\n",
        e.rows_mapped_to_conformance_construct, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows conformance-Covered by a passing fixture: {}/{}\n",
        e.rows_conformance_covered, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows Unmappable (no constructs.txt id exists): {}\n\n",
        e.rows_unmappable
    ));

    out.push_str("Supported (Proven) constructs -- ADR 0001 conformance cross-check:\n");
    for row in &summary.supported_conformance_cross_check {
        out.push_str(&format!(
            "  {:?}: {:?} (construct ids: {:?})\n",
            row.kind, row.status, row.construct_ids
        ));
    }
    out.push('\n');

    out.push_str("Full per-construct ledger:\n");
    for row in &summary.ledger.rows {
        let preds: Vec<&str> = row
            .discharging_predicates
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        let containment = row
            .containment
            .as_ref()
            .map(|c| c.citation.as_str())
            .unwrap_or("(none -- honest gap)");
        out.push_str(&format!(
            "  {:?}: disposition={:?} predicates={:?} conformance={:?} construct_ids={:?}\n    \
             containment: {}\n",
            row.kind, row.disposition, preds, row.conformance_status, row.construct_ids, containment
        ));
    }

    match &summary.plan_interaction {
        Some(pi) => {
            out.push_str(&format!(
                "\nPlan-node interaction coverage (grammar: {}):\n  required={} covered={} \
                 uncovered={} contains_unsupported={} retired={} unexpected_tuples={}\n",
                pi.grammar_path,
                pi.required_total,
                pi.covered,
                pi.uncovered,
                pi.contains_unsupported,
                pi.retired,
                pi.unexpected_tuples
            ));
            for row in &pi.rows {
                out.push_str(&format!(
                    "    {}: {} (tags: {:?})\n",
                    row.tuple, row.status, row.tags
                ));
            }
        }
        None => {
            out.push_str(
                "\nPlan-node interaction coverage: NOT COMPUTED -- no --grammar=<path> was given, \
                 so this section is honestly omitted rather than fabricated.\n",
            );
        }
    }

    out
}

/// `pangloss coverage [--json] [--grammar=<path>] [<out.json>]` — see this module's top doc for the
/// full contract. `<out.json>` omitted and `--json` unset prints the human-readable summary to
/// stdout; `--json` (with no `<out.json>`) prints the same canonical JSON to stdout instead;
/// `<out.json>` given always writes the canonical JSON there (mirroring `fst-health`'s own
/// stdout-vs-file convention) regardless of `--json`.
pub fn run_coverage(args: &[String]) -> Result<(), String> {
    let mut json = false;
    let mut grammar_path: Option<String> = None;
    let mut out_path: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--grammar" => {
                let v = it.next().ok_or("--grammar requires a value")?;
                grammar_path = Some(v.clone());
            }
            s if s.starts_with("--grammar=") => {
                grammar_path = Some(s["--grammar=".len()..].to_string());
            }
            s => {
                if out_path.is_some() {
                    return Err(format!(
                        "usage: coverage [--json] [--grammar=<path>] [<out.json>]; unexpected extra \
                         argument: {s}"
                    ));
                }
                out_path = Some(s.to_string());
            }
        }
    }

    let loaded_grammar = match &grammar_path {
        Some(path) => {
            let (grammar, warnings) = crate::load_grammar(path)?;
            crate::print_grammar_warnings(&warnings);
            Some((path.clone(), grammar))
        }
        None => None,
    };

    let summary = build_summary(
        loaded_grammar
            .as_ref()
            .map(|(path, g)| (path.as_str(), g)),
    );

    match &out_path {
        Some(path) => {
            let json_str = serde_json::to_string_pretty(&summary)
                .map_err(|e| format!("serialize coverage summary: {e}"))?;
            fs::write(path, &json_str).map_err(|e| format!("write {path}: {e}"))?;
            eprintln!(
                "coverage: wrote {path} ({} construct rows, {} conformance-covered)",
                summary.disposition_counts.total, summary.evidence_counts.rows_conformance_covered
            );
        }
        None if json => {
            let json_str = serde_json::to_string_pretty(&summary)
                .map_err(|e| format!("serialize coverage summary: {e}"))?;
            println!("{json_str}");
        }
        None => {
            print!("{}", render_human(&summary));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_summary_json_round_trips_and_counts_match_the_ledger() {
        let summary = build_summary(None);
        let json = serde_json::to_string_pretty(&summary).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["schema_version"], COVERAGE_CLI_SCHEMA_VERSION);

        // No independent recount: every count must equal a direct tally over the embedded ledger's
        // own rows (the exact same `summary.ledger` this JSON also carries).
        let ledger = &summary.ledger;
        let recount = compute_disposition_counts(ledger);
        assert_eq!(recount.proven, summary.disposition_counts.proven);
        assert_eq!(recount.confirm_only, summary.disposition_counts.confirm_only);
        assert_eq!(
            recount.config_predicate,
            summary.disposition_counts.config_predicate
        );
        assert_eq!(recount.fail_closed, summary.disposition_counts.fail_closed);
        assert_eq!(recount.total, summary.disposition_counts.total);
        assert_eq!(recount.total, CharacteristicKind::ALL.len());

        let recount_evidence = compute_evidence_counts(ledger);
        assert_eq!(
            recount_evidence.rows_with_discharging_predicate,
            summary.evidence_counts.rows_with_discharging_predicate
        );
        assert_eq!(
            recount_evidence.rows_with_containment_evidence,
            summary.evidence_counts.rows_with_containment_evidence
        );
        assert_eq!(
            recount_evidence.rows_mapped_to_conformance_construct,
            summary.evidence_counts.rows_mapped_to_conformance_construct
        );
        assert_eq!(
            recount_evidence.rows_conformance_covered,
            summary.evidence_counts.rows_conformance_covered
        );
        assert_eq!(
            recount_evidence.rows_unmappable,
            summary.evidence_counts.rows_unmappable
        );

        // supported_conformance_cross_check must be exactly the Proven subset of the SAME ledger.
        let proven_in_ledger = ledger
            .rows
            .iter()
            .filter(|r| r.disposition == Disposition::Proven)
            .count();
        assert_eq!(
            proven_in_ledger,
            summary.supported_conformance_cross_check.len()
        );

        // Plan-interaction section must be honestly absent when no grammar was supplied.
        assert!(value.get("plan_interaction").is_none() || value["plan_interaction"].is_null());

        // Full round trip: parsing back into serde_json::Value and re-serializing the original
        // struct must be stable (same content, not necessarily the same Value object identity).
        let json2 = serde_json::to_string_pretty(&summary).expect("serialize again");
        assert_eq!(json, json2, "serialization must be deterministic");
    }

    #[test]
    fn render_human_mentions_headline_and_every_kind() {
        let summary = build_summary(None);
        let text = render_human(&summary);
        assert!(text.contains(&summary.headline));
        for &kind in CharacteristicKind::ALL {
            assert!(
                text.contains(&format!("{kind:?}")),
                "human summary must mention {kind:?}"
            );
        }
    }

    #[test]
    fn plan_interaction_is_included_with_a_grammar_and_omitted_without() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CoverageCliFixture</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>at</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>at</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = pg_grammar::load(XML).expect("fixture grammar must load");

        let without = build_summary(None);
        assert!(without.plan_interaction.is_none());

        let with = build_summary(Some(("coverage-cli-fixture", &g)));
        assert!(with.plan_interaction.is_some());
        let pi = with.plan_interaction.unwrap();
        assert_eq!(pi.grammar_path, "coverage-cli-fixture");
        assert_eq!(pi.required_total, 7, "must report all 7 documented legal adjacency tuples");
    }
}
