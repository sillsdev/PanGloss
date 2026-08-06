//! Phonology probe ("Indonesian: phonology
//! via pre-probed junction variants"): drives the grammar's REAL synthesis machinery
//! (`pg_rules::surface_probe::probe_synthesize`) to discover, for one affix's underlying insert
//! text, every surface spelling it can realize as and every spelling that additionally causes an
//! adjacent morph's own leading segment to delete — the exact trick `hc-hybrid/src/surface.rs`'s
//! `SurfacePhonology` uses to build its trie. This is a FRESH port (`hc-hybrid`
//! is sunsetting — `pg-foma` must never depend on it), not a code reuse, and it is
//! deliberately SMALLER than the original in two ways:
//!
//! 1. **No `bare_root_surfaces`.** That method needs a live `pg_parse::Morpher` (obligatory-
//!    inflection checking) — out of scope for an emitter that never constructs one (the baseline
//!    emission path's bare roots are already permissive; see `emit.rs`'s module doc, "Bare roots
//!    skip the obligatory-inflection gate").
//! 2. **No `DeletionJunction::deleted_neighbor`/`_lanes` bookkeeping.** The original needs the
//!    deleted neighbor's own FeatureStruct lanes to GATE a deletion-skip edge onto only the roots
//!    whose actual onset unifies with that class (`FstTemplateAnalyzer.WireDeletionSkips`'s real
//!    unification test) — an exactness optimization that matters there because the trie shares one
//!    root-chain graph across every affix and a wrong gate would let one prefix's deletion wrongly
//!    skip into an unrelated root's chain structurally. `emit.rs`'s lexc encoding doesn't share
//!    state that way: it instead offers a root-initial-stripped SPELLING to every root uniformly
//!    whenever `PhonologyProbe::deletion_junctions` proves SOME context deletes the following
//!    segment for this affix text (see `emit.rs`'s "Junction-aware prefix emission" section) — an
//!    upward approximation (extra accepted spellings are harmless; confirm
//!    prunes them) that trades a little overgeneration for not needing lane-level gating at all.
//!
//! Capability-gated exactly like the original: `PhonologyProbe::new` returns `None` when the
//! grammar has no phonological rules at all (Sena), so the baseline phonology-unaware emission path
//! is completely untouched for grammars that don't need this probe — the Sena regression gate
//! (`tests/f1_sena_gate.rs`) depends on this being a true no-op, not just an empty result.

use std::collections::BTreeSet;
use std::sync::Mutex;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{Grammar, MorphRuleDef, PhonRuleDef};
use pg_rules::cache::RuleCache;
use pg_rules::surface_probe::{self, ProbeSeg};
use pg_shape::NodeKind;
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
use rustc_hash::FxHashMap as HashMap;

use crate::grammar_semantics::GrammarSemantics;

pub struct PhonologyProbe<'g> {
    g: &'g Grammar,
    table: &'g CharDefTable,
    /// One representative surface representation per Segment-kind char-def, table document order.
    alphabet: Vec<String>,
    /// `alphabet` restricted to segments that can actually start some root or affix in this grammar, used only for the C1 (outer) probe loop; narrowing C1 to this closed set cannot drop a real deletion junction, unlike C2 which stays over the full alphabet.
    neighbor_alphabet: Vec<String>,
    any_deletion_subrule: bool,
    // `Mutex`, not `RefCell`: `RefCell` is never `Sync`, which would block sharing `&self` across the rayon pool below.
    variants_cache: Mutex<HashMap<String, Vec<String>>>,
    junctions_cache: Mutex<HashMap<String, Vec<String>>>,
    /// Compile-once FST cache, built once here and reused across every probe rather than recompiling per call.
    rule_cache: RuleCache,
    /// A dedicated rayon pool, not the global default pool (`pg-parse/src/batch.rs` configures that one for its own stack needs), built once per grammar with `crate::emit::PROBE_STACK_BYTES`-sized stacks for `probe_synthesize`'s deep recursion; absent on wasm32, where the probe loops fall back to sequential.
    #[cfg(not(target_arch = "wasm32"))]
    pool: rayon::ThreadPool,
}

/// The Segment-kind char-def id of the first real segment in `text` (skipping leading Boundary-kind matches, greedy longest-match), or `None` if `text` is entirely boundaries/empty or unsegmentable.
fn first_segment_id(table: &CharDefTable, text: &str) -> Option<CharDefId> {
    let normalized = pg_grammar::nfd::nfd(text);
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

/// Every Segment-kind char-def id that starts some root allomorph's shape text or some morphological rule's first `InsertSegments` action: the complete, closed set of possible right-neighbor first-segments, since every synthesizable word's morph boundaries are only ever root/root or root/affix -- so restricting `compute_deletion_junctions`'s C1 loop to this set cannot lose recall.
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
        let allomorphs: &[pg_grammar::model::AffixAllomorphDef] = match r {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs,
            MorphRuleDef::Realizational(def) => &def.allomorphs,
            MorphRuleDef::Compounding(_) => &[],
        };
        for a in allomorphs {
            for act in &a.rhs {
                if let pg_grammar::model::OutputAction::InsertSegments { shape, .. } = act {
                    if let Some(id) = first_segment_id(table, &shape.text) {
                        ids.insert(id);
                    }
                    // Only the first `InsertSegments` action matters here: its first segment is the affix's first rendered segment regardless of how many more actions follow.
                    break;
                }
            }
        }
    }
    ids
}

impl<'g> PhonologyProbe<'g> {
    /// `None` when the grammar declares no phonological rules at all — nothing to probe, and
    /// `emit.rs` treats `None` as "this grammar's affix/root emission is unchanged from the
    /// baseline emission path".
    ///
    /// Derives a `GrammarSemantics` to answer the existence question. A caller that already holds
    /// one should use `Self::new_with_semantics`; `GrammarSemantics::derive` is cheap (it does not
    /// characterize), so this convenience form stays available for the many call sites that only
    /// have a `&Grammar`.
    pub fn new(g: &'g Grammar) -> Option<Self> {
        Self::new_with_semantics(&GrammarSemantics::derive(g))
    }

    /// `Self::new` over an already-derived `GrammarSemantics`.
    ///
    /// The existence gate is `GrammarSemantics::cascade_phonology`, NOT
    /// `GrammarSemantics::declared_phonology` — this probe drives the trailing per-stratum rewrite
    /// cascade, so a rule declared globally but named by no stratum's `phonologicalRules` list gives
    /// it nothing to probe. That is the exact predicate this function always used (`sd.prules` per
    /// stratum); it is now a named, owned fact rather than an inline loop, and its difference from
    /// `Applicability::HasPhonology`'s grammar-wide reading is documented at the owner rather than
    /// being an undiscovered disagreement between two files. See `grammar_semantics`'s module doc.
    ///
    /// The `semantics` borrow does NOT have to outlive the probe: only the `&'g Grammar` inside it
    /// does, which is why this takes `&GrammarSemantics<'g>` by short-lived reference and re-borrows
    /// the grammar out of it.
    pub fn new_with_semantics(semantics: &GrammarSemantics<'g>) -> Option<Self> {
        let g = semantics.grammar();
        let surface_stratum = g
            .strata
            .last()
            .expect("a loaded grammar always has a stratum");
        let table = &g.char_tables[surface_stratum.table.0 as usize];

        if !semantics.cascade_phonology() {
            return None;
        }

        let mut any_deletion_subrule = false;
        for sd in &g.strata {
            for &pid in &sd.prules {
                if let PhonRuleDef::Rewrite(r) = &g.prules[pid.0 as usize] {
                    if r.subrules.iter().any(|sr| sr.rhs.nodes.is_empty()) {
                        any_deletion_subrule = true;
                    }
                }
            }
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
            variants_cache: Mutex::new(HashMap::default()),
            junctions_cache: Mutex::new(HashMap::default()),
            rule_cache: RuleCache::build(g),
            #[cfg(not(target_arch = "wasm32"))]
            pool: rayon::ThreadPoolBuilder::new()
                .stack_size(crate::emit::PROBE_STACK_BYTES)
                .build()
                .expect("build phonology probe rayon pool"),
        })
    }

    /// Every surface realization of `underlying` in isolation, or with one alphabet neighbor
    /// on either side (mirrors `SurfacePhonology::variants`, `SurfacePhonology.cs:108-117`),
    /// memoized. Sorted (a `BTreeSet` internally) — callers don't need a particular order.
    pub fn variants(&self, underlying: &str) -> Vec<String> {
        {
            let cache = self.variants_cache.lock().expect("variants_cache poisoned");
            if let Some(v) = cache.get(underlying) {
                return v.clone();
            }
        }
        let computed = self.compute_variants(underlying);
        self.variants_cache
            .lock()
            .expect("variants_cache poisoned")
            .insert(underlying.to_string(), computed.clone());
        computed
    }

    /// Probes every alphabet neighbor `c` on both sides of `underlying`; each probe is independent and read-only over `&self`, so non-wasm32 targets run it across `Self::pool` while wasm32 stays sequential, sharing the same `probe_one` closure and collecting into the same order-independent `BTreeSet`.
    fn compute_variants(&self, underlying: &str) -> Vec<String> {
        let mut result: BTreeSet<String> = BTreeSet::new();
        let Some(underlying_len) = self.node_count(underlying) else {
            return result.into_iter().collect(); // unsegmentable: nothing extra to offer.
        };
        if let Some(isolation) = self.surface_of(underlying) {
            result.insert(isolation);
        }

        // Left neighbor: c + underlying, affix span is everything after the neighbor's segment; right neighbor: underlying + c, everything before it.
        let probe_one = |c: &String| -> [Option<String>; 2] {
            [
                self.boundary_variant(&format!("{c}{underlying}"), underlying_len, true),
                self.boundary_variant(&format!("{underlying}{c}"), underlying_len, false),
            ]
        };

        #[cfg(target_arch = "wasm32")]
        let hits: Vec<[Option<String>; 2]> = self.alphabet.iter().map(probe_one).collect();
        #[cfg(not(target_arch = "wasm32"))]
        let hits: Vec<[Option<String>; 2]> = self
            .pool
            .install(|| self.alphabet.par_iter().map(probe_one).collect());

        for pair in hits {
            result.extend(pair.into_iter().flatten());
        }
        result.into_iter().collect()
    }

    /// The affix's own rendered span, or `None` when the probe window is unsegmentable, structurally unreliable (an insertion fired), or a surviving node has no single representation.
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
        {
            let cache = self
                .junctions_cache
                .lock()
                .expect("junctions_cache poisoned");
            if let Some(v) = cache.get(underlying) {
                return v.clone();
            }
        }
        let computed = self.compute_deletion_junctions(underlying);
        self.junctions_cache
            .lock()
            .expect("junctions_cache poisoned")
            .insert(underlying.to_string(), computed.clone());
        computed
    }

    /// The C1×C2 fan-out (the dominant cost on Amharic, ~46 × ~417 probes per affix text). C1 (`neighbor_alphabet`) is the outer, parallelized loop; each c1's probe is independent of every other, so distributing across `Self::pool` changes only which thread computes it, not the per-c1 probe count. Same wasm32/non-wasm32 split as `Self::compute_variants`.
    fn compute_deletion_junctions(&self, underlying: &str) -> Vec<String> {
        let mut result: BTreeSet<String> = BTreeSet::new();
        if !self.any_deletion_subrule {
            return Vec::new(); // no rule can ever delete a segment => nothing to find, by construction.
        }
        let Some(underlying_len) = self.node_count(underlying) else {
            return Vec::new();
        };
        // C1 is restricted to `neighbor_alphabet`, a sound closed enumeration; C2 stays over the full `alphabet` since narrowing it has no equivalent soundness proof.
        let probe_one = |c1: &String| -> Option<String> {
            if let Some(hit) = self.try_probe_deletion(underlying, c1, None, underlying_len) {
                return Some(hit);
            }
            for c2 in &self.alphabet {
                if let Some(hit2) =
                    self.try_probe_deletion(underlying, c1, Some(c2), underlying_len)
                {
                    return Some(hit2); // one confirming c2 is enough to know c1's context can delete.
                }
            }
            None
        };

        #[cfg(target_arch = "wasm32")]
        let hits: Vec<Option<String>> = self.neighbor_alphabet.iter().map(probe_one).collect();
        #[cfg(not(target_arch = "wasm32"))]
        let hits: Vec<Option<String>> = self
            .pool
            .install(|| self.neighbor_alphabet.par_iter().map(probe_one).collect());

        result.extend(hits.into_iter().flatten());
        result.into_iter().collect()
    }

    /// `SurfacePhonology::TryProbeDeletion` (`SurfacePhonology.cs:260-293`), minus the deleted-neighbor lane/render bookkeeping (module doc).
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

    /// `SurfacePhonology::SurfaceNodes` (`SurfacePhonology.cs:305-322`): segment `str_`, run the full stratum cascade, filter to Segment-kind nodes; `None` on an unsegmentable string or unrepresentable probe.
    fn surface_segments(&self, str_: &str) -> Option<Vec<ProbeSeg>> {
        let shape = pg_rules::shape_feat::segment_with_features(self.g, self.table, str_).ok()?;
        surface_probe::probe_synthesize(self.g, &shape, &self.rule_cache)
    }

    /// `SurfacePhonology::NodeCount` (`SurfacePhonology.cs:327-339`): the number of Segment-kind nodes in the raw (pre-phonology) segmentation, or `None` if unsegmentable.
    fn node_count(&self, str_: &str) -> Option<usize> {
        let shape = pg_rules::shape_feat::segment_with_features(self.g, self.table, str_).ok()?;
        Some(
            (0..shape.len())
                .filter(|&i| shape.kind(i) == NodeKind::Segment)
                .count(),
        )
    }
}
