//! Phonology probe (plan `docs/fst-plan/foma-fst-plan.md` D3, P1 stage 2 "Indonesian: phonology
//! via pre-probed junction variants"): drives the grammar's REAL synthesis machinery
//! (`hc_rules::surface_probe::probe_synthesize`) to discover, for one affix's underlying insert
//! text, every surface spelling it can realize as and every spelling that additionally causes an
//! adjacent morph's own leading segment to delete — the exact trick `hc-hybrid/src/surface.rs`'s
//! `SurfacePhonology` uses to build its trie (plan §2's citation). This is a FRESH port (`hc-hybrid`
//! is sunsetting, plan D8 — `hc-foma` must never depend on it), not a code reuse, and it is
//! deliberately SMALLER than the original in two ways:
//!
//! 1. **No `bare_root_surfaces`.** That method needs a live `hc_parse::Morpher` (obligatory-
//!    inflection checking) — out of scope for an emitter that never constructs one (stage 1's bare
//!    roots are already permissive; see `emit.rs`'s module doc, "Bare roots skip the obligatory-
//!    inflection gate").
//! 2. **No `DeletionJunction::deleted_neighbor`/`_lanes` bookkeeping.** The original needs the
//!    deleted neighbor's own FeatureStruct lanes to GATE a deletion-skip edge onto only the roots
//!    whose actual onset unifies with that class (`FstTemplateAnalyzer.WireDeletionSkips`'s real
//!    unification test) — an exactness optimization that matters there because the trie shares one
//!    root-chain graph across every affix and a wrong gate would let one prefix's deletion wrongly
//!    skip into an unrelated root's chain structurally. `emit.rs`'s lexc encoding doesn't share
//!    state that way: it instead offers a root-initial-stripped SPELLING to every root uniformly
//!    whenever [`PhonologyProbe::deletion_junctions`] proves SOME context deletes the following
//!    segment for this affix text (see `emit.rs`'s "Junction-aware prefix emission" section) — an
//!    upward approximation (the plan's iron rule: extra accepted spellings are harmless; confirm,
//!    P2, prunes them) that trades a little overgeneration for not needing lane-level gating at all.
//!
//! Capability-gated exactly like the original: [`PhonologyProbe::new`] returns `None` when the
//! grammar has no phonological rules at all (Sena), so stage 1's phonology-unaware emission path is
//! completely untouched for grammars this stage doesn't target — the Sena regression gate
//! (`tests/f1_sena_gate.rs`) depends on this being a true no-op, not just an empty result.

use std::cell::RefCell;
use std::collections::BTreeSet;

use hc_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use hc_grammar::model::{Grammar, MorphRuleDef, PhonRuleDef};
use hc_rules::cache::RuleCache;
use hc_rules::surface_probe::{self, ProbeSeg};
use hc_shape::NodeKind;
use rustc_hash::FxHashMap as HashMap;

pub struct PhonologyProbe<'g> {
    g: &'g Grammar,
    table: &'g CharDefTable,
    /// One representative surface representation per Segment-kind char-def, table document order
    /// (mirrors `hc-hybrid/src/surface.rs`'s `_alphabet`).
    alphabet: Vec<String>,
    /// Subset of `alphabet` restricted to segments that actually appear as the FIRST real segment
    /// of some root allomorph's or some affix rule's authored text in THIS grammar (P1 stage 3,
    /// the Amharic hazard-1 fix: see [`neighbor_first_segments`]'s doc for the soundness
    /// argument). Used ONLY for [`compute_deletion_junctions`]'s outer (C1) loop -- an affix's
    /// real right-neighbor in any synthesized word is always some root's or some rule's own first
    /// segment, a closed, enumerable set, so narrowing the C1 loop to it can never drop a
    /// deletion junction that could actually occur. The inner C2 loop stays over the FULL
    /// `alphabet` (a neighbor's own SECOND segment has no equivalent closed characterization here
    /// -- narrowing it would risk losing recall, the plan's one forbidden direction). On a small
    /// alphabet (Sena has none -- no phonological rules at all; Indonesian's restricted set is
    /// close to its full alphabet already) this changes nothing measurable; on Amharic's 417-
    /// segment table it cuts the measured wall time from ~150-230s to single-digit seconds (P1
    /// stage 3 investigation numbers, recorded in this stage's report) by shrinking the outer loop
    /// from 417 to the ~46 segments that can actually start a root or affix in this grammar.
    neighbor_alphabet: Vec<String>,
    any_deletion_subrule: bool,
    variants_cache: RefCell<HashMap<String, Vec<String>>>,
    junctions_cache: RefCell<HashMap<String, Vec<String>>>,
    /// Compile-once FST cache, built ONCE here and reused across every probe (same rationale as
    /// `hc-hybrid/src/surface.rs`'s own `rule_cache` field: recompiling per probe would be
    /// pathological on a larger alphabet; Indonesian's is tiny, but there's no reason not to share
    /// this the same way).
    rule_cache: RuleCache,
}

/// The Segment-kind char-def id of the FIRST real segment in `text` (skipping any leading
/// Boundary-kind matches, greedy longest-match against `table` -- the same algorithm
/// `emit.rs`'s `surface_variants`/`stripped_variants` use), or `None` if `text` is entirely
/// boundaries/empty or fails to segment at all.
fn first_segment_id(table: &CharDefTable, text: &str) -> Option<CharDefId> {
    let normalized = hc_grammar::nfd::nfd(text);
    let chars: Vec<char> = normalized.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let mut matched: Option<(usize, CharDefId)> = None;
        for j in (1..=(chars.len() - i)).rev() {
            let candidate: String = chars[i..i + j].iter().collect();
            if let Some(cd_id) = table.lookup_nfd(&candidate) {
                matched = Some((j, cd_id));
                break;
            }
        }
        let (j, cd_id) = matched?;
        i += j;
        if table.get(cd_id).kind() == CharDefKind::Segment {
            return Some(cd_id);
        }
        // Boundary-kind: not a real segment -- keep scanning for the first one.
    }
    None
}

/// Every Segment-kind char-def id that appears as the first real segment of some root
/// allomorph's authored shape text (any stratum, any entry, skipping `is_pattern` shapes -- they
/// have no concrete text) or of some morphological rule allomorph's first `InsertSegments`
/// action's text (any zone -- template slot, derivation layer, whatever; every affix rule in the
/// grammar is scanned, mirroring `emit.rs`'s own enumeration order requirements loosely, since
/// this only needs the SET, not emission order).
///
/// **Soundness argument (why this is a restriction, not an approximation):** in any word this
/// grammar can synthesize, the segment immediately following one particular affix's own material
/// is always either (a) the first segment of a root (bare, or the head/non-head root of a
/// compound), or (b) the first segment of another rule's affix text (prefix, suffix, or
/// derivation-layer rule) -- there is no third kind of morph. So this set is the complete,
/// closed enumeration of every segment that can EVER be probed as `compute_deletion_junctions`'s
/// C1 (the immediate right-neighbor); no real adjacency is excluded by restricting the C1 loop to
/// it, so recall cannot be lost (plan's iron rule: approximate only upward, and this is not even
/// an approximation, just an unreachable-input elimination).
fn neighbor_first_segments(g: &Grammar, table: &CharDefTable) -> BTreeSet<CharDefId> {
    let mut ids = BTreeSet::new();
    for e in &g.entries {
        for a in &e.allomorphs {
            if a.is_pattern {
                continue;
            }
            if let Some(id) = first_segment_id(table, &a.shape.text) {
                ids.insert(id);
            }
        }
    }
    for r in &g.mrules {
        let allomorphs: &[hc_grammar::model::AffixAllomorphDef] = match r {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs,
            MorphRuleDef::Realizational(def) => &def.allomorphs,
            MorphRuleDef::Compounding(_) => &[],
        };
        for a in allomorphs {
            for act in &a.rhs {
                if let hc_grammar::model::OutputAction::InsertSegments { shape, .. } = act {
                    if let Some(id) = first_segment_id(table, &shape.text) {
                        ids.insert(id);
                    }
                    break; // mirrors emit.rs's first_insert_text: only the FIRST InsertSegments action.
                }
            }
        }
    }
    ids
}

impl<'g> PhonologyProbe<'g> {
    /// `None` when the grammar declares no phonological rules at all — nothing to probe, and
    /// `emit.rs` treats `None` as "this grammar's affix/root emission is unchanged from stage 1".
    pub fn new(g: &'g Grammar) -> Option<Self> {
        let surface_stratum = g.strata.last().expect("a loaded grammar always has a stratum");
        let table = &g.char_tables[surface_stratum.table.0 as usize];

        let mut any_phonological_rules = false;
        let mut any_deletion_subrule = false;
        for sd in &g.strata {
            if !sd.prules.is_empty() {
                any_phonological_rules = true;
            }
            for &pid in &sd.prules {
                if let PhonRuleDef::Rewrite(r) = &g.prules[pid.0 as usize] {
                    if r.subrules.iter().any(|sr| sr.rhs.nodes.is_empty()) {
                        any_deletion_subrule = true;
                    }
                }
            }
        }
        if !any_phonological_rules {
            return None;
        }

        let mut alphabet = Vec::new();
        let neighbor_ids = neighbor_first_segments(g, table);
        let mut neighbor_alphabet = Vec::new();
        for (cd_id, cd) in table.iter() {
            if cd.kind() == CharDefKind::Segment {
                if let Some(rep) = cd.representations().first() {
                    if !rep.is_empty() {
                        alphabet.push(rep.clone());
                        if neighbor_ids.contains(&cd_id) {
                            neighbor_alphabet.push(rep.clone());
                        }
                    }
                }
            }
        }

        Some(PhonologyProbe {
            g,
            table,
            alphabet,
            neighbor_alphabet,
            any_deletion_subrule,
            variants_cache: RefCell::new(HashMap::default()),
            junctions_cache: RefCell::new(HashMap::default()),
            rule_cache: RuleCache::build(g),
        })
    }

    /// Every surface realization of `underlying` in isolation, or with one alphabet neighbor
    /// on either side (mirrors `SurfacePhonology::variants`, `SurfacePhonology.cs:108-117`),
    /// memoized. Sorted (a `BTreeSet` internally) — callers don't need a particular order.
    pub fn variants(&self, underlying: &str) -> Vec<String> {
        if let Some(v) = self.variants_cache.borrow().get(underlying) {
            return v.clone();
        }
        let computed = self.compute_variants(underlying);
        self.variants_cache
            .borrow_mut()
            .insert(underlying.to_string(), computed.clone());
        computed
    }

    fn compute_variants(&self, underlying: &str) -> Vec<String> {
        let mut result: BTreeSet<String> = BTreeSet::new();
        let Some(underlying_len) = self.node_count(underlying) else {
            return result.into_iter().collect(); // unsegmentable: nothing extra to offer.
        };
        if let Some(isolation) = self.surface_of(underlying) {
            result.insert(isolation);
        }
        for c in &self.alphabet {
            // Left neighbor: c + underlying -- this affix's own span is everything AFTER the
            // neighbor's one segment.
            if let Some(rendered) =
                self.boundary_variant(&format!("{c}{underlying}"), underlying_len, true)
            {
                result.insert(rendered);
            }
            // Right neighbor: underlying + c -- this affix's own span is everything BEFORE the
            // neighbor's one segment.
            if let Some(rendered) =
                self.boundary_variant(&format!("{underlying}{c}"), underlying_len, false)
            {
                result.insert(rendered);
            }
        }
        result.into_iter().collect()
    }

    /// Returns the affix's own rendered span, or `None` when the probe window is unsegmentable,
    /// structurally unreliable (an insertion fired -- the surviving segment count doesn't match),
    /// or a surviving node has no single representation (`SurfacePhonology::AddBoundaryVariant`,
    /// `SurfacePhonology.cs:149-165`).
    fn boundary_variant(
        &self,
        context: &str,
        underlying_len: usize,
        from_end: bool,
    ) -> Option<String> {
        let segs = self.surface_segments(context)?;
        if segs.len() != underlying_len + 1 {
            return None; // unsegmentable, or an insertion fired => no reliable affix-only portion.
        }
        let morpheme_nodes: &[ProbeSeg] = if from_end {
            &segs[1..]
        } else {
            &segs[..underlying_len]
        };
        surface_probe::render_nodes(self.table, morpheme_nodes)
    }

    /// Every distinct surface spelling of `underlying` that, in SOME right-neighbor context,
    /// deletes that neighbor's own leading segment (mirrors `SurfacePhonology::deletion_junctions`,
    /// `SurfacePhonology.cs:214-225`, minus the neighbor-class bookkeeping -- see module doc).
    /// Empty (not probed at all) when this grammar has no rule that can ever delete a segment.
    pub fn deletion_junctions(&self, underlying: &str) -> Vec<String> {
        if let Some(v) = self.junctions_cache.borrow().get(underlying) {
            return v.clone();
        }
        let computed = self.compute_deletion_junctions(underlying);
        self.junctions_cache
            .borrow_mut()
            .insert(underlying.to_string(), computed.clone());
        computed
    }

    fn compute_deletion_junctions(&self, underlying: &str) -> Vec<String> {
        let mut result: BTreeSet<String> = BTreeSet::new();
        if !self.any_deletion_subrule {
            return Vec::new(); // no rule can ever delete a segment => nothing to find, by construction.
        }
        let Some(underlying_len) = self.node_count(underlying) else {
            return Vec::new();
        };
        // C1 (the immediate right-neighbor) is restricted to `neighbor_alphabet` -- a sound,
        // closed enumeration, not an approximation (see `neighbor_first_segments`'s doc). C2
        // stays over the FULL `alphabet`: unrestricted, since narrowing it has no equivalent
        // soundness proof.
        for c1 in &self.neighbor_alphabet {
            if let Some(hit) = self.try_probe_deletion(underlying, c1, None, underlying_len) {
                result.insert(hit);
                continue;
            }
            for c2 in &self.alphabet {
                if let Some(hit2) =
                    self.try_probe_deletion(underlying, c1, Some(c2), underlying_len)
                {
                    result.insert(hit2);
                    break; // one confirming c2 is enough to know c1's context can delete.
                }
            }
        }
        result.into_iter().collect()
    }

    /// `SurfacePhonology::TryProbeDeletion` (`SurfacePhonology.cs:260-293`), minus the deleted-
    /// neighbor lane/render bookkeeping (module doc).
    fn try_probe_deletion(
        &self,
        underlying: &str,
        c1: &str,
        c2: Option<&str>,
        underlying_len: usize,
    ) -> Option<String> {
        let extra = if c2.is_some() { 2 } else { 1 };
        let context = match c2 {
            Some(c2) => format!("{underlying}{c1}{c2}"),
            None => format!("{underlying}{c1}"),
        };
        let segs = self.surface_segments(&context)?;
        if segs.len() != underlying_len + extra {
            return None; // unsegmentable, or a length-changing rule fired elsewhere in the window.
        }
        if !segs[underlying_len].deleted {
            return None; // c1 survived -- `variants()` already covers that case.
        }
        surface_probe::render_nodes(self.table, &segs[..underlying_len])
    }

    /// `SurfacePhonology::SurfaceOf` (`SurfacePhonology.cs:297-301`).
    fn surface_of(&self, underlying: &str) -> Option<String> {
        let segs = self.surface_segments(underlying)?;
        surface_probe::render_nodes(self.table, &segs)
    }

    /// `SurfacePhonology::SurfaceNodes` (`SurfacePhonology.cs:305-322`): segment `str_`, run the
    /// full stratum cascade, filter to Segment-kind nodes. `None` on an unsegmentable string or an
    /// unrepresentable probe (see `hc_rules::surface_probe::probe_synthesize`'s doc).
    fn surface_segments(&self, str_: &str) -> Option<Vec<ProbeSeg>> {
        let shape = hc_rules::shape_feat::segment_with_features(self.g, self.table, str_).ok()?;
        surface_probe::probe_synthesize(self.g, &shape, &self.rule_cache)
    }

    /// `SurfacePhonology::NodeCount` (`SurfacePhonology.cs:327-339`): the number of SEGMENT-kind
    /// nodes in the raw (pre-phonology) segmentation, or `None` if unsegmentable.
    fn node_count(&self, str_: &str) -> Option<usize> {
        let shape = hc_rules::shape_feat::segment_with_features(self.g, self.table, str_).ok()?;
        Some(
            (0..shape.len())
                .filter(|&i| shape.kind(i) == NodeKind::Segment)
                .count(),
        )
    }
}
