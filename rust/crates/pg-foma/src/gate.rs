//! P6 MPR/POS subrule gating (`docs/fst-plan/foma-fst-plan.md` §P6 item 1 / `docs/fst-plan/
//! p6-prototype-report.md` §3, §6 item 4): closes the recall gap the prototype report named but
//! left open — a [`RewriteSubruleDef`] carrying `requiredPartsOfSpeech`/`requiredMPRFeatures`/
//! `excludedMPRFeatures` was compiled as if unconditional, so a rule that must NOT apply to one
//! root (Indonesian `prule5`'s `excludedMPRFeatures="mpr1"`) or must ONLY apply to POS-restricted
//! roots (Amharic `prule1`/`prule2`'s `requiredPartsOfSpeech`) fired for every root reaching it.
//!
//! ## Why this is NOT a flag-diacritics encoding (a real, load-bearing finding)
//! The obvious foma technique for "gate a rule on a per-root fact" is a flag diacritic: set
//! `@P.MPR1.1@` on the excluded root's lexc entry, test `@D.MPR1@`/`@R.POS.<sym>@` in the gated
//! subrule's own environment. A prototype build of exactly that (throwaway probes, not committed)
//! hit THREE separate toolkit issues in this vendored foma-rs (`=0.1.1`), bisected empirically, in
//! order:
//! 1. **A flag literal embedded in a replace rule's `||` context corrupts the compiled network.**
//!    `t -> 0 || a "@D.MPR1@" _` (or the same shape grouped `[a "@D.MPR1@"] _`) compiles without
//!    error but `apply_up`/`apply_down` return a NONDETERMINISTIC mix of "rule fired" and "rule
//!    didn't fire" paths for the SAME input — the flag test does not gate the replace rule's
//!    obligatory-application machinery the way it gates a plain concatenation regex (proven safe
//!    by `tests/f0_viability.rs`'s F0.3 and `tests/pk2_eliminate_flag_oracle.rs`, both of which
//!    only ever test flags OUTSIDE any `->` construct). A context consisting of JUST a flag literal
//!    (no real segment) additionally **crashed** (`STATUS_STACK_BUFFER_OVERRUN` inside
//!    `vendor/foma/src/minimize.rs`) on `apply_up`. Putting the gate in the LHS/RHS instead of the
//!    context does not help — `->` and flags do not mix safely in this port, full stop.
//! 2. **`fsm_compose` does not treat flag symbols as epsilon-transparent by default.**
//!    `FomaOptions::default().flag_is_epsilon == false` (`vendor/foma/src/options.rs`), and
//!    `fsm_compose`'s own doc comment (`vendor/foma/src/constructions/products.rs`) says why: with
//!    it off, a flag symbol present in one net's sigma but ABSENT from the other's is NOT treated
//!    as "invisible" during the sigma merge — the composed result is empty. Reproduced at the
//!    minimal possible case: `compose([a], [a "@D.MPR1@"])` (a flag-FREE net composed with a
//!    flag-BEARING one, the flag never even set) returns **empty**, not `{a}` (the vacuous-pass
//!    answer both nets alone happily give). Setting `flag_is_epsilon = true` fixes this specific
//!    case — but does NOT fix finding 1 (a flag inside a replace rule's `||` context still
//!    misbehaves/crashes with `flag_is_epsilon` either way).
//! 3. **Ordering is unforgiving and a Kleene-star "shadow the trigger char based on a flag"
//!    workaround (built to route AROUND finding 1 by keeping flags out of any `->` construct) is
//!    itself fragile.** A flag must be SET strictly before the tape position it is tested at
//!    (tape traversal is left-to-right and flag state is exactly "whatever the last `@P@` on this
//!    path assigned" — no different from any other left-to-right automaton read). A first version
//!    of the per-lexc-entry flag literal was appended AFTER the gated segment (mirroring
//!    `precision.rs`'s own y/n convention, which appends because ITS test is a lookahead for the
//!    NEXT entry) and silently never gated anything, because by the time the shadow transform read
//!    the segment, the flag hadn't been set yet on that path. Prepending fixed THAT half, but the
//!    Kleene-star "shadow-if-flagged, else pass through" construction
//!    (`[[c "@D.F@"] | [c:c_shadow "@R.F@"] | \c]*`) still gave wrong answers once composed with a
//!    real lexc net (right in structure alone, verified against a bare hand-built setter net) —
//!    root cause not fully isolated before the scope call below was made. Three toolkit surprises
//!    deep on one technique is itself the signal, not something to keep debugging blind.
//!
//! **Decision (matches this codebase's own "approximate only upward, report don't hide" ethos):**
//! stop fighting the toolkit and use a **static, flag-free partition** instead. It needs zero new
//! foma primitives — only ones already proven in this file's own sibling modules (lexc, plain `->`
//! rules with no flags, [`fsm_compose`], [`fsm_union`]) — and it is provably correct BY
//! CONSTRUCTION rather than by hoping a flag survives composition:
//!
//! ## The static-partition design
//! MPR/POS gating in this prototype's scope is **root-only** (see the caveat below): a lexical
//! entry's own [`pg_grammar::model::LexEntryDef::mpr`] and part of speech are fixed at grammar-load
//! time and never change before the trailing per-stratum phonological-rule cascade runs (the ONLY
//! place `pg_rules::rewrite::subrule_applicable` is ever consulted, `pg-rules/src/stratum.rs`'s
//! `synth_apply_stratum`). So the gate/no-gate decision for every (entry, gated subrule) pair is
//! fully static — computable once in Rust, at compile time, with NO runtime FST mechanism at all:
//!
//! 1. [`find_gated_subrules`] scans `prules_in_order` for every subrule declaring a nontrivial
//!    `required_pos`/`required_mpr`/`excluded_mpr` (Indonesian: exactly 1, `prule5`'s own subrule;
//!    the synthetic POS fixture: exactly 1).
//! 2. [`entry_gate_key`] computes, per lexical entry, the vector of booleans "is this gated subrule
//!    applicable to this entry" — by calling `pg_rules::rewrite::subrule_applicable` DIRECTLY (now
//!    `pub`, see that function's doc), the exact function the real engine's trailing-prule cascade
//!    calls. This is the guarantee this design rests on: the partition can never disagree with the
//!    oracle about which entries are gated, because it IS the oracle's own predicate, not a
//!    re-derivation of MPR-group All/Any semantics or the POS "unset = vacuous pass" rule.
//! 3. [`partition_entries`] groups all entries by that key (a `HashMap<Vec<bool>, HashSet<LexEntryId>>`
//!    collapsed to a `Vec`). Two test cases ⇒ 2 groups each; worst case is bounded by
//!    `2^(#gated subrules)`, but in practice bounded by the number of DISTINCT gating vectors
//!    actually realized by the grammar's own entries (≤ `#entries`), an honest, measurable
//!    per-grammar quantity — matches this repo's "keep old paths, measure don't guess" convention
//!    (`docs/fst-plan/foma-fst-plan.md`'s keep-old-paths directive) rather than a blind assumption
//!    it always stays small.
//! 4. [`compile_gated_grammar`] builds ONE composed network PER GROUP: [`crate::uflexc::
//!    emit_underlying_filtered`] restricted to that group's entries (affix chains unfiltered — see
//!    their own doc), [`crate::replace::compile_and_compose_rules_gated`] with every gated
//!    subrule's inclusion decided by that group's own key (ungated subrules always included), then
//!    `lexc_group .o. rules_group`. The per-group nets are then [`fsm_union`]ed into one final
//!    network.
//!
//! **Why the union is safe here** (the report's own §2.2 warning about union-of-complete-replace-
//! nets does NOT apply): that warning was about unioning several REPLACE-RULE nets that all
//! accept the SAME underlying alphabet and could each spuriously supply an "elsewhere identity"
//! path for a position some OTHER branch's context legitimately owns. Here, each group's ENTIRE
//! network (lexc included) only accepts underlying strings built from THAT group's own entries —
//! groups are lexically disjoint by construction (every entry belongs to exactly one partition), so
//! there is no shared input on which two groups' nets could disagree; the union is a plain,
//! ordinary disjoint union of languages, not a semantic hazard.
//!
//! **Why assimilation-before-deletion (prule4 → prule5) still works**: unlike the abandoned
//! shadow-token approach, prule4 is compiled UNCHANGED, over the REAL alphabet, in EVERY group
//! (prule4 has no gating at all) — nothing about a root's underlying spelling is altered before it
//! reaches prule4, so nasal-place assimilation fires identically in both groups; only prule5's
//! excluded-group cascade omits prule5's own (sole) subrule.
//!
//! ## Caveats (named, not hidden)
//! - **Root-only.** `AffixAllomorphDef::out_mpr` (an affix rule DYNAMICALLY adding an MPR feature,
//!   e.g. Indonesian's `mrule3` "per-" prefix, `MorphologicalOutput MPRFeatures="mpr1"`) is not
//!   threaded into the partition key — every group's affix chains are shared, unfiltered. This is
//!   verified zero-impact for the two acceptance tests here (Indonesian's `prule5` exclusion is
//!   exercised entirely by the 4 STATIC `ruleFeatures="mpr1"` entries acceptance-tested below; the
//!   "per-" prefix's own dynamic mpr1 output was independently checked to never reach `prule5`'s
//!   own environment at all — `per+X` never has the preceding nasal `prule5`'s left-environment
//!   requires, so this affix-propagation path is dead for THIS grammar's own rule shapes, matching
//!   the prior investigation's finding, re-derived independently here). A grammar whose recall
//!   genuinely depends on affix-time MPR propagation into a gated prule is a real, uncovered gap —
//!   costed the same "medium–large" the prototype report already flagged, not silently assumed
//!   solved by this step.
//! - **POS only reads the root's OWN declared part of speech** (`LexEntryDef::syn_fs`), not any
//!   mid-cascade `outputPartOfSpeech` an mrule may have assigned — same root-only scope boundary,
//!   same reasoning: no reference-grammar acceptance test here needs an affix-changed POS to reach
//!   a gated prule.
//! - **`MprGroupMatchType::Any`-typed MPR groups are excluded from partitioning entirely** (treated
//!   as always-ungated, i.e. compiled as if the subrule declared no MPR restriction) — see
//!   [`find_gated_subrules`]'s doc for why: this prototype's two acceptance grammars declare only
//!   `All`-type (or ungrouped) MPR features, so `Any` semantics are unexercised, and folding them in
//!   without a real test would risk silently mis-gating a shape nothing here proves correct.

use std::collections::{HashMap, HashSet};

use foma::constructions::{fsm_compose, fsm_union};
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{
    Grammar, LexEntryId, MprGroupMatchType, PhonRuleDef, RewriteSubruleDef,
};

use crate::replace::{compile_and_compose_rules_gated, SegAlphabet, TupleReport};
use crate::uflexc::{emit_underlying_filtered, UEmitReport};

/// One (rule position in `prules_in_order`, subrule index within that rule) pair whose
/// `RewriteSubruleDef` declares a nontrivial `required_pos`/`required_mpr`/`excluded_mpr` (module
/// doc). `Any`-type MPR groups are excluded (module doc's caveat) — a subrule whose ONLY
/// restriction is an `Any`-type group's members is treated as ungated here (conservative: it
/// compiles exactly as the pre-gating code did, over-generating rather than mis-gating an
/// unverified shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatedSubrule {
    pub rule_pos: usize,
    pub sub_idx: usize,
}

/// `true` iff `sr` declares a restriction this module partitions on. `required_mpr`/`excluded_mpr`
/// bits belonging to an `Any`-type [`pg_grammar::model::MprGroup`] are excluded from the check
/// (module doc caveat) by masking them out before testing emptiness.
fn is_gated(g: &Grammar, sr: &RewriteSubruleDef) -> bool {
    sr.required_pos.is_some()
        || !ungrouped_or_all(g, sr.required_mpr).is_empty()
        || !ungrouped_or_all(g, sr.excluded_mpr).is_empty()
}

/// `mpr` with every bit belonging to an `Any`-type group cleared (module doc caveat: `Any`-type
/// restrictions are not partitioned on in this prototype). Ungrouped bits and `All`-type-group bits
/// pass through unchanged.
fn ungrouped_or_all(g: &Grammar, mpr: pg_grammar::model::MprSet) -> pg_grammar::model::MprSet {
    let mut keep = mpr;
    for group in &g.mpr_groups {
        if group.match_type == MprGroupMatchType::Any {
            keep = pg_grammar::model::MprSet(keep.0 & !group.members.0);
        }
    }
    keep
}

/// Scan every `Rewrite`-kind [`PhonRuleDef`] in `prules_in_order` (document/cascade order, same
/// slice [`crate::replace::compile_and_compose_rules`] takes) for gated subrules (module doc).
pub fn find_gated_subrules(g: &Grammar, prules_in_order: &[&PhonRuleDef]) -> Vec<GatedSubrule> {
    let mut out = Vec::new();
    for (rule_pos, pr) in prules_in_order.iter().enumerate() {
        let PhonRuleDef::Rewrite(rule) = pr else { continue };
        for (sub_idx, sr) in rule.subrules.iter().enumerate() {
            if is_gated(g, sr) {
                out.push(GatedSubrule { rule_pos, sub_idx });
            }
        }
    }
    out
}

/// The gating KEY for one lexical entry: one bool per `gated` subrule, in the same order, `true`
/// iff `pg_rules::rewrite::subrule_applicable` (the REAL engine's own predicate — see that
/// function's doc for why this module calls it directly rather than re-deriving it) says this
/// entry's own syntactic FS / MPR set satisfies that subrule's restriction.
pub fn entry_gate_key(
    g: &Grammar,
    entry: &pg_grammar::model::LexEntryDef,
    gated: &[GatedSubrule],
    prules_in_order: &[&PhonRuleDef],
) -> Vec<bool> {
    let syn_fs = g.fs_interner.get(entry.syn_fs);
    gated
        .iter()
        .map(|gs| {
            let PhonRuleDef::Rewrite(rule) = prules_in_order[gs.rule_pos] else {
                unreachable!("GatedSubrule only ever indexes a Rewrite rule, see find_gated_subrules")
            };
            let sr = &rule.subrules[gs.sub_idx];
            pg_rules::rewrite::subrule_applicable(g, sr, syn_fs, entry.mpr)
        })
        .collect()
}

/// One partition: every lexical entry sharing one gating key (module doc step 3).
pub struct EntryGroup {
    pub key: Vec<bool>,
    pub entries: HashSet<LexEntryId>,
}

/// Partition every entry in `g.entries` by [`entry_gate_key`]. If `gated` is empty (no subrule in
/// `prules_in_order` is gated at all), returns exactly ONE group containing every entry with an
/// empty key — i.e. this degenerates to the pre-gating behavior exactly (verified by the Aweti/
/// Amharic-compile-only regression checks, which have no acceptance-tested gated subrule reachable
/// through this prototype's template-less emitter and so must collapse to 1 group).
pub fn partition_entries(
    g: &Grammar,
    gated: &[GatedSubrule],
    prules_in_order: &[&PhonRuleDef],
) -> Vec<EntryGroup> {
    let mut buckets: HashMap<Vec<bool>, HashSet<LexEntryId>> = HashMap::new();
    for (ei, entry) in g.entries.iter().enumerate() {
        let key = entry_gate_key(g, entry, gated, prules_in_order);
        buckets.entry(key).or_default().insert(LexEntryId(ei as u32));
    }
    buckets
        .into_iter()
        .map(|(key, entries)| EntryGroup { key, entries })
        .collect()
}

/// Full result of the gated compile (module doc step 4): the final unioned network, plus
/// diagnostics pooled across every group (skipped rules/allomorphs, alpha-tuple reports, per-group
/// entry/root counts) so a caller can report exactly what this prototype covers.
pub struct GatedCompileResult {
    pub net: Option<Fsm>,
    pub groups: usize,
    pub skipped_rules: Vec<String>,
    pub skipped_allomorphs: Vec<String>,
    pub tuple_reports: Vec<(String, Vec<TupleReport>)>,
    /// One entry per group: `(gate key, root entries emitted, prefix entries, suffix entries)` —
    /// `prefix_entries`/`suffix_entries` are identical across every group (module doc: affix chains
    /// are shared, unfiltered) and repeated here only for a caller's own sanity-printing.
    pub group_reports: Vec<(Vec<bool>, usize, usize, usize)>,
}

/// Orchestrates module doc steps 1-4: find the gated subrules, partition entries, compile one
/// lexc+rules network PER GROUP, union them. `prules_in_order` is the same stratum-cascade-order
/// slice every other `replace`/`uflexc` entry point takes.
pub fn compile_gated_grammar(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
) -> GatedCompileResult {
    let gated = find_gated_subrules(g, prules_in_order);
    let groups = partition_entries(g, &gated, prules_in_order);

    let mut final_net: Option<Fsm> = None;
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut skipped_allomorphs: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<TupleReport>)> = Vec::new();
    let mut group_reports = Vec::new();

    for group in &groups {
        let UEmitReport {
            lexc_source,
            skipped: uskipped,
            root_entries,
            prefix_entries,
            suffix_entries,
            ..
        } = emit_underlying_filtered(g, alphabet, Some(&group.entries));
        skipped_allomorphs.extend(uskipped);
        group_reports.push((group.key.clone(), root_entries, prefix_entries, suffix_entries));

        if root_entries == 0 {
            // An empty group (can happen if a gating key combination matches zero entries, e.g. a
            // grammar declares a POS no entry actually uses) contributes nothing -- skip rather
            // than compile-and-union an empty lexicon.
            continue;
        }

        let lexc_net = foma::lexcread::fsm_lexc_parse_string(opts, None, &lexc_source)
            .unwrap_or_else(|| panic!("gated group lexc failed to compile:\n{lexc_source}"));

        let subrule_ok = |rule_pos: usize, sub_idx: usize| -> bool {
            match gated.iter().position(|gs| gs.rule_pos == rule_pos && gs.sub_idx == sub_idx) {
                None => true, // ungated subrule: always included
                Some(gate_index) => group.key[gate_index],
            }
        };
        let mut group_skipped_rules = Vec::new();
        let rules_net = compile_and_compose_rules_gated(
            opts,
            g,
            alphabet,
            prules_in_order,
            &subrule_ok,
            &mut group_skipped_rules,
            &mut tuple_reports,
        );
        // Only report a rule as skipped once (every group would otherwise re-report the same
        // unsupported-construct rule) -- dedupe against what's already recorded.
        for s in group_skipped_rules {
            if !skipped_rules.contains(&s) {
                skipped_rules.push(s);
            }
        }

        let group_net = match rules_net {
            Some(rules) => fsm_compose(opts, lexc_net, rules),
            None => lexc_net,
        };
        final_net = Some(match final_net {
            None => group_net,
            // Safe union: groups are lexically disjoint (module doc "why the union is safe here").
            Some(prev) => fsm_union(opts, prev, group_net),
        });
    }

    GatedCompileResult {
        net: final_net,
        groups: groups.len(),
        skipped_rules,
        skipped_allomorphs,
        tuple_reports,
        group_reports,
    }
}
