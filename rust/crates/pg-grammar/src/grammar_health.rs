//! The `hc-*` grammar-authoring health checks, ported from C#
//! `SIL.Machine.Morphology.HermitCrab.GrammarHealthChecker`/`GrammarHealthCheckFinding`
//! (`sillsdev/machine` PR 475, branch `feature/grammar-health-checker`). Diagnostic only: running
//! these checks never fails, never refuses, and never changes how a [`Grammar`] compiles or
//! parses -- it reports, and the caller decides.
//!
//! This answers a *different* question from `pg_health::health`'s FST-compilation vocabulary
//! (`Severity`/`FindingCode`/`FindingClass`): that module asks "can this grammar be compiled to an
//! FST and published" (gated on `FindingClass`: Representability/Readiness/Containment/Process).
//! These checks ask "is this grammar well-formed for its author" -- an authoring-correctness
//! question with no publication-blocking tier and no representability/containment axis, so a
//! separate, smaller vocabulary lives here rather than contorting the FST schema's severity/class
//! pair to fit it. Wire codes are the stable C# strings (`hc-undeclared-segment`,
//! `hc-duplicate-feature-bundle`, `hc-partial-morpheme`) so the two implementations' output can be
//! compared directly.
//!
//! Lives in `pg-grammar`, not `pg-health`, because the checks read [`Grammar`] directly.
//! `pg-health` is a leaf crate whose only dependencies are `serde`/`serde_json`, kept that way so
//! it builds for `wasm32-unknown-unknown` with no compiler/model machinery in its graph
//! (`pg-wasm/tests/wasm_excludes_compiler.rs`); adding a `pg-grammar` dependency there would be a
//! new, unnecessary edge. `pg-grammar` is already an unconditional dependency of `pg-wasm` (see
//! that crate's `Cargo.toml`), so this module adds no new wasm32 exposure of its own.
//!
//! # What is NOT ported
//! C#'s `CheckUndeclaredSegments` also flags a `SegmentNaturalClass` member whose
//! `CharacterDefinition.CharacterDefinitionTable` is not one of the language's declared tables.
//! [`crate::model::NaturalClassKind::Segments`] stores only a per-table
//! [`crate::chardef::CharDefId`] with no owning-table reference, and `load.rs`'s natural-class pass
//! resolves every `<Segment segment="...">` reference through an index built ONLY from
//! already-declared tables -- so a member naming an undeclared table cannot exist once a grammar
//! has loaded, and the model carries no field this check could read even by hand construction.
//! Porting it would require adding a table reference to every `Segments` member, a model-shape
//! change out of scope for this port.

use crate::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use crate::model::{
    CompoundingRuleDef, Grammar, LexEntryDef, LexEntryId, MRuleId, MorphRuleDef, MorphemeId,
    OutputAction, TableId,
};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

/// How serious a [`GrammarHealthCheckFinding`] is. Mirrors C# `GrammarHealthSeverity`: `Error` means
/// the engine behaves incorrectly (or refuses the word outright) whenever the offending construct
/// is exercised; `Warning` means the construct is a genuine reliability risk whose actual impact
/// depends on how the grammar's rules use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarHealthSeverity {
    Warning,
    Error,
}

/// The stable finding codes this module reports (C# `GrammarHealthCodes`). Treat
/// [`GrammarHealthCode::wire`], not [`GrammarHealthCheckFinding::message`], as the identifier a host
/// filters/suppresses/tests on -- the message text is free to change. Serializes as the bare wire
/// string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GrammarHealthCode {
    #[serde(rename = "hc-undeclared-segment")]
    UndeclaredSegment,
    #[serde(rename = "hc-duplicate-feature-bundle")]
    DuplicateFeatureBundle,
    #[serde(rename = "hc-partial-morpheme")]
    PartialMorpheme,
}

impl GrammarHealthCode {
    /// The stable C# wire string this code shares with `GrammarHealthCodes`.
    pub const fn wire(self) -> &'static str {
        match self {
            GrammarHealthCode::UndeclaredSegment => "hc-undeclared-segment",
            GrammarHealthCode::DuplicateFeatureBundle => "hc-duplicate-feature-bundle",
            GrammarHealthCode::PartialMorpheme => "hc-partial-morpheme",
        }
    }
}

/// One model object a [`GrammarHealthCheckFinding`] is about, named by the grammar's own id/index plus
/// a human-readable display name. C#'s `Subjects` hands back the model objects themselves for
/// host navigation by reference equality; Rust has no such identity to hand back, so a host
/// navigates by id instead. Internally tagged (`"kind"`) so each variant is self-describing on the
/// wire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrammarHealthSubject {
    Table {
        table: u16,
        name: String,
    },
    CharDef {
        table: u16,
        char_def: u32,
        name: String,
    },
    LexEntry {
        entry: u32,
        name: String,
    },
    MorphRule {
        rule: u32,
        name: String,
    },
}

/// One problem or production-readiness risk found by [`check_grammar_health`]. Diagnostic only --
/// producing a finding never changes how the grammar parses. Ported from C#
/// `GrammarHealthCheckFinding`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarHealthCheckFinding {
    pub severity: GrammarHealthSeverity,
    pub code: GrammarHealthCode,
    /// A human-readable description naming the offending declaration(s). Free to change; use
    /// `code`, not this text, as the stable identifier.
    pub message: String,
    /// The model objects this finding is about, in the order most useful for a host to navigate.
    pub subjects: Vec<GrammarHealthSubject>,
}

/// Runs every registered check against `grammar` and returns the findings, in the order the
/// checks ran (C# `GrammarHealthChecker.Check`). An empty vec means every check passed, not that
/// nothing was checked.
pub fn check_grammar_health(grammar: &Grammar) -> Vec<GrammarHealthCheckFinding> {
    let mut findings = Vec::new();
    check_duplicate_feature_bundles(grammar, &mut findings);
    check_undeclared_segments(grammar, &mut findings);
    check_partial_morphemes(grammar, &mut findings);
    findings
}

// --- hc-duplicate-feature-bundle -------------------------------------------------------------

/// Distinct-bundle segments only; skipped for a zero-feature grammar, where every bundle is the same empty struct by construction.
fn check_duplicate_feature_bundles(
    grammar: &Grammar,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    if grammar.phon_features.is_empty() {
        return;
    }
    for (table_idx, table) in grammar.char_tables.iter().enumerate() {
        let table_id = TableId(table_idx as u16);
        let mut segs: Vec<(CharDefId, &CharDef)> = table
            .iter()
            .filter(|(_, cd)| cd.kind() == CharDefKind::Segment)
            .collect();
        segs.sort_by(|(_, a), (_, b)| first_representation(a).cmp(first_representation(b)));

        let mut groups: Vec<Vec<(CharDefId, &CharDef)>> = Vec::new();
        for entry in segs {
            match groups
                .iter_mut()
                .find(|g| stripped_lanes(g[0].1) == stripped_lanes(entry.1))
            {
                Some(group) => group.push(entry),
                None => groups.push(vec![entry]),
            }
        }

        for group in groups {
            if group.len() < 2 {
                continue;
            }
            let names: Vec<&str> = group
                .iter()
                .map(|(_, cd)| first_representation(cd))
                .collect();
            let mut subjects = vec![table_subject(table_id, table)];
            subjects.extend(
                group
                    .iter()
                    .map(|(id, cd)| char_def_subject(table_id, *id, cd)),
            );
            findings.push(GrammarHealthCheckFinding {
                severity: GrammarHealthSeverity::Warning,
                code: GrammarHealthCode::DuplicateFeatureBundle,
                message: format!(
                    "Character definition table '{}' has {} segments with an identical \
                     phonological feature bundle, so a segment-changing rule cannot reliably \
                     tell them apart: {}.",
                    table_display_name(table),
                    group.len(),
                    names.join(", ")
                ),
                subjects,
            });
        }
    }
}

// The `Type` lane is always appended last (`PhonFeatureSystem::from_raw`), so slicing it off is a bare truncation, not a search.
fn stripped_lanes(cd: &CharDef) -> &[u64] {
    let lanes = cd.feature_lanes();
    &lanes[..lanes.len() - 1]
}

fn first_representation(cd: &CharDef) -> &str {
    cd.representations()
        .first()
        .map_or_else(|| cd.xml_id(), String::as_str)
}

fn table_display_name(table: &CharDefTable) -> &str {
    table.name().unwrap_or_else(|| table.xml_id())
}

fn table_subject(id: TableId, table: &CharDefTable) -> GrammarHealthSubject {
    GrammarHealthSubject::Table {
        table: id.0,
        name: table_display_name(table).to_string(),
    }
}

fn char_def_subject(table_id: TableId, id: CharDefId, cd: &CharDef) -> GrammarHealthSubject {
    GrammarHealthSubject::CharDef {
        table: table_id.0,
        char_def: id.0,
        name: first_representation(cd).to_string(),
    }
}

fn lex_entry_subject(id: LexEntryId, name: &str) -> GrammarHealthSubject {
    GrammarHealthSubject::LexEntry {
        entry: id.0,
        name: name.to_string(),
    }
}

fn morph_rule_subject(id: MRuleId, name: &str) -> GrammarHealthSubject {
    GrammarHealthSubject::MorphRule {
        rule: id.0,
        name: name.to_string(),
    }
}

// --- hc-undeclared-segment ---------------------------------------------------------------------

/// Lex-entry allomorph segments plus rule `InsertSegments`, walking only a stratum's ordinary `mrules` -- matches C#'s own scope, so a template-slot-only rule is outside this check too.
fn check_undeclared_segments(grammar: &Grammar, findings: &mut Vec<GrammarHealthCheckFinding>) {
    for stratum in &grammar.strata {
        let Some(table) = grammar.char_tables.get(stratum.table.0 as usize) else {
            continue;
        };
        for &entry_id in &stratum.entries {
            let Some(entry) = grammar.entries.get(entry_id.0 as usize) else {
                continue;
            };
            let name = lex_entry_display_name(grammar, entry);
            for allomorph in &entry.allomorphs {
                check_segments_declared(
                    table,
                    stratum.table,
                    &allomorph.shape.shape,
                    &format!(
                        "Lexical entry '{name}' allomorph '{}'",
                        allomorph.shape.text
                    ),
                    lex_entry_subject(entry_id, &name),
                    findings,
                );
            }
        }

        for &rule_id in &stratum.mrules {
            let Some(rule) = grammar.mrules.get(rule_id.0 as usize) else {
                continue;
            };
            match rule {
                MorphRuleDef::AffixProcess(def) => {
                    let name = morph_rule_display_name(grammar, def.morpheme, def.name.as_deref());
                    let subject = morph_rule_subject(rule_id, &name);
                    for allomorph in &def.allomorphs {
                        for action in &allomorph.rhs {
                            check_insert_segments(
                                grammar,
                                action,
                                "Morphological rule",
                                &name,
                                subject.clone(),
                                findings,
                            );
                        }
                    }
                }
                MorphRuleDef::Compounding(def) => {
                    let name = compounding_rule_display_name(def);
                    let subject = morph_rule_subject(rule_id, &name);
                    for subrule in &def.subrules {
                        for action in &subrule.rhs {
                            check_insert_segments(
                                grammar,
                                action,
                                "Compounding rule",
                                &name,
                                subject.clone(),
                                findings,
                            );
                        }
                    }
                }
                // C# casts to `AffixProcessRule`/`CompoundingRule` only; `RealizationalAffixProcessRule` is never checked there either.
                MorphRuleDef::Realizational(_) => {}
            }
        }
    }
}

fn compounding_rule_display_name(def: &CompoundingRuleDef) -> String {
    def.name
        .as_deref()
        .filter(|n| !n.is_empty())
        .unwrap_or(&def.xml_id)
        .to_string()
}

fn check_insert_segments(
    grammar: &Grammar,
    action: &OutputAction,
    kind_label: &str,
    rule_name: &str,
    owner_subject: GrammarHealthSubject,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    let OutputAction::InsertSegments {
        table: table_id,
        shape,
    } = action
    else {
        return;
    };
    let Some(table) = grammar.char_tables.get(table_id.0 as usize) else {
        return;
    };
    check_segments_declared(
        table,
        *table_id,
        &shape.shape,
        &format!(
            "{kind_label} '{rule_name}' inserted segments '{}'",
            shape.text
        ),
        owner_subject,
        findings,
    );
}

/// Boundary/anchor nodes are structural and an abstract natural-class node (`NO_CHAR_DEF`) is already-declared, so both are skipped -- mirrors C#'s Segment-type-only filter.
fn check_segments_declared(
    table: &CharDefTable,
    table_id: TableId,
    shape: &Shape,
    where_desc: &str,
    owner_subject: GrammarHealthSubject,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    for (_, kind, cd, _) in shape.interior() {
        if kind != NodeKind::Segment || cd == NO_CHAR_DEF {
            continue;
        }
        let declared =
            (cd as usize) < table.len() && table.get(CharDefId(cd)).kind() == CharDefKind::Segment;
        if declared {
            continue;
        }
        findings.push(GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Error,
            code: GrammarHealthCode::UndeclaredSegment,
            message: format!(
                "{where_desc} contains a segment (character-definition id {cd}) that character \
                 definition table '{}' does not declare.",
                table_display_name(table)
            ),
            subjects: vec![table_subject(table_id, table), owner_subject.clone()],
        });
    }
}

fn lex_entry_display_name(grammar: &Grammar, entry: &LexEntryDef) -> String {
    if !entry.authored_id.is_empty() {
        return entry.authored_id.clone();
    }
    if let Some(gloss) = grammar
        .morphemes
        .get(entry.morpheme.0 as usize)
        .and_then(|info| info.gloss.as_deref())
        .filter(|g| !g.is_empty())
    {
        return gloss.to_string();
    }
    "unnamed".to_string()
}

fn morph_rule_display_name(grammar: &Grammar, morpheme: MorphemeId, name: Option<&str>) -> String {
    if let Some(n) = name.filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    if let Some(info) = grammar.morphemes.get(morpheme.0 as usize) {
        if !info.xml_key.is_empty() {
            return info.xml_key.clone();
        }
        if let Some(g) = info.gloss.as_deref().filter(|g| !g.is_empty()) {
            return g.to_string();
        }
    }
    "unnamed".to_string()
}

// --- hc-partial-morpheme -------------------------------------------------------------------

/// Reads `partial` directly (the model's own published fact); every rule lives once in `Grammar::mrules` regardless of slot references, so one pass already dedups without a seen-set.
fn check_partial_morphemes(grammar: &Grammar, findings: &mut Vec<GrammarHealthCheckFinding>) {
    for (i, entry) in grammar.entries.iter().enumerate() {
        if !entry.partial {
            continue;
        }
        let id = LexEntryId(i as u32);
        let name = lex_entry_display_name(grammar, entry);
        findings.push(partial_finding(
            "Lexical entry",
            &name,
            lex_entry_subject(id, &name),
        ));
    }
    for (i, rule) in grammar.mrules.iter().enumerate() {
        let MorphRuleDef::AffixProcess(def) = rule else {
            continue;
        };
        if !def.partial {
            continue;
        }
        let id = MRuleId(i as u32);
        let name = morph_rule_display_name(grammar, def.morpheme, def.name.as_deref());
        findings.push(partial_finding(
            "Morphological rule",
            &name,
            morph_rule_subject(id, &name),
        ));
    }
}

fn partial_finding(
    kind_label: &str,
    name: &str,
    subject: GrammarHealthSubject,
) -> GrammarHealthCheckFinding {
    GrammarHealthCheckFinding {
        severity: GrammarHealthSeverity::Warning,
        code: GrammarHealthCode::PartialMorpheme,
        message: format!(
            "{kind_label} '{name}' is partially analyzed. Supply its missing category or \
             template/slot analysis; leaving it partial can broaden analysis and disable safe \
             final-template pruning."
        ),
        subjects: vec![subject],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn grammar(xml: &str) -> Grammar {
        crate::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    fn codes(findings: &[GrammarHealthCheckFinding]) -> Vec<GrammarHealthCode> {
        findings.iter().map(|f| f.code).collect()
    }

    // --- hc-duplicate-feature-bundle ------------------------------------------------------

    const TWO_SEGMENTS_SHARE_BUNDLE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DuplicateBundle</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn two_segments_share_feature_bundle_reports_both_by_name() {
        let g = grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML);
        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, GrammarHealthCode::DuplicateFeatureBundle);
        assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
        assert!(finding.message.contains('a'));
        assert!(finding.message.contains('b'));
        assert!(matches!(
            &finding.subjects[0],
            GrammarHealthSubject::Table { name, .. } if name == "table1"
        ));
    }

    const DISTINCT_BUNDLES_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DistinctBundles</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_m" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn every_segment_has_distinct_feature_bundle_no_findings() {
        let g = grammar(DISTINCT_BUNDLES_XML);
        assert!(check_grammar_health(&g).is_empty());
    }

    const ZERO_FEATURE_SYSTEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeatureSystem</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn no_phonological_feature_system_does_not_flag_trivially_identical_bundles() {
        // No `<PhonologicalFeatureSystem>` at all (the real Sena shape) -- every bundle is the same empty struct, so this must not report a duplicate.
        let g = grammar(ZERO_FEATURE_SYSTEM_XML);
        assert!(check_grammar_health(&g).is_empty());
    }

    // --- hc-undeclared-segment -------------------------------------------------------------

    const CLEAN_LEXICON_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CleanLexicon</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn clean_grammar_no_findings_at_all() {
        let g = grammar(CLEAN_LEXICON_XML);
        assert!(check_grammar_health(&g).is_empty());
    }

    /// A hand-built `Shape` bypassing the table's own validated segmentation -- mirrors C#'s own test note that direct object-model construction need not go through it.
    fn undeclared_shape() -> Shape {
        let mut b = ShapeBuilder::new();
        b.push_segment(9_999);
        b.finish()
    }

    #[test]
    fn lexical_entry_uses_segment_no_table_declares_reports_finding() {
        let mut g = grammar(CLEAN_LEXICON_XML);
        g.entries[0].allomorphs[0].shape.shape = undeclared_shape();

        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert_eq!(findings[0].severity, GrammarHealthSeverity::Error);
        assert!(findings[0].message.contains("e1"));
    }

    const AFFIX_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AffixInsertSegments</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// Finds the sole `InsertSegments` action inside `grammar.mrules[0]`'s single allomorph.
    fn insert_segments_shape_mut(g: &mut Grammar) -> &mut Shape {
        let MorphRuleDef::AffixProcess(def) = &mut g.mrules[0] else {
            panic!("expected an AffixProcess rule");
        };
        for action in &mut def.allomorphs[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                return &mut shape.shape;
            }
        }
        panic!("expected an InsertSegments action");
    }

    #[test]
    fn affix_process_rule_insert_segments_undeclared_reports_finding() {
        let mut g = grammar(AFFIX_INSERT_SEGMENTS_XML);
        *insert_segments_shape_mut(&mut g) = undeclared_shape();

        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert!(findings[0].message.contains("plural"));
    }

    const COMPOUNDING_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CompoundingInsertSegments</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_bnd"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mrC">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="mrC">
            <Name>compound1</Name>
            <CompoundingSubrules><CompoundingSubrule>
              <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
              <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
            </CompoundingSubrule></CompoundingSubrules>
          </CompoundingRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn compounding_rule_insert_segments_undeclared_reports_finding() {
        let mut g = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
        let MorphRuleDef::Compounding(def) = &mut g.mrules[0] else {
            panic!("expected a Compounding rule");
        };
        let mut replaced = false;
        for action in &mut def.subrules[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                shape.shape = undeclared_shape();
                replaced = true;
            }
        }
        assert!(replaced, "fixture must contain an InsertSegments action");

        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert!(findings[0].message.contains("compound1"));
    }

    // --- hc-partial-morpheme -----------------------------------------------------------------

    const PARTIAL_LEX_ENTRY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialLexEntry</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_lexical_entry_reports_actionable_warning() {
        let g = grammar(PARTIAL_LEX_ENTRY_XML);
        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, GrammarHealthCode::PartialMorpheme);
        assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
        assert!(finding.message.contains("entry1"));
        assert!(finding.message.contains("partially analyzed"));
        assert!(finding.message.contains("final-template pruning"));
        assert!(matches!(
            &finding.subjects[..],
            [GrammarHealthSubject::LexEntry { name, .. }] if name == "entry1"
        ));
    }

    const PARTIAL_TEMPLATE_RULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialTemplateRule</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
            <Name>subject</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate>
            <Name>verb1</Name>
            <Slot morphologicalRules="mr1"><Name>Sl1</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>verb2</Name>
            <Slot morphologicalRules="mr1"><Name>Sl2</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_ordinary_rule_reports_rule() {
        // Distinct from the template-only fixture below: this one lists `mr1` in the stratum's own `morphologicalRules`, exercising the ordinary-rule path.
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialOrdinaryRule</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = grammar(XML);
        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::PartialMorpheme);
        assert!(findings[0].message.contains("plural"));
        assert!(matches!(
            &findings[0].subjects[..],
            [GrammarHealthSubject::MorphRule { name, .. }] if name == "plural"
        ));
    }

    #[test]
    fn partial_template_rule_referenced_twice_reports_once() {
        let g = grammar(PARTIAL_TEMPLATE_RULE_XML);
        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1, "referenced by two slots, reported once");
        assert_eq!(findings[0].code, GrammarHealthCode::PartialMorpheme);
        assert!(findings[0].message.contains("subject"));
    }

    const PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialAndDuplicate</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_morpheme_and_existing_problem_reports_both() {
        let g = grammar(PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML);
        let mut found = codes(&check_grammar_health(&g));
        found.sort_by_key(|c| c.wire());
        let mut expected = vec![
            GrammarHealthCode::DuplicateFeatureBundle,
            GrammarHealthCode::PartialMorpheme,
        ];
        expected.sort_by_key(|c| c.wire());
        assert_eq!(found, expected);
    }

    // --- serialization ------------------------------------------------------------------------

    #[test]
    fn findings_are_serializable() {
        let g = grammar(PARTIAL_LEX_ENTRY_XML);
        let findings = check_grammar_health(&g);
        let json = serde_json::to_string(&findings).expect("findings must serialize");
        assert!(json.contains("hc-partial-morpheme"));
        let round_tripped: Vec<GrammarHealthCheckFinding> =
            serde_json::from_str(&json).expect("findings must deserialize");
        assert_eq!(round_tripped, findings);
    }

    #[test]
    fn every_code_serializes_to_its_stable_wire_string() {
        for (code, wire) in [
            (
                GrammarHealthCode::UndeclaredSegment,
                "hc-undeclared-segment",
            ),
            (
                GrammarHealthCode::DuplicateFeatureBundle,
                "hc-duplicate-feature-bundle",
            ),
            (GrammarHealthCode::PartialMorpheme, "hc-partial-morpheme"),
        ] {
            assert_eq!(code.wire(), wire);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("{wire:?}"));
        }
    }
}
