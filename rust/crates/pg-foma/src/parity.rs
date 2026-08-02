//! The recipe parity relation: **deduplicated [`AnalysisIdentity`] set equality, per occurrence.**
//!
//! # What the relation is, and what it is not
//!
//! Two engines agree about one word occurrence when the SETS of analysis identities they produced
//! for it are equal. Three consequences, each of which this module exists to make unavoidable:
//!
//! - **Not full `WordAnalysis` equality.** [`pg_parse::WordAnalysis`] carries dense
//!   compiler-assigned ordinals plus engine-internal payload (`syn_fs`, `mpr`, the per-morpheme
//!   supplied-root slots) that [`AnalysisIdentity`] deliberately does not capture. Comparing whole
//!   `WordAnalysis` values makes engine internals observable as disagreement, which is the wrong
//!   relation — it reports two names for the same analysis as a parity miss.
//! - **Not multiset equality.** Multiplicity is NOT part of the relation. Two analyses that reach
//!   the same identity by different derivational paths are one member of the set, not two, so a
//!   candidate that finds an analysis twice agrees with an oracle that found it once. The
//!   duplicate-path fact is real evidence and is retained ([`IdentityEvidence::duplicate_paths`]) —
//!   it is simply not the verdict.
//! - **Deduplication is WITHIN one occurrence only.** Repeated corpus rows are separate
//!   observations and are never collapsed against one another. That is why the unit of this module
//!   is [`OccurrenceIdentities`] — one word occurrence's set — and why nothing here takes a corpus.
//!
//! # Faults are not misses
//!
//! [`AnalysisIdentity::project`] is fallible, and its failure means an internal inconsistency (an
//! analysis referencing a morpheme its own model lacks), never "these two disagree". Reporting such
//! a fault as an ordinary parity miss would blame the grammar for a bug in the engine; swallowing it
//! as "equal" would certify a candidate on evidence that was never computed. So a fault is typed
//! ([`ParityFault`]) and its only certification is a non-selectable truncation.
//!
//! # v1 certification scope
//!
//! For the v1 four-language certification, supplied roots are REFUSED and guessing is DISABLED. A
//! supplied root is grammar-external content injected at runtime, and a guessed analysis has a
//! fabricated root with no authored source at all ([`AnalysisIdentity`] records it as a `None`
//! morpheme key); neither is evidence about the compiled grammar, which is the only thing a recipe
//! certification is a statement about. Both are refused as typed faults rather than being silently
//! excluded, because a certification computed over a silently-narrowed analysis set would carry the
//! full corpus's name.
//!
//! Note the layering: [`OccurrenceIdentities::project`] RECORDS the guessed/supplied annotations as
//! evidence and refuses nothing; [`certified_occurrence`] applies the scope. Evidence and policy are
//! separate on purpose — a later profile that admits guessing needs a different policy, not a
//! different projector.

use std::collections::BTreeMap;

use pg_grammar::model::Grammar;
use pg_parse::identity::{AnalysisIdentity, IdentityError};
use pg_parse::{AnalysisProvenance, WordAnalysis};

/// One distinct identity observed within ONE word occurrence, plus the evidence that deduplicating
/// down to it erased.
///
/// The parity verdict reads [`Self::identity`] and nothing else. The other three fields exist so
/// that collapsing a set does not also destroy the diagnostic record of what was collapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityEvidence {
    /// The identity itself — the only field the parity relation looks at.
    pub identity: AnalysisIdentity,
    /// How many raw [`WordAnalysis`] values collapsed into this identity. Always at least 1.
    ///
    /// Greater than 1 means the engine reached the same analysis by more than one derivational
    /// path. That is a real property of the compilation (and a useful signal about redundant
    /// proposal work), but it is not disagreement, so it is recorded here and ignored by the
    /// verdict.
    pub duplicate_paths: u32,
    /// Whether ANY raw analysis that collapsed into this identity came from the guess branch.
    ///
    /// Kept even though the v1 scope refuses guessed analyses outright: the refusal happens in
    /// [`certified_occurrence`], and the report should be able to say WHICH identity carried the
    /// annotation rather than only that some did.
    pub guessed: bool,
    /// Whether ANY raw analysis that collapsed into this identity carried a supplied (runtime,
    /// grammar-external) root.
    pub supplied_root: bool,
}

/// The deduplicated identity set of ONE word occurrence, in a canonical order.
///
/// Entries are ordered by [`AnalysisIdentity`]'s own total order and are distinct by construction,
/// which is what makes [`Self::same_identities`] a genuine set comparison rather than an
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
    /// nor stable. The underlying [`IdentityError`]'s own text is available on the value itself for
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
/// This is the function a certification path should call; [`OccurrenceIdentities::project`] is the
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
