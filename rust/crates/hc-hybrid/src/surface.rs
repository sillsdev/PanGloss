//! `SurfacePhonology` (F2, HYBRID_FST_RUST_PLAN.md §8) — port of C#
//! `SIL.Machine.Morphology.HermitCrab.SurfacePhonology` (341 lines): build-time probing through the
//! grammar's REAL synthesis rules (no reimplemented phonology), producing three things the trie
//! builder (F3) will consume:
//! - [`SurfacePhonology::variants`]: an affix underlying form's distinct surface realizations
//!   (isolation + single-neighbor-on-either-side boundary probes).
//! - [`SurfacePhonology::deletion_junctions`]: which onset classes trigger the CASCADE deleting the
//!   neighbor's own leading segment (Indonesian's meN- + voiceless-obstruent case).
//! - [`SurfacePhonology::bare_root_surfaces`]: the un-affixed surface form(s) of a lexical entry
//!   (`Morpher::generate_words`, §7.1 item 2b — already built, not new work here).
//!
//! Memoized per underlying string (C#'s `_variantsCache`/`_deletionJunctionsCache`); capability-gated
//! on `_anyPhonologicalRules`/`_anyDeletionSubrule` so a grammar with no phonological rules at all
//! (Sena) skips all probing cheaply (identity `Variants`, empty `DeletionJunctions`) — a real
//! perf/correctness requirement per the feasibility report, not just an optimization.
//!
//! ## The node-position mechanism (§7.1 item 2a; see `hc_rules::rewrite`/`surface_probe`'s own docs)
//! `hc_shape::Shape` physically removes a deleted node — the frozen contract's simplification, fine
//! for the real per-word pipeline. C#'s node-position-based slicing (`outNodes.Skip(1)`/
//! `.Take(underlyingLen)`) needs stable positions across the WHOLE synthesis cascade instead, so
//! this module drives `hc_rules::surface_probe::probe_synthesize` (a soft-delete, position-preserving
//! sibling of the real synthesis path — deleted nodes stay, in place, forever, exactly like C#'s
//! `IsDeleted()` annotation) rather than the ordinary `hc_rules::rewrite::synthesize`.
//!
//! **Deviation from the plan's stated module location (§7.1 item 3):** the plan says "hc-shape:
//! deleted-node-aware `render_nodes`". `hc_shape::Shape` carries no deleted-node concept to skip —
//! deletion is physical there. The deletion-aware rendering lives where the deletion info actually
//! lives instead: `hc_rules::surface_probe::render_nodes`, operating on the probe's own `ProbeSeg`
//! list. Flagged per §4.3's "plan assumption vs. reality" process (advisor-reviewed during F2).

use std::cell::RefCell;
use std::collections::BTreeSet;

use hc_featstruct::full_mask;
use hc_grammar::chardef::{CharDefKind, CharDefTable};
use hc_grammar::model::{Grammar, LexEntryId, PhonRuleDef};
use hc_parse::{GenMorpheme, Morpher};
use hc_rules::cache::RuleCache;
use hc_rules::surface_probe::{self, ProbeSeg};
use hc_shape::NodeKind;
use rustc_hash::FxHashMap as HashMap;
use unicode_normalization::UnicodeNormalization;

/// One `DeletionJunctions` hit: the affix's own resulting surface, paired with the deleted
/// neighbor's underlying feature struct rendered in C#'s `FeatureStruct.ToString()` format (the
/// `fst-stats` golden's exact dump format — see [`render_feature_struct`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletionJunction {
    pub affix_surface: String,
    pub deleted_neighbor: String,
}

pub struct SurfacePhonology<'g> {
    g: &'g Grammar,
    table: &'g CharDefTable,
    /// One representative surface representation per Segment-kind char-def, table document order
    /// (C# `SurfacePhonology`'s `_alphabet`, `SurfacePhonology.cs:91-102`).
    alphabet: Vec<String>,
    any_phonological_rules: bool,
    any_deletion_subrule: bool,
    variants_cache: RefCell<HashMap<String, Vec<String>>>,
    junctions_cache: RefCell<HashMap<String, Vec<DeletionJunction>>>,
    /// Compile-once FST cache (`hc_rules::cache::RuleCache`), built ONCE here and reused across
    /// every probe -- see `hc_rules::rewrite::probe_apply_rule_cached`'s doc for why this is not
    /// optional (recompiling per probe made Amharic's already-slow probing impractically slower).
    rule_cache: RuleCache,
}

impl<'g> SurfacePhonology<'g> {
    /// C# constructor (`SurfacePhonology.cs:62-103`). `table`/stratum-order come from the grammar's
    /// own surface stratum (C# `Language.SurfaceStratum` = the LAST stratum, `Language.cs:64-71`);
    /// every one of the three reference grammars has exactly one stratum, so this is not yet
    /// exercised on a multi-stratum case, but the convention is faithful either way.
    pub fn new(g: &'g Grammar) -> Self {
        let surface_stratum = g.strata.last().expect("a grammar has at least one stratum");
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

        let mut alphabet = Vec::new();
        for (_, cd) in table.iter() {
            if cd.kind() == CharDefKind::Segment {
                if let Some(rep) = cd.representations().first() {
                    if !rep.is_empty() {
                        alphabet.push(rep.clone());
                    }
                }
            }
        }

        SurfacePhonology {
            g,
            table,
            alphabet,
            any_phonological_rules,
            any_deletion_subrule,
            variants_cache: RefCell::new(HashMap::default()),
            junctions_cache: RefCell::new(HashMap::default()),
            rule_cache: RuleCache::build(g),
        }
    }

    /// C# `Variants(string)` (`SurfacePhonology.cs:108-117`), memoized. Sorted-ordinal here (the
    /// C# `HashSet<string>`'s own order is unspecified; every consumer — the trie builder and the
    /// golden dump alike — sorts before use, so returning pre-sorted is a strengthening, not a
    /// deviation).
    pub fn variants(&self, underlying: &str) -> Vec<String> {
        if let Some(v) = self.variants_cache.borrow().get(underlying) {
            return v.clone();
        }
        let computed = self.compute_variants(underlying);
        self.variants_cache.borrow_mut().insert(underlying.to_string(), computed.clone());
        computed
    }

    fn compute_variants(&self, underlying: &str) -> Vec<String> {
        if !self.any_phonological_rules {
            // No rule exists ⇒ identity is exact, not an approximation (SurfacePhonology.cs:121-124).
            return vec![underlying.to_string()];
        }
        let mut result: BTreeSet<String> = BTreeSet::new();
        result.insert(underlying.to_string());

        let Some(underlying_len) = self.node_count(underlying) else {
            return result.into_iter().collect(); // unsegmentable ⇒ just the verbatim underlying.
        };

        if let Some(isolation) = self.surface_of(underlying) {
            result.insert(isolation);
        }

        for c in &self.alphabet {
            // Left neighbor: c + underlying — the morpheme's own span is everything AFTER the
            // neighbor's one segment (`outNodes.Skip(1)`, SurfacePhonology.cs:157-159).
            if let Some(rendered) = self.boundary_variant(&format!("{c}{underlying}"), underlying_len, true) {
                result.insert(rendered);
            }
            // Right neighbor: underlying + c — the morpheme's own span is everything BEFORE the
            // neighbor's one segment (`outNodes.Take(underlyingLen)`).
            if let Some(rendered) = self.boundary_variant(&format!("{underlying}{c}"), underlying_len, false) {
                result.insert(rendered);
            }
        }
        result.into_iter().collect()
    }

    /// C# `AddBoundaryVariant` (`SurfacePhonology.cs:149-165`), minus the `HashSet` insertion (the
    /// caller does that) — returns the morpheme's own rendered span, or `None` when the window is
    /// unsegmentable, structurally unreliable (an insertion occurred — see `hc_rules::rewrite`'s F2
    /// module note for why the segment-count check below is exactly C#'s epenthesis/insertion
    /// guard), or a surviving node has no single representation.
    fn boundary_variant(&self, context: &str, underlying_len: usize, from_end: bool) -> Option<String> {
        let segs = self.surface_segments(context)?;
        if segs.len() != underlying_len + 1 {
            return None; // unsegmentable, or an insertion fired ⇒ no reliable morpheme portion.
        }
        let morpheme_nodes: &[ProbeSeg] =
            if from_end { &segs[1..] } else { &segs[..underlying_len] };
        surface_probe::render_nodes(self.table, morpheme_nodes)
    }

    /// C# `DeletionJunctions(string)` (`SurfacePhonology.cs:214-225`), memoized.
    pub fn deletion_junctions(&self, underlying: &str) -> Vec<DeletionJunction> {
        if let Some(v) = self.junctions_cache.borrow().get(underlying) {
            return v.clone();
        }
        let computed = self.compute_deletion_junctions(underlying);
        self.junctions_cache.borrow_mut().insert(underlying.to_string(), computed.clone());
        computed
    }

    fn compute_deletion_junctions(&self, underlying: &str) -> Vec<DeletionJunction> {
        let mut result = Vec::new();
        if !self.any_deletion_subrule {
            return result; // no rule can ever delete a segment ⇒ nothing to find, by construction.
        }
        let Some(underlying_len) = self.node_count(underlying) else {
            return result;
        };
        for c1 in &self.alphabet {
            if let Some(hit) = self.try_probe_deletion(underlying, c1, None, underlying_len) {
                result.push(hit);
                continue;
            }
            for c2 in &self.alphabet {
                if let Some(hit2) = self.try_probe_deletion(underlying, c1, Some(c2), underlying_len) {
                    result.push(hit2);
                    break; // one confirming c2 is enough to know c1's class deletes in SOME context.
                }
            }
        }
        result
    }

    /// C# `TryProbeDeletion` (`SurfacePhonology.cs:260-293`).
    fn try_probe_deletion(
        &self,
        underlying: &str,
        c1: &str,
        c2: Option<&str>,
        underlying_len: usize,
    ) -> Option<DeletionJunction> {
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
            return None; // c1 survived — Variants() already covers that case.
        }
        let affix_surface = surface_probe::render_nodes(self.table, &segs[..underlying_len])?;
        let cd = self
            .table
            .iter()
            .find(|(_, cd)| cd.kind() == CharDefKind::Segment && cd.representations().iter().any(|r| r == c1))?
            .1;
        let deleted_neighbor = render_feature_struct(self.g, cd.feature_lanes());
        Some(DeletionJunction { affix_surface, deleted_neighbor })
    }

    /// C# `SurfaceOf` (`SurfacePhonology.cs:297-301`).
    fn surface_of(&self, underlying: &str) -> Option<String> {
        let segs = self.surface_segments(underlying)?;
        surface_probe::render_nodes(self.table, &segs)
    }

    /// C# `SurfaceNodes` (`SurfacePhonology.cs:305-322`): segment `str`, run the full stratum
    /// cascade, filter to Segment-kind nodes. `None` on an unsegmentable string or an
    /// unrepresentable probe (see `hc_rules::surface_probe::probe_synthesize`'s doc).
    fn surface_segments(&self, str_: &str) -> Option<Vec<ProbeSeg>> {
        let shape = hc_rules::shape_feat::segment_with_features(self.g, self.table, str_).ok()?;
        surface_probe::probe_synthesize(self.g, &shape, &self.rule_cache)
    }

    /// C# `NodeCount` (`SurfacePhonology.cs:327-339`): the number of SEGMENT-kind nodes in the raw
    /// (pre-phonology) segmentation, or `None` if unsegmentable.
    fn node_count(&self, str_: &str) -> Option<usize> {
        let shape = hc_rules::shape_feat::segment_with_features(self.g, self.table, str_).ok()?;
        Some((0..shape.len()).filter(|&i| shape.kind(i) == NodeKind::Segment).count())
    }

    /// C# `FstTemplateAnalyzer.BareRootSurfaces` / the `fst-stats` tool's own replica
    /// (`FstStatsCommand.cs:144-164`): the surface forms HC synthesizes for `entry` with no other
    /// morphemes (`Morpher::generate_words`, §7.1 item 2b), NFD-normalized and deduplicated —
    /// matching the TOOL'S dump convention exactly (the golden this gates against was produced by
    /// that tool, not by the internal `FstTemplateAnalyzer` method, which does not normalize at
    /// all — a real, confirmed difference between the two C# call sites, not a porting choice).
    /// Empty ⇒ the bare root is not a valid word (obligatory inflection).
    pub fn bare_root_surfaces(&self, morpher: &Morpher<'g>, entry: LexEntryId) -> Vec<String> {
        let raw = morpher.generate_words(entry, &[] as &[GenMorpheme], hc_featstruct::FeatureStruct::EMPTY);
        let mut out: BTreeSet<String> = BTreeSet::new();
        for s in raw {
            out.insert(s.nfd().collect());
        }
        out.into_iter().collect()
    }
}

/// C# `FeatureStruct.ToString()` (`SIL.Machine/FeatureModel/FeatureStruct.cs:1404-1460`), restricted
/// to the flat, non-reentrant, non-disjunctive-in-practice case a char-def's own authored
/// `FeatureStruct` always is (no cycles, no shared sub-structures possible for a leaf phonological
/// segment). `[feat1:val1, feat2:val2, ...]`, sorted by feature `Description` (`<Name>` text)
/// ordinal, `ANY` if nothing is constrained. A fully-unconstrained feature (`lanes[f] == full_mask`)
/// is omitted entirely — matching C#'s `_definite` dictionary, which never holds an entry for a
/// feature nobody set.
fn render_feature_struct(g: &Grammar, lanes: &[u64]) -> String {
    let fs = &g.phon_features;
    let mut entries: Vec<(&str, String)> = Vec::new();
    for i in 0..fs.len() {
        let flat = hc_grammar::featsys::FlatIndex(i as u32);
        let symbol_count = fs.symbol_count(flat);
        let bits = lanes.get(i).copied().unwrap_or(full_mask(symbol_count as u32));
        if bits == full_mask(symbol_count as u32) {
            continue; // unconstrained ⇒ no entry, matching C#'s sparse `_definite`.
        }
        let mut names: Vec<&str> = (0..symbol_count as u32)
            .filter(|&s| bits & (1u64 << s) != 0)
            .map(|s| fs.symbol_name(flat, s))
            .collect();
        if names.is_empty() {
            continue; // no symbol bits set at all (degenerate; never expected) ⇒ nothing to print.
        }
        let value = if names.len() == 1 {
            names[0].to_string()
        } else {
            names.sort_unstable();
            format!("{{{}}}", names.join(", "))
        };
        entries.push((fs.feature_name(flat), value));
    }
    if entries.is_empty() {
        return "ANY".to_string();
    }
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let joined: Vec<String> = entries.into_iter().map(|(name, value)| format!("{name}:{value}")).collect();
    format!("[{}]", joined.join(", "))
}
