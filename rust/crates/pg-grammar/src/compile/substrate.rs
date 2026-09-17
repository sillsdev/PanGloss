//! Completes a phonological substrate from `SelectionRecorder::text_uses` under `CompleteFromUsage`; never walks `Snapshot` directly, and never infers anything under `Strict`. Every issue below is per-allomorph and non-fatal; see `compile::tests::substrate_issue_and_the_real_owners_drop_agree_on_the_same_allomorph` for the redundancy this relies on.

use hashbrown::HashSet;

use pg_snapshot::{ConversionIssue, IssueClass, SourceRef};

use crate::chardef::{CharDefKind, CharDefTable, RawCharDef};
use crate::featsys::PhonFeatureSystem;
use crate::model::{NaturalClass, NaturalClassKind};
use crate::nfd::nfd;
use crate::segment::{nat_class_cd_set, segment};

use super::chardef::RawCharDefBuild;
use super::issues::{self, InferenceEvidence, InferredChar, SubstrateReport};
use super::options::ResolvedSubstratePolicy;

/// Unicode 15.0 `Zs` plus ASCII tab/CR/LF -- a committed exact scalar allowlist, never a runtime predicate.
const SAFE_BOUNDARY_TABLE_V1: &[char] = &[
    '\u{0009}', '\u{000A}', '\u{000D}', // ASCII tab, LF, CR
    '\u{0020}', '\u{00A0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}',
    '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200A}', '\u{202F}', '\u{205F}',
    '\u{3000}',
];

const SAFE_BOUNDARY_TABLE_VERSION: u16 = 1;

/// Deterministic id for an inferred character definition, from its NFD scalar values -- no hash or process state.
fn inferred_id(nfd_representation: &str) -> String {
    let scalars = nfd_representation
        .chars()
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-");
    format!("inferred:{scalars}")
}

/// Always `feature_values: Vec::new()`, so an inferred segment gets HC's ordinary unspecified-feature (full-mask) semantics, never an invented value.
fn inferred_raw_def(original_representation: &str, kind: CharDefKind) -> RawCharDef {
    let normalized_representation = nfd(original_representation);
    RawCharDef {
        xml_id: inferred_id(&normalized_representation),
        kind,
        representations: vec![original_representation.to_string()],
        feature_values: Vec::new(),
    }
}

enum Classification {
    Segment(InferenceEvidence),
    Boundary(InferenceEvidence),
    Ambiguous,
}

/// Priority order when more than one evidence source could apply: exemplar, then authored boundary, then the safe table.
fn classify(
    ch: char,
    exemplar_nfd: &HashSet<String>,
    authored_boundary_nfd: &HashSet<String>,
) -> Classification {
    let rep_nfd = nfd(&ch.to_string());
    if exemplar_nfd.contains(&rep_nfd) {
        return Classification::Segment(InferenceEvidence::LdmlExemplar);
    }
    if authored_boundary_nfd.contains(&rep_nfd) {
        return Classification::Boundary(InferenceEvidence::AuthoredBoundary);
    }
    if SAFE_BOUNDARY_TABLE_V1.contains(&ch) {
        return Classification::Boundary(InferenceEvidence::SafeBoundaryTable {
            version: SAFE_BOUNDARY_TABLE_VERSION,
        });
    }
    Classification::Ambiguous
}

/// `None` when `position` is past the end of `text` -- `remap_error_position`'s recompose-the-prefix heuristic can place a word-final standalone combining mark there, which is a mismap, not this module's own bug.
fn failing_char(text: &str, position: usize) -> Option<char> {
    text.chars().nth(position)
}

/// `Ok` is a trustworthy failing character; `Err` is a `remap_error_position` mismap that must refuse rather than be treated as the real failure -- `None` when `position` is past the end of the word (word-final combining mark, no original position at all), `Some` when it lands mid-word on an already-registered character (the next real character after the mark).
fn position_mismap(
    raw: &RawCharDefBuild,
    text: &str,
    position: usize,
) -> Result<char, Option<char>> {
    match failing_char(text, position) {
        None => Err(None),
        Some(ch) if raw.seen_nfd.contains(&nfd(&ch.to_string())) => Err(Some(ch)),
        Some(ch) => Ok(ch),
    }
}

/// Non-fatal and per-allomorph -- see the module doc's granularity note.
fn unsegmentable_issue(
    source: &SourceRef,
    text: &str,
    ch: char,
    position: usize,
) -> ConversionIssue {
    ConversionIssue {
        code: issues::SUBSTRATE_UNSEGMENTABLE_FORM.to_string(),
        class: IssueClass::SubstrateUnresolvable,
        source: Some(source.clone()),
        fatal: false,
        message: format!(
            "cannot segment {text:?}: no character definition matches {ch:?} at position \
             {position}, and this project's resolved substrate policy does not complete a \
             phonological substrate from usage"
        ),
    }
}

/// Non-fatal and per-allomorph for the same reason as [`unsegmentable_issue`].
fn ambiguous_issue(source: &SourceRef, text: &str, ch: char, position: usize) -> ConversionIssue {
    ConversionIssue {
        code: issues::SUBSTRATE_CLASSIFICATION_AMBIGUOUS.to_string(),
        class: IssueClass::SubstrateUnresolvable,
        source: Some(source.clone()),
        fatal: false,
        message: format!(
            "cannot segment {text:?}: {ch:?} at position {position} is neither a vernacular \
             exemplar, an authored boundary, nor in the safe boundary table; refusing rather than \
             guessing whether it is a segment or a boundary"
        ),
    }
}

/// Covers both `position_mismap` shapes with one code; non-fatal and per-allomorph as [`unsegmentable_issue`].
fn unmapped_position_issue(
    source: &SourceRef,
    text: &str,
    ch: Option<char>,
    position: usize,
) -> ConversionIssue {
    let detail = match ch {
        Some(ch) => format!("remaps to {ch:?}, which is already a registered character"),
        None => "is past the end of the word (word-final combining mark)".to_string(),
    };
    ConversionIssue {
        code: issues::SUBSTRATE_POSITION_UNMAPPED.to_string(),
        class: IssueClass::SubstrateUnresolvable,
        source: Some(source.clone()),
        fatal: false,
        message: format!(
            "cannot segment {text:?}: the failure position {position} {detail}; the true failing \
             element is likely a standalone combining mark from a decomposed character with no \
             clean original-text position, so refusing rather than guessing or duplicating"
        ),
    }
}

/// `complete`'s result; a fatal issue here is the caller's to fold into its own gate.
pub(crate) struct SubstrateCompletion {
    pub raw: RawCharDefBuild,
    pub report: SubstrateReport,
    pub issues: Vec<ConversionIssue>,
}

/// Infallible here: `raw_defs` is already deduplicated by NFD representation before anything is appended.
fn probe_table(raw_defs: &[RawCharDef], phon: &PhonFeatureSystem) -> CharDefTable {
    CharDefTable::from_raw("substrate-probe".to_string(), None, raw_defs.to_vec(), phon)
        .expect("substrate probe: raw defs are pre-deduplicated by NFD representation")
}

/// `CompleteFromUsage` grows the table one inferred character per retry, re-checking every use against the larger table; `Strict` runs one pass and stops.
pub(crate) fn complete(
    text_uses: &[(SourceRef, String)],
    exemplar_characters: &[String],
    authored_boundary_reps: &HashSet<String>,
    mut raw: RawCharDefBuild,
    phon: &PhonFeatureSystem,
    policy: ResolvedSubstratePolicy,
) -> SubstrateCompletion {
    let mut report = SubstrateReport::default();
    let mut issues = Vec::new();
    let exemplar_nfd: HashSet<String> = exemplar_characters.iter().map(|c| nfd(c)).collect();

    if policy == ResolvedSubstratePolicy::Strict {
        let table = probe_table(&raw.raw_defs, phon);
        for (source, text) in text_uses {
            if let Err(invalid) = segment(&table, text) {
                match position_mismap(&raw, text, invalid.position) {
                    Ok(ch) => issues.push(unsegmentable_issue(source, text, ch, invalid.position)),
                    Err(mismap_ch) => issues.push(unmapped_position_issue(
                        source,
                        text,
                        mismap_ch,
                        invalid.position,
                    )),
                }
                report.unresolved_uses.push(source.clone());
            }
        }
        return SubstrateCompletion {
            raw,
            report,
            issues,
        };
    }

    let mut already_reported: Vec<(SourceRef, char)> = Vec::new();
    let mut already_reported_unmapped: Vec<(SourceRef, usize)> = Vec::new();
    loop {
        let table = probe_table(&raw.raw_defs, phon);
        let mut to_add: Option<(char, CharDefKind, InferenceEvidence)> = None;
        for (source, text) in text_uses {
            let Err(invalid) = segment(&table, text) else {
                continue;
            };
            let ch = match position_mismap(&raw, text, invalid.position) {
                Ok(ch) => ch,
                Err(mismap_ch) => {
                    let key = (source.clone(), invalid.position);
                    if !already_reported_unmapped.contains(&key) {
                        issues.push(unmapped_position_issue(
                            source,
                            text,
                            mismap_ch,
                            invalid.position,
                        ));
                        report.ambiguous_uses.push(source.clone());
                        already_reported_unmapped.push(key);
                    }
                    continue;
                }
            };
            match classify(ch, &exemplar_nfd, authored_boundary_reps) {
                Classification::Segment(evidence) => {
                    to_add = Some((ch, CharDefKind::Segment, evidence));
                    break;
                }
                Classification::Boundary(evidence) => {
                    to_add = Some((ch, CharDefKind::Boundary, evidence));
                    break;
                }
                Classification::Ambiguous => {
                    let key = (source.clone(), ch);
                    if !already_reported.contains(&key) {
                        issues.push(ambiguous_issue(source, text, ch, invalid.position));
                        report.ambiguous_uses.push(source.clone());
                        already_reported.push(key);
                    }
                }
            }
        }
        let Some((ch, kind, evidence)) = to_add else {
            break;
        };
        let representation = ch.to_string();
        raw.seen_nfd.insert(nfd(&representation));
        raw.raw_defs.push(inferred_raw_def(&representation, kind));
        let inferred = InferredChar {
            representation,
            kind,
            evidence,
        };
        match kind {
            CharDefKind::Segment => report.inferred_segments.push(inferred),
            CharDefKind::Boundary => report.inferred_boundaries.push(inferred),
        }
    }

    SubstrateCompletion {
        raw,
        report,
        issues,
    }
}

/// Flags an inferred segment that satisfies a `Feature`-kind natural class only via HC's full-mask unspecified-lane default -- a migration difference, never fatal.
pub(crate) fn feature_rule_migration_issues(
    table: &CharDefTable,
    inferred_segments: &[InferredChar],
    natural_classes: &[NaturalClass],
) -> Vec<ConversionIssue> {
    let mut issues = Vec::new();
    for inferred in inferred_segments {
        let Some(id) = table.lookup_nfd(&nfd(&inferred.representation)) else {
            continue;
        };
        let matches_a_feature_class = natural_classes.iter().any(|nc| match &nc.kind {
            NaturalClassKind::Feature(_) => match nat_class_cd_set(table, nc) {
                pg_shape::CdSet::Unrestricted => true,
                pg_shape::CdSet::Members(bits) => bits.contains(id.0),
            },
            NaturalClassKind::Segments(_) => false,
        });
        if matches_a_feature_class {
            issues.push(ConversionIssue {
                code: issues::SUBSTRATE_INFERRED_SEGMENT_WITH_FEATURE_RULE.to_string(),
                class: IssueClass::MigrationDifference,
                source: None,
                fatal: false,
                message: format!(
                    "inferred segment {:?} carries no authored feature values, so it satisfies \
                     every feature-based natural class under HC's unspecified-lane-matches-anything \
                     default; the FieldWorks XAMPLE source never declared any feature system to \
                     check this against",
                    inferred.representation
                ),
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inferred_segment_has_no_authored_feature_values() {
        let q = inferred_raw_def("q", CharDefKind::Segment);
        assert!(q.feature_values.is_empty());
    }

    #[test]
    fn inferred_id_is_deterministic_from_nfd_scalars() {
        assert_eq!(inferred_id("q"), "inferred:71");
        assert_eq!(inferred_id(&nfd("q")), inferred_id("q"));
    }

    #[test]
    fn ascii_space_is_in_the_safe_boundary_table_but_common_punctuation_is_not() {
        let empty = HashSet::new();
        assert!(matches!(
            classify(' ', &empty, &empty),
            Classification::Boundary(InferenceEvidence::SafeBoundaryTable { version: 1 })
        ));
        for ch in ['\'', '\u{02BC}', '-', '\u{2011}', '§'] {
            assert!(
                matches!(classify(ch, &empty, &empty), Classification::Ambiguous),
                "{ch:?} must not be classifiable without authored/LDML evidence"
            );
        }
    }

    #[test]
    fn an_exemplar_character_classifies_as_a_segment_even_if_it_would_otherwise_be_ambiguous() {
        let exemplar: HashSet<String> = ["q".to_string()].into_iter().collect();
        let empty = HashSet::new();
        assert!(matches!(
            classify('q', &exemplar, &empty),
            Classification::Segment(InferenceEvidence::LdmlExemplar)
        ));
    }

    #[test]
    fn an_authored_boundary_representation_classifies_as_a_boundary() {
        let empty = HashSet::new();
        let authored: HashSet<String> = ["\u{2011}".to_string()].into_iter().collect();
        assert!(matches!(
            classify('\u{2011}', &empty, &authored),
            Classification::Boundary(InferenceEvidence::AuthoredBoundary)
        ));
    }
}
