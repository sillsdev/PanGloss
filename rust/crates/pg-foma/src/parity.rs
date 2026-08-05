//! The recipe parity relation: **deduplicated `AnalysisIdentity` set equality, per occurrence.**
//!
//! # What the relation is, and what it is not
//!
//! Two engines agree about one word occurrence when the SETS of analysis identities they produced
//! for it are equal. Three consequences, each of which this module exists to make unavoidable:
//!
//! - **Not full `WordAnalysis` equality.** `pg_parse::WordAnalysis` carries dense
//!   compiler-assigned ordinals plus engine-internal payload (`syn_fs`, `mpr`, the per-morpheme
//!   supplied-root slots) that `AnalysisIdentity` deliberately does not capture. Comparing whole
//!   `WordAnalysis` values makes engine internals observable as disagreement, which is the wrong
//!   relation — it reports two names for the same analysis as a parity miss.
//! - **Not multiset equality.** Multiplicity is NOT part of the relation. Two analyses that reach
//!   the same identity by different derivational paths are one member of the set, not two, so a
//!   candidate that finds an analysis twice agrees with an oracle that found it once. The
//!   duplicate-path fact is real evidence and is retained (`IdentityEvidence::duplicate_paths`) —
//!   it is simply not the verdict.
//! - **Deduplication is WITHIN one occurrence only.** Repeated corpus rows are separate
//!   observations and are never collapsed against one another. That is why the unit of this module
//!   is `OccurrenceIdentities` — one word occurrence's set — and why nothing here takes a corpus.
//!
//! # Faults are not misses
//!
//! `AnalysisIdentity::project` is fallible, and its failure means an internal inconsistency (an
//! analysis referencing a morpheme its own model lacks), never "these two disagree". Reporting such
//! a fault as an ordinary parity miss would blame the grammar for a bug in the engine; swallowing it
//! as "equal" would certify a candidate on evidence that was never computed. So a fault is typed
//! (`ParityFault`) and its only certification is a non-selectable truncation.
//!
//! # v1 certification scope
//!
//! For the v1 four-language certification, supplied roots are REFUSED and guessing is DISABLED. A
//! supplied root is grammar-external content injected at runtime, and a guessed analysis has a
//! fabricated root with no authored source at all (`AnalysisIdentity` records it as a `None`
//! morpheme key); neither is evidence about the compiled grammar, which is the only thing a recipe
//! certification is a statement about. Both are refused as typed faults rather than being silently
//! excluded, because a certification computed over a silently-narrowed analysis set would carry the
//! full corpus's name.
//!
//! Note the layering, pinned by `projection_records_annotations_while_certification_refuses_them`:
//! `OccurrenceIdentities::project` records the guessed/supplied annotations as evidence;
//! `certified_occurrence` applies the scope. Evidence and policy are separate on purpose — a
//! later profile that admits guessing needs a different policy, not a different projector.

use std::collections::BTreeMap;

use pg_grammar::model::Grammar;
use pg_parse::identity::{AnalysisIdentity, IdentityError};
use pg_parse::{AnalysisProvenance, WordAnalysis};

/// One distinct identity observed within ONE word occurrence, plus the evidence that deduplicating
/// down to it erased.
///
/// The parity verdict reads `Self::identity` and nothing else. The other three fields exist so
/// that collapsing a set does not also destroy the diagnostic record of what was collapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityEvidence {
    /// The identity itself — the only field the parity relation looks at.
    pub identity: AnalysisIdentity,
    /// How many raw `WordAnalysis` values collapsed into this identity. Always at least 1.
    ///
    /// Greater than 1 means the engine reached the same analysis by more than one derivational
    /// path. That is a real property of the compilation (and a useful signal about redundant
    /// proposal work), but it is not disagreement, so it is recorded here and ignored by the
    /// verdict.
    pub duplicate_paths: u32,
    /// Whether ANY raw analysis that collapsed into this identity came from the guess branch.
    ///
    /// Kept even though the v1 scope refuses guessed analyses outright: the refusal happens in
    /// `certified_occurrence`, and the report should be able to say WHICH identity carried the
    /// annotation rather than only that some did.
    pub guessed: bool,
    /// Whether ANY raw analysis that collapsed into this identity carried a supplied (runtime,
    /// grammar-external) root.
    pub supplied_root: bool,
}

/// The deduplicated identity set of ONE word occurrence, in a canonical order.
///
/// Entries are ordered by `AnalysisIdentity`'s own total order and are distinct by construction,
/// which is what makes `Self::same_identities` a genuine set comparison rather than an
/// order-sensitive one. Discovery order is deliberately discarded: it is not semantic, and letting
/// it reach a verdict would make engine traversal order observable as a grammar difference.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OccurrenceIdentities {
    entries: Vec<IdentityEvidence>,
}

impl OccurrenceIdentities {
    /// Project one occurrence's raw analyses, deduplicating by identity and retaining the erased
    /// duplicate-path and annotation evidence.
    ///
    /// Applies NO policy: a guessed or supplied-root analysis projects normally here and is merely
    /// annotated. See this module's doc for why evidence and policy are separated.
    pub fn project(analyses: &[WordAnalysis], grammar: &Grammar) -> Result<Self, IdentityError> {
        // Keyed by the identity value itself rather than by a digest of it: the value IS the key
        // here (`AnalysisIdentity` derives a total `Ord` over every field it carries), so there is
        // no hash to collide and no second canonicalization to keep in step with the first.
        let mut by_identity: BTreeMap<AnalysisIdentity, IdentityEvidence> = BTreeMap::new();
        for analysis in analyses {
            let identity = AnalysisIdentity::project(analysis, grammar)?;
            let guessed = is_guessed(analysis);
            let supplied_root = has_supplied_root(analysis);
            match by_identity.get_mut(&identity) {
                Some(existing) => {
                    existing.duplicate_paths = existing.duplicate_paths.saturating_add(1);
                    // Keep the fact that SOME path carried the annotation rather than letting
                    // whichever copy arrived last decide.
                    existing.guessed |= guessed;
                    existing.supplied_root |= supplied_root;
                }
                None => {
                    by_identity.insert(
                        identity.clone(),
                        IdentityEvidence {
                            identity,
                            duplicate_paths: 1,
                            guessed,
                            supplied_root,
                        },
                    );
                }
            }
        }
        Ok(Self {
            entries: by_identity.into_values().collect(),
        })
    }

    pub fn entries(&self) -> &[IdentityEvidence] {
        &self.entries
    }

    /// The set itself, in canonical order.
    pub fn identities(&self) -> impl ExactSizeIterator<Item = &AnalysisIdentity> + '_ {
        self.entries.iter().map(|entry| &entry.identity)
    }

    /// How many DISTINCT identities this occurrence yielded.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many RAW analyses were projected, before deduplication.
    pub fn raw_analyses(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| u64::from(entry.duplicate_paths))
            .sum()
    }

    /// How many raw analyses deduplication removed — `raw_analyses() - len()`.
    pub fn collapsed_paths(&self) -> u64 {
        self.raw_analyses() - self.len() as u64
    }

    pub fn any_guessed(&self) -> bool {
        self.entries.iter().any(|entry| entry.guessed)
    }

    pub fn any_supplied_root(&self) -> bool {
        self.entries.iter().any(|entry| entry.supplied_root)
    }

    /// **The parity relation.** Set equality over identities, blind to duplicate paths and to every
    /// annotation.
    pub fn same_identities(&self, other: &Self) -> bool {
        self.len() == other.len() && self.identities().eq(other.identities())
    }

    /// Identities present here and absent from `other`, in canonical order. Diagnostic only.
    pub fn identities_absent_from<'a>(&'a self, other: &Self) -> Vec<&'a AnalysisIdentity> {
        self.entries
            .iter()
            .map(|entry| &entry.identity)
            .filter(|identity| !other.contains(identity))
            .collect()
    }

    pub fn contains(&self, identity: &AnalysisIdentity) -> bool {
        self.entries
            .binary_search_by(|entry| entry.identity.cmp(identity))
            .is_ok()
    }

    /// How many members of this set share another member's **admission key** —
    /// `(morphemes, root_index)` — and therefore differ from it ONLY in `category`.
    ///
    /// `0` means the admission key is INJECTIVE on this set, which is the property a
    /// confirmation-free accuracy verdict needs: `crate::confirm::confirm_batch` routes a
    /// confirmed analysis to a candidate by exactly this key (`admission_key`), so a proposal set
    /// containing the key of every oracle analysis admits every oracle analysis — but only if one
    /// key cannot stand for two distinct identities. Where it can, "the key was proposed" is a
    /// weaker statement than "that identity is reachable", and the difference has to be counted
    /// rather than assumed away.
    ///
    /// Cheap and exact rather than a second hash pass: `entries` is already in
    /// `AnalysisIdentity`'s own total order, whose leading components are `morphemes` then
    /// `root_index`, so key-sharing members are necessarily ADJACENT.
    pub fn admission_key_collisions(&self) -> u64 {
        self.entries
            .windows(2)
            .filter(|pair| {
                pair[0].identity.morphemes == pair[1].identity.morphemes
                    && pair[0].identity.root_index == pair[1].identity.root_index
            })
            .count() as u64
    }
}

/// The **admission key** of one analysis or proposal: its ordered morpheme ordinals plus its root
/// position.
///
/// This is not a new notion invented here. It is verbatim the key
/// `crate::confirm::confirm_batch` builds its `by_key` routing map from, and verbatim the key
/// `propose UNION peel` deduplicates candidates by. Naming it once, here, is what stops a
/// confirmation-free accuracy check from inventing a *second*, subtly different notion of "the
/// candidate offered this analysis" — the drift that would make the fast verdict and the full
/// certification answer different questions while appearing to answer the same one.
///
/// Deliberately dense ORDINALS, not the stable source keys `AnalysisIdentity` carries: an
/// admission key is only ever compared WITHIN one compiled grammar and one run (proposals against
/// that same run's oracle analyses), where ordinals are exactly what confirm itself compares. An
/// identity is the cross-compilation notion; this is the intra-run one. Using an identity here
/// would also be strictly wrong, because it carries `category`, which no proposal has.
pub type AdmissionKey = (Vec<u32>, i32);

/// One analysis's `AdmissionKey`. Mirrors `confirm_batch`'s own
/// `(wa.morpheme_ids.clone(), wa.root_morpheme_index)`.
pub fn admission_key(analysis: &WordAnalysis) -> AdmissionKey {
    (
        analysis.morpheme_ids.clone(),
        analysis.root_morpheme_index,
    )
}

/// Counted evidence about HOW two identity sets diverged, accumulated over a whole run.
///
/// # Why this exists, and why it is counted rather than reasoned about
///
/// A confirmation-free accuracy verdict rests on one direction of the parity relation being free:
/// the candidate's confirmed set cannot contain an identity the oracle's set lacks. The argument
/// for that is strong — the candidate's confirm is a RESTRICTED `parse_word_selected` and the
/// oracle is the same engine unrestricted, so the candidate explores a subset — but it is not
/// airtight. `pg_rules::word::WordKey`, the analysis-search dedup key, deliberately EXCLUDES the
/// syntactic feature struct, while `AnalysisIdentity::category` is projected from it (via
/// `WordAnalysis::pos_id`). Two search states differing only in `syn_fs` therefore collapse to one
/// map entry, and which one's `syn_fs` survives is decided first-wins by traversal order — which
/// the restriction changes. So a restricted run can in principle surface a category the
/// unrestricted run deduplicated away, producing a **candidate-only identity**.
///
/// That is inference, not observation. Building containment on top of it without measuring would
/// silently certify a wrong answer on the day it happens, so `Self::candidate_only_identities` is
/// counted on the ordinary certification path and reported per run.
///
/// # "I could not look" is a distinct outcome
///
/// `Self::occurrences_not_compared` exists so a run that never got to compare anything (a
/// projection fault, a v1-scope refusal, a resource breach, a refused corpus) cannot be read as a
/// run that compared everything and found nothing wrong. A zero in
/// `Self::candidate_only_identities` is only evidence in proportion to
/// `Self::occurrences_compared`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IdentityDivergence {
    /// Occurrences whose two identity sets were both projected and actually compared.
    pub occurrences_compared: u64,
    /// Occurrences for which no comparison happened at all — see the type's own doc.
    pub occurrences_not_compared: u64,
    /// Distinct oracle identities summed over compared occurrences.
    pub oracle_identities: u64,
    /// Distinct candidate identities summed over compared occurrences.
    pub candidate_identities: u64,
    /// **The recall failures.** Identities the oracle found and the candidate did not.
    pub oracle_only_identities: u64,
    /// **The soundness hazard.** Identities the candidate produced that the oracle's set does not
    /// contain. Expected to be 0; a non-zero value invalidates the free-containment argument and is
    /// a finding in its own right.
    pub candidate_only_identities: u64,
    /// Occurrences contributing at least one `Self::candidate_only_identities` — so a single
    /// pathological word cannot be mistaken for a widespread divergence, or vice versa.
    pub occurrences_with_candidate_only: u64,
    /// Summed `OccurrenceIdentities::admission_key_collisions` over the ORACLE side of every
    /// compared occurrence. Expected 0; non-zero means an admission key stands for more than one
    /// oracle identity on some word, so key containment is coarser than identity containment there.
    pub oracle_admission_key_collisions: u64,
    /// The same, on the candidate side.
    pub candidate_admission_key_collisions: u64,
}

impl IdentityDivergence {
    /// The divergence of ONE compared occurrence.
    pub fn compare(oracle: &OccurrenceIdentities, candidate: &OccurrenceIdentities) -> Self {
        let candidate_only = candidate.identities_absent_from(oracle).len() as u64;
        Self {
            occurrences_compared: 1,
            occurrences_not_compared: 0,
            oracle_identities: oracle.len() as u64,
            candidate_identities: candidate.len() as u64,
            oracle_only_identities: oracle.identities_absent_from(candidate).len() as u64,
            candidate_only_identities: candidate_only,
            occurrences_with_candidate_only: u64::from(candidate_only > 0),
            oracle_admission_key_collisions: oracle.admission_key_collisions(),
            candidate_admission_key_collisions: candidate.admission_key_collisions(),
        }
    }

    /// One occurrence that could not be compared at all.
    pub fn not_compared(occurrences: u64) -> Self {
        Self {
            occurrences_not_compared: occurrences,
            ..Self::default()
        }
    }

    /// Field-wise saturating sum, so accumulating over a corpus or a run cannot wrap.
    pub fn absorb(&mut self, other: Self) {
        self.occurrences_compared = self.occurrences_compared.saturating_add(other.occurrences_compared);
        self.occurrences_not_compared = self
            .occurrences_not_compared
            .saturating_add(other.occurrences_not_compared);
        self.oracle_identities = self.oracle_identities.saturating_add(other.oracle_identities);
        self.candidate_identities = self
            .candidate_identities
            .saturating_add(other.candidate_identities);
        self.oracle_only_identities = self
            .oracle_only_identities
            .saturating_add(other.oracle_only_identities);
        self.candidate_only_identities = self
            .candidate_only_identities
            .saturating_add(other.candidate_only_identities);
        self.occurrences_with_candidate_only = self
            .occurrences_with_candidate_only
            .saturating_add(other.occurrences_with_candidate_only);
        self.oracle_admission_key_collisions = self
            .oracle_admission_key_collisions
            .saturating_add(other.oracle_admission_key_collisions);
        self.candidate_admission_key_collisions = self
            .candidate_admission_key_collisions
            .saturating_add(other.candidate_admission_key_collisions);
    }

    /// Whether this run's evidence supports the free-containment argument: something WAS compared,
    /// and nothing candidate-only was found. Deliberately not `candidate_only == 0` alone — that is
    /// also true of a run that compared nothing.
    pub fn supports_free_containment(&self) -> bool {
        self.occurrences_compared > 0 && self.candidate_only_identities == 0
    }
}

/// Which side of a parity comparison a fault was found on.
///
/// Worth distinguishing because the two diagnoses are completely different: an oracle-side fault is
/// a bug in the ground truth (nothing about the candidate has been learned), while a candidate-side
/// fault is a bug in the compilation under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParitySide {
    /// The ground-truth `pg_parse::Morpher` result.
    Oracle,
    /// The compiled candidate's confirmed result.
    Candidate,
}

impl ParitySide {
    fn suffix(self) -> &'static str {
        match self {
            ParitySide::Oracle => "oracle",
            ParitySide::Candidate => "candidate",
        }
    }
}

/// Stage prefix for a projection failure — an internal inconsistency, never a parity miss.
pub const STAGE_IDENTITY_PROJECTION_FAILED: &str = "identity-projection-failed";
/// Stage prefix for an analysis carrying a supplied (runtime, grammar-external) root.
pub const STAGE_SUPPLIED_ROOT_REFUSED: &str = "supplied-root-refused";
/// Stage prefix for an analysis from the guess branch.
pub const STAGE_GUESSING_REFUSED: &str = "guessing-refused";

/// Why an occurrence could not be certified AT ALL — a typed fault, never a disagreement.
///
/// Every variant makes the candidate non-selectable. None of them may ever be reported as an
/// ordinary parity miss: a miss says "the two engines disagree about this word", which is a claim
/// about the grammar, and none of these faults support that claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParityFault {
    /// An analysis references a morpheme or category its own model lacks.
    IdentityProjection {
        side: ParitySide,
        error: IdentityError,
    },
    /// An analysis carried a supplied root. Refused by the v1 certification scope.
    SuppliedRoot { side: ParitySide },
    /// An analysis came from the guess branch. Refused by the v1 certification scope.
    Guessed { side: ParitySide },
}

impl ParityFault {
    /// The `Certification::Truncated { stage }` string this fault is reported under.
    ///
    /// Deliberately a short, closed set of six kebab-case values (three causes x two sides) rather
    /// than a formatted message: `Truncated` carries no detail field, stage strings are matched
    /// exactly by gates, and an unbounded message interpolated into one would be neither greppable
    /// nor stable. The underlying `IdentityError`'s own text is available on the value itself for
    /// a caller that wants to log it.
    pub fn stage(&self) -> String {
        let (cause, side) = match self {
            ParityFault::IdentityProjection { side, .. } => {
                (STAGE_IDENTITY_PROJECTION_FAILED, *side)
            }
            ParityFault::SuppliedRoot { side } => (STAGE_SUPPLIED_ROOT_REFUSED, *side),
            ParityFault::Guessed { side } => (STAGE_GUESSING_REFUSED, *side),
        };
        format!("{cause}-{}", side.suffix())
    }

    pub fn side(&self) -> ParitySide {
        match self {
            ParityFault::IdentityProjection { side, .. }
            | ParityFault::SuppliedRoot { side }
            | ParityFault::Guessed { side } => *side,
        }
    }
}

impl std::fmt::Display for ParityFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParityFault::IdentityProjection { side, error } => write!(
                f,
                "{side:?}-side analysis could not be projected to stable identity keys: {error}"
            ),
            ParityFault::SuppliedRoot { side } => write!(
                f,
                "{side:?}-side analysis carries a supplied root; v1 recipe certification refuses \
                 grammar-external roots"
            ),
            ParityFault::Guessed { side } => write!(
                f,
                "{side:?}-side analysis came from the guess branch; v1 recipe certification \
                 disables guessing"
            ),
        }
    }
}

impl std::error::Error for ParityFault {}

/// Project one occurrence AND apply the v1 certification scope.
///
/// This is the function a certification path should call; `OccurrenceIdentities::project` is the
/// evidence-only half, for a report that wants the annotations without the policy.
pub fn certified_occurrence(
    analyses: &[WordAnalysis],
    grammar: &Grammar,
    side: ParitySide,
) -> Result<OccurrenceIdentities, ParityFault> {
    let identities = OccurrenceIdentities::project(analyses, grammar)
        .map_err(|error| ParityFault::IdentityProjection { side, error })?;
    // Supplied roots first: a supplied root is the more specific and more actionable diagnosis, and
    // an analysis that is both is refused either way.
    if identities.any_supplied_root() {
        return Err(ParityFault::SuppliedRoot { side });
    }
    if identities.any_guessed() {
        return Err(ParityFault::Guessed { side });
    }
    Ok(identities)
}

fn is_guessed(analysis: &WordAnalysis) -> bool {
    analysis.guessed || matches!(analysis.provenance, AnalysisProvenance::Guessed)
}

fn has_supplied_root(analysis: &WordAnalysis) -> bool {
    analysis.supplied_root.is_some()
        || analysis.morpheme_roots.iter().any(Option::is_some)
        || matches!(
            analysis.provenance,
            AnalysisProvenance::Supplied { .. } | AnalysisProvenance::SuppliedOverride { .. }
        )
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::test_support::{
        parity_analysis as analysis, parity_fixture_grammar as test_grammar,
    };

    #[test]
    fn duplicate_paths_collapse_into_one_set_member_and_are_counted() {
        let g = test_grammar();
        let projected = OccurrenceIdentities::project(&[analysis(0), analysis(0), analysis(1)], &g)
            .expect("every ordinal has a model row");
        assert_eq!(projected.len(), 2, "two DISTINCT identities");
        assert_eq!(projected.raw_analyses(), 3);
        assert_eq!(projected.collapsed_paths(), 1);
        let duplicated_key = g.morphemes[0].xml_key.clone();
        let first = projected
            .entries()
            .iter()
            .find(|entry| entry.identity.morphemes == vec![Some(duplicated_key.clone())])
            .expect("the duplicated identity is a member");
        assert_eq!(first.duplicate_paths, 2);
    }

    #[test]
    fn discovery_order_does_not_change_the_set() {
        let g = test_grammar();
        let forward = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();
        let reversed = OccurrenceIdentities::project(&[analysis(1), analysis(0)], &g).unwrap();
        assert!(forward.same_identities(&reversed));
        assert_eq!(
            forward, reversed,
            "canonical order makes the values equal too"
        );
    }

    #[test]
    fn an_unresolvable_ordinal_is_a_projection_error_not_an_empty_set() {
        let g = test_grammar();
        let err = OccurrenceIdentities::project(&[analysis(9_999)], &g)
            .expect_err("ordinal 9999 has no model row");
        assert!(matches!(
            err,
            IdentityError::UnresolvedMorpheme { ordinal: 9_999 }
        ));
    }

    #[test]
    fn an_admission_key_standing_for_two_identities_is_counted_not_ignored() {
        // Two identities with the SAME ordered morphemes and the SAME root position, differing only
        // in `category`. This is the shape that makes admission-key containment coarser than
        // identity containment, so it must be COUNTED rather than silently collapsed.
        let shared = vec![Some("m0".to_string())];
        let one = AnalysisIdentity {
            morphemes: shared.clone(),
            root_index: 0,
            category: Some("posA".into()),
        };
        let two = AnalysisIdentity {
            morphemes: shared,
            root_index: 0,
            category: Some("posB".into()),
        };
        let mut set = OccurrenceIdentities::default();
        // Built directly rather than projected: the projector needs a grammar carrying two
        // categories for one morpheme sequence, and the property under test is a property of the
        // SET, not of projection.
        set.entries = vec![
            IdentityEvidence {
                identity: one,
                duplicate_paths: 1,
                guessed: false,
                supplied_root: false,
            },
            IdentityEvidence {
                identity: two,
                duplicate_paths: 1,
                guessed: false,
                supplied_root: false,
            },
        ];
        assert_eq!(set.admission_key_collisions(), 1);

        // A set whose members differ in a component of the key itself has no collision.
        let g = test_grammar();
        let distinct = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct.admission_key_collisions(), 0);
    }

    #[test]
    fn divergence_separates_the_two_directions_and_never_calls_nothing_agreement() {
        let g = test_grammar();
        let oracle = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();
        let candidate = OccurrenceIdentities::project(&[analysis(1), analysis(2)], &g).unwrap();
        let divergence = IdentityDivergence::compare(&oracle, &candidate);
        assert_eq!(divergence.occurrences_compared, 1);
        assert_eq!(divergence.oracle_identities, 2);
        assert_eq!(divergence.candidate_identities, 2);
        // `analysis(0)` is oracle-only (a recall failure); `analysis(2)` is candidate-only (the
        // soundness hazard). The two must never be summed into one "they differ" number.
        assert_eq!(divergence.oracle_only_identities, 1);
        assert_eq!(divergence.candidate_only_identities, 1);
        assert_eq!(divergence.occurrences_with_candidate_only, 1);
        assert!(
            !divergence.supports_free_containment(),
            "a candidate-only identity must withdraw support for free containment"
        );

        let agreeing = IdentityDivergence::compare(&oracle, &oracle);
        assert_eq!(agreeing.candidate_only_identities, 0);
        assert_eq!(agreeing.oracle_only_identities, 0);
        assert!(agreeing.supports_free_containment());

        // "I could not look" is not "everything is fine": zero candidate-only identities over zero
        // compared occurrences supports nothing.
        let blind = IdentityDivergence::not_compared(7);
        assert_eq!(blind.candidate_only_identities, 0);
        assert_eq!(blind.occurrences_not_compared, 7);
        assert!(
            !blind.supports_free_containment(),
            "an uncompared run must not read as a clean run"
        );

        let mut total = agreeing;
        total.absorb(blind);
        assert_eq!(total.occurrences_compared, 1);
        assert_eq!(total.occurrences_not_compared, 7);
    }

    #[test]
    fn the_admission_key_is_the_key_confirm_routes_on() {
        // Pins the correspondence this whole mechanism rests on: `confirm_batch` keys its routing
        // map on `(wa.morpheme_ids.clone(), wa.root_morpheme_index)`. If `admission_key` ever stops
        // being that exact pair, key containment stops implying admissibility.
        let a = analysis(1);
        assert_eq!(
            admission_key(&a),
            (a.morpheme_ids.clone(), a.root_morpheme_index)
        );
    }

    #[test]
    fn every_fault_names_its_cause_and_its_side() {
        // The six stage strings are a closed set that gates match exactly; pin them.
        let projection = ParityFault::IdentityProjection {
            side: ParitySide::Oracle,
            error: IdentityError::UnresolvedMorpheme { ordinal: 1 },
        };
        assert_eq!(projection.stage(), "identity-projection-failed-oracle");
        assert_eq!(
            ParityFault::SuppliedRoot {
                side: ParitySide::Candidate
            }
            .stage(),
            "supplied-root-refused-candidate"
        );
        assert_eq!(
            ParityFault::Guessed {
                side: ParitySide::Oracle
            }
            .stage(),
            "guessing-refused-oracle"
        );
    }

    #[test]
    fn projection_records_annotations_while_certification_refuses_them() {
        // The layering this module's doc describes: `project` annotates, `certified_occurrence`
        // applies policy. A test that only exercised the policy could not tell the difference
        // between "annotated then refused" and "never recorded".
        let g = test_grammar();
        let mut guessed = analysis(0);
        guessed.guessed = true;
        let projected = OccurrenceIdentities::project(std::slice::from_ref(&guessed), &g)
            .expect("a guessed analysis still projects");
        assert!(projected.any_guessed());
        assert_eq!(projected.len(), 1, "policy is not applied by the projector");

        let refused = certified_occurrence(std::slice::from_ref(&guessed), &g, ParitySide::Oracle)
            .expect_err("v1 certification disables guessing");
        assert_eq!(refused.stage(), "guessing-refused-oracle");
    }
}
