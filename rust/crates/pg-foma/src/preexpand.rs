//! Rule-application pre-expansion + boundary-fusion composite probing: closes the two structural
//! miss classes the P1c investigation found on Amharic (32/32 misses classified, no third
//! class) --
//!
//! 1. **Interdigitation** (`Role::Infix` rules -- Amharic's `-pfv-`/`-conv-`, 24/32 of the P1c
//!    misses): a standalone rule whose RHS interleaves `InsertSegments` actions AROUND a `Copy` of
//!    the root's own material has no literal string a plain lexc entry can express -- the inserted
//!    "ä" sits INSIDE the root's own copied consonants (root "ውልድ" + `-pfv-` -> "ውäልäድ"), so there
//!    is no boundary a two-entry (root-entry, then-continue-to-affix-entry) encoding can cut apart.
//! 2. **Ge'ez boundary fusion** (ordinary `Role::Prefix`/`Role::Suffix` rules whose adjacency to a
//!    SPECIFIC root's own final/initial glyph coalesces into a DIFFERENT glyph, 8/32): the existing
//!    deletion-only junction model (`crate::junctions::PhonologyProbe`) can express "one
//!    neighbouring segment vanishes outright" but not "two adjacent segments merge into a
//!    differently-spelled third segment" -- Ge'ez being an abugida, adjacent consonant+vowel glyphs
//!    at a morph boundary regularly do this (root "ልጅ" + pl suffix "+ዮች" -> "ልጆች", never the
//!    literal "ልጅዮች").
//!
//! ## Shared mechanism
//! Both classes are closed by the SAME technique -- **rule-application pre-expansion**: seed a
//! `pg_rules::word::Word` from one root allomorph's own FEATURE-BEARING shape (re-segmented with
//! features exactly the way `pg_parse::Morpher`'s own lexical lookup does it --
//! `pg_rules::shape_feat::segment_with_features` on the allomorph's stored TEXT, NOT the loader's
//! feature-LESS `RootAllomorphDef::shape.shape` directly -- using the stored shape makes every
//! natural-class LHS check fail silently, a real bug this stage's investigation caught: 0/76 roots
//! matched any of the three infix rules until the fix, 36/76 after), apply the REAL rule via
//! `pg_rules::morph::synthesize` (the exact function the engine's own per-word synthesis pipeline
//! calls -- not a re-implementation), then run the REAL phonological cascade over the result via
//! `pg_rules::surface_probe::probe_synthesize` (the same probe machinery
//! `crate::junctions::PhonologyProbe` already drives) to get the TRUE, phonology-resolved surface
//! spelling. When that surface differs from a naive (pre-phonology) rendering of the very same
//! synthesized shape -- ALWAYS true for an `Infix` rule (there is no non-interleaved literal to even
//! compare against), SOMETIMES true for a `Prefix`/`Suffix` rule (fusion) -- emit ONE lexc entry
//! carrying BOTH the root's tag and the rule's tag, in the ENGINE'S OWN morph order.
//!
//! That order is COMPUTED, never assumed: `morph_order_tags` replays
//! `pg_parse::Morpher::allomorphs_in_morph_order`'s own algorithm (sort the synthesized `Word`'s
//! `morphs` by `order`, keep only the first occurrence of each distinct `AllomorphId`) over the
//! synthesized `Word` -- so this is correct regardless of whether the rule is a leading, trailing,
//! or interior insertion, with NO per-role special-casing: a `Prefix` composite naturally comes out
//! rule-tag-then-root-tag, a `Suffix`/`Infix` composite root-tag-then-rule-tag, because that is
//! genuinely where each one's own first surface material sits (root is always seeded at `order = 0`,
//! `pg-parse/src/morpher.rs:564`'s convention, mirrored here). Verified directly against the real
//! engine (this stage's investigation): `pg_parse::Morpher` analyzing "ሄደ" ("go.pfv.3m") returns
//! `morpheme_ids = [entry43(go), mrule13(-pfv-), mrule18(pfv.3m)]` -- root first, matching what this
//! module's own `morph_order_tags` computes independently for the same (root, rule) pair.
//!
//! Generic by construction (plan §0: "do not special-case Amharic"): this runs for EVERY
//! `Role::{Infix, Prefix, Suffix}` rule in the grammar against EVERY root allomorph (and,
//! recursively, against every chain stem -- see "Chaining" below), gated only by the SAME
//! `required_syn_fs` unifiability check `pg_rules::morph::synth_syn_fs` applies internally (a
//! cheap, behavior-PRESERVING pre-filter: `synthesize` would reject an incompatible pair anyway,
//! this just skips building a `Word`/compiling the rule's LHS FST for one that provably cannot
//! match). Measured on Amharic (release build): depth-0 alone is 6,612 raw (root, rule)
//! combinations pre-filtered to 1,389; with depth-3 chaining the total is ~305k pairs probed,
//! yielding 2,930 interdigitation + 51,023 fusion composite entries in ~30-47s of emit wall time
//! (the dominant emit cost -- see SCALE BRIDGE below). A grammar with zero `Infix` rules and zero
//! phonological rules at all (Sena) computes zero pairs and emits zero composites --
//! `should_run` short-circuits before touching a single entry, which is what keeps Sena's
//! emitted lexc source byte-for-byte unchanged (its own regression gate depends on this).
//! Indonesian (real phonology, no infix rules, no coalescence) probes 457 pairs and emits ZERO
//! composites -- every junction it has is already reachable through the existing deletion-junction
//! model, verified by the redundancy check below.
//!
//! ## Chaining (a composite may itself need a further composite — through CLEAN steps too)
//! An interdigitated or fused stem is not always word-final: Amharic's pfv/conv stems obligatorily
//! take a subject-agreement suffix (`root + -pfv- + pfv.3m`), and that agreement suffix ITSELF fuses
//! with the composite stem's own final glyph (`"ሄድ" (root+`-pfv-`) + "+ä" (pfv.3m) -> "ሄደ"`, not the
//! literal `"ሄድä"`) — discovered empirically by this stage's own recall gate (a first single-level
//! implementation still missed "ሄደ" outright). Worse, a fusion can follow a byte-CLEAN step:
//! "ሌባዎቹ" is `root ሌባ + def.m (clean: "ሌባው") + pl (ው+o fuse: "ሌባዎች") + poss.3m (ች+u fuse:
//! "ሌባዎቹ")` — caught by the gate at 31/32. `extend` therefore recurses on EVERY successful rule
//! application (dirty or clean), bounded by the grammar's application counts, EMITTING a composite (all of the
//! chain's tags, engine morph order) only for dirty steps: a clean step is already realized by the
//! ordinary per-rule lexc entries, so emitting it would only duplicate paths, but its output word
//! must still be explored for deeper fusions. Dirtiness at every depth is judged by the SAME
//! `reachable_via_ordinary_emission` check: the "one side" baseline is the root's own
//! spellings/stripped-spellings at depth 0, and the previous level's single rendered surface
//! (plus its stripped form ONLY if that level was clean — a dirty stem exists only as a composite
//! entry, which has no `Stripped` sibling) at depth >= 1.
//!
//! ## Avoiding redundant entries with the existing junction model
//! A `Prefix`/`Suffix` composite is only emitted when the fused spelling is NOT already reachable
//! through the ORDINARY two-entry path (`emit.rs`'s own literal root/affix entries, enriched by
//! `PhonologyProbe::variants`/`PhonologyProbe::deletion_junctions` when the grammar has any
//! phonological rules) -- `reachable_via_ordinary_emission` recomputes that same candidate string
//! set (every affix spelling × every root spelling, plus the deletion-junction × stripped-root
//! combination `emit.rs` itself wires) and checks membership before minting a new composite. This is
//! what keeps Indonesian's `meN+tulis -> menulis` (already correctly produced by the EXISTING
//! deletion-junction mechanism: "men" + stripped "ulis") from ALSO growing a redundant joint
//! composite entry, while Ge'ez's true fusions (inexpressible by literal concatenation on EITHER
//! side, in ANY combination the existing model offers) do get one. An `Infix` rule has no "ordinary
//! two-entry path" to compare against at all (that is the whole reason it was routed to `uncovered`
//! upstream), so it is always emitted once a matching (root, rule) pair is found.
//!
//! ## SCALE BRIDGE (plan §0 scale mandate)
//! This is an O(roots × rules^depth) enumeration -- workable at Amharic's 76 entries × 87 rules ×
//! depth 3 (measured: ~305k pairs, ~54k entries) but decidedly NOT at FLEx scale (10⁴-10⁵ entries,
//! hundreds of rules). Each probe calls `pg_rules::morph::synthesize_cached` (the `RuleCache`-aware
//! sibling of `synthesize`, now `pub` for exactly this cross-crate caller) against the SAME
//! `RuleCache` `build_composites` builds once per grammar, so a rule's LHS FST is compiled once at
//! cache-build time and read (not recompiled) on every one of its ~305k probes -- before this, each
//! probe recompiled that FST from scratch (Thompson NFA build + determinize), which dominated
//! Amharic's emit wall time (~35s of it). The **P6 successor** (replace-rule compilation) retires
//! this bridge entirely by compiling interdigitating/fusing rules as real foma replace-calculus
//! rules over root natural-class
//! patterns, composed directly into the network instead of enumerated per root and per chain --
//! exactly the same successor already named for `crate::junctions::PhonologyProbe`'s own
//! enumeration bridge.
//!
//! ## Morphotactic pruning (the Aweti scale fix)
//! `extend`'s flat recursion above chains **every** candidate rule onto every root at every
//! depth, gated only by the cheap `required_syn_fs` pre-filter -- workable at Amharic's scale
//! (module doc, "SCALE BRIDGE") but not at Aweti's (855 roots x 123 candidate rules): the flat
//! recursion explores rule orders the engine's own morphotactics (`pg-rules/src/stratum.rs`:
//! `synth_apply_mrules`/`synth_apply_templates`/`synth_slots_generic`) can never produce in
//! synthesis. `crate::morphotactics::MorphotacticIndex` builds a subset-construction automaton
//! over those exact engine functions once per grammar; `extend` consults
//! `crate::morphotactics::MorphotacticIndex::next_state` immediately before recursing on a
//! candidate rule, restricting the recursion to a STRICT SUBSET of what the flat version explored
//! (pruned exploration subset-of flat exploration by construction, so emitted composites are a
//! subset in the same relative order -- recall-preserving, never widening). See that module's own
//! doc for the full automaton design (loose-rule strata / template-slot sites / the vacuous-slot
//! recall trap).

use pg_featstruct::{flat_unifiable, is_unifiable, FsId};
use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    AllomorphId, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId, OutputAction,
};
use pg_rules::cache::RuleCache;
use pg_rules::morph::synthesize_cached;
use pg_rules::surface_probe::{self, ProbeSeg};
use pg_rules::word::{MorphRecord, Word};
use pg_shape::NO_CHAR_DEF;
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

use crate::emit::{is_structural_rule, rule_role, stripped_variants, surface_variants, Role};
use crate::junctions::PhonologyProbe;
use crate::morphotactics::{
    ChainState, EnumerationBudget, ExploreMode, MorphotacticIndex, ProbeBudget,
};
use crate::tags;

/// One rule-application/fusion composite: an extra "root-like" lexc entry whose upper tape carries
/// MULTIPLE tag symbols (root plus one or more rules) instead of one, in the engine's own morph order
/// (`morph_order_tags`). Wired by `emit.rs` into one shared `Composites` lexicon reachable from
/// every roots-lexicon emission site (bare `Root`, `TLRoots`, each `G{gi}Roots`) and continuing
/// into `CompositeExit` (the union of every post-root continuation), so an interdigitated/fused
/// stem can still take ordinary prefixes/suffixes around it (root-section replacement, not
/// bare-only).
pub(crate) struct CompositeRec {
    /// Every morpheme whose tag appears in `tag_lexc`, as `(is_root, id)` — `emit.rs` declares each
    /// in `Multichar_Symbols` (an Infix rule's morpheme is in NO deriv layer or slot, so no other
    /// collection site would declare it).
    pub chain_morphemes: Vec<(bool, MorphemeId)>,
    /// The escaped, ALREADY-CONCATENATED upper-tape tag string (all the chain's tags, in engine
    /// morph order).
    pub tag_lexc: String,
    /// The rendered, phonology-resolved surface spelling(s) (usually one; a `Vec` because a
    /// character-definition table can have a historical letter-series merger — see
    /// `render_all_variants` — or because a rule's own disjunctive allomorphs produce more than
    /// one distinct rendering for the same tag pair).
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompositeReport {
    /// (root allomorph, candidate rule) pairs actually attempted (after the cheap required-FS
    /// pre-filter) -- the module doc's scale-bridge number.
    pub pairs_probed: usize,
    /// Same count, broken down by recursion depth (0-indexed) -- the dynamic tree is the real
    /// unknown pruning must be measured against, and a flat total alone can't show WHERE the cost
    /// concentrates.
    pub pairs_probed_by_depth: Vec<usize>,
    /// Number of probed pairs where `synthesize_cached` returned at least one word -- the
    /// dynamic-filter yield counterpart to `pairs_probed`.
    pub synth_successes: usize,
    /// Composite entries emitted for `Role::Infix` rules (miss class 5a).
    pub interdigitation_entries: usize,
    /// Composite entries emitted for `Role::Prefix`/`Role::Suffix` rules whose fused surface differs
    /// from what the ordinary two-entry emission already reaches (miss class 5b).
    pub fusion_entries: usize,
    /// `g.mrules` indices of the non-dropping `Role::Infix` rules THIS module claims (`candidate_rules`)
    /// that produced at least one composite entry -- `emit.rs` suppresses the "standalone rule
    /// classifies as Infix; not representable" uncovered routing for exactly these (the construct IS
    /// representable now, via this module); an infix rule that matched zero roots stays uncovered,
    /// honestly. A drop-shaped `Role::Infix` rule is never a candidate here at all (census C4's
    /// ownership handoff, `candidate_rules`'s own doc) -- `emit.rs` clears ITS uncovered entry from
    /// `crate::emit::build_structural_composites`'s own covered-rules set instead.
    pub covered_infix_rules: std::collections::BTreeSet<u32>,
    /// Legal synthesized successors found beyond the configured closure-depth limit.
    /// Any nonzero value proves this construction incomplete and prevents artifact creation.
    pub pending_successors: usize,
    /// Stable grammar ordinals of rules that produced pending successors.
    pub pending_rule_ordinals: std::collections::BTreeSet<u32>,
}

/// Whether this grammar can possibly need either mechanism at all -- `false` short-circuits
/// `build_composites` to a zero-cost, zero-entry no-op (module doc: what keeps Sena's gate
/// byte-for-byte).
pub(crate) fn should_run(g: &Grammar, phon: Option<&PhonologyProbe>) -> bool {
    phon.is_some() || any_infix_rule(g)
}

fn any_infix_rule(g: &Grammar) -> bool {
    (0..g.mrules.len()).any(|i| rule_role(g, MRuleId(i as u32)) == Role::Infix)
}

/// Every rule id whose PRIMARY allomorph classifies `Infix`/`Prefix`/`Suffix` (mirrors `emit.rs`'s
/// own `rule_role` convention for "how this rule is treated" everywhere else in the emitter), MINUS
/// every rule `crate::emit::is_structural_rule` claims. Structural synthesis owns the whole rule,
/// including ordinary first allomorphs whose later alternatives require that route; keeping one in
/// both candidate sets would duplicate closure work and blur which mechanism guarantees recall.
/// Primary `Reduplication` (the peel's job when structurally invertible),
/// `CircumfixPrefix`/`CircumfixSuffix`, `Process`, and `None` are out of this stage's scope.
/// Diagnostic-only: `candidate_rules(g).len()` without exposing the `Role` classification itself
/// outside the crate (`crate::emit`'s `composite_scale_hint` is the one external caller — see that
/// function's doc for why this exists).
pub(crate) fn candidate_rule_count(g: &Grammar) -> usize {
    candidate_rules(g).len()
}

pub(crate) fn candidate_rules(g: &Grammar) -> Vec<(MRuleId, Role)> {
    let mut out = Vec::new();
    for (i, r) in g.mrules.iter().enumerate() {
        if matches!(r, MorphRuleDef::Compounding(_)) {
            continue;
        }
        let mid = MRuleId(i as u32);
        let role = rule_role(g, mid);
        if matches!(role, Role::Prefix | Role::Suffix | Role::Infix) {
            if is_structural_rule(g, mid) {
                continue;
            }
            out.push((mid, role));
        }
    }
    out
}

/// Whether a realizational rule lacks both an authored bound and the engine's feature-presence
/// block. A non-empty realizational feature structure is written into the word's syntactic
/// features and makes the same rule inapplicable on the next step; only an empty one can repeat
/// without a semantic bound.
pub(crate) fn realizational_rule_is_semantically_unbounded(g: &Grammar, mid: MRuleId) -> bool {
    match &g.mrules[mid.0 as usize] {
        MorphRuleDef::Realizational(rule) => g.fs_interner.get(rule.real_fs).is_empty(),
        MorphRuleDef::AffixProcess(_) | MorphRuleDef::Compounding(_) => false,
    }
}

/// Semantically unbounded realizational candidates whose stable ordinals let the caller refuse
/// eager closure before it mistakes `u16::MAX` for a finite proof.
pub(crate) fn unbounded_candidate_rules(g: &Grammar) -> Vec<MRuleId> {
    candidate_rules(g)
        .into_iter()
        .map(|(mid, _)| mid)
        .filter(|&mid| loose_rule_is_active(g, mid))
        .filter(|&mid| realizational_rule_is_semantically_unbounded(g, mid))
        .collect()
}

/// Pure-Rust count of reachable rule-pair work in TunedSurface pre-expansion. This follows the
/// production morphotactic transitions, feature checks, application counters, and morphology
/// synthesizer, but never probes phonology or emits lexc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreexpandClosureCharacterization {
    pub candidate_rules: usize,
    pub root_allomorphs: usize,
    pub rule_pairs_visited: usize,
    pub synthesized_successors: usize,
    pub limit: usize,
    pub exceeded: bool,
    pub dominant_rule_ordinals: Vec<(u32, usize)>,
}

struct PreexpandCharacterizationContext<'a> {
    grammar: &'a Grammar,
    rules: &'a [(MRuleId, Role)],
    morphotactics: &'a MorphotacticIndex,
    cache: &'a RuleCache,
    limit: usize,
}

fn characterize_preexpand_chain(
    context: &PreexpandCharacterizationContext<'_>,
    root_table: &CharDefTable,
    word: &Word,
    state: &ChainState,
    depth: usize,
    result: &mut PreexpandClosureCharacterization,
    per_rule: &mut [usize],
) {
    if result.exceeded {
        return;
    }
    let base_fs = word.syn_fs.clone();
    for &(mid, _) in context.rules {
        let rule = &context.grammar.mrules[mid.0 as usize];
        let (required, _) = rule_fs_and_morpheme(rule);
        let Some(next_state) =
            context
                .morphotactics
                .next_state(state, mid, &base_fs, &context.grammar.fs_interner)
        else {
            continue;
        };
        let required_fs = context.grammar.fs_interner.get(required);
        if !required_fs.is_empty() && !is_unifiable(required_fs, &base_fs) {
            continue;
        }

        result.rule_pairs_visited = result.rule_pairs_visited.saturating_add(1);
        per_rule[mid.0 as usize] = per_rule[mid.0 as usize].saturating_add(1);
        if result.rule_pairs_visited > context.limit {
            result.exceeded = true;
            return;
        }

        let synthesized = synthesize_cached(context.grammar, mid, word, rule, context.cache);
        result.synthesized_successors = result
            .synthesized_successors
            .saturating_add(synthesized.len());
        if depth >= DEFAULT_CLOSURE_DEPTH_BUDGET {
            continue;
        }
        for successor in synthesized {
            let Some(segments) =
                surface_probe::probe_synthesize(context.grammar, &successor.shape, context.cache)
            else {
                continue;
            };
            if !render_all_variants(root_table, &segments)
                .iter()
                .any(|surface| !surface.is_empty())
            {
                continue;
            }
            characterize_preexpand_chain(
                context,
                root_table,
                &successor,
                &next_state,
                depth + 1,
                result,
                per_rule,
            );
            if result.exceeded {
                return;
            }
        }
    }
}

pub(crate) fn characterize_preexpand_closure(
    grammar: &Grammar,
    limit: usize,
) -> PreexpandClosureCharacterization {
    let runs = grammar
        .strata
        .iter()
        .any(|stratum| !stratum.prules.is_empty())
        || any_infix_rule(grammar);
    let rules = if runs {
        candidate_rules(grammar)
    } else {
        Vec::new()
    };
    let morphotactics = MorphotacticIndex::build(grammar);
    let cache = RuleCache::build(grammar);
    let context = PreexpandCharacterizationContext {
        grammar,
        rules: &rules,
        morphotactics: &morphotactics,
        cache: &cache,
        limit,
    };
    let mut result = PreexpandClosureCharacterization {
        candidate_rules: rules.len(),
        root_allomorphs: 0,
        rule_pairs_visited: 0,
        synthesized_successors: 0,
        limit,
        exceeded: false,
        dominant_rule_ordinals: Vec::new(),
    };
    let mut per_rule = vec![0usize; grammar.mrules.len()];

    for stratum in &grammar.strata {
        for &entry_id in &stratum.entries {
            let entry = &grammar.entries[entry_id.0 as usize];
            let root_stratum = grammar.morphemes[entry.morpheme.0 as usize].stratum;
            let table =
                &grammar.char_tables[grammar.strata[root_stratum.0 as usize].table.0 as usize];
            let entry_fs = grammar.fs_interner.get(entry.syn_fs);
            for allomorph in &entry.allomorphs {
                if allomorph.is_pattern {
                    continue;
                }
                let Ok(shape) = pg_rules::shape_feat::segment_with_features(
                    grammar,
                    table,
                    &allomorph.shape.text,
                ) else {
                    continue;
                };
                result.root_allomorphs = result.root_allomorphs.saturating_add(1);
                let mut word = Word::new(shape, root_stratum);
                word.syn_fs = entry_fs.clone();
                word.mpr = entry.mpr;
                word.root_allomorph = Some(allomorph.id);
                word.morphs = vec![MorphRecord::new(allomorph.id, entry.morpheme, 0)];
                let state = ChainState::seed(grammar, root_stratum.0, entry.partial);
                characterize_preexpand_chain(
                    &context,
                    table,
                    &word,
                    &state,
                    0,
                    &mut result,
                    &mut per_rule,
                );
                if result.exceeded {
                    break;
                }
            }
            if result.exceeded {
                break;
            }
        }
        if result.exceeded {
            break;
        }
    }

    let mut dominant: Vec<(u32, usize)> = per_rule
        .into_iter()
        .enumerate()
        .filter_map(|(ordinal, count)| (count != 0).then_some((ordinal as u32, count)))
        .collect();
    dominant.sort_by_key(|&(ordinal, count)| (std::cmp::Reverse(count), ordinal));
    dominant.truncate(5);
    result.dominant_rule_ordinals = dominant;
    result
}

pub(crate) fn loose_rule_is_active(g: &Grammar, mid: MRuleId) -> bool {
    g.strata.iter().any(|stratum| stratum.mrules.contains(&mid))
}

/// `(required_syn_fs, out_syn_fs, owning morpheme)` for the two rule kinds that carry allomorphs; `candidate_rules` filters `Compounding` out before this is ever reached.
pub(crate) fn rule_fs_and_morpheme(rule: &MorphRuleDef) -> (FsId, MorphemeId) {
    match rule {
        MorphRuleDef::AffixProcess(def) => (def.required_syn_fs, def.morpheme),
        MorphRuleDef::Realizational(def) => (def.required_syn_fs, def.morpheme),
        MorphRuleDef::Compounding(_) => unreachable!("candidate_rules excludes Compounding"),
    }
}

/// Replays `Morpher::allomorphs_in_morph_order`'s own algorithm over a freshly-synthesized composite `Word`, so tag order is computed from the same bookkeeping the real engine uses, never assumed from a rule's role.
fn morph_order_tags(w: &Word, known: &[(MorphemeId, String)]) -> Option<String> {
    let mut ms = w.morphs.clone();
    ms.sort_by_key(|m| m.order);
    let mut seen: Vec<AllomorphId> = Vec::new();
    let mut out = String::new();
    for m in ms {
        if seen.contains(&m.allomorph) {
            continue;
        }
        seen.push(m.allomorph);
        match known.iter().find(|(mid, _)| *mid == m.morpheme) {
            Some((_, tag)) => out.push_str(tag),
            None => return None,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// One rule-allomorph's precomputed ordinary-surface strings, hoisted out of `reachable_via_ordinary_emission`'s hot path; see `build_allomorph_variants`.
pub(crate) struct AllomorphVariants {
    /// `surface_variants(text) ∪ phon.variants(text)` for one allomorph's `InsertSegments` text.
    ordinary: Vec<String>,
    /// `phon.deletion_junctions(text)`, prefix-side only, mirroring `emit.rs`'s `{roots}Stripped` convention.
    deletion: Vec<String>,
}

/// Precomputes `reachable_via_ordinary_emission`'s per-allomorph input once per candidate rule rather than per probe; why this precompute matters at Amharic's probe volume: docs/research/pg-foma-preexpand-design-notes.md.
pub(crate) fn build_allomorph_variants(
    table: &CharDefTable,
    phon: Option<&PhonologyProbe>,
    rule: &MorphRuleDef,
) -> Vec<AllomorphVariants> {
    let allomorphs = match rule {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return Vec::new(),
    };
    allomorphs
        .iter()
        .filter_map(|allo| {
            let text = allo.rhs.iter().find_map(|a| match a {
                OutputAction::InsertSegments { shape, .. } => Some(shape.text.as_str()),
                _ => None,
            })?;
            let mut ordinary: Vec<String> = surface_variants(table, text)
                .map(|(v, _)| v)
                .unwrap_or_default();
            if let Some(p) = phon {
                ordinary.extend(p.variants(text));
            }
            let deletion = phon.map(|p| p.deletion_junctions(text)).unwrap_or_default();
            Some(AllomorphVariants { ordinary, deletion })
        })
        .collect()
}

/// Whether the ordinary two-entry emission already reaches `fused` through some combination, avoiding a redundant composite entry; the routing rules mirrored from `emit.rs`: docs/research/pg-foma-preexpand-design-notes.md.
pub(crate) fn reachable_via_ordinary_emission(
    root_variants: &[String],
    root_stripped: &[String],
    allo_variants: &[AllomorphVariants],
    is_prefix: bool,
    fused: &str,
) -> bool {
    for av in allo_variants {
        for a in &av.ordinary {
            for r in root_variants {
                let concat = if is_prefix {
                    format!("{a}{r}")
                } else {
                    format!("{r}{a}")
                };
                if concat == fused {
                    return true;
                }
            }
        }
        if is_prefix {
            for a in &av.deletion {
                for r in root_stripped {
                    if format!("{a}{r}") == fused {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Cap on `render_all_variants`'s Cartesian product; kept deliberately small since the fallback branch it bounds fires on ~30% of probed segments on Amharic and a generous cap blows up emitted lexc size: docs/research/pg-foma-preexpand-design-notes.md.
const MAX_RENDER_VARIANTS: usize = 4;

/// Every literal spelling a probed segment node can honestly mean; fast path returns just the node's own concrete representations, falling back to a full lane-unifiable table search when identity was cleared or invalidated: docs/research/pg-foma-preexpand-design-notes.md.
fn matching_reps_local(table: &CharDefTable, char_def: u32, lanes: &[u64]) -> Vec<String> {
    if char_def != NO_CHAR_DEF {
        let cd = table.get(CharDefId(char_def));
        if flat_unifiable(lanes, cd.feature_lanes()) {
            return cd.representations().to_vec();
        }
    }
    let mut out = Vec::new();
    for (id, cd) in table.iter() {
        if cd.kind() != CharDefKind::Segment {
            continue;
        }
        let member = if char_def != NO_CHAR_DEF {
            id.0 == char_def
                || table
                    .unifiable_cds(CharDefId(char_def))
                    .is_some_and(|b| b.contains(id.0))
        } else {
            true // NO_CHAR_DEF (post-rewrite abstract node): pure lane unification, no identity gate.
        };
        if !member {
            continue;
        }
        if flat_unifiable(lanes, cd.feature_lanes()) {
            out.extend(cd.representations().iter().cloned());
        }
    }
    out
}

/// Every distinct literal surface rendering of `segs`: a recall fix for a real miss class `render_nodes` cannot see past, since it collapses each node to only its first matching representation; root cause and the Amharic case that motivated it: docs/research/pg-foma-preexpand-design-notes.md.
fn render_all_variants(table: &CharDefTable, segs: &[ProbeSeg]) -> Vec<String> {
    let mut variants: Vec<String> = vec![String::new()];
    for seg in segs {
        if seg.deleted {
            continue;
        }
        let reps = matching_reps_local(table, seg.char_def, &seg.lanes);
        if reps.is_empty() {
            return Vec::new();
        }
        let mut next = Vec::with_capacity(variants.len() * reps.len());
        for v in &variants {
            for r in &reps {
                next.push(format!("{v}{r}"));
            }
        }
        next.truncate(MAX_RENDER_VARIANTS);
        variants = next;
    }
    variants
}

/// A live successor at this resource boundary refuses the artifact instead of truncating it.
pub(crate) const DEFAULT_CLOSURE_DEPTH_BUDGET: usize = 64;

/// One in-progress composite chain step's context, threaded through `extend`'s recursion.
struct ExtendCtx<'a> {
    g: &'a Grammar,
    root_table: &'a CharDefTable,
    rules: &'a [(MRuleId, Role)],
    /// `build_allomorph_variants` output for each of `rules`, precomputed once per grammar; the only place `phon` still matters to `extend`'s recursion, folded in at precompute time.
    rule_variants: &'a [Vec<AllomorphVariants>],
    cache: &'a RuleCache,
    /// The morphotactic automaton `extend` consults before recursing on a candidate rule, plus the flat/pruned escape hatch for A/B measurement.
    mt: &'a MorphotacticIndex,
    mode: ExploreMode,
    /// Measurement-only safety valve; `None` in production, so every read below is a single branch with zero further cost.
    probe_budget: Option<ProbeBudget<'a>>,
    /// Default-on fail-fast enumeration budget, always live and never panics; `extend` checks it before every recursive step.
    enum_budget: &'a EnumerationBudget,
    closure_trace: Option<&'a crate::characterization::ClosureTrace>,
}

/// `extend`'s output accumulator: composite records, a `(tag_lexc, spelling)` dedup set (a hash set rather than an `O(n^2)` scan, since chains visit thousands of candidates), and the counts report.
struct Acc {
    recs: Vec<CompositeRec>,
    seen: rustc_hash::FxHashSet<(String, String)>,
    report: CompositeReport,
}

/// Explores authored applications within the closure-depth limit.
#[allow(clippy::too_many_arguments)]
fn extend(
    ctx: &ExtendCtx,
    base_word: &Word,
    chain: &[(MorphemeId, String)],
    redundancy_variants: &[String],
    redundancy_stripped: &[String],
    depth: usize,
    width: usize,
    state: &ChainState,
    acc: &mut Acc,
) {
    // Checked once at the top of every call, same shape as the depth guard above: docs/research/pg-foma-preexpand-design-notes.md.
    if ctx.enum_budget.is_tripped() {
        return;
    }
    let base_fs = base_word.syn_fs.clone();
    for (ridx, &(mid, role)) in ctx.rules.iter().enumerate() {
        if ctx.enum_budget.is_tripped() {
            return;
        }
        let rule = &ctx.g.mrules[mid.0 as usize];
        let (req, rule_morpheme) = rule_fs_and_morpheme(rule);
        // Restricts recursion to a rule adjacency the stratum/template machinery can actually produce; `Flat` is an A/B-measurement escape hatch, production always runs `Pruned`. A pure subset restriction: it can only skip a candidate the flat version would also have tried.
        let Some(next_state) = (match ctx.mode {
            ExploreMode::Flat => ctx.mt.next_state_unpruned(state, mid),
            ExploreMode::Pruned => ctx.mt.next_state(state, mid, &base_fs, &ctx.g.fs_interner),
        }) else {
            continue;
        };
        let req_fs = ctx.g.fs_interner.get(req);
        // The same unifiability check `synth_syn_fs` makes internally, so a provably non-matching pair is skipped before building/compiling.
        if !req_fs.is_empty() && !is_unifiable(req_fs, &base_fs) {
            continue;
        }
        if let Some(trace) = ctx.closure_trace {
            if !trace.begin_pair(depth, mid.0) {
                return;
            }
        }
        acc.report.pairs_probed += 1;
        if acc.report.pairs_probed_by_depth.len() <= depth {
            acc.report.pairs_probed_by_depth.resize(depth + 1, 0);
        }
        acc.report.pairs_probed_by_depth[depth] += 1;
        if let Some(budget) = &ctx.probe_budget {
            budget.tick();
        }
        ctx.enum_budget.tick_probe();
        if ctx.enum_budget.is_tripped() {
            return;
        }

        let synth_out = synthesize_cached(ctx.g, mid, base_word, rule, ctx.cache);
        if let Some(trace) = ctx.closure_trace {
            if !trace.record_successors(depth, mid.0, synth_out.len()) {
                return;
            }
        }
        if !synth_out.is_empty() {
            acc.report.synth_successes += 1;
        }
        let depth_limit = ctx
            .closure_trace
            .map_or(DEFAULT_CLOSURE_DEPTH_BUDGET, |trace| trace.depth_cap());
        if depth >= depth_limit {
            if !synth_out.is_empty() {
                acc.report.pending_rule_ordinals.insert(mid.0);
            }
            acc.report.pending_successors += synth_out.len();
            continue;
        }
        for w in synth_out {
            if ctx.enum_budget.is_tripped() {
                return;
            }
            let Some(segs) = surface_probe::probe_synthesize(ctx.g, &w.shape, ctx.cache) else {
                continue;
            };
            // Computed once per synthesized `w`, not once per recursive call, so this stays O(pairs_probed) rather than O(pairs_probed x variants^depth): only the set of strings a step considers grows, the recursion tree does not branch on it.
            let posts: Vec<String> = render_all_variants(ctx.root_table, &segs)
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            if posts.is_empty() {
                continue;
            }
            let is_infix = role == Role::Infix;
            // A variant is "dirty" iff not already reachable via the ordinary two-entry path; only dirty variants need their own composite entry.
            let dirty_posts: Vec<&String> = posts
                .iter()
                .filter(|post| {
                    is_infix
                        || !reachable_via_ordinary_emission(
                            redundancy_variants,
                            redundancy_stripped,
                            &ctx.rule_variants[ridx],
                            role == Role::Prefix,
                            post,
                        )
                })
                .collect();

            let mut next_chain = chain.to_vec();
            next_chain.push((rule_morpheme, tags::morph_tag_lexc(rule_morpheme, width)));

            // Dirty variants are emitted as one composite record; a clean step is not emitted (ordinary lexc entries already realize it) but is still recursed through below.
            if let Some(tag_lexc) = (!dirty_posts.is_empty())
                .then(|| morph_order_tags(&w, &next_chain))
                .flatten()
            {
                let new_variants: Vec<String> = dirty_posts
                    .iter()
                    .filter(|post| acc.seen.insert((tag_lexc.clone(), (***post).clone())))
                    .map(|post| (**post).clone())
                    .collect();
                if !new_variants.is_empty() {
                    acc.recs.push(CompositeRec {
                        // `next_chain[0]` is always the seeding root; later elements are rules.
                        chain_morphemes: next_chain
                            .iter()
                            .enumerate()
                            .map(|(i, (m, _))| (i == 0, *m))
                            .collect(),
                        tag_lexc,
                        variants: new_variants,
                    });
                    if is_infix {
                        acc.report.interdigitation_entries += 1;
                    } else {
                        acc.report.fusion_entries += 1;
                    }
                    // The "composite entries" measure: the one that actually predicts an Aweti-scale blow-up.
                    ctx.enum_budget.add_entries(1);
                    if ctx.enum_budget.is_tripped() {
                        return;
                    }
                }
                if is_infix {
                    acc.report.covered_infix_rules.insert(mid.0);
                }
            }

            // Recurses once per synthesized `w`, never once per variant; why the redundancy baseline includes clean variants' stripped forms only when every variant at this level was clean: docs/research/pg-foma-preexpand-design-notes.md.
            let all_clean = dirty_posts.is_empty();
            let deeper_stripped = if all_clean {
                let mut v = Vec::new();
                for post in &posts {
                    v.extend(
                        stripped_variants(ctx.root_table, post)
                            .map(|(sv, _)| sv)
                            .unwrap_or_default(),
                    );
                }
                v
            } else {
                Vec::new()
            };
            extend(
                ctx,
                &w,
                &next_chain,
                &posts,
                &deeper_stripped,
                depth + 1,
                width,
                &next_state,
                acc,
            );
            if ctx.enum_budget.is_tripped() {
                return;
            }
        }
    }
}

/// Precomputes `build_allomorph_variants` for every candidate rule against every distinct root table up front, warming `PhonologyProbe`'s caches before `build_composites`'s parallel root workers start; deliberately sequential, not `par_iter()` — why: docs/research/pg-foma-preexpand-design-notes.md.
fn build_rule_variants_all_tables(
    g: &Grammar,
    phon: Option<&PhonologyProbe>,
    rules: &[(MRuleId, Role)],
    table_ids: &[u16],
) -> rustc_hash::FxHashMap<u16, Vec<Vec<AllomorphVariants>>> {
    let one_table = |table_id: u16| -> (u16, Vec<Vec<AllomorphVariants>>) {
        let root_table = &g.char_tables[table_id as usize];
        let variants: Vec<Vec<AllomorphVariants>> = rules
            .iter()
            .map(|(mid, _)| build_allomorph_variants(root_table, phon, &g.mrules[mid.0 as usize]))
            .collect();
        (table_id, variants)
    };

    let per_table: Vec<(u16, Vec<Vec<AllomorphVariants>>)> =
        table_ids.iter().map(|&t| one_table(t)).collect();

    per_table.into_iter().collect()
}

/// One `build_composites` top-level work item: a single lexical entry; why this is the parallelization granularity, not per-allomorph or grammar-wide: docs/research/pg-foma-preexpand-design-notes.md.
struct RootWork {
    entry_id: LexEntryId,
}

/// Processes one `RootWork` item (all of one entry's non-pattern allomorphs) with a fresh, entry-local `Acc`, the parallel unit `build_composites` maps over.
#[allow(clippy::too_many_arguments)]
fn process_root_work(
    g: &Grammar,
    width: usize,
    rules: &[(MRuleId, Role)],
    cache: &RuleCache,
    rule_variants_by_table: &rustc_hash::FxHashMap<u16, Vec<Vec<AllomorphVariants>>>,
    mt: &MorphotacticIndex,
    mode: ExploreMode,
    probe_budget: Option<ProbeBudget<'_>>,
    enum_budget: &EnumerationBudget,
    closure_trace: Option<&crate::characterization::ClosureTrace>,
    work: &RootWork,
) -> (Vec<CompositeRec>, CompositeReport) {
    let mut acc = Acc {
        recs: Vec::new(),
        seen: rustc_hash::FxHashSet::default(),
        report: CompositeReport::default(),
    };

    // Skips this root entirely once tripped, cheaper than entering the allomorph loop below even though each `extend` call would bail near-instantly anyway.
    if enum_budget.is_tripped() {
        return (acc.recs, acc.report);
    }

    let entry = &g.entries[work.entry_id.0 as usize];
    let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
    let table_id = g.strata[root_stratum.0 as usize].table.0;
    let root_table = &g.char_tables[table_id as usize];
    let entry_fs = g.fs_interner.get(entry.syn_fs);
    let rule_variants = &rule_variants_by_table[&table_id];

    for allo in &entry.allomorphs {
        if allo.is_pattern {
            continue;
        }
        let Some((root_variants, _)) = surface_variants(root_table, &allo.shape.text) else {
            continue; // unsegmentable -- collect_roots already reports this once.
        };
        let root_stripped = stripped_variants(root_table, &allo.shape.text)
            .map(|(v, _)| v)
            .unwrap_or_default();

        let Ok(shape) =
            pg_rules::shape_feat::segment_with_features(g, root_table, &allo.shape.text)
        else {
            continue;
        };
        let mut word = Word::new(shape, root_stratum);
        word.syn_fs = entry_fs.clone();
        word.mpr = entry.mpr;
        word.root_allomorph = Some(allo.id);
        word.morphs = vec![MorphRecord::new(allo.id, entry.morpheme, 0)];

        let root_tag = tags::root_tag_lexc(entry.morpheme, width);
        let chain0 = vec![(entry.morpheme, root_tag)];
        // A fresh `ExtendCtx` per root: `root_table` is the owning stratum's table, which can in principle differ per root in a multi-table grammar, so it is never assumed shared.
        let root_ctx = ExtendCtx {
            g,
            root_table,
            rules,
            rule_variants,
            cache,
            mt,
            mode,
            probe_budget,
            enum_budget,
            closure_trace,
        };
        // Seeds the chain's automaton state at the root's own stratum, disabling template entry forever if the root is partial.
        let seed_state = ChainState::seed(g, root_stratum.0, entry.partial);
        extend(
            &root_ctx,
            &word,
            &chain0,
            &root_variants,
            &root_stripped,
            0,
            width,
            &seed_state,
            &mut acc,
        );
    }

    (acc.recs, acc.report)
}

/// `build_composites`'s thin, env-driven wrapper: builds its own `MorphotacticIndex` (grammar-
/// cheap -- linear in rules/templates/slots, never the expensive recursive probe), resolves
/// `ExploreMode` from `HC_PREEXPAND_FLAT`, and an optional `ProbeBudget` from
/// `HC_PREEXPAND_PROBE_CAP`. Test-only: a convenience wrapper for tests that don't need a shared
/// `MorphotacticIndex`/`ProbeBudget`.
#[cfg(test)]
pub(crate) fn build_composites(
    g: &Grammar,
    width: usize,
    phon: Option<&PhonologyProbe>,
) -> (Vec<CompositeRec>, CompositeReport) {
    let mt = MorphotacticIndex::build(g);
    let mode = crate::morphotactics::explore_mode_from_env();
    let cap = crate::morphotactics::probe_cap_from_env();
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let probe_budget = cap.map(|cap| ProbeBudget {
        cap,
        counter: &counter,
    });
    // Default-on budget, env-driven like everything else in this thin wrapper.
    let enum_budget = EnumerationBudget::from_env();
    build_composites_with_mode(g, width, phon, &mt, mode, probe_budget, &enum_budget)
}

/// Build every rule-application/fusion composite for `g` (module doc). `width` is the same tag
/// digit width `crate::emit::emit` computes; `phon` is the SAME `PhonologyProbe` instance
/// `emit.rs` already builds once per grammar (`None` for a grammar with no phonological rules at
/// all). `mt`/`mode` are the morphotactic-pruning automaton and its flat/pruned escape hatch
/// (module doc addendum, `crate::morphotactics`) -- `crate::emit::emit_with_precision` builds `mt`
/// ONCE and shares it with `crate::emit::build_structural_composites` too. `probe_budget` is the
/// `HC_PREEXPAND_PROBE_CAP` measurement-only safety valve (`crate::morphotactics::ProbeBudget`'s own
/// doc), also shared across both builders by the same caller -- `None` in production.
///
/// **Parallelized across roots** (perf: this function's own `extend`-driven `probe_synthesize`
/// fan-out was measured at 53% of Amharic's emit wall time, `HC_EMIT_PROFILE`): the outer
/// `(stratum, entry)` loop is flattened into `RootWork` items and run through a dedicated rayon
/// pool with `crate::emit::PROBE_STACK_BYTES`-sized worker stacks (same reason
/// `crate::junctions::PhonologyProbe`'s own pool needs one -- `probe_synthesize`'s recursion
/// overflows rayon's default 2-8MB stacks on Amharic's deep composite chains), one item per work
/// unit, each producing its OWN local `(Vec<CompositeRec>, CompositeReport)` (see
/// `process_root_work`'s doc for why per-entry, not per-allomorph or grammar-wide, is the correct
/// granularity for `Acc::seen`). Results are collected via `par_iter().map(..).collect::<Vec<_>>()`,
/// which preserves the ORIGINAL input order regardless of completion order, then merged into the
/// final `recs`/`report` by iterating that Vec in order -- so the emitted lexc's composite-entry
/// order (`emit.rs` writes `composites` in exactly the order this function returns them) is
/// byte-for-byte identical to the old sequential loop's. `rayon::iter::ParallelExtend`/`sum` are
/// deliberately NOT used for the report counters: a plain ordered fold keeps the merge trivially
/// auditable against the sequential version, and the counts are cheap `usize` adds regardless.
/// Recursion inside `extend` itself stays entirely sequential and grammar-bounded, as
/// before -- only this OUTERMOST per-root level is parallelized. `RuleCache` is built once here and
/// shared read-only (its own module doc: "thereafter read-only... shares one `&RuleCache` across
/// every worker with zero contention"); `PhonologyProbe`'s two per-text caches are `Mutex`-backed
/// and already safe under concurrent access (its own doc) -- pre-warmed by
/// `build_rule_variants_all_tables` below so the parallel root workers never race a first-touch
/// compute into them.
#[cfg(test)]
pub(crate) fn build_composites_with_mode(
    g: &Grammar,
    width: usize,
    phon: Option<&PhonologyProbe>,
    mt: &MorphotacticIndex,
    mode: ExploreMode,
    probe_budget: Option<ProbeBudget<'_>>,
    enum_budget: &EnumerationBudget,
) -> (Vec<CompositeRec>, CompositeReport) {
    build_composites_with_mode_and_trace(
        g,
        width,
        phon,
        mt,
        mode,
        probe_budget,
        enum_budget,
        None,
    )
}

pub(crate) fn build_composites_with_mode_and_trace(
    g: &Grammar,
    width: usize,
    phon: Option<&PhonologyProbe>,
    mt: &MorphotacticIndex,
    mode: ExploreMode,
    probe_budget: Option<ProbeBudget<'_>>,
    enum_budget: &EnumerationBudget,
    closure_trace: Option<&crate::characterization::ClosureTrace>,
) -> (Vec<CompositeRec>, CompositeReport) {
    if !should_run(g, phon) {
        return (Vec::new(), CompositeReport::default());
    }

    let rules = candidate_rules(g);
    let cache = RuleCache::build(g);

    // Flattened in original order: this is exactly what determines emitted composite lexc line order, so the merge below must reassemble per-entry results in the same sequence.
    let mut work: Vec<RootWork> = Vec::new();
    let mut table_ids: Vec<u16> = Vec::new();
    for sd in &g.strata {
        for &entry_id in &sd.entries {
            let entry = &g.entries[entry_id.0 as usize];
            let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
            let table_id = g.strata[root_stratum.0 as usize].table.0;
            if !table_ids.contains(&table_id) {
                table_ids.push(table_id);
            }
            work.push(RootWork { entry_id });
        }
    }

    // Warms every `PhonologyProbe` cache entry `build_allomorph_variants` needs, for every table, before any root worker below can race a first-touch compute into it.
    let rule_variants_by_table = build_rule_variants_all_tables(g, phon, &rules, &table_ids);

    #[cfg(not(target_arch = "wasm32"))]
    let pool = rayon::ThreadPoolBuilder::new()
        .stack_size(crate::emit::PROBE_STACK_BYTES)
        .build()
        .expect("build preexpand composite rayon pool");

    #[cfg(target_arch = "wasm32")]
    let per_entry: Vec<(Vec<CompositeRec>, CompositeReport)> = work
        .iter()
        .map(|w| {
            process_root_work(
                g,
                width,
                &rules,
                &cache,
                &rule_variants_by_table,
                mt,
                mode,
                probe_budget,
                enum_budget,
                closure_trace,
                w,
            )
        })
        .collect();
    #[cfg(not(target_arch = "wasm32"))]
    let per_entry: Vec<(Vec<CompositeRec>, CompositeReport)> = if closure_trace.is_some() {
        // Stable root/rule order keeps traced terminal evidence independent of rayon scheduling.
        work.iter()
            .map(|w| {
                process_root_work(
                    g,
                    width,
                    &rules,
                    &cache,
                    &rule_variants_by_table,
                    mt,
                    mode,
                    probe_budget,
                    enum_budget,
                    closure_trace,
                    w,
                )
            })
            .collect()
    } else {
        pool.install(|| {
            work.par_iter()
                .map(|w| {
                    process_root_work(
                        g,
                        width,
                        &rules,
                        &cache,
                        &rule_variants_by_table,
                        mt,
                        mode,
                        probe_budget,
                        enum_budget,
                        closure_trace,
                        w,
                    )
                })
                .collect()
        })
    };

    let mut recs = Vec::new();
    let mut report = CompositeReport::default();
    for (r, rep) in per_entry {
        recs.extend(r);
        report.pairs_probed += rep.pairs_probed;
        if report.pairs_probed_by_depth.len() < rep.pairs_probed_by_depth.len() {
            report
                .pairs_probed_by_depth
                .resize(rep.pairs_probed_by_depth.len(), 0);
        }
        for (depth, count) in rep.pairs_probed_by_depth.into_iter().enumerate() {
            report.pairs_probed_by_depth[depth] += count;
        }
        report.synth_successes += rep.synth_successes;
        report.interdigitation_entries += rep.interdigitation_entries;
        report.fusion_entries += rep.fusion_entries;
        report.pending_successors += rep.pending_successors;
        report
            .pending_rule_ordinals
            .extend(rep.pending_rule_ordinals);
        report.covered_infix_rules.extend(rep.covered_infix_rules);
    }

    (recs, report)
}

#[cfg(test)]
mod pruning_tests {
    use super::*;
    use crate::morphotactics::ExploreMode;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
    }

    /// One stratum, one template `[slot0: mrA; slot1: mrB]`, plus a trivial phonological rule so `should_run` is true; `vacuous` selects slot0's rule shape, real surface material vs. a bare `CopyFromInput`.
    fn slot_gate_fixture(vacuous: bool) -> String {
        let slot0_output = if vacuous {
            r#"<CopyFromInput index="stemA" />"#.to_string()
        } else {
            r#"<InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments><CopyFromInput index="stemA" />"#.to_string()
        };
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>PruningDepth0Gate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="pr1">
        <Name>PR</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="pr1">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrA" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>a</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subA">
                <MorphologicalInput><PhoneticSequence id="stemA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput>{slot0_output}</MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>A</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrB" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>b</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subB">
                <MorphologicalInput><PhoneticSequence id="stemB"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments><CopyFromInput index="stemB" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>B</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>T</Name>
            <Slot morphologicalRules="mrA"><Name>s0</Name></Slot>
            <Slot morphologicalRules="mrB"><Name>s1</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
        )
    }

    /// A slot-only rule not first-reachable is not probed at depth 0 under `Pruned`, but is under `Flat` (kept as the A/B baseline) -- proving `extend` actually consults the automaton, not just that it computes the right answer in isolation.
    #[test]
    fn mandatory_non_vacuous_slot0_blocks_slot1_probe_at_depth0() {
        let g = load(&slot_gate_fixture(false));
        assert!(
            should_run(&g, PhonologyProbe::new(&g).as_ref()),
            "fixture must exercise should_run"
        );
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);

        let (_, flat) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Flat,
            None,
            &EnumerationBudget::unbounded(),
        );
        let (_, pruned) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Pruned,
            None,
            &EnumerationBudget::unbounded(),
        );

        // Flat (ignores morphotactics) probes both mrA and mrB at depth 0; pruned must skip mrB, blocked by the mandatory non-vacuous slot 0.
        assert_eq!(
            flat.pairs_probed_by_depth[0], 2,
            "flat mode probes both candidates at depth 0"
        );
        assert_eq!(
            pruned.pairs_probed_by_depth[0], 1,
            "pruned mode must not probe slot 1's rule at depth 0 while slot 0 is mandatory/non-vacuous"
        );
    }

    /// The variant where slot0's rule is instead vacuous (bare `CopyFromInput`, no surface material): it classifies `Role::None` and is never a candidate rule itself, but slot 1's rule must still be probed at depth 0 under `Pruned` -- a skippable mandatory slot must never become a hard barrier.
    #[test]
    fn vacuous_slot0_lets_slot1_be_probed_at_depth0_under_pruning() {
        let g = load(&slot_gate_fixture(true));
        assert!(
            should_run(&g, PhonologyProbe::new(&g).as_ref()),
            "fixture must exercise should_run"
        );
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);

        let (_, flat) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Flat,
            None,
            &EnumerationBudget::unbounded(),
        );
        let (_, pruned) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Pruned,
            None,
            &EnumerationBudget::unbounded(),
        );

        // mrA (vacuous) is `Role::None`, so only mrB is ever attempted at depth 0; the point here is that pruning does not lose that attempt.
        assert_eq!(
            flat.pairs_probed_by_depth[0], 1,
            "only mrB is a candidate rule in this fixture"
        );
        assert_eq!(
            pruned.pairs_probed_by_depth[0], 1,
            "a vacuous mandatory slot 0 must not block slot 1's rule from depth-0 probing"
        );
    }

    /// `build_composites`'s default (unset `HC_PREEXPAND_FLAT`) path behaves like `build_composites_with_mode` under `Pruned` mode: the same depth-0 gating applies.
    #[test]
    fn build_composites_thin_wrapper_defaults_to_pruned() {
        assert!(
            std::env::var("HC_PREEXPAND_FLAT").is_err(),
            "this test assumes the env var is unset in the test process; do not set it globally"
        );
        let g = load(&slot_gate_fixture(false));
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let (_, report) = build_composites(&g, width, phon.as_ref());
        assert_eq!(
            report.pairs_probed_by_depth[0], 1,
            "build_composites must default to Pruned mode (mrB blocked at depth 0)"
        );
    }

    #[test]
    fn ordinary_preexpand_exhausts_a_four_rule_chain() {
        let phonology = r#"
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="pr1">
        <Name>identity</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
  <Strata>"#;
        let xml = include_str!(
            "../tests/fixtures/pangloss/fst-completeness/late-structural-anchor-five-rule-chain/grammar.xml"
        )
        .replacen("<Strata>", phonology, 1)
        .replacen(
            "<Stratum characterDefinitionTable=\"t1\"",
            "<Stratum characterDefinitionTable=\"t1\" phonologicalRules=\"pr1\"",
            1,
        );
        let g = load(&xml);
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);

        let (_, report) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Pruned,
            None,
            &EnumerationBudget::unbounded(),
        );

        assert_eq!(report.pending_successors, 0);
        assert!(
            report.pairs_probed_by_depth.get(3).copied().unwrap_or(0) > 0,
            "the fourth ordinary rule must be explored rather than hidden behind a depth boundary"
        );
    }

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/data")
            .join(name);
        path.exists().then_some(path)
    }

    fn load_amharic() -> Option<Grammar> {
        let path = sample_path("amharic-hc.xml")?;
        let xml = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        Some(
            pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}")),
        )
    }

    /// The Amharic A/B subset gate: pruned exploration is a strict subset of flat exploration, recall-preserving by construction since pruning only removes adjacencies the morphotactics could never produce.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with \
                --include-ignored"]
    fn amharic_pruned_composites_are_a_subset_of_flat() {
        let Some(g) = load_amharic() else {
            eprintln!("skipping: amharic-hc.xml not present on disk");
            return;
        };
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);

        let t_flat = std::time::Instant::now();
        let (flat_recs, flat_report) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Flat,
            None,
            &EnumerationBudget::unbounded(),
        );
        let flat_elapsed = t_flat.elapsed();

        let t_pruned = std::time::Instant::now();
        let (pruned_recs, pruned_report) = build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Pruned,
            None,
            &EnumerationBudget::unbounded(),
        );
        let pruned_elapsed = t_pruned.elapsed();

        let flat_set: rustc_hash::FxHashSet<(String, String)> = flat_recs
            .iter()
            .flat_map(|r| {
                r.variants
                    .iter()
                    .map(move |v| (r.tag_lexc.clone(), v.clone()))
            })
            .collect();
        let pruned_set: rustc_hash::FxHashSet<(String, String)> = pruned_recs
            .iter()
            .flat_map(|r| {
                r.variants
                    .iter()
                    .map(move |v| (r.tag_lexc.clone(), v.clone()))
            })
            .collect();

        let missing: Vec<&(String, String)> = pruned_set.difference(&flat_set).collect();
        assert!(
            missing.is_empty(),
            "pruned composites must be a SUBSET of flat -- {} pruned entries are NOT in the flat \
             set (pruning must only ever REMOVE candidates, never add): {:?}",
            missing.len(),
            missing.iter().take(5).collect::<Vec<_>>()
        );

        let shrink_ratio = if pruned_report.pairs_probed > 0 {
            flat_report.pairs_probed as f64 / pruned_report.pairs_probed as f64
        } else {
            f64::INFINITY
        };
        println!(
            "Amharic pruning A/B: flat pairs_probed={} ({:?}), pruned pairs_probed={} ({:?}), \
             shrink={shrink_ratio:.2}x; flat entries={}, pruned entries={} (subset={})",
            flat_report.pairs_probed,
            flat_elapsed,
            pruned_report.pairs_probed,
            pruned_elapsed,
            flat_set.len(),
            pruned_set.len(),
            pruned_set.len() <= flat_set.len(),
        );
    }
}
