//! `pangloss diagnose` — the report schema and CLI skeleton for
//! `openspec/changes/add-grammar-diagnostics`.
//!
//! # STAGING rework (read this before the rest of this module's doc)
//! `openspec/changes/STAGING.md`'s "Downstream — post-multi-topology, pre-ship" section reworks
//! this change down to two concrete pieces, since its own `design.md`/`tasks.md` depend on several
//! changes that have not merged yet (`define-grammar-coverage-contract`'s Complete/Truncated
//! per-word contract, `harden-foma-resource-safety`'s cumulative batch budgets and worker
//! watchdog, `certify-four-language-matrix`'s comparison plumbing): *"fix the apply-path
//! containment to ADR 0003's in-process cooperative magnitude budgets (not 'the watchdog', which
//! is compile-only)"*. This module implements exactly that plus the additive report schema/CLI
//! skeleton the fix needs to be reportable at all — see "What this module defers" below for the
//! larger pieces of `tasks.md` deliberately left as follow-on work.
//!
//! # (a) Apply-path containment: ADR 0003, not a watchdog
//! `docs/adr/0003-apply-time-containment.md`: apply-time (per-word) analysis runs **in-process**
//! and is contained by **deterministic cooperative magnitude budgets**, never a watchdog — a
//! watchdog is `harden-foma-resource-safety`'s COMPILE-time-only mechanism (one killable worker
//! per grammar compile), and apply runs constantly, per word, in the caller's own process, where a
//! native thread cannot be safely hard-killed. This module's [`assess_words`] measures the
//! production foma pipeline's apply path through
//! [`pg_foma::analyzer::FomaProposer::propose_budgeted`] against an explicit
//! [`pg_foma::compose_budget::ApplyBudget`] (that method's own module, `compose_budget.rs`'s
//! "Apply-path dimension" section, is this same change's other half of the fix) and records
//! [`WordApplyStatus::Incomplete`] — naming the tripped dimension, value, and limit — exactly the
//! ADR 0003 contract: *"a word either completes ... or returns a typed incomplete outcome naming
//! the dimension and value it hit"*.
//!
//! # (b) Reused gloss-signature emission unit — no duplicate schema
//! Every [`WordAssessmentEntry::gloss_signature`] is produced by calling
//! `pg_realize::word_gloss_signature` (`rust/crates/pg-realize/src/signature.rs`, Stage 0E of
//! `openspec/changes/add-reference-hermitcrab-parity`) directly — this module never re-renders a
//! gloss chain or re-implements the tagged `g:`/`m:`/`|s:` encoding itself. Health findings, when
//! any exist, are `pg_foma::health::HealthReport` values verbatim (Stage 0D of
//! `define-fst-compilation-health`, R6 — `openspec/changes/IMPLEMENTATION-READINESS.md`), never a
//! parallel report shape.
//!
//! # What this module defers (documented follow-on, per this change's own STAGING scope)
//! - **Health findings are always empty today.** `pg_foma::health`'s own module doc: "purely
//!   additive... a later change wires a real evaluator." [`build_report`] embeds a real, empty
//!   [`pg_foma::health::HealthReport`] (never a duplicate/placeholder type) so this report's shape
//!   never has to change when that evaluator lands — it will simply start populating findings.
//! - **No default-engine (`pg_parse::Morpher`-only) comparison pipeline, no golden-identity diff,
//!   no build/assessment report-to-report comparison** (`tasks.md` 2.5–2.11): those need the
//!   coverage contract's canonical analysis-identity/Complete-Truncated types, which have not
//!   landed (`define-grammar-coverage-contract`'s own tasks are all unchecked as of this writing).
//!   Inventing a substitute contract here would violate design.md D2 ("Diagnostics serializes
//!   these values but does not define substitute caps or certification rules").
//! - **No cumulative/batch-level resource policy, no worker watchdog integration**
//!   (`harden-foma-resource-safety`, Stage 0C): every one of that change's tasks is unchecked. ADR
//!   0003 itself is explicit that a watchdog would be the wrong tool for the apply path this
//!   module measures in the first place (see "(a)" above) — the batch-level POLICY that change
//!   would add (aggregate batch budgets, cooperative cancellation) is a real, separate, larger
//!   piece of work this module does not attempt.
//! - **No `glosses.tsv`/`debug.jsonl`, no PowerShell `scripts/diagnose.ps1`, no `incoming/`
//!   convention, no CI smoke fixture, no `.claude/skills/grammar-diagnostic/`** (`tasks.md`
//!   3.2–3.5, 4.1–4.5): all additive presentation/orchestration layered on top of the report
//!   schema this module defines; none of it changes the schema's own shape, so it is safe to add
//!   later without another schema revision.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use pg_foma::analyzer::FomaProposer;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::compose_budget::{ApplyBudget, ApplyOutcome};
use pg_foma::health::HealthReport;
use pg_grammar::model::Grammar;

/// This module's own schema version, written into every [`BuildReport`]/[`AssessmentReport`].
/// Bump only on a wire-incompatible change to either type (mirrors `pg_foma::health`'s
/// `HEALTH_SCHEMA_VERSION` convention one crate over).
pub const DIAGNOSTICS_SCHEMA_VERSION: u32 = 1;

/// D1 (design.md): "separate immutable `build.json` and `assessment.json`" — this is the
/// build-side report, produced once per grammar load, independent of any word list. Compilation
/// may stay entirely in memory (design.md D1): nothing here requires a `.pgpack` to exist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildReport {
    pub schema_version: u32,
    /// `Grammar::name`, verbatim (the loader's own optional `<Name>`/snapshot field — this report
    /// never invents a name when the grammar declares none).
    pub grammar_name: Option<String>,
    /// `Grammar::entries.len()` — the lexical-entry count, part of this report's own lightweight
    /// build-identity fingerprint (design.md D1: "record grammar/network fingerprint" — a full
    /// content hash is future work; these three counts are the honest, cheap fingerprint available
    /// today without adding a hashing dependency to this crate).
    pub lex_entry_count: usize,
    pub morpheme_count: usize,
    pub stratum_count: usize,
    /// [`crate::load_grammar`]'s own compile/import warnings (dangling refs, unsupported
    /// constructs) — printed to stderr by every other subcommand already
    /// ([`crate::print_grammar_warnings`]); recorded here too so a `build.json` consumer has them
    /// without re-running the load.
    pub load_warnings: Vec<String>,
    /// `pg_foma::health::HealthReport` verbatim (this module's own top doc, "(b)" / "What this
    /// module defers") — always empty today (no evaluator exists yet anywhere in this workspace),
    /// never a duplicate schema.
    pub health: HealthReport,
}

/// Builds a [`BuildReport`] from an already-loaded grammar plus [`crate::load_grammar`]'s own
/// warnings. Pure (no I/O) so it is directly unit-testable against a synthetic fixture.
pub fn build_report(grammar: &Grammar, load_warnings: Vec<String>) -> BuildReport {
    BuildReport {
        schema_version: DIAGNOSTICS_SCHEMA_VERSION,
        grammar_name: grammar.name.clone(),
        lex_entry_count: grammar.entries.len(),
        morpheme_count: grammar.morphemes.len(),
        stratum_count: grammar.strata.len(),
        load_warnings,
        health: HealthReport::new(Vec::new()),
    }
}

/// D5 (design.md): "Diagnostic evidence is observational" — this report never infers construct
/// support, corpus recall, or supported-language status from these two outcomes. Mirrors ADR
/// 0003's own two-way split ("a word either completes ... or returns a typed incomplete outcome").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WordApplyStatus {
    /// [`pg_foma::compose_budget::ApplyOutcome::Complete`]: the apply-path budget never tripped
    /// for this word. A zero-candidate result is still `Complete` (ADR 0003: "Zero analyses is a
    /// valid complete result", the same convention one layer up from confirmed analyses).
    Complete,
    /// [`pg_foma::compose_budget::ApplyOutcome::Incomplete`]: the in-process cooperative magnitude
    /// budget tripped for this word. `dimension` is
    /// `pg_foma::compose_budget::ApplyDimension::label()`'s own stable string (`"decoded apply_up
    /// paths"` / `"distinct candidates"`) — this module never re-derives or re-labels it.
    Incomplete {
        dimension: String,
        value: u64,
        limit: u64,
    },
}

/// One word's assessment-report row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordAssessmentEntry {
    pub word: String,
    /// ADR 0003's apply-path containment outcome, measured independently of whether the
    /// production `pg_foma::composite::FomaAnalyzer` pipeline below happened to complete for this
    /// word (see [`assess_words`]'s own doc for why these are measured separately).
    pub apply_status: WordApplyStatus,
    /// `pg_foma::composite::FomaOutcome::candidates_generated` — distinct propose∪peel candidates
    /// offered to confirm, verbatim from the existing producer.
    pub candidates_generated: usize,
    /// `pg_foma::composite::FomaOutcome::confirmed` — `structured.len()`, verbatim.
    pub confirmed: usize,
    /// `pg_realize::word_gloss_signature` verbatim — this module's own top doc "(b)". `"-"` for a
    /// zero-analysis word, by that function's own documented convention.
    pub gloss_signature: String,
}

/// Aggregate counts across [`AssessmentReport::entries`] — cheap, honest evidence (D5: purely
/// observational, no derived quality score).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssessmentSummary {
    pub complete: u64,
    pub incomplete: u64,
}

/// D1: the word-run-side report, always separate from [`BuildReport`] — "Build comparison
/// consumes build reports; semantic comparison consumes assessment reports only" (design.md D1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssessmentReport {
    pub schema_version: u32,
    pub grammar_name: Option<String>,
    /// The pipeline this assessment measured. Always `"foma-composite"` today — the active
    /// production path design.md's own Context section names (`FomaAnalyzer -> FomaProposer ->
    /// emit_with_budget -> fsm_lexc_parse_string -> apply_up -> confirm_batch`). A default-engine
    /// (`pg_parse::Morpher`-only) pipeline label is documented follow-on (this module's top doc).
    pub engine: String,
    pub entries: Vec<WordAssessmentEntry>,
    pub summary: AssessmentSummary,
}

/// Assesses `words` against `grammar`'s production foma pipeline, applying `apply_budget` to each
/// word's apply-path traversal (ADR 0003; this module's own top-doc "(a)").
///
/// Builds TWO compiled foma artifacts for `grammar` — one inside the [`FomaAnalyzer`] this
/// function uses for the real propose→confirm pipeline (reusing that existing producer rather
/// than re-implementing candidate/confirm wiring, per this change's own hard rule), and one
/// standalone [`FomaProposer`] used only to measure [`FomaProposer::propose_budgeted`] against
/// `apply_budget`. This is deliberate, not an oversight: `FomaAnalyzer` does not expose its own
/// internal proposer for injected-budget use (and should not, for this additive step — that field
/// is `composite.rs`'s own, and `composite.rs` is a named single-owner merge hotspot,
/// `openspec/changes/STAGING.md`'s "Merge hotspots" list), so this diagnostic measures the SAME
/// grammar's apply path independently rather than reaching into that struct's private state. The
/// one-time cost of a second compiled network is acceptable for an offline diagnostic command;
/// nothing here is a hot path.
pub fn assess_words(
    grammar: &Grammar,
    words: &[String],
    apply_budget: &ApplyBudget,
) -> Result<AssessmentReport, String> {
    let mut analyzer =
        FomaAnalyzer::new(grammar).map_err(|e| format!("foma analyzer build failed: {e}"))?;
    let mut budgeted_proposer =
        FomaProposer::new(grammar).map_err(|e| format!("foma proposer build failed: {e}"))?;

    let mut entries = Vec::with_capacity(words.len());
    let mut summary = AssessmentSummary::default();

    for word in words {
        let apply_status = match budgeted_proposer.propose_budgeted(word, apply_budget) {
            ApplyOutcome::Complete(_) => {
                summary.complete += 1;
                WordApplyStatus::Complete
            }
            ApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                summary.incomplete += 1;
                WordApplyStatus::Incomplete {
                    dimension: dimension.label().to_string(),
                    value: value as u64,
                    limit: limit as u64,
                }
            }
        };

        let outcome = analyzer.analyze_word(word);
        let pairs: Vec<(pg_parse::WordAnalysis, String)> = outcome
            .structured
            .iter()
            .cloned()
            .zip(outcome.analyses.iter().map(|(_join, surface)| surface.clone()))
            .collect();
        let gloss_signature = pg_realize::word_gloss_signature(grammar, &pairs);

        entries.push(WordAssessmentEntry {
            word: word.clone(),
            apply_status,
            candidates_generated: outcome.candidates_generated,
            confirmed: outcome.confirmed,
            gloss_signature,
        });
    }

    Ok(AssessmentReport {
        schema_version: DIAGNOSTICS_SCHEMA_VERSION,
        grammar_name: grammar.name.clone(),
        engine: "foma-composite".to_string(),
        entries,
        summary,
    })
}

/// `pangloss diagnose <grammar> <words.txt> <out-dir>` (tasks.md 1.1, reworked scope — see this
/// module's top doc): loads `grammar` via [`crate::load_grammar`] (the existing `.xml`/`.json`/
/// `.fwdata` extension dispatch every other subcommand already uses), reads one word per
/// non-empty trimmed line from `words.txt` (the same convention `run_batch` already uses), and
/// writes `<out-dir>/build.json` and `<out-dir>/assessment.json` — always both, always separate
/// files (D1), never a combined artifact. `<out-dir>` is created if missing.
pub fn run_diagnose(args: &[String]) -> Result<(), String> {
    let [grammar_path, words_path, out_dir] = args else {
        return Err("usage: diagnose <grammar> <words.txt> <out-dir>".to_string());
    };

    let (grammar, load_warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&load_warnings);

    let words: Vec<String> = fs::read_to_string(words_path)
        .map_err(|e| format!("read {words_path}: {e}"))?
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();

    let build = build_report(&grammar, load_warnings);
    let assessment = assess_words(&grammar, &words, &ApplyBudget::from_env())?;

    fs::create_dir_all(out_dir).map_err(|e| format!("create {out_dir}: {e}"))?;
    let build_path = Path::new(out_dir).join("build.json");
    let assessment_path = Path::new(out_dir).join("assessment.json");

    fs::write(
        &build_path,
        serde_json::to_string_pretty(&build).map_err(|e| format!("serialize build.json: {e}"))?,
    )
    .map_err(|e| format!("write {}: {e}", build_path.display()))?;
    fs::write(
        &assessment_path,
        serde_json::to_string_pretty(&assessment)
            .map_err(|e| format!("serialize assessment.json: {e}"))?,
    )
    .map_err(|e| format!("write {}: {e}", assessment_path.display()))?;

    eprintln!(
        "diagnose complete: {} words ({} complete, {} incomplete) -> {}",
        assessment.entries.len(),
        assessment.summary.complete,
        assessment.summary.incomplete,
        out_dir
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic (delanguaged), tiny fixture: one bare root `eRoot` (gloss `GX`, surface `kal`) and
    /// one affixed pair `eBare` (no gloss, root, surface `tuz`) + `eAff` (`<MorphemeId>M7</MorphemeId>`,
    /// suffix `+i`, surface `i`) via a trivial suffixing rule -- just enough ambiguity/gloss variety
    /// to exercise both `WordApplyStatus` variants and both `g:`/`m:` gloss-signature tags in one
    /// small grammar. Invented spellings only (repo-wide synthetic-fixture convention).
    const FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DiagnosticsFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot" partOfSpeech="n">
            <Allomorphs><Allomorph id="eRoot-1"><PhoneticShape>kal</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>GX</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eBare" partOfSpeech="n">
            <Allomorphs><Allomorph id="eBare-1"><PhoneticShape>tuz</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn grammar() -> Grammar {
        pg_grammar::load(FIXTURE_XML)
            .unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    #[test]
    fn build_report_reflects_grammar_counts_and_carries_an_empty_health_report() {
        let g = grammar();
        let report = build_report(&g, vec!["warn: synthetic".to_string()]);
        assert_eq!(report.schema_version, DIAGNOSTICS_SCHEMA_VERSION);
        assert_eq!(report.grammar_name.as_deref(), Some("DiagnosticsFixture"));
        assert_eq!(report.lex_entry_count, 2);
        assert_eq!(report.stratum_count, 1);
        assert_eq!(report.load_warnings, vec!["warn: synthetic".to_string()]);
        assert_eq!(
            report.health,
            HealthReport::new(Vec::new()),
            "no evaluator exists yet anywhere in this workspace; the health report must stay \
             honestly empty rather than fabricate a finding"
        );
    }

    #[test]
    fn assess_words_reports_complete_status_and_reuses_the_gloss_signature_unit() {
        let g = grammar();
        let words = vec!["kal".to_string(), "tuz".to_string(), "zzzz".to_string()];
        let report =
            assess_words(&g, &words, &ApplyBudget::unbounded()).expect("assessment must succeed");

        assert_eq!(report.engine, "foma-composite");
        assert_eq!(report.entries.len(), 3);
        assert_eq!(report.summary.complete, 3, "an unbounded budget never trips");
        assert_eq!(report.summary.incomplete, 0);

        let kal = &report.entries[0];
        assert_eq!(kal.apply_status, WordApplyStatus::Complete);
        assert_eq!(kal.confirmed, 1);
        assert_eq!(
            kal.gloss_signature,
            r#"g:"GX"|s:"kal""#,
            "must match pg_realize::signature's own documented encoding exactly, not a re-derived one"
        );

        let tuz = &report.entries[1];
        assert_eq!(tuz.confirmed, 1);
        assert_eq!(tuz.gloss_signature, r#"m:""|s:"tuz""#);

        let unmatched = &report.entries[2];
        assert_eq!(unmatched.confirmed, 0);
        assert_eq!(
            unmatched.gloss_signature, "-",
            "zero confirmed analyses must render the same '-' literal pg_realize::signature \
             documents for an empty entry set"
        );
    }

    #[test]
    fn assess_words_reports_incomplete_status_when_the_apply_budget_trips() {
        let g = grammar();
        let words = vec!["kal".to_string()];
        // cap=0 on decoded paths: the bare root must still decode at least one apply_up result,
        // tripping the budget on the very first one -- deterministic, in-process, no watchdog.
        let budget = ApplyBudget::with_caps(Some(0), None);
        let report = assess_words(&g, &words, &budget).expect("assessment must succeed");

        assert_eq!(report.summary.incomplete, 1);
        assert_eq!(report.summary.complete, 0);
        match &report.entries[0].apply_status {
            WordApplyStatus::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, "decoded apply_up paths");
                assert_eq!(*value, 1);
                assert_eq!(*limit, 0);
            }
            other => panic!("expected Incomplete, got {other:?}"),
        }
    }

    #[test]
    fn word_apply_status_golden_json() {
        // Pins the wire shape of both variants directly -- this is the schema every downstream
        // report consumer (and any future report-to-report comparison) depends on.
        let complete = WordApplyStatus::Complete;
        assert_eq!(
            serde_json::to_string(&complete).expect("serialize"),
            r#"{"kind":"complete"}"#
        );

        let incomplete = WordApplyStatus::Incomplete {
            dimension: "decoded apply_up paths".to_string(),
            value: 1,
            limit: 0,
        };
        assert_eq!(
            serde_json::to_string(&incomplete).expect("serialize"),
            r#"{"kind":"incomplete","dimension":"decoded apply_up paths","value":1,"limit":0}"#
        );
    }

    /// Golden full-report test (tasks.md 1.4's "golden schema" test, scoped to this step's
    /// actual shape): a deterministic, fully-populated [`AssessmentReport`] round-trips through
    /// canonical JSON losslessly and matches a committed golden string exactly. Catches accidental
    /// field reordering/renaming/type drift the same way `pg_foma::health`'s own golden test does.
    #[test]
    fn assessment_report_golden_json_round_trips() {
        let g = grammar();
        let words = vec!["kal".to_string()];
        let report =
            assess_words(&g, &words, &ApplyBudget::unbounded()).expect("assessment must succeed");
        let json = serde_json::to_string_pretty(&report).expect("serialize");

        const GOLDEN: &str = r#"{
  "schema_version": 1,
  "grammar_name": "DiagnosticsFixture",
  "engine": "foma-composite",
  "entries": [
    {
      "word": "kal",
      "apply_status": {
        "kind": "complete"
      },
      "candidates_generated": 1,
      "confirmed": 1,
      "gloss_signature": "g:\"GX\"|s:\"kal\""
    }
  ],
  "summary": {
    "complete": 1,
    "incomplete": 0
  }
}"#;
        assert_eq!(json, GOLDEN, "canonical JSON drifted from the committed golden");

        let parsed: AssessmentReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, report, "round trip through canonical JSON must be lossless");
    }
}
