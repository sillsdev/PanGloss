//! Supplies provisional letter and boundary definitions for selected literal text uses.

use hashbrown::HashSet;
use unicode_segmentation::UnicodeSegmentation;

use pg_snapshot::{ConversionIssue, FwClass, ImportWarningCode, IssueClass, SourceRef};

use crate::chardef::{CharDefKind, CharDefTable, RawCharDef};
use crate::featsys::PhonFeatureSystem;
use crate::nfd::nfd;
use crate::segment::segment;

use super::chardef::RawCharDefBuild;
use super::issues::{InferenceEvidence, InferredChar, SubstrateReport};

const SAFE_BOUNDARIES: &[char] = &[
    '\u{0009}', '\u{000A}', '\u{000D}', '\u{0020}', '\u{00A0}', '\u{1680}', '\u{2000}', '\u{2001}',
    '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}',
    '\u{200A}', '\u{202F}', '\u{205F}', '\u{3000}',
];

fn inferred_id(representation: &str) -> String {
    let scalars = representation
        .chars()
        .map(|ch| format!("{:x}", ch as u32))
        .collect::<Vec<_>>()
        .join("-");
    format!("inferred:{scalars}")
}

fn inferred_raw_def(representation: &str, kind: CharDefKind) -> RawCharDef {
    RawCharDef {
        xml_id: inferred_id(representation),
        source_guid: None,
        kind,
        representations: vec![representation.to_string()],
        feature_values: Vec::new(),
    }
}

fn exemplar_letter<'a>(text: &'a str, exemplars: &[String]) -> Option<&'a str> {
    let letter = text.graphemes(true).next()?;
    exemplars
        .iter()
        .any(|exemplar| exemplar == letter)
        .then_some(letter)
}

fn failing_letter<'a>(
    text: &'a str,
    position: usize,
    exemplars: &[String],
) -> (&'a str, InferenceEvidence) {
    let byte = text
        .char_indices()
        .nth(position)
        .expect("NFD segment failure is within the text")
        .0;
    let mut offset = 0;
    while offset < text.len() {
        let rest = &text[offset..];
        let (letter, evidence) = match exemplar_letter(rest, exemplars) {
            Some(letter) => (letter, InferenceEvidence::LdmlExemplar),
            None => (
                rest.graphemes(true).next().expect("nonempty remainder"),
                InferenceEvidence::GraphemeCluster,
            ),
        };
        if byte < offset + letter.len() {
            return (letter, evidence);
        }
        offset += letter.len();
    }
    unreachable!("NFD segment failure belongs to a letter")
}

fn classify(
    letter: &str,
    evidence: InferenceEvidence,
    boundaries: &HashSet<String>,
) -> Option<(CharDefKind, InferenceEvidence)> {
    if letter.chars().any(char::is_control)
        && !letter.chars().all(|ch| SAFE_BOUNDARIES.contains(&ch))
    {
        return None;
    }
    if boundaries.contains(letter) {
        Some((CharDefKind::Boundary, InferenceEvidence::AuthoredBoundary))
    } else if letter.chars().all(|ch| SAFE_BOUNDARIES.contains(&ch)) {
        Some((
            CharDefKind::Boundary,
            InferenceEvidence::SafeBoundaryTable { version: 1 },
        ))
    } else {
        Some((CharDefKind::Segment, evidence))
    }
}

pub(crate) struct SubstrateCompletion {
    pub raw: RawCharDefBuild,
    pub report: SubstrateReport,
    pub issues: Vec<ConversionIssue>,
}

pub(crate) fn complete(
    text_uses: &[(SourceRef, String)],
    exemplar_characters: &[String],
    authored_boundary_reps: &HashSet<String>,
    mut raw: RawCharDefBuild,
    phon: &PhonFeatureSystem,
) -> SubstrateCompletion {
    let mut report = SubstrateReport::default();
    let mut issues = Vec::new();
    let mut exemplars: Vec<String> = exemplar_characters
        .iter()
        .map(|letter| nfd(letter))
        .filter(|letter| !letter.is_empty())
        .collect();
    exemplars.sort();
    exemplars.dedup();
    let mut reported = Vec::new();
    loop {
        let table =
            CharDefTable::from_raw("substrate-probe".into(), None, raw.raw_defs.clone(), phon)
                .expect("raw representations are NFD-deduplicated");
        let mut addition = None;
        for (source, text) in text_uses {
            // NFD input keeps the owner's failure position in the same coordinates as letter units.
            let normalized = nfd(text);
            let Err(invalid) = segment(&table, &normalized) else {
                continue;
            };
            let (letter, evidence) = failing_letter(&normalized, invalid.position, &exemplars);
            if let Some((kind, evidence)) = classify(letter, evidence, authored_boundary_reps) {
                if !raw.seen_nfd.contains(letter) {
                    addition = Some((letter.to_string(), kind, evidence));
                    break;
                }
            }
            if !reported.contains(source) {
                let reason = if raw.seen_nfd.contains(letter) {
                    "it already has a definition, but its spelling still cannot be segmented"
                } else {
                    "it contains an unclassifiable control character"
                };
                issues.push(ConversionIssue {
                    code: ImportWarningCode::SubstrateClassificationAmbiguous,
                    class: IssueClass::SubstrateUnresolvable,
                    source: Some(source.clone()),
                    fatal: true,
                    message: format!("Cannot define a provisional letter for {letter:?} in {text:?}: {reason}. Correct this allomorph's spelling in FieldWorks."),
                });
                report.ambiguous_uses.push(source.clone());
                reported.push(source.clone());
            }
        }
        let Some((representation, kind, evidence)) = addition else {
            break;
        };
        raw.seen_nfd.insert(representation.clone());
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
    for definition in report
        .inferred_segments
        .iter()
        .chain(&report.inferred_boundaries)
    {
        let (code, kind, assumption, source_kind, replacement) = match definition.kind {
            CharDefKind::Segment => (ImportWarningCode::ProvisionalLetter, "letter", "a letter of its own that belongs to no natural class that names letters or requires a feature value. An unconstrained class still matches it", FwClass::PhPhoneme, "phoneme"),
            CharDefKind::Boundary => (ImportWarningCode::ProvisionalBoundary, "boundary", "a boundary of its own", FwClass::PhBdryMarker, "boundary marker"),
        };
        issues.push(ConversionIssue {
            code,
            class: IssueClass::MigrationDifference,
            source: Some(SourceRef { kind: source_kind, id: format!("inferred:{}", definition.representation) }),
            fatal: false,
            message: format!("The {kind} '{}' isn't defined in this project's phonology. PanGloss is treating it as {assumption}. Define it as a {replacement} in FieldWorks to replace this provisional definition.", definition.representation),
        });
    }
    SubstrateCompletion {
        raw,
        report,
        issues,
    }
}

#[cfg(test)]
mod tests;
