//! `FomaProposer`: the thin `emit + foma-compile + apply-up` wrapper (plan §1's "propose" half of
//! propose→confirm; confirm itself is P2's job, not built here).
//!
//! Compiles [`crate::emit::emit`]'s lexc source with the pure-Rust `foma` crate (gate F0) and
//! exposes [`FomaProposer::propose`]: normalize the query word the SAME way [`crate::emit`]
//! normalized surface text (NFD — see that module's doc), `apply_up` it, decode every resulting
//! tag path, and split each into [`tags::Candidate`]s, deduped by `(morphemes, root_index)`
//! preserving first-seen order (matching the propose→verify contract, plan §2: "Allomorph IDs are
//! NOT part of candidate identity").

use std::collections::HashSet;
use std::fmt;

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::structures::fsm_sort_arcs;
use foma::types::ApplyHandle;

use pg_grammar::model::Grammar;

use crate::emit::{self, EmitReport};
use crate::tags::{self, Candidate};

/// Errors constructing a [`FomaProposer`]. Deliberately small (this stage doesn't need a rich
/// error hierarchy) — a grammar whose foma path fails to compile should fall back to the full
/// engine (plan §1's per-grammar tiering), which only needs to know THAT it failed.
#[derive(Debug)]
pub enum FomaError {
    /// `fsm_lexc_parse_string` returned `None` — the emitted lexc source failed to compile. Carries
    /// the emitter's own report (uncovered constructs, counts) since that is the first place to
    /// look when this happens.
    LexcCompileFailed(EmitReport),
    /// Fix 1 (fail-fast enumeration budget, `crate::morphotactics::EnumerationBudget`'s own doc):
    /// `emit::emit`'s default-on budget tripped before a usable lexc source could even be built —
    /// this grammar's morphotactic composite enumeration would have produced far more lexc material
    /// than the eager Rust-side enumerator can safely expand (the Aweti grammar -- 855 roots, 123
    /// rules, 3 strata -- is the motivating case: 2,833,559 fusion entries, a 691MB/9.7M-line lexc,
    /// and an ~8.8GB `apply_up` allocation that killed the process outright). An HONEST,
    /// compiler-gap error, returned immediately -- never a panic, never a silent OOM, never lost
    /// recall for a grammar that would actually have fit.
    EnumerationBudgetExceeded {
        /// Which measure tripped (`crate::morphotactics::EnumMeasure::label`'s text, e.g.
        /// "composite lexc entries (fusion + interdigitation + structural)").
        measure: &'static str,
        /// The measured value at the moment the budget tripped.
        value: usize,
        /// The threshold that was exceeded (the default, or an `HC_ENUM_ENTRY_BUDGET`/
        /// `HC_ENUM_PROBE_BUDGET` override).
        limit: usize,
    },
}

impl fmt::Display for FomaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FomaError::LexcCompileFailed(report) => write!(
                f,
                "foma lexc compile failed (emit report: {} uncovered constructs, tier {:?})",
                report.uncovered.len(),
                report.tier
            ),
            FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
            } => write!(
                f,
                "grammar exceeds the foma-engine's eager-enumeration budget: {measure} = {value} \
                 (limit {limit}). This grammar's morphotactics produce more composite lexc material \
                 than the eager Rust-side enumerator can safely expand into a literal lexc source \
                 without risking a multi-GB `.lexc` file and an out-of-memory crash in foma's own \
                 `apply_up`. Use the default (full) morphological-parser engine for this grammar \
                 instead of the foma-composite engine, or -- only if you understand why this \
                 grammar's dynamic enumeration tree is this large -- raise the budget via \
                 HC_ENUM_ENTRY_BUDGET/HC_ENUM_PROBE_BUDGET and re-run."
            ),
        }
    }
}

impl std::error::Error for FomaError {}

pub type Result<T> = std::result::Result<T, FomaError>;

/// Minimum arc count before `FomaProposer::new` pays `fsm_sort_arcs`'s one-time cost to switch
/// `apply_up`'s per-word traversal from foma's linear arc-scan branch to its binary-search branch
/// (gated on `net.arcs_sorted_out`, apply.rs's `apply_up`/`apply_follow_next_arc`).
///
/// Measured (prototype tracer, `examples/sort_probe.rs`): sorting is a clear win on real grammars
/// — sena (85,763 arcs) 1.49x propose speedup, amharic (177,177 arcs) 2.05x — with traversal-
/// identical results (states-entered and candidate sets identical, sorted vs. unsorted). But on a
/// tiny network (indonesian, 3,263 arcs, ~337 arcs examined/word) the binary-search bookkeeping
/// OUTWEIGHS the win: propose throughput regressed ~30%. This constant gates the sort so small
/// grammars stay on the (cheaper, for them) linear scan while large ones get the binary-search
/// speedup. 10,000 sits comfortably between indonesian's 3,263 (stays unsorted) and sena's 85,763
/// (gets sorted).
const ARC_SORT_MIN_ARCS: i32 = 10_000;

/// The compiled foma network for one grammar (as a live [`ApplyHandle`], see below), plus the
/// emitter's own report (uncovered constructs, counts, tier — plan P1 gate F1's "counts are
/// plausible" assertions read this).
pub struct FomaProposer {
    // Built ONCE in `new` via `apply_init` and reused across every `propose` call (see that
    // method's doc for why this is sound). `ApplyHandle` owns a full clone of the compiled `Fsm`
    // (`foma::apply::apply_init`'s doc: "DEVIATION from C (owns a clone; the handle never mutates
    // it, so observably equivalent for application)") plus its own grammar-static index tables
    // (`apply_create_statemap`/`apply_create_sigarray`, built once inside `apply_init` itself) —
    // it is fully owned/`'static`, not a borrow of any `Fsm` this struct would also need to store,
    // so there is no self-referential-struct trap here: the `Fsm` `fsm_lexc_parse_string` returns
    // is consumed by `apply_init` and can be (is) dropped once the handle exists.
    handle: Box<ApplyHandle>,
    pub report: EmitReport,
}

impl FomaProposer {
    /// Emit `g`'s lexc source, compile it, and build the (word-independent) `ApplyHandle` once.
    /// `Err` iff `foma`'s lexc compiler itself rejects the source (a bug in this crate's emitter,
    /// not a grammar-content problem — the emitter's own `uncovered` list is how grammar CONTENT
    /// gaps are reported, always alongside `Ok`) OR iff Fix 1's default-on enumeration budget
    /// (`crate::morphotactics::EnumerationBudget`'s own doc) trips.
    ///
    /// Thin, env-driven wrapper over [`Self::new_with_budget`] -- same convention
    /// `crate::emit::emit_with_precision` uses for the same reason (its own doc): reads
    /// `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET` exactly once, here, in the production entry
    /// point, so parallel test processes never race process-global env state.
    pub fn new(g: &Grammar) -> Result<Self> {
        let enum_budget = crate::morphotactics::EnumerationBudget::from_env();
        Self::new_with_budget(g, &enum_budget)
    }

    /// [`Self::new`]'s core, with the Fix 1 enumeration budget threaded in explicitly rather than
    /// read from env -- what tests call directly (with a small
    /// [`crate::morphotactics::EnumerationBudget::with_caps`]) to exercise
    /// `FomaError::EnumerationBudgetExceeded` deterministically and fast, without setting
    /// `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET` (this crate's tests never touch that env var,
    /// mirroring `crate::morphotactics::ExploreMode`'s own doc's reasoning for `HC_PREEXPAND_FLAT`).
    pub(crate) fn new_with_budget(
        g: &Grammar,
        enum_budget: &crate::morphotactics::EnumerationBudget,
    ) -> Result<Self> {
        let result = emit::emit_with_budget(g, crate::precision::PrecisionConfig::Strip, enum_budget);
        // Fix 1 (fail-fast enumeration budget): checked FIRST, before ever handing `result.lexc_source`
        // to `fsm_lexc_parse_string` -- when this is `Some`, `emit::emit_with_budget` already bailed
        // out early (its own doc: the budget check sits before the expensive derivation-layer/
        // lexc-string-writing work), so `lexc_source` here is deliberately empty and must never be
        // compiled. This is the ONE typed, honest error this whole mechanism exists to produce: no
        // panic, no silent OOM, and it surfaces to `FomaAnalyzer::new`'s own caller (`composite.rs`)
        // exactly the same way `LexcCompileFailed` already does.
        if let Some(exceeded) = result.report.enum_budget_exceeded {
            return Err(FomaError::EnumerationBudgetExceeded {
                measure: exceeded.measure,
                value: exceeded.value,
                limit: exceeded.limit,
            });
        }
        let opts = FomaOptions::default();
        match fsm_lexc_parse_string(&opts, None, &result.lexc_source) {
            Some(mut net) => {
                // direction 2 = "out": apply_up (propose's entry point) gates its binsearch
                // branch on `net.arcs_sorted_out` (apply.rs's `apply_up`, ~line 469). See
                // `ARC_SORT_MIN_ARCS`'s doc for why this is gated on network size rather than
                // called unconditionally.
                if net.arccount >= ARC_SORT_MIN_ARCS {
                    fsm_sort_arcs(&mut net, 2);
                }
                Ok(FomaProposer {
                    handle: apply_init(&net),
                    report: result.report,
                })
            }
            None => Err(FomaError::LexcCompileFailed(result.report)),
        }
    }

    /// Propose every candidate analysis for `word`. NFD-normalizes first (matching
    /// [`crate::emit::kept_surface_text`]'s own normalization — see that function's doc for why
    /// this must be consistent on both sides regardless of the caller's on-disk encoding).
    /// Dedups by `(morphemes, root_index)`, preserving first-seen order across BOTH the
    /// `apply_up` path order and, within one path, the compound-split order (`tags::to_candidates`
    /// already yields ascending root-position order for a single path).
    ///
    /// Reuses `self.handle` across calls rather than rebuilding it per word (vendored
    /// `foma::apply::apply_init`, ~apply.rs:481-577, unconditionally deep-clones the whole
    /// compiled `Fsm` and rebuilds `apply_create_statemap`/`apply_create_sigarray` — all a
    /// function of the NETWORK only, never the word). The per-word entry point,
    /// `foma::apply::apply_up` (apply.rs:462-475, reached via `ApplyHandle::up`, apply.rs:667-669),
    /// resets only per-word state — `h.instring`, `apply_create_sigmatch` (word-derived sigma
    /// matches), and `apply_force_clear_stack` (apply.rs:424-433's `apply_updown`, the `Some(w)`
    /// arm) — leaving `last_net`/`statemap`/`sigmatch_array`/`sigma_trie` (the grammar-static
    /// tables) untouched, so repeated `up` calls on one handle are exactly the reuse this needs.
    pub fn propose(&mut self, word: &str) -> Vec<Candidate> {
        let normalized = pg_grammar::nfd::nfd(word);
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        for s in self.handle.up(&normalized) {
            let Some(path) = tags::decode_path(&s) else {
                continue;
            };
            for c in tags::to_candidates(&path) {
                let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if seen.insert(key) {
                    out.push(c);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod budget_tests {
    //! Fix 1 regression tests (`docs/fst-plan/morphotactic-composite-pruning.md`'s addendum, "Fix 1:
    //! fail-fast enumeration budget"): the default-on `crate::morphotactics::EnumerationBudget` must
    //! abort `FomaProposer::new`'s build with the typed [`FomaError::EnumerationBudgetExceeded`] --
    //! never a panic, never an unbounded run toward the Aweti-scale blow-up (551s emit, 691MB lexc,
    //! ~8.8GB `apply_up` allocation, process death on the very first word) -- and it must do so FAST.
    //!
    //! These tests inject an explicit, tiny [`crate::morphotactics::EnumerationBudget`] via
    //! [`FomaProposer::new_with_budget`] rather than setting `HC_ENUM_ENTRY_BUDGET`/
    //! `HC_ENUM_PROBE_BUDGET`, mirroring this crate's existing convention for `HC_PREEXPAND_FLAT`/
    //! `HC_PREEXPAND_PROBE_CAP` (`crate::morphotactics::ExploreMode`'s own doc: "tests must construct
    //! ... directly, never call [the env-reading fn], so parallel test threads/processes never race
    //! process-global env state"). This also decouples the test from the exact DEFAULT threshold
    //! numbers (documented and justified separately in `EnumerationBudget`'s own doc) -- it proves
    //! the MECHANISM trips and propagates correctly, fast, regardless of where the default is set.

    use super::*;
    use crate::morphotactics::EnumerationBudget;

    /// Loads the real Aweti grammar (the motivating case for this fix: 855 roots, 123 rules, 3
    /// strata, 14 templates -- see `docs/fst-plan/morphotactic-composite-pruning.md`'s addendum) if
    /// present on disk. `samples/data/aweti.json`/`aweti-words.txt` are gitignored (same convention
    /// as every other real-corpus fixture this crate's gates use, e.g. `preexpand.rs`'s own
    /// `sample_path` helper) -- copy them from the main checkout's `samples/data/` if missing.
    fn load_aweti() -> Option<Grammar> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/data/aweti.json");
        if !path.exists() {
            return None;
        }
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let snapshot = pg_snapshot::Snapshot::from_json(&json)
            .unwrap_or_else(|e| panic!("parse aweti.json snapshot: {e}"));
        let (g, _warnings) = pg_grammar::compile_project(&snapshot)
            .unwrap_or_else(|e| panic!("compile aweti project: {e}"));
        Some(g)
    }

    /// The core regression: a tiny composite-entry cap must trip on Aweti fast (nowhere near the
    /// full 551s/2.8M-entry enumeration) and surface as a typed error, not a panic and not a hang.
    #[test]
    fn aweti_trips_enumeration_budget_fast_with_typed_error() {
        let Some(g) = load_aweti() else {
            eprintln!("skipping: samples/data/aweti.json not present on disk");
            return;
        };
        // Entry cap of 10 composite entries -- far below Amharic's real 22,775 (so a grammar that
        // actually fits stays completely unaffected by the PRODUCTION default; this cap is only
        // ever used here, injected directly) but small enough that Aweti's dense composite tree
        // crosses it almost immediately. Probe cap left effectively unbounded so this test isolates
        // the ENTRY measure specifically (`crate::morphotactics::EnumMeasure::CompositeEntries`) --
        // the one the module doc identifies as the one that actually predicts Aweti's blow-up (a
        // pairs-probed cap alone would not catch it before the artifact-size disaster).
        let budget = EnumerationBudget::with_caps(10, usize::MAX);

        let t0 = std::time::Instant::now();
        let result = FomaProposer::new_with_budget(&g, &budget);
        let elapsed = t0.elapsed();
        eprintln!("aweti tiny-entry-budget trip took {elapsed:?}");

        match result {
            Err(FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
            }) => {
                assert_eq!(limit, 10, "limit must echo back the injected cap");
                assert!(value > limit, "tripped value {value} must exceed the limit {limit}");
                assert_eq!(
                    measure,
                    "composite lexc entries (fusion + interdigitation + structural)",
                    "a tiny entry cap (probe cap unbounded) must trip on the ENTRY measure"
                );
            }
            Err(other) => panic!(
                "expected FomaError::EnumerationBudgetExceeded, got a different FomaError: {other}"
            ),
            Ok(_) => panic!(
                "expected the tiny entry budget (cap=10) to trip on Aweti; \
                 FomaProposer::new_with_budget succeeded instead"
            ),
        }

        // The whole point of a FAIL-FAST budget: this must be nowhere near the ~551s the
        // uncapped Rust-side emit takes on Aweti (module doc). A generous ceiling here still
        // catches a regression that silently disables the early bail-out (e.g. a budget check
        // that got moved to only run once, at the very end).
        assert!(
            elapsed.as_secs() < 120,
            "fail-fast budget should trip in well under the ~551s uncapped runtime, took {elapsed:?}"
        );
    }

    /// The probe-count measure, isolated: an effectively-unlimited entry cap paired with a tiny
    /// probe cap must still trip -- and report the OTHER measure (`PairsProbed`), proving the two
    /// measures are independently wired, not just the entry one (module doc: "budgets on BOTH").
    #[test]
    fn aweti_trips_on_probe_measure_when_entry_cap_is_unbounded() {
        let Some(g) = load_aweti() else {
            eprintln!("skipping: samples/data/aweti.json not present on disk");
            return;
        };
        let budget = EnumerationBudget::with_caps(usize::MAX, 5);

        let t0 = std::time::Instant::now();
        let result = FomaProposer::new_with_budget(&g, &budget);
        let elapsed = t0.elapsed();
        eprintln!("aweti tiny-probe-budget trip took {elapsed:?}");

        match result {
            Err(FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
            }) => {
                assert_eq!(limit, 5);
                assert!(value > limit);
                assert_eq!(measure, "(root, rule) pairs probed");
            }
            Err(other) => panic!(
                "expected FomaError::EnumerationBudgetExceeded, got a different FomaError: {other}"
            ),
            Ok(_) => panic!("expected the tiny probe budget (cap=5) to trip on Aweti"),
        }
        assert!(elapsed.as_secs() < 120, "took {elapsed:?}");
    }

    /// Sanity check the OTHER direction on a tiny, hand-built grammar with no real composite
    /// mechanism at all (no phonological rules, no `Infix` rules -- `should_run` is false): an
    /// unbounded budget must never trip, and `FomaProposer::new_with_budget` must succeed exactly
    /// like plain `FomaProposer::new` would. Guards against an over-eager budget wiring that
    /// spuriously trips on every grammar regardless of scale.
    #[test]
    fn tiny_grammar_never_trips_unbounded_budget() {
        const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MtBudgetSmoke</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>ka</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;
        let g = pg_grammar::load(FIXTURE).unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let budget = EnumerationBudget::unbounded();
        let result = FomaProposer::new_with_budget(&g, &budget);
        assert!(
            result.is_ok(),
            "an unbounded budget must never trip on a tiny, non-composite grammar"
        );
    }
}
