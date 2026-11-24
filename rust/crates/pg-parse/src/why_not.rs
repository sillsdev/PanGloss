//! Why-not (constrained parse), Motif #386: parse one word along GIVEN morph candidates and report
//! the first failing constraint instead of searching every path.
//!
//! The types, input validation and wire shape (`pangloss.whynot.v1`, `docs/formats/why-not.md`)
//! are what a consumer codes against; [`why_not`] validates its input and answers each candidate
//! with `engine`. Vocabulary is the
//! rejection-diagnostics v1 vocabulary (`docs/formats/rejection-trace-v1.md`): `hc_reason` carries
//! the C# `FailureReason` name, the grammar object carries `type`/`id`/`guid`, and `entry_guid` plus
//! `msa_guid` are the same FieldWorks identities the v1 selector resolves. Morphemes and stem names
//! are named by their grammar.xml id (`MorphemeInfo::xml_key`, `StemNameDef::xml_id`) and allomorphs
//! by `Grammar::allomorph_xml_key`.

mod engine;

use std::fmt;

use pg_grammar_model::model::{AllomorphId, AllomorphOwner, Grammar, MorphRuleDef, MorphemeId};
use serde::{Deserialize, Serialize};

use pg_rules::trace::FailureReason;

use crate::Morpher;

/// The wire schema identifier written as `schema` in every report.
pub const SCHEMA: &str = "pangloss.whynot.v1";

/// FieldWorks identities of one expected morph, as the rejection-diagnostics v1 selector spells them.
/// Both are optional: HC ids are the required identity, GUIDs are filled in when the caller holds
/// the FieldWorks model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GuidRef {
    #[serde(default)]
    pub entry_guid: Option<String>,
    #[serde(default)]
    pub msa_guid: Option<String>,
}

/// One choice at a position: a morpheme named by its grammar.xml id, and optionally one of its
/// allomorphs by the allomorph's grammar.xml id. `guid` disambiguates a morpheme id that several
/// morphemes share (a variant sharing an MSA) and, when given, must match the morpheme it names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedMorph {
    pub morpheme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allomorph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<GuidRef>,
}

impl ExpectedMorph {
    pub fn new(morpheme: impl Into<String>) -> Self {
        Self {
            morpheme: morpheme.into(),
            allomorph: None,
            guid: None,
        }
    }
}

/// One candidate: positions in surface order, each a non-empty list of choices that are all tried.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Candidate {
    pub positions: Vec<Vec<ExpectedMorph>>,
}

/// A why-not request: one word and one or more candidates.
#[derive(Clone, Debug)]
pub struct WhyNotRequest<'a> {
    pub word: &'a str,
    pub candidates: Vec<Candidate>,
    /// Position (within every candidate) of the root, when the caller knows it. Validated but not used by the search yet; the search finds the root itself.
    pub root_index: Option<usize>,
}

/// Why a candidate has no verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Accepted when reading a report, so a report that carries it still parses.
    NotImplemented,
    WorkCap,
    Deadline,
    /// A failure was found that cannot fill a required failure field (the C# tool emits this).
    FailureNotExpressible,
    /// No position names a lexical entry, so no root can be pinned.
    NoRoot,
    /// A pinned morpheme is neither a lexical entry nor an affix rule, so it cannot be pinned.
    UnsupportedMorpheme,
    /// Several positions name lexical entries; compounding is not pinned.
    CompoundingUnsupported,
    /// The word cannot be segmented with the grammar's character table.
    WordNotSegmentable,
    /// The furthest derivation fails a check the engine does not name yet.
    UnclassifiedFailure,
    /// The choices at the positions multiply past the number of sequences the engine will try.
    ChoicesCapped,
}

fn stop_unclassified(note: &str) -> WhyNotOutcome {
    WhyNotOutcome::Unknown(Stop {
        reason: StopReason::UnclassifiedFailure,
        note: Some(note.to_string()),
    })
}

/// A stop with its reason and an optional sentence saying what was reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    pub reason: StopReason,
    pub note: Option<String>,
}

impl From<StopReason> for Stop {
    fn from(reason: StopReason) -> Self {
        Stop { reason, note: None }
    }
}

/// The typed first failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    NoMorphMatch,
    EnvironmentUnmet,
    SlotOrder,
    RequiredSlotEmpty,
    FeatureClash,
    AdhocProhibition,
    StemName,
    InflectionClass,
    SurfaceMismatch,
    OtherGate,
}

impl FailureKind {
    /// Whether the engine can answer this kind today; every other kind comes back `unknown`. The match is exhaustive so a new kind must be classified here.
    pub const fn is_answered(self) -> bool {
        match self {
            FailureKind::NoMorphMatch
            | FailureKind::EnvironmentUnmet
            | FailureKind::AdhocProhibition
            | FailureKind::OtherGate => true,
            FailureKind::SlotOrder
            | FailureKind::RequiredSlotEmpty
            | FailureKind::FeatureClash
            | FailureKind::StemName
            | FailureKind::InflectionClass
            | FailureKind::SurfaceMismatch => false,
        }
    }

    /// The kinds that are answered today, in `ALL` order, as their wire names.
    pub fn answered() -> Vec<String> {
        Self::ALL
            .into_iter()
            .filter(|k| k.is_answered())
            .map(|k| {
                serde_json::to_value(k)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .expect("a kind serializes as a string")
            })
            .collect()
    }

    pub const ALL: [FailureKind; 10] = [
        FailureKind::NoMorphMatch,
        FailureKind::EnvironmentUnmet,
        FailureKind::SlotOrder,
        FailureKind::RequiredSlotEmpty,
        FailureKind::FeatureClash,
        FailureKind::AdhocProhibition,
        FailureKind::StemName,
        FailureKind::InflectionClass,
        FailureKind::SurfaceMismatch,
        FailureKind::OtherGate,
    ];
}

/// How far a derivation got before failing: the slot of the allomorph whose check failed (a
/// word-level failure is slot 0, the surface match the last slot), then the check's place in
/// `Allomorph.IsWordValid`'s order. The one key that ranks failures within a sequence, across
/// sequences and across candidates; a larger key is further.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Progress {
    slot: u32,
    check: u8,
}

impl Progress {
    /// `position` is the failing allomorph's index in morph order, when the evidence names one.
    pub(crate) fn of_reason(position: Option<usize>, reason: FailureReason) -> Self {
        let check = match reason {
            FailureReason::PartialParse => 1,
            FailureReason::ObligatorySyntacticFeatures => 2,
            FailureReason::BoundRoot => 3,
            FailureReason::RequiredStemName | FailureReason::ExcludedStemName => 4,
            FailureReason::AllomorphCoOccurrenceRules => 5,
            FailureReason::MorphemeCoOccurrenceRules => 6,
            FailureReason::Environments => 7,
            FailureReason::DisjunctiveAllomorph => 8,
            FailureReason::SurfaceFormMismatch => 9,
            _ => 0,
        };
        let slot = match reason {
            FailureReason::PartialParse | FailureReason::ObligatorySyntacticFeatures => 0,
            FailureReason::SurfaceFormMismatch => u32::MAX,
            _ => position.map_or(0, |p| p as u32 + 1),
        };
        Progress { slot, check }
    }

    pub(crate) fn of_failure(f: &FirstFailure) -> Self {
        let reason = match f.kind {
            FailureKind::NoMorphMatch => {
                return Progress { slot: 0, check: 0 };
            }
            FailureKind::EnvironmentUnmet => FailureReason::Environments,
            FailureKind::AdhocProhibition
                if f.hc_reason.as_deref() == Some("MorphemeCoOccurrenceRules") =>
            {
                FailureReason::MorphemeCoOccurrenceRules
            }
            FailureKind::AdhocProhibition => FailureReason::AllomorphCoOccurrenceRules,
            FailureKind::StemName => FailureReason::RequiredStemName,
            FailureKind::SurfaceMismatch => FailureReason::SurfaceFormMismatch,
            FailureKind::SlotOrder | FailureKind::RequiredSlotEmpty => FailureReason::PartialParse,
            FailureKind::OtherGate if f.hc_reason.as_deref() == Some("DisjunctiveAllomorph") => {
                FailureReason::DisjunctiveAllomorph
            }
            FailureKind::FeatureClash | FailureKind::InflectionClass | FailureKind::OtherGate => {
                FailureReason::ObligatorySyntacticFeatures
            }
        };
        Progress::of_reason(f.position, reason)
    }
}

/// Whether C# reports the failure or PanGloss inferred it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    HcReported,
    Derived,
}

/// Kind of grammar object a failure is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Environment,
    Allomorph,
    Affix,
    Slot,
    Template,
    Rule,
    Class,
    StemName,
    Feature,
}

/// The grammar object a fix Proposal would touch, for kinds without a dedicated field
/// (`environment` and `stem_name` carry their own identity).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FailureObject {
    #[serde(rename = "type")]
    pub kind: ObjectType,
    pub id: Option<String>,
    pub guid: Option<String>,
}

/// An environment has no id of its own: it is the `index`th environment of an allomorph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FailureEnvironment {
    pub allomorph_id: String,
    /// The FieldWorks environment guid; only on a snapshot-route grammar (HC XML gives environments none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// The environment as the author wrote it, such as "/ _ #"; only on a snapshot-route grammar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Omitted when the engine does not know which alternative failed (HC does not name it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

/// A morph involved in a failure, in the element shape of an expected parse. For a position with
/// several choices this is the choice that was used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorphRef {
    pub morpheme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allomorph: Option<String>,
    /// The FieldWorks entry and MSA identities of the morpheme, when the grammar was built from a FieldWorks model; omitted for a hand-written grammar.xml, which has neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<GuidRef>,
    /// The FieldWorks `MoForm` guid of the allomorph (the first form of a circumfix or other two-form allomorph); only on a snapshot-route grammar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_guid: Option<String>,
}

/// Actual left and right context of an unmet environment. HC does not report it; the engine or
/// adapter computes it, so `provenance` is `derived` even when the failure itself is `hc_reported`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvContext {
    pub left: String,
    pub right: String,
    pub provenance: Provenance,
}

impl EnvContext {
    pub fn derived(left: impl Into<String>, right: impl Into<String>) -> Self {
        Self {
            left: left.into(),
            right: right.into(),
            provenance: Provenance::Derived,
        }
    }
}

/// One mismatching feature path: the word's feature state against one requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDiff {
    pub path: String,
    pub required: String,
    pub actual: String,
}

/// The first failing check of a candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FirstFailure {
    pub kind: FailureKind,
    pub plain: String,
    /// 0-based index into the candidate's positions.
    pub position: Option<usize>,
    /// The engine's own reason, the C# `FailureReason` name when HC reports it; null when inferred.
    pub hc_reason: Option<String>,
    pub provenance: Provenance,
    pub object: Option<FailureObject>,
    pub morphs: Vec<MorphRef>,
    /// `adhoc_prohibition` only: every member of the prohibition, primary first (`morphs` holds just the pair that clashed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MorphRef>,
    /// `environment_unmet` with several failed environments (the allomorph's any-of list): each one; omitted when one environment failed, which `environment` already describes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<FailureEnvironment>,
    pub context: Option<EnvContext>,
    /// `environment_unmet` only: which environment.
    pub environment: Option<FailureEnvironment>,
    /// `stem_name` only: the stem name's id.
    pub stem_name: Option<String>,
    /// `feature_clash` only: one entry per mismatching path; empty otherwise.
    pub features: Vec<FeatureDiff>,
    /// How many other derivations for this candidate also fail; null when not counted.
    pub alternatives: Option<u32>,
}

/// The accepted derivation named by authored ids only: the engine's dense morpheme, allomorph and
/// part-of-speech ordinals shift whenever the grammar changes, so none of them is reported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthoredAnalysis {
    /// Every morph of the derivation in surface order, each allomorph once.
    pub morphs: Vec<MorphRef>,
    /// The root's index within `morphs`.
    pub root_index: i32,
    /// The part of speech's `grammar.xml` id; null when the analysis carries none.
    pub category: Option<String>,
    pub guessed: bool,
}

/// An accepted candidate: the derivation by authored ids and the choice used at each position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedAnalysis {
    pub analysis: AuthoredAnalysis,
    /// One entry per position: the choice that produced the analysis.
    pub used: Vec<MorphRef>,
}

/// Verdict on one candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhyNotOutcome {
    Accepted(Box<AcceptedAnalysis>),
    Rejected(Box<FirstFailure>),
    Unknown(Stop),
}

/// One candidate with its verdict, in input order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "CandidateWire", try_from = "CandidateWire")]
pub struct CandidateResult {
    pub expect: Candidate,
    pub outcome: WhyNotOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResultTag {
    Accepted,
    Rejected,
    Unknown,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidateWire {
    expect: Candidate,
    result: ResultTag,
    analysis: Option<AcceptedAnalysis>,
    failure: Option<FirstFailure>,
    stop: Option<StopReason>,
    #[serde(default, rename = "stop_note", skip_serializing_if = "Option::is_none")]
    stop_note: Option<String>,
}

impl From<CandidateResult> for CandidateWire {
    fn from(r: CandidateResult) -> Self {
        let mut wire = CandidateWire {
            expect: r.expect,
            result: ResultTag::Unknown,
            analysis: None,
            failure: None,
            stop: None,
            stop_note: None,
        };
        match r.outcome {
            WhyNotOutcome::Accepted(a) => {
                wire.result = ResultTag::Accepted;
                wire.analysis = Some(*a);
            }
            WhyNotOutcome::Rejected(f) => {
                wire.result = ResultTag::Rejected;
                wire.failure = Some(*f);
            }
            WhyNotOutcome::Unknown(s) => {
                wire.stop = Some(s.reason);
                wire.stop_note = s.note;
            }
        }
        wire
    }
}

impl TryFrom<CandidateWire> for CandidateResult {
    type Error = String;

    fn try_from(w: CandidateWire) -> Result<Self, String> {
        let note = w.stop_note;
        let outcome = match (w.result, w.analysis, w.failure, w.stop) {
            (ResultTag::Accepted, Some(a), None, None) => WhyNotOutcome::Accepted(Box::new(a)),
            (ResultTag::Rejected, None, Some(f), None) => WhyNotOutcome::Rejected(Box::new(f)),
            (ResultTag::Unknown, None, None, Some(reason)) => {
                WhyNotOutcome::Unknown(Stop { reason, note })
            }
            _ => return Err("result must carry exactly its own analysis, failure or stop".into()),
        };
        Ok(CandidateResult {
            expect: w.expect,
            outcome,
        })
    }
}

/// The whole answer: one result per candidate in input order, and the best candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhyNotReport {
    pub schema: String,
    pub word: String,
    pub results: Vec<CandidateResult>,
    pub best: Option<usize>,
}

/// Input errors. These are never a failure kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhyNotError {
    EmptyWord,
    NoCandidates,
    EmptyCandidate {
        candidate: usize,
    },
    EmptyPosition {
        candidate: usize,
        position: usize,
    },
    UnknownMorpheme {
        morpheme: String,
    },
    /// Several morphemes carry this grammar.xml id (a variant sharing an MSA).
    AmbiguousMorpheme {
        morpheme: String,
    },
    /// The choice's GUIDs do not match the morpheme its id names.
    GuidMismatch {
        morpheme: String,
    },
    UnknownAllomorph {
        allomorph: String,
    },
    /// Several allomorphs carry this grammar.xml id.
    AmbiguousAllomorph {
        allomorph: String,
    },
    /// The allomorph exists but belongs to another morpheme.
    AllomorphNotOfMorpheme {
        allomorph: String,
        morpheme: String,
    },
    UnknownGuid {
        entry_guid: String,
        msa_guid: String,
    },
    /// A guid-only choice pins an allomorph but the guid names several morphemes.
    AmbiguousGuidAllomorph {
        entry_guid: String,
        msa_guid: String,
        morphemes: usize,
    },
    RootIndexOutOfRange {
        candidate: usize,
        root_index: usize,
    },
    RootNotLexEntry {
        candidate: usize,
        position: usize,
        morpheme: String,
    },
}

impl fmt::Display for WhyNotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyWord => write!(f, "the word is empty"),
            Self::NoCandidates => write!(f, "no expected candidate was given"),
            Self::EmptyCandidate { candidate } => {
                write!(f, "candidate {candidate} lists no morphs")
            }
            Self::EmptyPosition {
                candidate,
                position,
            } => write!(f, "candidate {candidate} position {position} lists no choice"),
            Self::UnknownMorpheme { morpheme } => write!(f, "unknown morpheme id {morpheme}"),
            Self::AmbiguousMorpheme { morpheme } => write!(
                f,
                "morpheme id {morpheme} is shared by several morphemes; name the one you mean with --expect-guid entry_guid:msa_guid"
            ),
            Self::GuidMismatch { morpheme } => write!(
                f,
                "the given guid does not belong to morpheme {morpheme}"
            ),
            Self::UnknownAllomorph { allomorph } => write!(f, "unknown allomorph id {allomorph}"),
            Self::AmbiguousAllomorph { allomorph } => {
                write!(f, "allomorph id {allomorph} is shared by several allomorphs")
            }
            Self::AllomorphNotOfMorpheme {
                allomorph,
                morpheme,
            } => write!(f, "allomorph {allomorph} does not belong to morpheme {morpheme}"),
            Self::UnknownGuid {
                entry_guid,
                msa_guid,
            } => write!(f, "no morpheme for entry_guid {entry_guid} msa_guid {msa_guid}"),
            Self::AmbiguousGuidAllomorph {
                entry_guid,
                msa_guid,
                morphemes,
            } => write!(
                f,
                "entry_guid {entry_guid} msa_guid {msa_guid} names {morphemes} morphemes; give the `morpheme` id to pin an allomorph"
            ),
            Self::RootIndexOutOfRange {
                candidate,
                root_index,
            } => write!(f, "candidate {candidate} has no position {root_index} for the root"),
            Self::RootNotLexEntry {
                candidate,
                position,
                morpheme,
            } => write!(
                f,
                "candidate {candidate} position {position}: morpheme {morpheme} is not a lexical entry, so it cannot be the root"
            ),
        }
    }
}

impl std::error::Error for WhyNotError {}

/// The grammar.xml ids of the morphemes whose owning entry is `entry_guid` and whose MSA is
/// `msa_guid` (case-insensitive), as the rejection-diagnostics v1 selector matches them.
pub fn resolve_guid(g: &Grammar, entry_guid: &str, msa_guid: &str) -> Vec<String> {
    g.morphemes
        .iter()
        .filter(|m| {
            m.owner
                .as_ref()
                .is_some_and(|o| o.entry_guid.eq_ignore_ascii_case(entry_guid))
                && m.source_msa_guid
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(msa_guid))
        })
        .map(|m| m.xml_key.clone())
        .collect()
}

/// The choices a guid names: one per morpheme sharing the entry and MSA (a variant sharing an MSA gives several), each carrying the guid. `allomorph` pins one allomorph and so needs the guid to name a single morpheme.
pub fn expand_guid(
    g: &Grammar,
    guid: &GuidRef,
    allomorph: Option<&str>,
) -> Result<Vec<ExpectedMorph>, WhyNotError> {
    let entry = guid.entry_guid.clone().unwrap_or_default();
    let msa = guid.msa_guid.clone().unwrap_or_default();
    let ids = resolve_guid(g, &entry, &msa);
    if ids.is_empty() {
        return Err(WhyNotError::UnknownGuid {
            entry_guid: entry,
            msa_guid: msa,
        });
    }
    if allomorph.is_some() && ids.len() > 1 {
        return Err(WhyNotError::AmbiguousGuidAllomorph {
            entry_guid: entry,
            msa_guid: msa,
            morphemes: ids.len(),
        });
    }
    Ok(ids
        .into_iter()
        .map(|m| {
            let mut morph = ExpectedMorph::new(m);
            morph.allomorph = allomorph.map(str::to_string);
            morph.guid = Some(guid.clone());
            morph
        })
        .collect())
}

fn matches_guid(m: &pg_grammar_model::model::MorphemeInfo, guid: &GuidRef) -> bool {
    let same = |a: Option<&str>, b: &Option<String>| match b {
        Some(b) => a.is_some_and(|a| a.eq_ignore_ascii_case(b)),
        None => true,
    };
    same(
        m.owner.as_ref().map(|o| o.entry_guid.as_str()),
        &guid.entry_guid,
    ) && same(m.source_msa_guid.as_deref(), &guid.msa_guid)
}

/// The morpheme a choice names: its grammar.xml id, narrowed by its GUIDs when several morphemes
/// share that id, and checked against them when only one does. Unknown, still-ambiguous and
/// guid-contradicted ids are input errors.
pub fn resolve_morpheme(g: &Grammar, choice: &ExpectedMorph) -> Result<MorphemeId, WhyNotError> {
    let mut hits: Vec<usize> = g
        .morphemes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.xml_key == choice.morpheme)
        .map(|(i, _)| i)
        .collect();
    let named = hits.len();
    if let Some(guid) = &choice.guid {
        hits.retain(|&i| matches_guid(&g.morphemes[i], guid));
    }
    match hits[..] {
        [] if named == 0 => Err(WhyNotError::UnknownMorpheme {
            morpheme: choice.morpheme.clone(),
        }),
        [] => Err(WhyNotError::GuidMismatch {
            morpheme: choice.morpheme.clone(),
        }),
        [one] => Ok(MorphemeId(one as u32)),
        _ => Err(WhyNotError::AmbiguousMorpheme {
            morpheme: choice.morpheme.clone(),
        }),
    }
}

/// The FieldWorks identities a morpheme was built from; `None` when the grammar carries neither (a hand-written grammar.xml).
pub(crate) fn morpheme_guid(g: &Grammar, m: MorphemeId) -> Option<GuidRef> {
    let info = g.morphemes.get(m.0 as usize)?;
    let guid = GuidRef {
        entry_guid: info.owner.as_ref().map(|o| o.entry_guid.clone()),
        msa_guid: info.source_msa_guid.clone(),
    };
    (guid.entry_guid.is_some() || guid.msa_guid.is_some()).then_some(guid)
}

/// The morpheme an allomorph realizes; `None` when the id is out of range or its owner is a compounding rule.
pub(crate) fn allomorph_morpheme(g: &Grammar, a: AllomorphId) -> Option<MorphemeId> {
    match *g.allomorph_owners.get(a.0 as usize)? {
        AllomorphOwner::Root(le, _) => Some(g.entries.get(le.0 as usize)?.morpheme),
        AllomorphOwner::Affix(rule, _) => match g.mrules.get(rule.0 as usize)? {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            MorphRuleDef::Realizational(def) => Some(def.morpheme),
            MorphRuleDef::Compounding(_) => None,
        },
    }
}

/// A choice resolved against the grammar: the morpheme and, when named, its allomorph.
pub(crate) fn resolve_choice(
    g: &Grammar,
    choice: &ExpectedMorph,
) -> Result<engine::Pin, WhyNotError> {
    let morpheme = resolve_morpheme(g, choice)?;
    let allomorph = match &choice.allomorph {
        None => None,
        Some(key) => {
            let id = g.allomorph_by_xml_key(key).map_err(|e| match e {
                pg_grammar_model::model::AllomorphKeyError::Missing => {
                    WhyNotError::UnknownAllomorph {
                        allomorph: key.clone(),
                    }
                }
                pg_grammar_model::model::AllomorphKeyError::Duplicate(_) => {
                    WhyNotError::AmbiguousAllomorph {
                        allomorph: key.clone(),
                    }
                }
            })?;
            if allomorph_morpheme(g, id) != Some(morpheme) {
                return Err(WhyNotError::AllomorphNotOfMorpheme {
                    allomorph: key.clone(),
                    morpheme: choice.morpheme.clone(),
                });
            }
            Some(id)
        }
    };
    Ok(engine::Pin {
        morpheme,
        allomorph,
    })
}

/// Reject what no search could answer: an empty word, no or empty candidates, an unknown,
/// ambiguous or contradicted morpheme or allomorph id, a root position that is out of range or not
/// an entry.
pub fn validate(g: &Grammar, req: &WhyNotRequest<'_>) -> Result<(), WhyNotError> {
    if req.word.is_empty() {
        return Err(WhyNotError::EmptyWord);
    }
    if req.candidates.is_empty() {
        return Err(WhyNotError::NoCandidates);
    }
    for (ci, cand) in req.candidates.iter().enumerate() {
        if cand.positions.is_empty() {
            return Err(WhyNotError::EmptyCandidate { candidate: ci });
        }
        if let Some(root) = req.root_index {
            if root >= cand.positions.len() {
                return Err(WhyNotError::RootIndexOutOfRange {
                    candidate: ci,
                    root_index: root,
                });
            }
        }
        for (pi, position) in cand.positions.iter().enumerate() {
            if position.is_empty() {
                return Err(WhyNotError::EmptyPosition {
                    candidate: ci,
                    position: pi,
                });
            }
            for choice in position {
                let pin = resolve_choice(g, choice)?;
                if req.root_index == Some(pi)
                    && !g.entries.iter().any(|e| e.morpheme == pin.morpheme)
                {
                    return Err(WhyNotError::RootNotLexEntry {
                        candidate: ci,
                        position: pi,
                        morpheme: choice.morpheme.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// The best candidate: the first accepted one; else the rejected one with the furthest `Progress`,
/// earliest in input order on a tie; else `None` (every candidate unknown).
pub fn select_best(results: &[CandidateResult]) -> Option<usize> {
    if let Some(i) = results
        .iter()
        .position(|r| matches!(r.outcome, WhyNotOutcome::Accepted(_)))
    {
        return Some(i);
    }
    let mut best: Option<(usize, Progress)> = None;
    for (i, r) in results.iter().enumerate() {
        if let WhyNotOutcome::Rejected(f) = &r.outcome {
            let key = Progress::of_failure(f);
            if best.is_none_or(|(_, k)| key > k) {
                best = Some((i, key));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Every concrete sequence a candidate expands to: one choice per position, in position order.
fn expand(g: &Grammar, cand: &Candidate) -> Result<Vec<engine::ResolvedSequence>, WhyNotError> {
    let mut out = vec![engine::ResolvedSequence {
        pins: Vec::new(),
        choices: Vec::new(),
    }];
    for position in &cand.positions {
        let mut next = Vec::with_capacity(out.len() * position.len());
        for prefix in &out {
            for choice in position {
                let mut seq = engine::ResolvedSequence {
                    pins: prefix.pins.clone(),
                    choices: prefix.choices.clone(),
                };
                seq.pins.push(resolve_choice(g, choice)?);
                seq.choices.push(choice.clone());
                next.push(seq);
            }
        }
        out = next;
        if out.len() > engine::MAX_SEQUENCES {
            break;
        }
    }
    Ok(out)
}

/// Accepted if any sequence is, else unknown if any is (it could hide one), else the furthest rejection.
fn answer(
    searcher: &engine::Searcher<'_>,
    g: &Grammar,
    word: &str,
    cand: &Candidate,
) -> Result<WhyNotOutcome, WhyNotError> {
    let sequences = expand(g, cand)?;
    if sequences.len() > engine::MAX_SEQUENCES {
        return Ok(WhyNotOutcome::Unknown(Stop {
            reason: StopReason::ChoicesCapped,
            note: Some(format!(
                "the choices multiply to more than {} sequences",
                engine::MAX_SEQUENCES
            )),
        }));
    }
    let mut unknown: Option<Stop> = None;
    let mut best: Option<(Box<FirstFailure>, Progress)> = None;
    for seq in &sequences {
        match searcher.search(word, seq) {
            accepted @ WhyNotOutcome::Accepted(_) => return Ok(accepted),
            WhyNotOutcome::Unknown(stop) => {
                unknown.get_or_insert(stop);
            }
            WhyNotOutcome::Rejected(f) => {
                let key = Progress::of_failure(&f);
                if best.as_ref().is_none_or(|(_, k)| key > *k) {
                    best = Some((f, key));
                }
            }
        }
    }
    Ok(match (unknown, best) {
        (Some(stop), _) => WhyNotOutcome::Unknown(stop),
        (None, Some((f, _))) => WhyNotOutcome::Rejected(f),
        (None, None) => stop_unclassified("the candidate expanded to no sequence"),
    })
}

/// Answer a why-not request: validate it, then try each candidate against `morpher`, whose work
/// cap and word timeout bound every reparse.
pub fn why_not(
    g: &Grammar,
    morpher: &Morpher<'_>,
    req: &WhyNotRequest<'_>,
) -> Result<WhyNotReport, WhyNotError> {
    validate(g, req)?;
    let searcher = engine::Searcher::new(g, morpher);
    let mut results = Vec::with_capacity(req.candidates.len());
    for cand in &req.candidates {
        results.push(CandidateResult {
            expect: cand.clone(),
            outcome: answer(&searcher, g, req.word, cand)?,
        });
    }
    let best = select_best(&results);
    Ok(WhyNotReport {
        schema: SCHEMA.to_string(),
        word: req.word.to_string(),
        results,
        best,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand() -> Candidate {
        Candidate {
            positions: vec![
                vec![ExpectedMorph::new("m1"), ExpectedMorph::new("m3")],
                vec![ExpectedMorph::new("m4")],
            ],
        }
    }

    fn analysis() -> AuthoredAnalysis {
        AuthoredAnalysis {
            morphs: vec![MorphRef {
                morpheme: "m1".into(),
                allomorph: Some("a2".into()),
                guid: None,
                form_guid: None,
            }],
            root_index: 0,
            category: None,
            guessed: false,
        }
    }

    fn failure(kind: FailureKind, position: usize) -> FirstFailure {
        FirstFailure {
            kind,
            plain: format!("{kind:?} at {position}"),
            position: Some(position),
            hc_reason: Some("Environments".into()),
            provenance: Provenance::HcReported,
            object: None,
            morphs: vec![MorphRef {
                morpheme: "m1".into(),
                allomorph: Some("a2".into()),
                guid: None,
                form_guid: None,
            }],
            members: vec![],
            environments: vec![],
            context: Some(EnvContext::derived("ab", "c")),
            environment: Some(FailureEnvironment {
                allomorph_id: "a2".into(),
                guid: None,
                text: None,
                index: Some(0),
            }),
            stem_name: Some("sn6".into()),
            features: vec![FeatureDiff {
                path: "syn.tense".into(),
                required: "past".into(),
                actual: "future".into(),
            }],
            alternatives: Some(1),
        }
    }

    fn result(outcome: WhyNotOutcome) -> CandidateResult {
        CandidateResult {
            expect: cand(),
            outcome,
        }
    }

    #[test]
    fn every_failure_kind_round_trips_with_its_snake_case_name() {
        for kind in FailureKind::ALL {
            let r = result(WhyNotOutcome::Rejected(Box::new(failure(kind, 1))));
            let json = serde_json::to_value(&r).unwrap();
            assert_eq!(json["result"], "rejected");
            assert_eq!(json["failure"]["kind"], serde_json::to_value(kind).unwrap());
            assert!(json["failure"]["kind"].as_str().unwrap().is_ascii());
            let back: CandidateResult = serde_json::from_value(json).unwrap();
            assert_eq!(back, r);
        }
        let names: Vec<_> = FailureKind::ALL
            .iter()
            .map(|k| {
                serde_json::to_value(k)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            names,
            [
                "no_morph_match",
                "environment_unmet",
                "slot_order",
                "required_slot_empty",
                "feature_clash",
                "adhoc_prohibition",
                "stem_name",
                "inflection_class",
                "surface_mismatch",
                "other_gate"
            ]
        );
    }

    #[test]
    fn failure_fields_use_the_shared_suite_names() {
        let json = serde_json::to_value(failure(FailureKind::EnvironmentUnmet, 1)).unwrap();
        assert_eq!(json["hc_reason"], "Environments");
        assert_eq!(json["environment"]["allomorph_id"], "a2");
        assert_eq!(json["environment"]["index"], 0);
        assert_eq!(json["stem_name"], "sn6");
        assert_eq!(json["context"]["provenance"], "derived");
        assert_eq!(json["provenance"], "hc_reported");
        assert_eq!(json["features"][0]["path"], "syn.tense");
        assert_eq!(json["morphs"][0]["morpheme"], "m1");
        assert_eq!(json["morphs"][0]["allomorph"], "a2");
        assert!(json.get("feature").is_none() && json.get("hcReason").is_none());
    }

    #[test]
    fn an_unknown_environment_index_is_omitted_and_unknown_fields_are_ignored() {
        let mut f = failure(FailureKind::EnvironmentUnmet, 1);
        f.environment = Some(FailureEnvironment {
            allomorph_id: "a2".into(),
            guid: None,
            text: None,
            index: None,
        });
        let mut json = serde_json::to_value(&f).unwrap();
        assert!(json["environment"].get("index").is_none());
        json["future_field"] = serde_json::json!(1);
        let back: FirstFailure = serde_json::from_value(json).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn guids_are_optional_everywhere() {
        let c: Candidate =
            serde_json::from_str(r#"[[{"morpheme":"m1","guid":{"entry_guid":"e"}}]]"#).unwrap();
        let guid = c.positions[0][0].guid.as_ref().unwrap();
        assert_eq!(guid.entry_guid.as_deref(), Some("e"));
        assert_eq!(guid.msa_guid, None);
        let mut f = failure(FailureKind::NoMorphMatch, 0);
        f.object = Some(FailureObject {
            kind: ObjectType::Affix,
            id: Some("m4".into()),
            guid: None,
        });
        let back: FirstFailure = serde_json::from_value(serde_json::to_value(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn accepted_and_unknown_round_trip() {
        let accepted = result(WhyNotOutcome::Accepted(Box::new(AcceptedAnalysis {
            analysis: analysis(),
            used: vec![
                MorphRef {
                    morpheme: "m3".into(),
                    allomorph: None,
                    guid: None,
                    form_guid: None,
                },
                MorphRef {
                    morpheme: "m4".into(),
                    allomorph: None,
                    guid: None,
                    form_guid: None,
                },
            ],
        })));
        for r in [
            accepted,
            result(WhyNotOutcome::Unknown(StopReason::NotImplemented.into())),
            result(WhyNotOutcome::Unknown(StopReason::WorkCap.into())),
            result(WhyNotOutcome::Unknown(StopReason::Deadline.into())),
            result(WhyNotOutcome::Unknown(
                StopReason::FailureNotExpressible.into(),
            )),
        ] {
            let text = serde_json::to_string(&r).unwrap();
            let back: CandidateResult = serde_json::from_str(&text).unwrap();
            assert_eq!(back, r);
        }
    }

    #[test]
    fn a_result_with_the_wrong_payload_is_refused() {
        let bad = r#"{"expect":[[{"morpheme":"m1"}]],"result":"accepted","analysis":null,"failure":null,"stop":"work_cap"}"#;
        assert!(serde_json::from_str::<CandidateResult>(bad).is_err());
    }

    #[test]
    fn choices_default_their_optional_fields() {
        let c: Candidate =
            serde_json::from_str(r#"[[{"morpheme":"m5"},{"morpheme":"m6","allomorph":"a7"}]]"#)
                .unwrap();
        assert_eq!(c.positions[0][0], ExpectedMorph::new("m5"));
        assert_eq!(c.positions[0][1].allomorph.as_deref(), Some("a7"));
        assert!(serde_json::from_str::<Candidate>(r#"[[{"morpheme":"m5","bogus":1}]]"#).is_err());
    }

    #[test]
    fn best_is_the_first_accepted() {
        let acc = || {
            result(WhyNotOutcome::Accepted(Box::new(AcceptedAnalysis {
                analysis: analysis(),
                used: vec![],
            })))
        };
        let rs = vec![
            result(WhyNotOutcome::Rejected(Box::new(failure(
                FailureKind::SurfaceMismatch,
                0,
            )))),
            acc(),
            acc(),
        ];
        assert_eq!(select_best(&rs), Some(1));
    }

    #[test]
    fn best_rejected_is_furthest_then_earliest() {
        let rej = |k, p| result(WhyNotOutcome::Rejected(Box::new(failure(k, p))));
        let rs = vec![
            rej(FailureKind::NoMorphMatch, 3),
            rej(FailureKind::EnvironmentUnmet, 0),
            rej(FailureKind::EnvironmentUnmet, 0),
            result(WhyNotOutcome::Unknown(StopReason::WorkCap.into())),
        ];
        assert_eq!(select_best(&rs), Some(1));
        let tie = vec![
            rej(FailureKind::FeatureClash, 2),
            rej(FailureKind::FeatureClash, 2),
        ];
        assert_eq!(select_best(&tie), Some(0));
    }

    #[test]
    fn best_is_null_when_every_candidate_is_unknown() {
        let rs = vec![
            result(WhyNotOutcome::Unknown(StopReason::NotImplemented.into())),
            result(WhyNotOutcome::Unknown(StopReason::Deadline.into())),
        ];
        assert_eq!(select_best(&rs), None);
        assert_eq!(select_best(&[]), None);
    }

    #[test]
    fn progress_orders_slot_before_check_and_ranks_no_match_lowest() {
        let at = |slot: usize, reason| Progress::of_reason(Some(slot), reason);
        assert!(at(1, FailureReason::DisjunctiveAllomorph) > at(1, FailureReason::Environments));
        assert!(at(2, FailureReason::BoundRoot) > at(1, FailureReason::DisjunctiveAllomorph));
        assert!(
            at(1, FailureReason::AllomorphCoOccurrenceRules) > at(0, FailureReason::Environments)
        );
        assert!(
            at(1, FailureReason::Environments) > at(1, FailureReason::MorphemeCoOccurrenceRules)
        );
        assert!(
            Progress::of_reason(None, FailureReason::SurfaceFormMismatch)
                > at(9, FailureReason::Environments)
        );
        assert!(
            Progress::of_failure(&failure(FailureKind::EnvironmentUnmet, 0))
                > Progress::of_failure(&failure(FailureKind::NoMorphMatch, 5))
        );
    }
}
