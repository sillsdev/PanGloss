//! Reduplication peel: a fresh port of
//! `hc-hybrid/src/proposers.rs::ReduplicationProposer` (`ReduplicationProposer.cs`'s four scan
//! kinds — prefix-copy, suffix-copy, separator+tail-copy, separator+suffix-peel), with the
//! recursion target swapped from the trie-based bare walker to the caller's foma proposer:
//! reduplication peeling is proposer-agnostic — it only needs a `fn(&str) -> Vec<Candidate>` to
//! recurse residuals into.
//!
//! Reuses `crate::emit`'s own port of `hc-hybrid/src/token.rs`'s `MorphOp`/`ClassifyAffix`
//! (`Role`/`classify_affix`, made `pub(crate)` there for exactly this reason) plus its
//! `owning_morpheme`/`surface_table` helpers, rather than re-porting the same classification logic
//! a second time in this module — both the emitter and this peel need the identical affix-role
//! answer, and `hc-hybrid` itself is being sunset, so neither may depend on it.
//!
//! ## Chain depth and nested reduplication
//! `ReduplicationPeeler::peel_candidates` originally peeled at most ONE layer: strip a
//! prefix/suffix/separator copy, then hand the residual STRAIGHT to the caller's FST `propose`
//! closure (never back to itself). That is faithful for every reference/synthetic grammar this
//! crate has seen (a single reduplication rule per word), but is a real, silent recall gap for a
//! grammar whose confirm-side oracle (`pg_rules::morph::synthesize`/`pg_parse::Morpher`) can
//! legitimately chain TWO reduplication-classified rules (or the same rule applied twice via
//! `max_apps`) — HermitCrab's own morphotactics has no rule against it, so `propose`-side
//! under-generation here would be exactly the "implemented in confirm, never proposed" shape this
//! crate's honest-capability discipline exists to close (`Compounding`/`MorphRuleOrder::Unordered`'s
//! own citation). This module now
//! ALSO tries peeling the residual again — the same operation one level deeper — closing that gap.
//!
//! **The hazard this creates, and how the chain-depth cap closes it.** A self-similar surface string (the
//! degenerate case: every character identical) matches this module's prefix/suffix/separator scans
//! at MANY positions simultaneously, and — once residuals recurse into a further peel — each match
//! spawns its own recursive subtree. Recursion depth is bounded above by the word's own length (each
//! layer consumes >= 1 character), but for a long enough adversarial word this is exactly the
//! Aweti-style "derivation chain deep enough to matter" failure class (deep native
//! recursion risks a stack overflow; the branching multiplies total work superlinearly in the
//! number of layers actually taken). `crate::compose_budget::ComposeBudget::check_chain_depth` —
//! until this change, a schema-only type with no production caller (that module's own doc) — is
//! wired here as the fix: `ReduplicationPeeler::propose_for_residual` checks it once per
//! reduplication layer it is ABOUT to use, turning a runaway chain into the typed, deterministic
//! `crate::compose_budget::ComposeError::ChainDepthExceeded` instead of an unbounded
//! stack/candidate blow-up. This
//! module's own operation is declared as the required-runtime-feature
//! `RUNTIME_FEATURE_REDUPLICATION_PEEL` — see that constant's own doc.
//!
//! **Why the check sits at "a real match was found," not at "entering the recursive scan."** The
//! obvious-looking alternative — check the budget at the TOP of the recursive scan function, before
//! it does any work — is UNSOUND for this specific shape: because the nested-peel attempt on a
//! residual is unconditional (this module cannot know in advance whether a residual has any further
//! reduplication structure without scanning it), gating function ENTRY would make even a single
//! ordinary, non-nested reduplication (whose residual has no further structure at all, e.g. this
//! suite's own `machine/conformance/languages/suffixing-extension-slot-ordering`'s `mrRedup`) trip a
//! small configured cap merely because a second, ultimately-empty, cheap attempt was made — a false
//! refusal of a construct this crate already faithfully supports. Checking instead at the point
//! `ReduplicationPeeler::propose_for_residual` is about to USE a layer (i.e. is only ever reached
//! because THIS depth's own scan found a real match) means an attempt that finds nothing is free —
//! it never consults the budget at all — while a genuine chain of N real, successive matches trips
//! the cap at exactly the (N+1)th real layer, never one that was merely tried and empty.
//!
//! **Big-O.** Absent this change: `peel_candidates` is `O(word length)` per call (one scan, no
//! recursion) — this is exactly the shape the module previously had, and remains the shape for
//! every word whose residual has no further reduplication structure of its own (every reference/
//! synthetic single-layer grammar this crate has seen, so their own cost is unchanged). With genuine
//! D-deep nested structure, cost is `O(word length ^ D)` in the worst (fully self-similar) case
//! before this change's chain-depth cap intervenes — `D` bounded by
//! the configured `chain_depth_cap` field once one is configured (`None`,
//! production's default via `crate::compose_budget::ComposeBudget::from_env`, leaves `D` bounded
//! only by the word's own length, per `crate::compose_budget::ComposeBudget`'s own documented
//! "uncalibrated default" caveat — the same one every other dimension in that module already
//! carries until a calibrated number lands.
//!
//! ## The recall proof — precisely what is proven vs. left open
//! Status, stated precisely rather than rounded up:
//! - **Proven**: for the one real, previously-zero-coverage in-repo construct available to check
//!   (`machine/conformance/languages/suffixing-extension-slot-ordering`'s `mrRedup`,
//!   "kimbiakimbia"), the depth-1 peeler proposes every analysis accepted by `pg_parse::Morpher`.
//!   The peeler may safely over-propose because the confirm pass rejects spurious candidates
//!   (`tests/f6_reduplication_peel_chain_depth.rs::kimbiakimbia_reduplication_is_recovered_with_oracle_containment`).
//!   This is one word/one grammar, not an exhaustive proof over every possible single-layer shape
//!   the four scan kinds (prefix/suffix/separator+tail/separator+suffix-peel) can produce.
//! - **Proven**: the new nested-nested (depth >= 2) recursion this change adds never REGRESSES the
//!   depth-1 case (`peel::tests::ordinary_single_layer_reduplication_never_trips_the_smallest_cap`)
//!   and never explodes/hangs on an adversarial input — it fails deterministically once genuinely
//!   deep (`peel::tests::deep_self_similar_chain_is_refused_deterministically_under_a_small_cap`,
//!   `tests/f6_reduplication_peel_chain_depth.rs::deep_self_similar_chain_is_refused_deterministically`).
//! - **Left OPEN**: whether depth >= 2 nested reduplication itself achieves oracle CONTAINMENT (not
//!   just "doesn't crash") is genuinely unproven — no in-repo conformance fixture exercises a real
//!   TWO-rule reduplication chain (or one rule at `max_apps >= 2`) today, so there is no oracle
//!   witness to check the new recursive candidates against at all. The capability disposition
//!   reflects this honestly: `crate::capability::ReduplicationPeelSupportedPredicate` verdicts
//!   ConfirmOnly for the depth-1-eligible case (never Admit), which already
//!   means confirm is trusted to prune whatever this module over-generates, nested candidates
//!   included; a wrong/spurious nested candidate is therefore safe by construction (confirm drops
//!   it), but a grammar whose ORACLE truly needs depth >= 2 to recall a real word has no witness
//!   proving this module's nested candidates actually reach it. Closing this needs either a new
//!   conformance fixture with a genuine 2-reduplication-rule chain, or a `max_apps >= 2` single-rule
//!   case — flagged here for a follow-on, not silently assumed proven.
//! - **Left OPEN** (unrelated to nesting): multiplicity beyond "exactly 1" is unchecked — the one
//!   proven word above happens to have exactly one oracle analysis; whether this peel preserves
//!   the multiplicity-recovery guarantee (`crate::confirm`'s doc) for a word with SEVERAL
//!   distinct reduplication-derived analyses is not separately witnessed.

use pg_grammar::chardef::{CharDefId, CharDefTable};
use pg_grammar::model::{Grammar, MRuleId, OutputAction};
use pg_shape::{NodeKind, Shape};

use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::emission_support::{
    classify_affix, owning_morpheme, reduplication_rule_is_peelable, surface_table, Role,
};
use crate::tags::Candidate;

/// The required-runtime-feature identifier
/// this module's own operation contributes to a compiled pack's
/// `pg_pack::compat::RequiredRuntimeFeatures::runtime_operations` set: only constructs needing a
/// runtime operation (e.g. reduplication → the query-time peel op) contribute. A grammar with `ReduplicationPeeler::has_redup_rules() == true` requires a
/// Runtime whose OWN provided set includes this string; a grammar with none needs nothing from this
/// module at all (every reference/synthetic grammar with zero reduplication rules — `has_redup_
/// rules() == false` — never depends on it: most constructs are fully lowered and impose no
/// runtime requirement).
///
/// **Declared in `pg-health`; consuming it is `pg-pack`'s responsibility.** `pg-pack` is a
/// separate crate/single-owner boundary this module does not cross; this constant is the stable
/// identifier a `pg-pack` manifest-builder should read `ReduplicationPeeler::has_redup_rules`
/// against and push into the pack's required set, rather than inventing a second ad hoc name for
/// the same operation. Re-exported here at its historical path so this crate's own callers are
/// unaffected by the move.
pub use pg_health::runtime_features::RUNTIME_FEATURE_REDUPLICATION_PEEL;

/// `ComposeBudget::check_chain_depth`'s `site` label for every check this module makes.
const CHAIN_DEPTH_SITE: &str = "peel::ReduplicationPeeler::propose_for_residual";

/// C# `ReduplicationProposer.IsReduplication` port (`.any()` over every allomorph, unlike `crate::emit::rule_role`'s first-allomorph-only): only `AffixProcessRule` is checked, never `RealizationalAffixProcessRule`; an allomorph `classify_affix` resolves to `CircumfixPrefix` over `Reduplication` correctly drops out of this scan too, since this module's one-sided surface match could never recall a circumfix-plus-reduplication surface anyway.
fn is_reduplication_rule(g: &Grammar, mid: MRuleId) -> bool {
    reduplication_rule_is_peelable(g, mid)
}

/// C# `ReduplicationProposer.RenderSurfaceOnly`: renders only the Segment-kind nodes of `shape` through `table`'s first representation, `None` the instant any Segment node has none.
fn render_surface_only(table: &CharDefTable, shape: &Shape) -> Option<String> {
    let mut out = String::new();
    for (_, kind, cd, _flags) in shape.interior() {
        if kind != NodeKind::Segment {
            continue;
        }
        match table.get(CharDefId(cd)).representations().first() {
            Some(rep) if !rep.is_empty() => out.push_str(rep),
            _ => return None,
        }
    }
    Some(out)
}

/// Grammar-only rule discovery for the redup peel — ported from
/// `hc-hybrid/src/proposers.rs::ReduplicationProposer`'s fields + `new` (`proposers.rs:90-139`).
/// Built once per grammar (identical every call, unlike the ephemeral `Trie`/beam-work params the
/// original's constructor also took: this port needs neither, since residuals recurse through the
/// caller's `propose` closure instead of a shared trie/walker).
pub struct ReduplicationPeeler {
    /// `AffixProcessRule`s whose RHS classifies as reduplication, in grammar document order.
    redup_rules: Vec<MRuleId>,
    /// `(suffix surface text, owning rule)` pairs for every SUFFIX-classified allomorph in the grammar; the separator+suffix-peel scan's search list.
    suffix_surfaces: Vec<(String, MRuleId)>,
}

impl ReduplicationPeeler {
    pub fn new(g: &Grammar) -> Self {
        let table = surface_table(g);
        let mut redup_rules = Vec::new();
        let mut suffix_surfaces = Vec::new();
        for stratum in &g.strata {
            for &mrule_id in &stratum.mrules {
                let def = &g.mrules[mrule_id.0 as usize];
                if is_reduplication_rule(g, mrule_id) {
                    redup_rules.push(mrule_id);
                    continue;
                }
                let Some(allomorphs) = def.affix_allomorphs() else {
                    continue; // CompoundingRule: not a MorphemicMorphologicalRule in C# either.
                };
                for allomorph in allomorphs {
                    if classify_affix(&allomorph.rhs) != Role::Suffix {
                        continue;
                    }
                    let Some(insert_shape) = allomorph.rhs.iter().find_map(|a| match a {
                        OutputAction::InsertSegments { shape, .. } => Some(shape),
                        _ => None,
                    }) else {
                        continue;
                    };
                    if let Some(surface_text) = render_surface_only(table, &insert_shape.shape) {
                        if !surface_text.is_empty() {
                            suffix_surfaces.push((surface_text, mrule_id));
                        }
                    }
                }
            }
        }
        ReduplicationPeeler {
            redup_rules,
            suffix_surfaces,
        }
    }

    /// Whether this grammar has any reduplication rule at all — `Self::peel_candidates` already
    /// early-returns empty when this is `false` (mirroring the original's own early-out), exposed
    /// separately so a caller (e.g. `crate::composite::FomaAnalyzer`) can skip building a
    /// `propose` closure entirely for a no-redup grammar like Sena.
    pub fn has_redup_rules(&self) -> bool {
        !self.redup_rules.is_empty()
    }

    /// C# `ReduplicationProposer.AnalyzeWord` (`ReduplicationProposer.cs:134-209`), recursion target
    /// swapped to the caller's `propose` closure instead of the trie-based bare walker —
    /// plus ALSO back to itself for a residual that
    /// carries its own further reduplication structure (module doc, "Chain depth and nested
    /// reduplication"). Operates on `char`s (Rust's `char` == a Unicode scalar value; every
    /// reference grammar's alphabet is BMP-only, where C#'s UTF-16 `string.Length`/`Substring`
    /// indexing and a `Vec<char>`'s indexing coincide exactly), so this never panics on a non-ASCII
    /// grammar's multi-byte UTF-8 word.
    ///
    /// `budget` is threaded through to `Self::propose_for_residual`'s
    /// `crate::compose_budget::ComposeBudget::check_chain_depth` call (module doc's "Big-O"
    /// section) — `Err(ComposeError::ChainDepthExceeded)` means a
    /// genuinely deep nested-reduplication chain exceeded `budget`'s configured
    /// `chain_depth_cap` field; the caller gets a typed, honest
    /// refusal for this word rather than this module silently doing an unbounded amount of work.
    pub fn peel_candidates(
        &self,
        g: &Grammar,
        word: &str,
        budget: &ComposeBudget,
        propose: &mut dyn FnMut(&str) -> Vec<Candidate>,
    ) -> Result<Vec<Candidate>, ComposeError> {
        self.peel_at_depth(g, word, 1, budget, propose)
    }

    /// `Self::peel_candidates`'s recursive core: `depth` (1-based) names which reduplication layer this call is peeling. See module doc for why the budget check lives in `Self::propose_for_residual`, not here.
    fn peel_at_depth(
        &self,
        g: &Grammar,
        word: &str,
        depth: usize,
        budget: &ComposeBudget,
        propose: &mut dyn FnMut(&str) -> Vec<Candidate>,
    ) -> Result<Vec<Candidate>, ComposeError> {
        let mut out = Vec::new();
        if self.redup_rules.is_empty() {
            return Ok(out);
        }
        let chars: Vec<char> = word.chars().collect();
        let len = chars.len();
        let max_copy_len = len / 2;

        for l in 1..=max_copy_len {
            // Prefix copy: chars[0..l] repeats immediately after -- strip it. The reduplicant sits at the front, so its morpheme precedes the base's -- `prepend = true`.
            if chars[0..l] == chars[l..2 * l] {
                let residual: String = chars[l..len].iter().collect();
                self.propose_for_residual(
                    g, &residual, None, true, depth, budget, propose, &mut out,
                )?;
            }
            // Suffix copy: the last l chars repeat the l chars before them -- strip it; the reduplicant trails the base, so `prepend = false`.
            if chars[len - l..len] == chars[len - 2 * l..len - l] {
                let residual: String = chars[0..len - l].iter().collect();
                self.propose_for_residual(
                    g, &residual, None, false, depth, budget, propose, &mut out,
                )?;
            }
        }

        // Separator + tail copy, and separator + suffix-peel + tail copy.
        for sep_pos in 1..len.saturating_sub(1) {
            let before = &chars[0..sep_pos];
            let copy = &chars[sep_pos + 1..len];
            if copy.is_empty() {
                continue;
            }
            if before.len() >= copy.len() && before[before.len() - copy.len()..] == *copy {
                let residual: String = before.iter().collect();
                // separator + tail copy: the reduplicant (the tail copy) is at the END -> append.
                self.propose_for_residual(
                    g, &residual, None, false, depth, budget, propose, &mut out,
                )?;
                continue; // plain tail matched -- do not also try the suffix-peel fallback.
            }
            for (suffix_text, suffix_rule) in &self.suffix_surfaces {
                let suffix_chars: Vec<char> = suffix_text.chars().collect();
                if suffix_chars.len() > copy.len() {
                    continue;
                }
                if copy[copy.len() - suffix_chars.len()..] != suffix_chars[..] {
                    continue;
                }
                let stripped_len = copy.len() - suffix_chars.len();
                if stripped_len == 0 {
                    continue;
                }
                let stripped_copy = &copy[..stripped_len];
                if before.len() >= stripped_copy.len()
                    && before[before.len() - stripped_copy.len()..] == *stripped_copy
                {
                    let residual: String = before.iter().collect();
                    // separator + suffix-peel + tail copy: reduplicant + peeled suffix both trail the base -> append.
                    self.propose_for_residual(
                        g,
                        &residual,
                        Some(*suffix_rule),
                        false,
                        depth,
                        budget,
                        propose,
                        &mut out,
                    )?;
                }
            }
        }
        Ok(out)
    }

    /// C# `ProposeForResidual` port: recurses `residual` through the caller's proposer and one layer deeper into `Self::peel_at_depth`, then wraps every base candidate with the reduplication (and any peeled suffix) morpheme. Called only on a real match, so `budget.check_chain_depth` here counts only genuine chains, never a failed attempt; `prepend` puts the redup morpheme first (and shifts `root_index`) for a prefix reduplicant, else it trails the base.
    #[allow(clippy::too_many_arguments)]
    fn propose_for_residual(
        &self,
        g: &Grammar,
        residual: &str,
        extra_suffix: Option<MRuleId>,
        prepend: bool,
        depth: usize,
        budget: &ComposeBudget,
        propose: &mut dyn FnMut(&str) -> Vec<Candidate>,
        out: &mut Vec<Candidate>,
    ) -> Result<(), ComposeError> {
        // A real reduplication layer at `depth` is about to be used -- gate it now (module doc).
        budget.check_chain_depth(depth, CHAIN_DEPTH_SITE)?;
        let mut base_candidates = propose(residual);
        // Nested reduplication: `residual` may carry its own further structure -- peel it again one layer deeper; a no-op (no propose call, no budget check) when it does not.
        base_candidates.extend(self.peel_at_depth(g, residual, depth + 1, budget, propose)?);
        for base in &base_candidates {
            for &redup in &self.redup_rules {
                let redup_m = owning_morpheme(g, redup);
                if prepend {
                    let mut morphemes = Vec::with_capacity(base.morphemes.len() + 1);
                    morphemes.push(redup_m);
                    morphemes.extend_from_slice(&base.morphemes);
                    out.push(Candidate {
                        morphemes,
                        // Prepending one morpheme shifts every base morpheme (root included) one position right.
                        root_index: if base.root_index < 0 {
                            base.root_index
                        } else {
                            base.root_index + 1
                        },
                    });
                } else {
                    let mut morphemes = base.morphemes.clone();
                    morphemes.push(redup_m);
                    if let Some(suf) = extra_suffix {
                        morphemes.push(owning_morpheme(g, suf));
                    }
                    out.push(Candidate {
                        morphemes,
                        root_index: base.root_index,
                    });
                }
            }
        }
        Ok(())
    }
}

// Chain-depth / nested-reduplication tests, using a minimal hand-authored Grammar (never #[ignore]d) with a stub propose closure.
#[cfg(feature = "test-support")]
/// A grammar with exactly one `AffixProcessRule` classifying `Role::Reduplication` (`Copy(Input(0))` twice), wired into stratum 0.
pub fn minimal_redup_grammar_for_test() -> Grammar {
    use pg_grammar::model::{
        AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, MRuleId as ModelMRuleId, MorphRuleDef,
        MorphRuleOrder, MorphemeId, PartRef, Pattern, StratumDef, TableId, VarTable,
    };
    const MINIMAL_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PeelChainDepthFixture</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
  </Language>
</HermitCrabInput>"#;
    let mut g = pg_grammar::load(MINIMAL_XML).expect("minimal fixture loads");
    let redup_mrule = ModelMRuleId(g.mrules.len() as u32);
    g.mrules
        .push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
            morpheme: MorphemeId(0),
            name: Some("redupChainDepthFixture".to_string()),
            blockable: false,
            partial: false,
            max_apps: 1,
            required_syn_fs: pg_featstruct::FsId(0),
            out_syn_fs: pg_featstruct::FsId(0),
            obligatory_features: vec![],
            required_stem_name: None,
            is_template_rule: false,
            allomorphs: vec![AffixAllomorphDef {
                id: AllomorphId(0),
                environments: vec![],
                co_occurrence: vec![],
                required_syn_fs: pg_featstruct::FsId(0),
                vars: VarTable::default(),
                required_mpr: pg_grammar::model::MprSet::EMPTY,
                excluded_mpr: pg_grammar::model::MprSet::EMPTY,
                out_mpr: pg_grammar::model::MprSet::EMPTY,
                redup_hint: pg_grammar::model::ReduplicationHint::Suffix,
                lhs: vec![Pattern::default()],
                // Copy(Input(0)) twice, no other actions: classify_affix's exact Role::Reduplication trigger.
                rhs: vec![
                    OutputAction::Copy(PartRef::Input(0)),
                    OutputAction::Copy(PartRef::Input(0)),
                ],
                properties: vec![],
            }],
        }));
    g.strata.push(StratumDef {
        name: Some("chainDepthStratum".to_string()),
        table: TableId(0),
        mrule_order: MorphRuleOrder::Linear,
        prules: vec![],
        mrules: vec![redup_mrule],
        templates: vec![],
        entries: vec![],
    });
    g
}

#[cfg(test)]
mod tests;
