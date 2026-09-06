//! Normalizes `pg-parse`'s own analysis output into [`crate::model`] values, so an HC-Rust run and
//! a `xample-projector parse` capture become comparable through the same [`XampleResult`] shape.
//!
//! This calls `pg_parse::identity::AnalysisIdentity::project` for the stable-key projection rather
//! than re-deriving it: that module already resolves `WordAnalysis::morpheme_ids`/`pos_id` (dense,
//! compiler-assigned ordinals) against `Grammar::morphemes`/the part-of-speech symbol table into
//! the same stable keys the LibLCM/XAMPLE path calls a GUID (see that module's own doc).

use crate::model::{AnalysisSignature, XampleResult};
use pg_grammar::model::Grammar;
use pg_parse::identity::{AnalysisIdentity, IdentityError};
use pg_parse::{ParseOutcome, WordAnalysis};
use std::collections::BTreeMap;
use std::fmt;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug)]
pub enum HcNormalizationError {
    /// `ParseOutcome::guessed`: the analysis's root is fabricated, not an authored morpheme, so it
    /// has no stable key an oracle comparison could ever match against.
    GuessedAnalysesNotComparable,
    /// Defensive: `AnalysisIdentity::project` documents `None` as arising only when
    /// `WordAnalysis::guessed`, which is already refused above — reaching this means that
    /// invariant broke, not that the grammar did anything.
    UnexpectedGuessedSlot,
    Identity(IdentityError),
}

impl fmt::Display for HcNormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HcNormalizationError::GuessedAnalysesNotComparable => write!(
                f,
                "a guessed-root analysis has no authored identity to compare against an oracle"
            ),
            HcNormalizationError::UnexpectedGuessedSlot => write!(
                f,
                "a non-guessed analysis carried a guessed-root identity slot (internal fault)"
            ),
            HcNormalizationError::Identity(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HcNormalizationError {}

/// Normalize a whole word's [`ParseOutcome`] into the same multiset shape a `parse` capture reads
/// into (`crate::reader::read_parse_response`), so the two are directly comparable.
pub fn xample_result_from_hc_outcome(
    outcome: &ParseOutcome,
    grammar: &Grammar,
) -> Result<XampleResult, HcNormalizationError> {
    if outcome.guessed {
        return Err(HcNormalizationError::GuessedAnalysesNotComparable);
    }
    let mut analyses: BTreeMap<AnalysisSignature, usize> = BTreeMap::new();
    for (analysis, (_, surface)) in outcome.structured.iter().zip(outcome.analyses.iter()) {
        let signature = signature_from_word_analysis(analysis, surface, grammar)?;
        *analyses.entry(signature).or_insert(0) += 1;
    }
    let reached_max_analyses = outcome.capped.then_some(outcome.structured.len());
    let engine_error = if outcome.invalid_shape {
        Some("invalid shape: surface word did not segment".to_string())
    } else if outcome.timed_out {
        Some("word timeout exceeded".to_string())
    } else {
        None
    };
    Ok(XampleResult { analyses, reached_max_analyses, engine_error })
}

fn signature_from_word_analysis(
    analysis: &WordAnalysis,
    surface: &str,
    grammar: &Grammar,
) -> Result<AnalysisSignature, HcNormalizationError> {
    let identity =
        AnalysisIdentity::project(analysis, grammar).map_err(HcNormalizationError::Identity)?;
    let mut msa_ids = Vec::with_capacity(identity.morphemes.len());
    for key in identity.morphemes {
        msa_ids.push(key.ok_or(HcNormalizationError::UnexpectedGuessedSlot)?);
    }
    let morphemes = analysis.morpheme_ids.iter().map(|&ordinal| gloss_of(grammar, ordinal)).collect();
    Ok(AnalysisSignature {
        morphemes,
        msa_ids,
        category_id: identity.category,
        surface_nfd: surface.nfd().collect(),
    })
}

/// A morpheme's display gloss, empty (never the stable key) when the grammar names none.
fn gloss_of(grammar: &Grammar, ordinal: u32) -> String {
    grammar
        .morphemes
        .get(ordinal as usize)
        .and_then(|m| m.gloss.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_parse::Morpher;

    /// A 2-slot reduction of the same `deep-optional-affix-nesting` grammar the `tests/data/captured-parse-*.json` fixtures were captured from.
    fn two_slot_grammar() -> Grammar {
        const XML: &str = r#"<HermitCrabInput><Language><Name>PathologicalDeepOptionalAffixNestingReduced</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1">
            <Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
              <Name>Main</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mrP1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                  <Name>p1</Name>
                  <MorphologicalSubrules><MorphologicalSubrule id="subP1">
                    <MorphologicalInput><PhoneticSequence id="stemP1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="stemP1" /></MorphologicalOutput>
                  </MorphologicalSubrule></MorphologicalSubrules>
                  <MorphemeId>P1</MorphemeId>
                </MorphologicalRule>
                <MorphologicalRule id="mrP2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                  <Name>p2</Name>
                  <MorphologicalSubrules><MorphologicalSubrule id="subP2">
                    <MorphologicalInput><PhoneticSequence id="stemP2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="stemP2" /></MorphologicalOutput>
                  </MorphologicalSubrule></MorphologicalSubrules>
                  <MorphemeId>P2</MorphemeId>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <AffixTemplates>
                <AffixTemplate requiredPartsOfSpeech="posV">
                  <Name>reducedTemplate</Name>
                  <Slot optional="true" morphologicalRules="mrP1"><Name>slot1</Name></Slot>
                  <Slot optional="true" morphologicalRules="mrP2"><Name>slot2</Name></Slot>
                </AffixTemplate>
              </AffixTemplates>
              <LexicalEntries>
                <LexicalEntry id="eK" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
                  <MorphemeId>K</MorphemeId>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        pg_grammar::load(XML).unwrap_or_else(|e| panic!("two_slot_grammar failed to load: {e}"))
    }

    #[test]
    fn root_only_word_normalizes_to_a_single_member_result() {
        let g = two_slot_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let outcome = m.parse_word("k");
        let result = xample_result_from_hc_outcome(&outcome, &g).expect("k must normalize");
        assert_eq!(result.analyses.values().sum::<usize>(), 1);
        assert_eq!(result.engine_error, None);
        assert_eq!(result.reached_max_analyses, None);
        let (signature, count) = result.analyses.iter().next().unwrap();
        assert_eq!(*count, 1);
        assert_eq!(signature.msa_ids, vec!["eK".to_string()]);
        assert_eq!(signature.surface_nfd, "k");
    }

    #[test]
    fn two_optional_prefixes_normalize_to_two_distinct_signatures() {
        let g = two_slot_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let outcome = m.parse_word("xk");
        let result = xample_result_from_hc_outcome(&outcome, &g).expect("xk must normalize");
        assert_eq!(result.analyses.len(), 2, "P1-fired and P2-fired are distinct analyses");
        assert_eq!(result.analyses.values().sum::<usize>(), 2);
        let msa_id_sets: Vec<&Vec<String>> = result.analyses.keys().map(|s| &s.msa_ids).collect();
        assert!(msa_id_sets.contains(&&vec!["eK".to_string(), "mrP1".to_string()])
            || msa_id_sets.contains(&&vec!["mrP1".to_string(), "eK".to_string()]));
    }

    #[test]
    fn unparseable_word_normalizes_to_invalid_shape_engine_error() {
        let g = two_slot_grammar();
        let m = Morpher::new(&g, usize::MAX);
        // "q" has no representation in the character table, so it never segments.
        let outcome = m.parse_word("q");
        let result = xample_result_from_hc_outcome(&outcome, &g).expect("q must normalize (empty, not an error state HC lacks a flag for)");
        if outcome.invalid_shape {
            assert!(result.engine_error.as_deref().unwrap_or_default().contains("invalid shape"));
        }
        assert!(result.analyses.is_empty());
    }
}
