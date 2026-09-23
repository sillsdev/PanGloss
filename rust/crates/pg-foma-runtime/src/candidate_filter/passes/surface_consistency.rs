//! Whether ANY per-morpheme allomorph choice's literal characters could even fit inside the
//! surface word a candidate was proposed for.
//!
//! This is the one check expressible from the tape's own inputs -- (word surface, `MorphemeId`
//! sequence, root index, static grammar tables) -- that needs no allomorph identity and no rule
//! ordering. It is a sound UNDER-approximation on purpose: `Infeasible` is a safe rejection,
//! `Feasible`/`Undecidable` never prove a derivation exists. Two things keep it safe:
//!
//! - Boundary markers (`+`, `^0`, `*0`, `.` -- every reference grammar's own
//!   `<BoundaryDefinition>` set) and archiphoneme/gemination notation (the Spacing-Modifier-Letter
//!   and Superscript-and-Subscript Unicode blocks) never count as required surface characters.
//!   Counting them literally is exactly the bug this module's own measurement caught: it flagged
//!   `membaca`, an HC-CONFIRMED candidate, as impossible before the exclusion existed.
//! - `Copy`/`Modify`/`InsertContext` affix actions count as needing nothing (only `InsertSegments`
//!   text is literal), and the per-morpheme combination search is capped
//!   ([`MAX_SURFACE_COMBOS`]) rather than sampled past it -- both are permissive defaults, so this
//!   can only under-detect, never wrongly reject a real derivation.
//! - A character any [`PhonRuleDef::Rewrite`] input pattern could match is never required either,
//!   however it appears in `InsertSegments` text: a rule can rewrite it before the surface is
//!   reached (an underspecified nasal placeholder that a place-assimilation rule resolves to `m`
//!   or `n`, for instance), so treating it as literal risks the same false rejection the boundary
//!   exclusion above already guards against. Resolving a rule's own natural classes against every
//!   character-definition table in the grammar, rather than only the table its owning stratum
//!   declares, can only over-collect volatile characters -- and over-collection here only makes
//!   the check decline more often, never wrongly reject.
//!
//! The claim this pass emits ([`ProofCategory::ImpossibleSurfaceComposition`]) carries no unit
//! citation: it rests on the candidate identity plus the grammar and the word, neither of which
//! any `TraceUnit` fact holds, so there is nothing for the generic proof verifier to re-derive
//! beyond the shared envelope. Deeper verification is this module's own recall-safety gate.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pg_grammar::chardef::{CharDef, CharDefId};
use pg_grammar::model::{
    AffixAllomorphDef, Grammar, MorphemeId, NaturalClass, NaturalClassKind, OutputAction, Pattern,
    PatternNode, PhonRuleDef,
};

use crate::candidate_filter::decision::{
    AdmissibleProof, DeferReason, PassDecision, ProofCategory, ProofClaim, ProofWitness,
    RejectionProof, StablePassId, StableRuleId,
};
use crate::candidate_filter::model::CandidateWitness;
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::candidate_filter::pipeline::FilterContext;
use crate::confirm::{build_morpheme_owners, MorphemeOwner};
use crate::tags::Candidate;

const SURFACE_CONSISTENCY_PASS: StablePassId = StablePassId("surface.consistency.v1");

/// No per-morpheme allomorph combination's literal characters fit the surface.
const COMPOSITION_RULE: StableRuleId = StableRuleId {
    family: "surface.consistency",
    ordinal: 1,
};

/// Above this many per-morpheme choice combinations, the check declines rather than sampling.
pub const MAX_SURFACE_COMBOS: usize = 20_000;

/// Whether `c` is real orthographic material, not grammar notation (module doc's two false positives).
fn is_literal_surface_char(c: char) -> bool {
    !matches!(c, '+' | '^' | '*' | '.' | '0')
        && !('\u{02B0}'..='\u{02FF}').contains(&c)
        && !('\u{2070}'..='\u{209F}').contains(&c)
}

fn char_multiset(text: &str) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for c in text.chars() {
        *counts.entry(c).or_insert(0) += 1;
    }
    counts
}

/// `char_multiset`, filtered through [`is_literal_surface_char`] and `volatile`.
fn required_multiset(text: &str, volatile: &BTreeSet<char>) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for c in text
        .chars()
        .filter(|&c| is_literal_surface_char(c) && !volatile.contains(&c))
    {
        *counts.entry(c).or_insert(0) += 1;
    }
    counts
}

/// An affix allomorph's own literal contribution: only its `InsertSegments` text counts.
fn affix_literal_multiset(
    a: &AffixAllomorphDef,
    volatile: &BTreeSet<char>,
) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for action in &a.rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            for (&c, &n) in &required_multiset(&shape.text, volatile) {
                *counts.entry(c).or_insert(0) += n;
            }
        }
    }
    counts
}

/// Every character any [`PhonRuleDef::Rewrite`] input pattern could match -- see the module doc.
fn volatile_chars(g: &Grammar) -> BTreeSet<char> {
    let mut classes = Vec::new();
    let mut direct_ids = BTreeSet::new();
    let mut direct_chars = BTreeSet::new();
    for rule in &g.prules {
        if let PhonRuleDef::Rewrite(rule) = rule {
            collect_pattern_refs(
                g,
                &rule.lhs,
                &mut classes,
                &mut direct_ids,
                &mut direct_chars,
            );
        }
    }
    let mut out = direct_chars;
    for table in &g.char_tables {
        for (id, cd) in table.iter() {
            let matches =
                direct_ids.contains(&id) || classes.iter().any(|nc| nat_class_matches(nc, id, cd));
            if matches {
                for rep in cd.representations() {
                    out.extend(rep.chars());
                }
            }
        }
    }
    out
}

fn collect_pattern_refs<'g>(
    g: &'g Grammar,
    pattern: &Pattern,
    classes: &mut Vec<&'g NaturalClass>,
    direct_ids: &mut BTreeSet<CharDefId>,
    direct_chars: &mut BTreeSet<char>,
) {
    for node in &pattern.nodes {
        collect_node_refs(g, node, classes, direct_ids, direct_chars);
    }
}

fn collect_node_refs<'g>(
    g: &'g Grammar,
    node: &PatternNode,
    classes: &mut Vec<&'g NaturalClass>,
    direct_ids: &mut BTreeSet<CharDefId>,
    direct_chars: &mut BTreeSet<char>,
) {
    match node {
        PatternNode::Context(sc) => classes.push(&g.natural_classes[sc.nat_class.0 as usize]),
        PatternNode::CharDef(id) => {
            direct_ids.insert(*id);
        }
        // A literal shape pattern: every character it names is consumed by the match too.
        PatternNode::Segments { shape, .. } => direct_chars.extend(shape.text.chars()),
        PatternNode::Quantifier { children, .. } => {
            for child in children {
                collect_node_refs(g, child, classes, direct_ids, direct_chars);
            }
        }
        PatternNode::Anchor(_) => {}
    }
}

fn nat_class_matches(nc: &NaturalClass, id: CharDefId, cd: &CharDef) -> bool {
    match &nc.kind {
        NaturalClassKind::Segments(ids) => ids.contains(&id),
        NaturalClassKind::Feature(pairs) => pairs.iter().all(|&(f, bits)| {
            cd.feature_lanes()
                .get(f.0 as usize)
                .is_some_and(|&lane| lane & bits.0 != 0)
        }),
    }
}

fn fits(required: &BTreeMap<char, usize>, available: &BTreeMap<char, usize>) -> bool {
    required
        .iter()
        .all(|(c, &n)| available.get(c).copied().unwrap_or(0) >= n)
}

/// Every literal-character option one morpheme's owner could contribute; empty means one free option.
fn literal_char_options(
    g: &Grammar,
    owner: Option<MorphemeOwner>,
    volatile: &BTreeSet<char>,
) -> Vec<BTreeMap<char, usize>> {
    match owner {
        Some(MorphemeOwner::LexEntry(le)) => g.entries[le.0 as usize]
            .allomorphs
            .iter()
            .map(|a| required_multiset(&a.shape.text, volatile))
            .collect(),
        Some(MorphemeOwner::MRule(mid)) => match g.mrules[mid.0 as usize].affix_allomorphs() {
            Some(allos) if !allos.is_empty() => allos
                .iter()
                .map(|a| affix_literal_multiset(a, volatile))
                .collect(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

/// The three things this check can conclude about one candidate against one surface word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceVerdict {
    /// No per-morpheme allomorph combination's required characters fit the surface: a safe
    /// rejection regardless of anything else about the candidate.
    Infeasible,
    /// Some combination fits. This does NOT prove a real derivation exists.
    Feasible,
    /// The combination space exceeded [`MAX_SURFACE_COMBOS`]; declined rather than sampled.
    Undecidable,
}

/// The immutable, grammar-derived facts this check reads: every morpheme's own literal-character
/// options, precomputed once so a per-candidate call does no grammar lookups of its own.
#[derive(Debug)]
pub struct SurfaceConsistencyIndex {
    /// Indexed by `MorphemeId`; empty for a morpheme this check cannot say anything about.
    per_morpheme: Vec<Vec<BTreeMap<char, usize>>>,
}

impl SurfaceConsistencyIndex {
    pub fn build(g: &Grammar) -> Self {
        let owners = build_morpheme_owners(g);
        let volatile = volatile_chars(g);
        let per_morpheme = (0..g.morphemes.len())
            .map(|i| literal_char_options(g, owners[i], &volatile))
            .collect();
        Self { per_morpheme }
    }

    fn options(&self, m: MorphemeId) -> &[BTreeMap<char, usize>] {
        self.per_morpheme
            .get(m.0 as usize)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Whether some per-morpheme allomorph combination's literal characters fit `surface`.
    pub fn verdict(&self, candidate: &Candidate, surface: &str) -> SurfaceVerdict {
        let surface_counts = char_multiset(surface);
        let option_lists: Vec<&[BTreeMap<char, usize>]> = candidate
            .morphemes
            .iter()
            .map(|&m| self.options(m))
            .collect();
        let combo_count = option_lists
            .iter()
            .try_fold(1usize, |acc, opts| acc.checked_mul(opts.len().max(1)))
            .unwrap_or(usize::MAX);
        if combo_count == 0 || combo_count > MAX_SURFACE_COMBOS {
            return SurfaceVerdict::Undecidable;
        }

        let mut indices = vec![0usize; option_lists.len()];
        loop {
            let mut total: BTreeMap<char, usize> = BTreeMap::new();
            for (opts, &idx) in option_lists.iter().zip(&indices) {
                if let Some(multiset) = opts.get(idx) {
                    for (&c, &n) in multiset {
                        *total.entry(c).or_insert(0) += n;
                    }
                }
            }
            if fits(&total, &surface_counts) {
                return SurfaceVerdict::Feasible;
            }
            let mut i = 0;
            loop {
                if i == indices.len() {
                    return SurfaceVerdict::Infeasible;
                }
                indices[i] += 1;
                if indices[i] < option_lists[i].len().max(1) {
                    break;
                }
                indices[i] = 0;
                i += 1;
            }
        }
    }
}

/// Rejects a candidate no per-morpheme allomorph combination could compose into its surface word.
pub struct SurfaceConsistencyPass {
    index: Arc<SurfaceConsistencyIndex>,
}

impl SurfaceConsistencyPass {
    pub fn new(index: Arc<SurfaceConsistencyIndex>) -> Self {
        Self { index }
    }
}

impl CandidateFilterPass for SurfaceConsistencyPass {
    fn id(&self) -> StablePassId {
        SURFACE_CONSISTENCY_PASS
    }

    fn admissible_proofs(&self) -> Vec<AdmissibleProof> {
        vec![AdmissibleProof {
            rule_id: COMPOSITION_RULE,
            category: ProofCategory::ImpossibleSurfaceComposition,
        }]
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        let Some(word) = context.word() else {
            return PassDecision::Defer(DeferReason::UnsupportedConstruct);
        };
        match self.index.verdict(context.identity(), word) {
            SurfaceVerdict::Infeasible => reject(context.identity(), witness),
            SurfaceVerdict::Feasible => PassDecision::Keep,
            SurfaceVerdict::Undecidable => PassDecision::Defer(DeferReason::UnsupportedConstruct),
        }
    }
}

fn reject(identity: &Candidate, witness: &CandidateWitness) -> PassDecision {
    PassDecision::Reject(RejectionProof {
        pass_id: SURFACE_CONSISTENCY_PASS,
        rule_id: COMPOSITION_RULE,
        category: ProofCategory::ImpossibleSurfaceComposition,
        witness: ProofWitness {
            candidate_identity: identity.clone(),
            witness_id: witness.witness_id,
            grammar_revision: witness.provenance.grammar_revision,
            lexicon_revision: witness.lexicon_revision,
            lexical_origin: witness.lexical_origin,
            unit_indices: Vec::new(),
            claim: ProofClaim::ImpossibleSurfaceComposition,
        },
    })
}
