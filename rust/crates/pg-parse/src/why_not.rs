//! Why-not (constrained parse), Motif #386: parse one word along GIVEN morph candidates and report
//! the first failing constraint instead of searching every path.
//!
//! Slice 0 is the contract only. The types, input validation and wire shape (`pangloss.whynot.v1`,
//! `docs/formats/why-not.md`) are final enough for a consumer to code against; [`why_not`] validates
//! its input and then answers [`StopReason::NotImplemented`] for every candidate. Vocabulary is the
//! rejection-diagnostics v1 vocabulary (`docs/formats/rejection-trace-v1.md`): `hc_reason` carries
//! the C# `FailureReason` name, the grammar object carries `type`/`id`/`guid`, and `entry_guid` plus
//! `msa_guid` are the same FieldWorks identities the v1 selector resolves. Morphemes and stem names
//! are named by their grammar.xml id (`MorphemeInfo::xml_key`, `StemNameDef::xml_id`), as the shared
//! conformance suite names them.

use std::fmt;

use pg_grammar_model::model::{Grammar, MorphemeId};
use serde::{Deserialize, Serialize};

use crate::WordAnalysis;

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

/// One choice at a position: a morpheme named by its grammar.xml id. `allomorph` is reserved for
/// allomorph XML ids, which the loader does not keep yet, so any value is an input error today.
/// `guid` disambiguates a morpheme id that several morphemes share (a variant sharing an MSA).
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
    /// Position (within every candidate) of the root, when the caller knows it.
    pub root_index: Option<usize>,
}

/// Why a candidate has no verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Slice 0: the search is not built yet.
    NotImplemented,
    WorkCap,
    Deadline,
    /// A failure was found that cannot fill a required failure field (the C# tool emits this).
    FailureNotExpressible,
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

    /// How many checks a derivation survives before failing this way, in the C# order: analysis
    /// side (strata and templates unapplied outermost first), synthesis gates, `IsWordValid`,
    /// per-allomorph checks (root, co-occurrence, environments), surface match. Larger is further.
    pub fn check_rank(self) -> u8 {
        match self {
            FailureKind::NoMorphMatch => 1,
            FailureKind::SlotOrder => 2,
            FailureKind::RequiredSlotEmpty => 3,
            FailureKind::FeatureClash => 4,
            FailureKind::OtherGate => 5,
            FailureKind::StemName | FailureKind::InflectionClass => 6,
            FailureKind::AdhocProhibition => 7,
            FailureKind::EnvironmentUnmet => 8,
            FailureKind::SurfaceMismatch => 9,
        }
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

/// An accepted candidate: the ordinary analysis and the choice used at each position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedAnalysis {
    pub analysis: WordAnalysis,
    /// One entry per position: the choice that produced the analysis.
    pub used: Vec<MorphRef>,
}

/// Verdict on one candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhyNotOutcome {
    Accepted(Box<AcceptedAnalysis>),
    Rejected(Box<FirstFailure>),
    Unknown(StopReason),
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
}

impl From<CandidateResult> for CandidateWire {
    fn from(r: CandidateResult) -> Self {
        let mut wire = CandidateWire {
            expect: r.expect,
            result: ResultTag::Unknown,
            analysis: None,
            failure: None,
            stop: None,
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
            WhyNotOutcome::Unknown(s) => wire.stop = Some(s),
        }
        wire
    }
}

impl TryFrom<CandidateWire> for CandidateResult {
    type Error = String;

    fn try_from(w: CandidateWire) -> Result<Self, String> {
        let outcome = match (w.result, w.analysis, w.failure, w.stop) {
            (ResultTag::Accepted, Some(a), None, None) => WhyNotOutcome::Accepted(Box::new(a)),
            (ResultTag::Rejected, None, Some(f), None) => WhyNotOutcome::Rejected(Box::new(f)),
            (ResultTag::Unknown, None, None, Some(s)) => WhyNotOutcome::Unknown(s),
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
    AllomorphSelectionUnavailable,
    UnknownGuid {
        entry_guid: String,
        msa_guid: String,
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
                "morpheme id {morpheme} is shared by several morphemes; name the one you mean with --expect-guid entryGuid:msaGuid"
            ),
            Self::AllomorphSelectionUnavailable => write!(
                f,
                "allomorph selection by id arrives with allomorph XML ids; give the morpheme only"
            ),
            Self::UnknownGuid {
                entry_guid,
                msa_guid,
            } => write!(f, "no morpheme for entryGuid {entry_guid} msaGuid {msa_guid}"),
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
/// share that id. Unknown and still-ambiguous ids are input errors.
pub fn resolve_morpheme(g: &Grammar, choice: &ExpectedMorph) -> Result<MorphemeId, WhyNotError> {
    if choice.allomorph.is_some() {
        return Err(WhyNotError::AllomorphSelectionUnavailable);
    }
    let mut hits: Vec<usize> = g
        .morphemes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.xml_key == choice.morpheme)
        .map(|(i, _)| i)
        .collect();
    if hits.len() > 1 {
        if let Some(guid) = &choice.guid {
            hits.retain(|&i| matches_guid(&g.morphemes[i], guid));
        }
    }
    match hits[..] {
        [] => Err(WhyNotError::UnknownMorpheme {
            morpheme: choice.morpheme.clone(),
        }),
        [one] => Ok(MorphemeId(one as u32)),
        _ => Err(WhyNotError::AmbiguousMorpheme {
            morpheme: choice.morpheme.clone(),
        }),
    }
}

/// Reject what no search could answer: an empty word, no or empty candidates, an unknown or
/// ambiguous morpheme id, an allomorph id (not selectable yet), a root position that is out of
/// range or not an entry.
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
                let morpheme = resolve_morpheme(g, choice)?;
                if req.root_index == Some(pi) && !g.entries.iter().any(|e| e.morpheme == morpheme) {
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

/// The best candidate: the first accepted one; else the rejected one that survived the most checks
/// ([`FailureKind::check_rank`], then the later `position`), earliest in input order on a tie; else
/// `None` (every candidate unknown).
pub fn select_best(results: &[CandidateResult]) -> Option<usize> {
    if let Some(i) = results
        .iter()
        .position(|r| matches!(r.outcome, WhyNotOutcome::Accepted(_)))
    {
        return Some(i);
    }
    let mut best: Option<(usize, (u8, usize))> = None;
    for (i, r) in results.iter().enumerate() {
        if let WhyNotOutcome::Rejected(f) = &r.outcome {
            let key = (f.kind.check_rank(), f.position.unwrap_or(0));
            if best.is_none_or(|(_, k)| key > k) {
                best = Some((i, key));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Answer a why-not request. Slice 0: validates, then returns `Unknown(NotImplemented)` for every
/// candidate (so `best` is null).
pub fn why_not(g: &Grammar, req: &WhyNotRequest<'_>) -> Result<WhyNotReport, WhyNotError> {
    validate(g, req)?;
    let results: Vec<CandidateResult> = req
        .candidates
        .iter()
        .map(|c| CandidateResult {
            expect: c.clone(),
            outcome: WhyNotOutcome::Unknown(StopReason::NotImplemented),
        })
        .collect();
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
    use crate::{AnalysisProvenance, MorphOccurrence};

    fn cand() -> Candidate {
        Candidate {
            positions: vec![
                vec![ExpectedMorph::new("m1"), ExpectedMorph::new("m3")],
                vec![ExpectedMorph::new("m4")],
            ],
        }
    }

    fn analysis() -> WordAnalysis {
        WordAnalysis {
            morpheme_ids: vec![1, 4],
            morph_occurrences: vec![MorphOccurrence {
                allomorph_id: 2,
                morpheme_id: 1,
                order: 0,
            }],
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: pg_featstruct::FeatureStruct::EMPTY,
            mpr: Default::default(),
            guessed: false,
            guessed_string: None,
            guessed_pattern: None,
            provenance: AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None, None],
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
            }],
            context: Some(EnvContext::derived("ab", "c")),
            environment: Some(FailureEnvironment {
                allomorph_id: "a2".into(),
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
                },
                MorphRef {
                    morpheme: "m4".into(),
                    allomorph: None,
                },
            ],
        })));
        for r in [
            accepted,
            result(WhyNotOutcome::Unknown(StopReason::NotImplemented)),
            result(WhyNotOutcome::Unknown(StopReason::WorkCap)),
            result(WhyNotOutcome::Unknown(StopReason::Deadline)),
            result(WhyNotOutcome::Unknown(StopReason::FailureNotExpressible)),
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
            result(WhyNotOutcome::Unknown(StopReason::WorkCap)),
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
            result(WhyNotOutcome::Unknown(StopReason::NotImplemented)),
            result(WhyNotOutcome::Unknown(StopReason::Deadline)),
        ];
        assert_eq!(select_best(&rs), None);
        assert_eq!(select_best(&[]), None);
    }

    #[test]
    fn check_ranks_are_distinct_enough_to_order_the_c_sharp_sequence() {
        assert!(
            FailureKind::NoMorphMatch.check_rank() < FailureKind::EnvironmentUnmet.check_rank()
        );
        assert!(
            FailureKind::EnvironmentUnmet.check_rank() < FailureKind::SurfaceMismatch.check_rank()
        );
    }
}
