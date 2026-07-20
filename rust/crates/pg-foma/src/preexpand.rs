//! Rule-application pre-expansion + boundary-fusion composite probing (plan
//! `docs/fst-plan/foma-fst-plan.md` P1d): closes the two structural miss classes the P1c
//! investigation found on Amharic (32/32 misses classified, no third class) --
//!
//! 1. **Interdigitation** (`Role::Infix` rules -- Amharic's `-pfv-`/`-conv-`, 24/32 of the P1c
//!    misses): a standalone rule whose RHS interleaves `InsertSegments` actions AROUND a `Copy` of
//!    the root's own material has no literal string a plain lexc entry can express -- the inserted
//!    "ä" sits INSIDE the root's own copied consonants (root "ውልድ" + `-pfv-` -> "ውäልäድ"), so there
//!    is no boundary a two-entry (root-entry, then-continue-to-affix-entry) encoding can cut apart.
//! 2. **Ge'ez boundary fusion** (ordinary `Role::Prefix`/`Role::Suffix` rules whose adjacency to a
//!    SPECIFIC root's own final/initial glyph coalesces into a DIFFERENT glyph, 8/32): the existing
//!    deletion-only junction model ([`crate::junctions::PhonologyProbe`]) can express "one
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
//! [`pg_rules::morph::synthesize`] (the exact function the engine's own per-word synthesis pipeline
//! calls -- not a re-implementation), then run the REAL phonological cascade over the result via
//! [`pg_rules::surface_probe::probe_synthesize`] (the same probe machinery
//! [`crate::junctions::PhonologyProbe`] already drives) to get the TRUE, phonology-resolved surface
//! spelling. When that surface differs from a naive (pre-phonology) rendering of the very same
//! synthesized shape -- ALWAYS true for an `Infix` rule (there is no non-interleaved literal to even
//! compare against), SOMETIMES true for a `Prefix`/`Suffix` rule (fusion) -- emit ONE lexc entry
//! carrying BOTH the root's tag and the rule's tag, in the ENGINE'S OWN morph order.
//!
//! That order is COMPUTED, never assumed: [`morph_order_tags`] replays
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
//! [`should_run`] short-circuits before touching a single entry, which is what keeps Sena's
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
//! "ሌባዎቹ")` — caught by the gate at 31/32. [`extend`] therefore recurses on EVERY successful rule
//! application (dirty or clean), bounded by [`MAX_EXTRA_RULES`], EMITTING a composite (all of the
//! chain's tags, engine morph order) only for dirty steps: a clean step is already realized by the
//! ordinary per-rule lexc entries, so emitting it would only duplicate paths, but its output word
//! must still be explored for deeper fusions. Dirtiness at every depth is judged by the SAME
//! [`reachable_via_ordinary_emission`] check: the "one side" baseline is the root's own
//! spellings/stripped-spellings at depth 0, and the previous level's single rendered surface
//! (plus its stripped form ONLY if that level was clean — a dirty stem exists only as a composite
//! entry, which has no `Stripped` sibling) at depth >= 1.
//!
//! ## Avoiding redundant entries with the existing junction model
//! A `Prefix`/`Suffix` composite is only emitted when the fused spelling is NOT already reachable
//! through the ORDINARY two-entry path (`emit.rs`'s own literal root/affix entries, enriched by
//! [`PhonologyProbe::variants`]/[`PhonologyProbe::deletion_junctions`] when the grammar has any
//! phonological rules) -- [`reachable_via_ordinary_emission`] recomputes that same candidate string
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
//! hundreds of rules). Each probe calls [`pg_rules::morph::synthesize_cached`] (the `RuleCache`-aware
//! sibling of `synthesize`, now `pub` for exactly this cross-crate caller) against the SAME
//! `RuleCache` [`build_composites`] builds once per grammar, so a rule's LHS FST is compiled once at
//! cache-build time and read (not recompiled) on every one of its ~305k probes -- before this, each
//! probe recompiled that FST from scratch (Thompson NFA build + determinize), which dominated
//! Amharic's emit wall time (~35s of it). The **P6 successor** (replace-rule compilation,
//! `docs/fst-plan/foma-fst-plan.md` P6 item 1) retires this bridge entirely by compiling
//! interdigitating/fusing rules as real foma replace-calculus rules over root natural-class
//! patterns, composed directly into the network instead of enumerated per root and per chain --
//! exactly the same successor already named for [`crate::junctions::PhonologyProbe`]'s own
//! enumeration bridge.
//!
//! ## Morphotactic pruning (`docs/fst-plan/morphotactic-composite-pruning.md`, the Aweti scale fix)
//! [`extend`]'s flat recursion above chains **every** candidate rule onto every root at every
//! depth, gated only by the cheap `required_syn_fs` pre-filter -- workable at Amharic's scale
//! (module doc, "SCALE BRIDGE") but not at Aweti's (855 roots x 123 candidate rules): the flat
//! recursion explores rule orders the engine's own morphotactics (`pg-rules/src/stratum.rs`:
//! `synth_apply_mrules`/`synth_apply_templates`/`synth_slots_generic`) can never produce in
//! synthesis. [`crate::morphotactics::MorphotacticIndex`] builds a subset-construction automaton
//! over those exact engine functions once per grammar; [`extend`] consults
//! [`crate::morphotactics::MorphotacticIndex::next_state`] immediately before recursing on a
//! candidate rule, restricting the recursion to a STRICT SUBSET of what the flat version explored
//! (pruned exploration subset-of flat exploration by construction, so emitted composites are a
//! subset in the same relative order -- recall-preserving, never widening). See that module's own
//! doc for the full automaton design (loose-rule strata / template-slot sites / the vacuous-slot
//! recall trap) and the plan doc for the sizing investigation that motivated it.

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

use crate::emit::{rule_role, stripped_variants, surface_variants, Role};
use crate::junctions::PhonologyProbe;
use crate::morphotactics::{ChainState, EnumerationBudget, ExploreMode, MorphotacticIndex, ProbeBudget};
use crate::tags;

/// One rule-application/fusion composite: an extra "root-like" lexc entry whose upper tape carries
/// MULTIPLE tag symbols (root + 1..=[`MAX_EXTRA_RULES`] rules) instead of one, in the engine's own morph order
/// ([`morph_order_tags`]). Wired by `emit.rs` into one shared `Composites` lexicon reachable from
/// every roots-lexicon emission site (bare `Root`, `TLRoots`, each `G{gi}Roots`) and continuing
/// into `CompositeExit` (the union of every post-root continuation), so an interdigitated/fused
/// stem can still take ordinary prefixes/suffixes around it (plan P1d interaction item 4:
/// root-section replacement, not bare-only).
pub(crate) struct CompositeRec {
    /// The root morpheme this composite is anchored to (bookkeeping/diagnostics only; kept for a
    /// future gate/debug print that wants to group composite counts by root without re-deriving it
    /// from `tag_lexc`).
    #[allow(dead_code)]
    pub morpheme: MorphemeId,
    /// Every morpheme whose tag appears in `tag_lexc`, as `(is_root, id)` — `emit.rs` declares each
    /// in `Multichar_Symbols` (an Infix rule's morpheme is in NO deriv layer or slot, so no other
    /// collection site would declare it).
    pub chain_morphemes: Vec<(bool, MorphemeId)>,
    /// The escaped, ALREADY-CONCATENATED upper-tape tag string (all the chain's tags, in engine
    /// morph order).
    pub tag_lexc: String,
    /// The rendered, phonology-resolved surface spelling(s) (usually one; a `Vec` because a
    /// character-definition table can have a historical letter-series merger — see
    /// [`render_all_variants`] — or because a rule's own disjunctive allomorphs produce more than
    /// one distinct rendering for the same tag pair).
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompositeReport {
    /// (root allomorph, candidate rule) pairs actually attempted (after the cheap required-FS
    /// pre-filter) -- the module doc's scale-bridge number.
    pub pairs_probed: usize,
    /// Same count, broken down by recursion depth (0-indexed) -- morphotactic-pruning-plan doc
    /// "Instrumentation": the dynamic tree is the real unknown pruning must be measured against,
    /// and a flat total alone can't show WHERE the cost concentrates.
    pub pairs_probed_by_depth: [usize; MAX_EXTRA_RULES],
    /// Number of probed pairs where `synthesize_cached` returned at least one word (morphotactic-
    /// pruning-plan doc "Instrumentation") -- the dynamic-filter yield counterpart to `pairs_probed`.
    pub synth_successes: usize,
    /// Composite entries emitted for `Role::Infix` rules (miss class 5a).
    pub interdigitation_entries: usize,
    /// Composite entries emitted for `Role::Prefix`/`Role::Suffix` rules whose fused surface differs
    /// from what the ordinary two-entry emission already reaches (miss class 5b).
    pub fusion_entries: usize,
    /// `g.mrules` indices of `Role::Infix` rules that produced at least one composite entry --
    /// `emit.rs` suppresses the "standalone rule classifies as Infix; not representable" uncovered
    /// routing for exactly these (the construct IS representable now, via this module); an infix
    /// rule that matched zero roots stays uncovered, honestly.
    pub covered_infix_rules: std::collections::BTreeSet<u32>,
}

/// Whether this grammar can possibly need either mechanism at all -- `false` short-circuits
/// [`build_composites`] to a zero-cost, zero-entry no-op (module doc: what keeps Sena's gate
/// byte-for-byte).
pub(crate) fn should_run(g: &Grammar, phon: Option<&PhonologyProbe>) -> bool {
    phon.is_some() || any_infix_rule(g)
}

fn any_infix_rule(g: &Grammar) -> bool {
    (0..g.mrules.len())
        .any(|i| rule_role(g, MRuleId(i as u32)) == Role::Infix)
}

/// Every rule id whose PRIMARY allomorph classifies `Infix`/`Prefix`/`Suffix` (mirrors `emit.rs`'s
/// own `rule_role` convention for "how this rule is treated" everywhere else in the emitter).
/// `Reduplication` (peel's job, D6), `CircumfixPrefix`/`CircumfixSuffix` (P1d item 3, not exercised
/// by any reference-grammar corpus fixture at this stage), `Process`, and `None` are out of this
/// stage's scope.
/// Diagnostic-only: `candidate_rules(g).len()` without exposing the `Role` classification itself
/// outside the crate (`crate::emit`'s `composite_scale_hint` is the one external caller — see that
/// function's doc for why this exists).
pub(crate) fn candidate_rule_count(g: &Grammar) -> usize {
    candidate_rules(g).len()
}

fn candidate_rules(g: &Grammar) -> Vec<(MRuleId, Role)> {
    let mut out = Vec::new();
    for (i, r) in g.mrules.iter().enumerate() {
        if matches!(r, MorphRuleDef::Compounding(_)) {
            continue;
        }
        let mid = MRuleId(i as u32);
        let role = rule_role(g, mid);
        if matches!(role, Role::Prefix | Role::Suffix | Role::Infix) {
            out.push((mid, role));
        }
    }
    out
}

/// `(required_syn_fs, out_syn_fs, owning morpheme)` for the two rule kinds that carry allomorphs
/// (never called on `Compounding` -- [`candidate_rules`] never includes one).
fn rule_fs_and_morpheme(rule: &MorphRuleDef) -> (FsId, MorphemeId) {
    match rule {
        MorphRuleDef::AffixProcess(def) => (def.required_syn_fs, def.morpheme),
        MorphRuleDef::Realizational(def) => (def.required_syn_fs, def.morpheme),
        MorphRuleDef::Compounding(_) => unreachable!("candidate_rules excludes Compounding"),
    }
}

/// Replays `pg_parse::Morpher::allomorphs_in_morph_order`'s own algorithm (sort `Word::morphs` by
/// `order`, keep the FIRST occurrence of each distinct `AllomorphId`) over a freshly-synthesized
/// composite `Word`, then maps each surviving record to its pre-rendered tag string via `known` --
/// so composite tag order is COMPUTED from the exact same bookkeeping the real engine uses, never
/// assumed from any rule's role. Generic over chain length (`known` may name the root plus ANY
/// number of applied rules -- see the module doc's "Chaining" section). Returns `None` if a
/// surviving record's morpheme isn't in `known` (defensive; should never happen for a word seeded
/// with exactly one root record and only rules from this same chain applied to it).
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

/// One rule-allomorph's precomputed ordinary-surface strings (module doc's "Avoiding redundant
/// entries") -- see [`build_allomorph_variants`]'s doc for why this is hoisted out of
/// [`reachable_via_ordinary_emission`]'s hot path.
struct AllomorphVariants {
    /// `surface_variants(text) ∪ phon.variants(text)` for one allomorph's `InsertSegments` text.
    ordinary: Vec<String>,
    /// `phon.deletion_junctions(text)` for the same text (empty when `phon` is `None`; only ever
    /// consulted on the prefix side, mirroring `emit.rs`'s own `{roots}Stripped` convention).
    deletion: Vec<String>,
}

/// Precomputes [`reachable_via_ordinary_emission`]'s per-allomorph input ONCE per candidate rule,
/// rather than recomputing `surface_variants`/`PhonologyProbe::variants`/`deletion_junctions` for
/// the SAME allomorph text on every one of the ~305k (root, rule, depth) probes [`extend`] tries on
/// Amharic: `(table, phon, rule)` are all grammar-static for the lifetime of one [`build_composites`]
/// call, so each allomorph's ordinary/deletion surface sets never change across roots or depths --
/// only `root_variants`/`root_stripped`/`fused` (genuinely per-probe) still need to be supplied at
/// call time. Skips (empty entry) any allomorph with no `InsertSegments` text and returns an empty
/// `Vec` for a `Compounding` rule (mirrors the old inline function's `continue`/early-`return false`).
fn build_allomorph_variants(
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
            let mut ordinary: Vec<String> = surface_variants(table, text).map(|(v, _)| v).unwrap_or_default();
            if let Some(p) = phon {
                ordinary.extend(p.variants(text));
            }
            let deletion = phon.map(|p| p.deletion_junctions(text)).unwrap_or_default();
            Some(AllomorphVariants { ordinary, deletion })
        })
        .collect()
}

/// Module doc's "Avoiding redundant entries": does the ORDINARY two-entry emission (literal root
/// spelling(s), literal affix spelling(s), optionally enriched by [`PhonologyProbe`]) already reach
/// `fused` through some combination? Mirrors `emit.rs`'s own two routing rules exactly:
/// `PhonologyProbe::variants` spellings concatenate with a FULL root spelling; `deletion_junctions`
/// spellings concatenate with a root spelling that has had its OWN leading segment stripped
/// ([`stripped_variants`]) -- the `{roots}Stripped` mechanism `emit.rs`'s `build_deriv_chain` wires,
/// PREFIX-only (there is no suffix-side equivalent in `emit.rs` today, so a `Suffix` rule only ever
/// checks the plain `variants` × full-root combination). `allo_variants` is this rule's
/// [`build_allomorph_variants`] output, precomputed once outside the root/depth probe loop.
fn reachable_via_ordinary_emission(
    root_variants: &[String],
    root_stripped: &[String],
    allo_variants: &[AllomorphVariants],
    is_prefix: bool,
    fused: &str,
) -> bool {
    for av in allo_variants {
        for a in &av.ordinary {
            for r in root_variants {
                let concat = if is_prefix { format!("{a}{r}") } else { format!("{r}{a}") };
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

/// Cap on [`render_all_variants`]'s Cartesian product. Kept SMALL (not a large defensive-only
/// bound): measured on Amharic, `matching_reps_local`'s fallback branch (needed for genuine
/// correctness — see that function's doc) fires on ~30% of all probed segments, because vowel
/// quality changes on a Ge'ez consonant-vowel glyph are the ORDINARY case for this templatic
/// language family, not a rare exception — so an unbounded (or generously bounded) product
/// multiplies across every ambiguous position in a word and blows up total emitted lexc size
/// catastrophically (measured: cap 64 grew Amharic's lexc source from 4.59 MB/71,142 lines to
/// 21.65 MB/288,650 lines, which overflows the foma lexc compiler's own parse stack — a hard
/// crash, not just a slowdown). The Amharic "ገለፀ" recall miss this module fixes needs exactly 2
/// variants for its own composite record (`"ገለጸ"`/`"ገለፀ"` — verified directly); 4 leaves headroom
/// for a second independently-ambiguous position in the same word without reopening the blow-up
/// (measured: cap 4 keeps the same Amharic build well under the original baseline's order of
/// magnitude). A grammar that genuinely needs more than 4 co-occurring alternatives on one
/// composite would show up as its own recall-gate miss, at which point this constant (or the P6
/// replace-rule successor, module doc) is the fix, not silently raising it speculatively.
const MAX_RENDER_VARIANTS: usize = 4;

/// Every literal spelling a probed SEGMENT node can honestly mean, for [`render_all_variants`].
///
/// **Fast, common path — concrete, unmutated identity** (`char_def != NO_CHAR_DEF` and its own
/// feature lanes still unify with the node's current `lanes`): return ONLY that char-def's OWN
/// representations, mirroring `crate::emit::surface_variants`'s already-established pattern for
/// the identical shape of ambiguity (Sena `char4` = {"m","n"}: multiple spellings of the SAME
/// character, never a search across OTHER characters). This is both correct (this IS the
/// character the root was authored with) and cheap (no cross-table search).
///
/// **Fallback — abstract or mutated identity**: when `char_def == NO_CHAR_DEF` (a post-rewrite
/// node whose identity was cleared by a feature-changing rule) or the node's OWN char-def no
/// longer unifies with its current lanes (a rewrite changed features — e.g. a vowel-quality change
/// on a Ge'ez consonant-vowel glyph — without reassigning identity), there IS no preferred
/// spelling — fall through to the full `pg-rules`-equivalent search across every lane-unifiable
/// segment char-def (same result its own private `matching_reps`, `pg-rules/src/
/// surface_probe.rs:95-117`, computes — duplicated here for the same cross-crate reason that
/// module's own doc already accepts: `pg-rules` cannot expose a private helper for this one
/// caller). This is the branch [`MAX_RENDER_VARIANTS`]'s doc measures as ~30% of all probed
/// segments on Amharic — NOT rare, because vowel-quality changes are the ordinary case for this
/// templatic language family, which is exactly why the cap above must stay small.
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
                || table.unifiable_cds(CharDefId(char_def)).is_some_and(|b| b.contains(id.0))
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

/// Every distinct literal surface rendering of `segs` — the recall fix for a real miss class
/// `pg_rules::surface_probe::render_nodes` cannot see past: that function collapses each surviving
/// segment node to its FIRST matching character-definition representation in table order,
/// discarding every OTHER representation that also unifies with the node's own feature lanes (its
/// own private `matching_reps` already computes the full set; `render_nodes` just takes
/// `.next()`). For an alphabet with a historical letter-series merger — Ge'ez ጸ/ፀ are separate
/// `CharDefId`s but mutually unifiable (the same modern phoneme spelled two ways) — the specific
/// member a root's OWN allomorph was authored with is not necessarily table-order-first, so
/// `render_nodes` can silently render the WRONG literal spelling for a composite whose final
/// segment lands in such a class. Measured root cause of the Amharic "ገለፀ" recall miss (entry30
/// "explain" + mrule13 "-pfv-" infix + mrule18 "pfv.3m" suffix): `render_nodes` produced "ገለጸ"
/// (wrong series on the final consonant), which never matches the true surface, so `propose`
/// found zero candidates for a word the full engine confirms in exactly one analysis.
///
/// Fixed the only sound way per plan §0 (propose may only OVER-generate, never under-generate):
/// render every combination of each node's own matching representations (Cartesian product across
/// positions, [`MAX_RENDER_VARIANTS`]-capped — see that constant's doc for why the cap must stay
/// small on this grammar family) instead of guessing one. [`extend`]'s caller then tries each
/// resulting string exactly the way it tried the old single `post` — confirm prunes whichever
/// variants don't actually re-derive. Empty iff any surviving node has no matching representation
/// at all (an under-specified node — `render_nodes`'s own `None` case, ported as an empty `Vec`
/// here since this function's return type has no `Option`).
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

/// Bound on total composite chain length beyond the root (module doc, "Chaining"): a root plus at
/// most this many applied rules in one composite entry. `3` is the longest chain the recall gate
/// actually demanded (Amharic "ሌባዎቹ": root + def.m (CLEAN concatenation) + pl (fuses with def.m's
/// own ው) + poss.3m (fuses again) — the clean first step is why [`extend`] recurses through
/// non-dirty steps too, not just dirty ones); a grammar that genuinely needs a fourth stacked
/// fusion would show up as a recall-gate miss with an otherwise-empty class, at which point this
/// constant (or the P6 replace-rule successor, module doc) is the fix, not silently raising it
/// speculatively.
const MAX_EXTRA_RULES: usize = 3;

/// One in-progress composite chain step's context, threaded through [`extend`]'s recursion.
struct ExtendCtx<'a> {
    g: &'a Grammar,
    root_table: &'a CharDefTable,
    rules: &'a [(MRuleId, Role)],
    /// [`build_allomorph_variants`] output for each of `rules`, same order/indexing -- precomputed
    /// once per grammar (see that function's doc for why this must not be recomputed per probe).
    /// This is the only place `phon` still matters to [`extend`]'s own recursion (folded into each
    /// allomorph's `ordinary`/`deletion` sets at precompute time) -- no separate `phon` field is
    /// needed here anymore.
    rule_variants: &'a [Vec<AllomorphVariants>],
    cache: &'a RuleCache,
    /// Morphotactic pruning (module doc addendum): the automaton [`extend`] consults immediately
    /// before recursing on a candidate rule, and the flat/pruned escape hatch that lets it be
    /// bypassed for A/B measurement (`crate::morphotactics::ExploreMode`'s own doc).
    mt: &'a MorphotacticIndex,
    mode: ExploreMode,
    /// `HC_PREEXPAND_PROBE_CAP` measurement-only safety valve (`crate::morphotactics::ProbeBudget`'s
    /// own doc) -- `None` in production (the env var unset), so every `ctx.probe_budget` read below
    /// is a single branch on a `None` with zero further cost.
    probe_budget: Option<ProbeBudget<'a>>,
    /// Default-ON fail-fast enumeration budget (`crate::morphotactics::EnumerationBudget`'s own
    /// doc, Fix 1 for the Aweti-scale blow-up) -- unlike `probe_budget` above, this is ALWAYS live
    /// and never panics; `extend` checks it before every recursive step and ticks it alongside the
    /// existing counters.
    enum_budget: &'a EnumerationBudget,
}

/// [`extend`]'s output accumulator: the composite records, a `(tag_lexc, spelling)` dedup set
/// (multiple root allomorphs, rule orders, or disjunctive rule allomorphs can converge on a
/// byte-identical entry — a hash set, not an `O(n²)` scan, since chains at [`MAX_EXTRA_RULES`]` = 3`
/// visit thousands of candidates), and the counts report.
struct Acc {
    recs: Vec<CompositeRec>,
    seen: rustc_hash::FxHashSet<(String, String)>,
    report: CompositeReport,
}

/// Try extending `base_word` (already carrying `chain`'s tags, `chain.last()` being the most
/// recently applied step) with every remaining candidate rule; recurse up to
/// [`MAX_EXTRA_RULES`]. `redundancy_variants`/`redundancy_stripped` are the "one side" strings the
/// ORDINARY (non-composite) lexc path would concatenate the OTHER side's literal spelling against
/// at THIS level: at depth 0 that is the root's own [`surface_variants`]/[`stripped_variants`]; at
/// depth ≥ 1 it is the rendered surface(s) of the composite chain built so far (the previous
/// level's own lexc entry is a fixed, already-decided set of strings by the time a further rule's
/// ordinary entry would concatenate against it) — [`build_composites`]/this function's own
/// recursive call construct the right one for each depth, so [`reachable_via_ordinary_emission`]
/// is checked UNIFORMLY at every depth, never skipped by a `pre == post` shortcut (module doc's
/// investigation: a shortcut there is unsound whenever a rule's OWN LHS pattern silently drops
/// part of what it matched — e.g. Amharic's "ላ" ("to") rule's LHS consumes-but-does-not-copy the
/// pronoun root's leading glottal segment, so `pre` (the rule's own output) and `post` (after
/// phonology) already agree with EACH OTHER while both still differ from what `emit.rs`'s literal,
/// whole-root-text concatenation would produce — exactly the gap this composite mechanism exists
/// to close).
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
    if depth >= MAX_EXTRA_RULES {
        return;
    }
    // Fail-fast enumeration budget (Fix 1, `crate::morphotactics::EnumerationBudget`'s own doc):
    // checked once at the top of every call, same shape as the `depth >= MAX_EXTRA_RULES` guard
    // just above -- once ANY parallel root worker trips either measure, every other in-flight or
    // subsequent call (this grammar's remaining roots, and this call's own remaining recursion)
    // bails out here almost immediately instead of continuing to burn CPU toward the blow-up.
    if ctx.enum_budget.is_tripped() {
        return;
    }
    let base_fs = base_word.syn_fs.clone();
    for (ridx, &(mid, role)) in ctx.rules.iter().enumerate() {
        let rule = &ctx.g.mrules[mid.0 as usize];
        let (req, rule_morpheme) = rule_fs_and_morpheme(rule);
        // A rule already in this chain cannot apply again in the SAME composite (every reference
        // grammar's rules default `multipleApplication = 1`; a cheap guard against re-exploring the
        // same step, not a correctness requirement `synthesize` itself would enforce here).
        if chain.iter().any(|(m, _)| *m == rule_morpheme) {
            continue;
        }
        // Morphotactic pruning (module doc addendum, `crate::morphotactics`): restricts the
        // recursion to a rule adjacency the engine's own stratum/template machinery can actually
        // produce. `ExploreMode::Flat` is the pre-fix escape hatch (`Some(state.clone())`
        // unconditionally) kept for A/B measurement; production always runs `Pruned`. This sits
        // BEFORE the FS pre-filter below (plan doc "Wiring") -- a pure subset restriction, so
        // dirty/clean logic, redundancy baselines, dedup, rendering, and emitted order below are
        // completely unaffected; pruning can only skip a candidate the flat version would also have
        // tried.
        let Some(next_state) = (match ctx.mode {
            ExploreMode::Flat => Some(state.clone()),
            ExploreMode::Pruned => ctx.mt.next_state(state, mid, &base_fs, &ctx.g.fs_interner),
        }) else {
            continue;
        };
        let req_fs = ctx.g.fs_interner.get(req);
        // Cheap pre-filter (module doc): the SAME unifiability check `pg_rules::morph::synth_syn_fs`
        // makes internally -- skip building/compiling for a pair that provably cannot match.
        if !req_fs.is_empty() && !is_unifiable(req_fs, &base_fs) {
            continue;
        }
        acc.report.pairs_probed += 1;
        acc.report.pairs_probed_by_depth[depth] += 1;
        if let Some(budget) = &ctx.probe_budget {
            budget.tick();
        }
        ctx.enum_budget.tick_probe();

        let synth_out = synthesize_cached(ctx.g, mid, base_word, rule, ctx.cache);
        if !synth_out.is_empty() {
            acc.report.synth_successes += 1;
        }
        for w in synth_out {
            let Some(segs) = surface_probe::probe_synthesize(ctx.g, &w.shape, ctx.cache) else {
                continue;
            };
            // Recall fix (module doc addendum, `render_all_variants`): a character-definition
            // table can have merged letter-series ambiguity (Ge'ez ጸ/ፀ), where `pg_rules::
            // surface_probe::render_nodes` collapses to a single, sometimes-WRONG rendering.
            // `render_all_variants` yields every literal spelling the probed shape can honestly
            // mean, computed ONCE per synthesized `w` here (NOT once per recursive call — see
            // below) so this stays the same O(pairs_probed) shape as the original single-`post`
            // code, never O(pairs_probed × variants^depth): only the SET OF STRINGS a single
            // step considers grows; the recursion tree itself does not branch on it.
            let posts: Vec<String> = render_all_variants(ctx.root_table, &segs)
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            if posts.is_empty() {
                continue;
            }
            let is_infix = role == Role::Infix;
            // A variant is "dirty" iff it isn't already reachable via the ordinary two-entry
            // path; only DIRTY variants need their own composite entry (a clean one is already
            // realized correctly without this mechanism). Checking every variant here is cheap
            // (a handful of short-string comparisons) — the expensive part avoided is recursing
            // once per variant, not computing this per-variant flag.
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

            // Dirty variants are emitted as ONE composite record carrying every dirty spelling
            // (module doc, "Chaining": a dirty step is emitted; a clean step is NOT emitted — the
            // ordinary lexc entries already realize it correctly — but is still recursed through
            // below, see the recursion comment further down).
            if let Some(tag_lexc) = (!dirty_posts.is_empty()).then(|| morph_order_tags(&w, &next_chain)).flatten() {
                let new_variants: Vec<String> = dirty_posts
                    .iter()
                    .filter(|post| acc.seen.insert((tag_lexc.clone(), (***post).clone())))
                    .map(|post| (**post).clone())
                    .collect();
                if !new_variants.is_empty() {
                    acc.recs.push(CompositeRec {
                        morpheme: next_chain[0].0,
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
                    // Fail-fast enumeration budget: one composite lexc entry was just recorded
                    // (module doc's "composite entries" measure, the one that actually predicts the
                    // Aweti-scale blow-up -- see `crate::morphotactics::EnumerationBudget`'s doc).
                    ctx.enum_budget.add_entries(1);
                }
                if is_infix {
                    acc.report.covered_infix_rules.insert(mid.0);
                }
            }

            // Recurse (module doc, "Chaining") — ONCE per synthesized `w`, dirty or clean, never
            // once per variant (see the comment above `posts`). The ordinary-emission redundancy
            // baseline one level deeper is EVERY variant THIS level rendered (already a `Vec`, so
            // passing all of them costs nothing extra) — and, ONLY when EVERY variant at this
            // level was clean (no dirty variant at all: a mixed clean/dirty step still means a
            // composite got recorded above, so the stem is not purely ordinary), also each clean
            // variant's stripped (first-segment-removed) form: a clean stem is realized by
            // ordinary entries whose root half DOES have a `{roots}Stripped` sibling, so a
            // deletion-junction prefix one level up (Indonesian `meN` over a suffixed stem:
            // `tuliskan -> menuliskan`) is ordinary-reachable and must not read as dirty (measured:
            // without this, Indonesian grew 42 spurious fusion composites; with it, zero). After a
            // DIRTY step the stem exists ONLY as a composite entry, which has no Stripped sibling —
            // offering a stripped baseline there could mark a genuinely-needed deeper composite
            // clean, a downward (recall-losing) error, the plan's one forbidden direction.
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
        }
    }
}

/// Precomputes [`build_allomorph_variants`] for EVERY candidate rule against EVERY distinct root
/// table this grammar's entries actually use, up front, before [`build_composites`]'s per-root loop
/// starts (perf: parallelization plan part 2). This used to be a lazily-triggered
/// `HashMap::or_insert_with` inside the root loop -- correct, but paid its ~3.4s (Amharic,
/// profiled) first-touch cost serially, on whichever root happened to reach a given table first,
/// and would have needed its own locking to stay sound once the root loop itself went parallel
/// (below). Precomputing here instead means: (1) the cost becomes a one-time pass instead of a
/// serial tax paid inside the hot loop, and (2) every [`PhonologyProbe`] cache entry
/// [`build_allomorph_variants`] can populate (`variants`/`deletion_junctions`, both `Mutex`-backed
/// -- see that struct's doc) is already warm by the time [`build_composites`]'s parallel root
/// workers start calling into it, so those workers only ever hit a cache HIT (a cheap lock+clone),
/// never a concurrent first-touch compute race.
///
/// **Deliberately SEQUENTIAL over `rules`/`table_ids`, not `par_iter()`**: each cache-missing rule's
/// `PhonologyProbe::variants`/`deletion_junctions` call already fans out internally across
/// [`PhonologyProbe`]'s own dedicated pool (`junctions.rs`'s `pool` field, built once, sized for
/// `probe_synthesize`'s deep recursion). Driving THIS outer loop in parallel too -- whether on
/// rayon's global pool (the original nested-pools shape) or, tried next, funneled into that same
/// dedicated pool via an `install` indirection -- both measured SLOWER than plain sequential on
/// Amharic: the first oversubscribes (global-pool threads blocked-and-waiting on top of the
/// dedicated pool's own worker threads, both live at once); the second starves each individual
/// rule's expensive C1×C2 probe of workers by letting up to ~87 outer tasks compete for the SAME
/// pool's threads at once, so a single rule's fan-out -- the actually-expensive unit of work here
/// -- finds few or no idle workers left to steal onto. A plain sequential loop instead gives EVERY
/// cache-missing rule's probe call the ENTIRE dedicated pool to itself, one rule at a time --
/// slower-looking by rule-count but faster in wall time because the pool's parallelism lands where
/// the actual cost is (the alphabet-sized C1/C1×C2 fan-out inside one probe call), not spread thin
/// across many simultaneously-cheap-until-they-aren't outer tasks. Table-level nesting is kept
/// (`one_table` is still a closure over `rules`) purely for structure -- `table_ids` has length 1 on
/// every reference grammar, so this loop's own iteration count is never the bottleneck either way.
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

/// One [`build_composites`] top-level work item: a single lexical entry, together with the
/// grammar-static handles its (sequential, within-item) allomorph loop needs. Parallelized at THIS
/// granularity, not per-allomorph, because [`Acc::seen`]'s cross-allomorph dedup (module doc's
/// `Acc` field comment: "multiple root allomorphs ... can converge on a byte-identical entry") is
/// only ever exercised WITHIN one entry's own allomorphs -- every chain's `tag_lexc` begins with
/// [`tags::root_tag_lexc`] of THIS entry's own `morpheme`, a value unique to that `MorphemeId`
/// (`pg_foma::tags`'s fixed-width, prefix-disambiguated encoding: `<R:nnnn>` vs `<M:nnnn>` tokens of
/// equal length per numeral width, so two distinct token sequences can never collide as strings) --
/// so no two DIFFERENT entries can ever produce the same `(tag_lexc, spelling)` key. Keeping each
/// entry's own allomorphs on one worker with a fresh, entry-local `seen` set therefore reproduces
/// the old shared-`Acc`-with-one-global-`seen` sequential result byte-for-byte, while still letting
/// different entries run on different threads.
struct RootWork {
    entry_id: LexEntryId,
}

/// Process one [`RootWork`] item (all of one entry's own, non-pattern allomorphs) with a fresh,
/// entry-local [`Acc`] -- the parallel unit [`build_composites`] maps over. Mirrors the body of the
/// old sequential `for entry_id in &sd.entries { for allo in &entry.allomorphs { .. } }` loop
/// exactly, just scoped to one entry and returning its own accumulator instead of writing into a
/// grammar-wide shared one.
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
    work: &RootWork,
) -> (Vec<CompositeRec>, CompositeReport) {
    let mut acc = Acc {
        recs: Vec::new(),
        seen: rustc_hash::FxHashSet::default(),
        report: CompositeReport::default(),
    };

    // Fail-fast enumeration budget: skip this root entirely once tripped -- cheaper than even
    // entering the allomorph loop below (each `extend` call would bail near-instantly anyway, but
    // this avoids the per-allomorph setup work too).
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

        let Ok(shape) = pg_rules::shape_feat::segment_with_features(g, root_table, &allo.shape.text)
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
        // A fresh `ExtendCtx` per root: `root_table` is the OWNING stratum's table, which can in
        // principle differ per root in a multi-table grammar (module doc's
        // `f3_amharic_gate.rs`-documented hazard 2 — a non-issue for Amharic BY CONTENT, since all
        // 3 strata share one table, but not assumed here).
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
        };
        // Morphotactic pruning (module doc addendum): seed the chain's automaton state at the
        // root's own stratum, disabling template entry forever if the root is partial (engine fact
        // 5, `crate::morphotactics::ChainState::seed`'s own doc).
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

/// [`build_composites`]'s thin, env-driven wrapper: builds its own [`MorphotacticIndex`] (grammar-
/// cheap -- linear in rules/templates/slots, never the expensive recursive probe), resolves
/// [`ExploreMode`] from `HC_PREEXPAND_FLAT`, and an optional [`ProbeBudget`] from
/// `HC_PREEXPAND_PROBE_CAP`, purely for callers/tests that don't already have their own (the
/// PRODUCTION path -- `crate::emit::emit_with_precision` -- builds ONE shared
/// [`MorphotacticIndex`]/[`ProbeBudget`] for BOTH composite builders instead and calls
/// [`build_composites_with_mode`] directly; see that function's doc). `#[allow(dead_code)]`: no
/// non-test caller needs this convenience wrapper today (only `pruning_tests::
/// build_composites_thin_wrapper_defaults_to_pruned` exercises it) -- kept per the plan doc's own
/// "keep `build_composites` as thin wrapper... for any other callers/tests" instruction, same as
/// `#[cfg(test)]`-only accessors elsewhere in this crate.
#[allow(dead_code)]
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
    // Fix 1's default-on budget, env-driven like everything else in this thin wrapper (production
    // callers never hit this path -- `crate::emit::emit_with_precision` builds its own
    // `EnumerationBudget` once and calls `build_composites_with_mode` directly, module doc above).
    let enum_budget = EnumerationBudget::from_env();
    build_composites_with_mode(g, width, phon, &mt, mode, probe_budget, &enum_budget)
}

/// Build every rule-application/fusion composite for `g` (module doc). `width` is the same tag
/// digit width [`crate::emit::emit`] computes; `phon` is the SAME [`PhonologyProbe`] instance
/// `emit.rs` already builds once per grammar (`None` for a grammar with no phonological rules at
/// all). `mt`/`mode` are the morphotactic-pruning automaton and its flat/pruned escape hatch
/// (module doc addendum, `crate::morphotactics`) -- `crate::emit::emit_with_precision` builds `mt`
/// ONCE and shares it with `crate::emit::build_structural_composites` too. `probe_budget` is the
/// `HC_PREEXPAND_PROBE_CAP` measurement-only safety valve (`crate::morphotactics::ProbeBudget`'s own
/// doc), also shared across both builders by the same caller -- `None` in production.
///
/// **Parallelized across roots** (perf: this function's own `extend`-driven `probe_synthesize`
/// fan-out was measured at 53% of Amharic's emit wall time, `HC_EMIT_PROFILE`): the outer
/// `(stratum, entry)` loop is flattened into [`RootWork`] items and run through a dedicated rayon
/// pool with [`crate::emit::PROBE_STACK_BYTES`]-sized worker stacks (same reason
/// [`crate::junctions::PhonologyProbe`]'s own pool needs one -- `probe_synthesize`'s recursion
/// overflows rayon's default 2-8MB stacks on Amharic's deep composite chains), one item per work
/// unit, each producing its OWN local `(Vec<CompositeRec>, CompositeReport)` (see
/// [`process_root_work`]'s doc for why per-entry, not per-allomorph or grammar-wide, is the correct
/// granularity for `Acc::seen`). Results are collected via `par_iter().map(..).collect::<Vec<_>>()`,
/// which preserves the ORIGINAL input order regardless of completion order, then merged into the
/// final `recs`/`report` by iterating that Vec in order -- so the emitted lexc's composite-entry
/// order (`emit.rs` writes `composites` in exactly the order this function returns them) is
/// byte-for-byte identical to the old sequential loop's. `rayon::iter::ParallelExtend`/`sum` are
/// deliberately NOT used for the report counters: a plain ordered fold keeps the merge trivially
/// auditable against the sequential version, and the counts are cheap `usize` adds regardless.
/// Recursion inside [`extend`] itself (depth ≤ [`MAX_EXTRA_RULES`]) stays entirely sequential, as
/// before -- only this OUTERMOST per-root level is parallelized. `RuleCache` is built once here and
/// shared read-only (its own module doc: "thereafter read-only... shares one `&RuleCache` across
/// every worker with zero contention"); [`PhonologyProbe`]'s two per-text caches are `Mutex`-backed
/// and already safe under concurrent access (its own doc) -- pre-warmed by
/// [`build_rule_variants_all_tables`] below so the parallel root workers never race a first-touch
/// compute into them.
pub(crate) fn build_composites_with_mode(
    g: &Grammar,
    width: usize,
    phon: Option<&PhonologyProbe>,
    mt: &MorphotacticIndex,
    mode: ExploreMode,
    probe_budget: Option<ProbeBudget<'_>>,
    enum_budget: &EnumerationBudget,
) -> (Vec<CompositeRec>, CompositeReport) {
    if !should_run(g, phon) {
        return (Vec::new(), CompositeReport::default());
    }

    let rules = candidate_rules(g);
    let cache = RuleCache::build(g);

    // Flatten the `(stratum, entry)` work list in ORIGINAL order -- this order (plus each entry's
    // own internal allomorph/rule ordering, unaffected by any of this) is exactly what determines
    // the emitted composite lexc line order, so the merge below must reassemble per-entry results in
    // this same sequence.
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

    // Part 2 of the perf fix: warm every `PhonologyProbe` cache entry `build_allomorph_variants`
    // needs, for every table, before any root worker below can race a first-touch compute into it.
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
                w,
            )
        })
        .collect();
    #[cfg(not(target_arch = "wasm32"))]
    let per_entry: Vec<(Vec<CompositeRec>, CompositeReport)> = pool.install(|| {
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
                    w,
                )
            })
            .collect()
    });

    let mut recs = Vec::new();
    let mut report = CompositeReport::default();
    for (r, rep) in per_entry {
        recs.extend(r);
        report.pairs_probed += rep.pairs_probed;
        for d in 0..MAX_EXTRA_RULES {
            report.pairs_probed_by_depth[d] += rep.pairs_probed_by_depth[d];
        }
        report.synth_successes += rep.synth_successes;
        report.interdigitation_entries += rep.interdigitation_entries;
        report.fusion_entries += rep.fusion_entries;
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

    /// One stratum, one template `[slot0: mrA (mandatory); slot1: mrB (mandatory)]`, plus a trivial
    /// phonological rule so [`should_run`] is true (module doc's morphotactic-pruning addendum
    /// requires exercising the REAL `should_run`-gated pipeline, not a synthetic bypass). `vacuous`
    /// selects slot0's rule shape: `false` -> `mrA`'s rhs is `[InsertSegments("a"), Copy(0)]` (real
    /// surface material, NOT skippable); `true` -> rhs is a bare `CopyFromInput` (module doc's
    /// vacuous-slot recall trap -- skippable).
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

    /// New tests item 2 (plan doc): a slot-only rule not first-reachable must not be probed at
    /// depth 0 under `Pruned` -- but IS probed at depth 0 under `Flat` (the pre-fix behavior, kept
    /// only as the A/B baseline) -- proving the wiring in [`extend`] actually consults
    /// [`crate::morphotactics::MorphotacticIndex::next_state`], not just that the automaton itself
    /// (covered by `crate::morphotactics`'s own unit tests) computes the right answer in isolation.
    #[test]
    fn mandatory_non_vacuous_slot0_blocks_slot1_probe_at_depth0() {
        let g = load(&slot_gate_fixture(false));
        assert!(should_run(&g, PhonologyProbe::new(&g).as_ref()), "fixture must exercise should_run");
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);

        let (_, flat) =
            build_composites_with_mode(
                &g,
                width,
                phon.as_ref(),
                &mt,
                ExploreMode::Flat,
                None,
                &EnumerationBudget::unbounded(),
            );
        let (_, pruned) =
            build_composites_with_mode(
                &g,
                width,
                phon.as_ref(),
                &mt,
                ExploreMode::Pruned,
                None,
                &EnumerationBudget::unbounded(),
            );

        // Depth 0: mrA (slot 0, first-reachable) plus mrB under FLAT (which ignores morphotactics
        // entirely) -- 2 candidates probed. Under PRUNED, mrB (slot 1, blocked by the mandatory
        // non-vacuous slot 0) must NOT be probed at depth 0.
        assert_eq!(flat.pairs_probed_by_depth[0], 2, "flat mode probes both candidates at depth 0");
        assert_eq!(
            pruned.pairs_probed_by_depth[0], 1,
            "pruned mode must not probe slot 1's rule at depth 0 while slot 0 is mandatory/non-vacuous"
        );
    }

    /// New tests item 2's variant: when slot0's rule is instead VACUOUS (module doc's recall trap:
    /// its rhs is a bare `CopyFromInput`, no `InsertSegments` at all), it necessarily classifies
    /// `Role::None` ([`crate::emit::classify_affix`] -- no leading/trailing insert) and so is NEVER
    /// itself a `crate::preexpand` candidate rule, in EITHER mode (this is expected and correct:
    /// `classify_affix`/`candidate_rules` are unaffected by morphotactic pruning). What pruning must
    /// get right is slot 1's rule (`mrB`, a real `Role::Suffix` candidate): it must still be probed
    /// at depth 0 under `Pruned` (skippable slot 0 makes slot 1 first-reachable) -- the automaton
    /// must not lose recall by treating a surface-vacuous mandatory slot as a hard barrier. Contrast
    /// with `mandatory_non_vacuous_slot0_blocks_slot1_probe_at_depth0`, where the SAME slot 1 rule is
    /// correctly blocked when slot 0 is NOT vacuous.
    #[test]
    fn vacuous_slot0_lets_slot1_be_probed_at_depth0_under_pruning() {
        let g = load(&slot_gate_fixture(true));
        assert!(should_run(&g, PhonologyProbe::new(&g).as_ref()), "fixture must exercise should_run");
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);

        let (_, flat) =
            build_composites_with_mode(
                &g,
                width,
                phon.as_ref(),
                &mt,
                ExploreMode::Flat,
                None,
                &EnumerationBudget::unbounded(),
            );
        let (_, pruned) =
            build_composites_with_mode(
                &g,
                width,
                phon.as_ref(),
                &mt,
                ExploreMode::Pruned,
                None,
                &EnumerationBudget::unbounded(),
            );

        // mrA (vacuous) is `Role::None` -- never a candidate rule at all, in either mode -- so only
        // mrB is ever attempted at depth 0. The point of this test is that pruning does NOT lose
        // that attempt (unlike the non-vacuous fixture, where it correctly does).
        assert_eq!(flat.pairs_probed_by_depth[0], 1, "only mrB is a candidate rule in this fixture");
        assert_eq!(
            pruned.pairs_probed_by_depth[0], 1,
            "a vacuous mandatory slot 0 must not block slot 1's rule from depth-0 probing"
        );
    }

    /// [`build_composites`] (the thin, env-driven wrapper -- module doc addendum) must still behave
    /// like [`build_composites_with_mode`] under the default (unset `HC_PREEXPAND_FLAT`) production
    /// path: `Pruned` mode, so the same depth-0 gating as
    /// `mandatory_non_vacuous_slot0_blocks_slot1_probe_at_depth0` applies.
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
        Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}")))
    }

    /// New tests item 2 (plan doc): the Amharic A/B subset gate -- pruned exploration must be a
    /// STRICT SUBSET of flat exploration (recall-preserving by construction: pruning only removes
    /// rule adjacencies the engine's own morphotactics could never produce, module doc).
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
        let (flat_recs, flat_report) =
            build_composites_with_mode(
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
        let (pruned_recs, pruned_report) =
            build_composites_with_mode(
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
            .flat_map(|r| r.variants.iter().map(move |v| (r.tag_lexc.clone(), v.clone())))
            .collect();
        let pruned_set: rustc_hash::FxHashSet<(String, String)> = pruned_recs
            .iter()
            .flat_map(|r| r.variants.iter().map(move |v| (r.tag_lexc.clone(), v.clone())))
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
