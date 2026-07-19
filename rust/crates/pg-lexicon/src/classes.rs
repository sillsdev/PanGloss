//! Candidate inflection-class enumeration and shape validation (add-to-dictionary design doc,
//! Sub-project 1, components 2 and 4).
//!
//! A "candidate class" is a distinct `(POS, MprSet)` pair observed over `Grammar::entries` — the
//! same grouping HermitCrab's own inflection-class membership is defined by
//! (`pg_grammar::model::LexEntryDef::mpr`, `pg_grammar::model::Grammar::mpr_names`). This module
//! never mutates or rebuilds the grammar; it only reads the already-loaded tables.

use pg_featstruct::FeatureValue;
use pg_grammar::model::{Grammar, LexEntryDef, LexEntryId, MprId, MprSet, SynFeatureKind};
use serde::Serialize;

/// One distinct `(POS, MprSet)` pair found over `Grammar::entries` — a candidate inflection class
/// the user can assign a new dictionary entry to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClassCandidate {
    /// Stable identity: `"<pos>|<mpr-name>|<mpr-name>..."`, MPR names sorted. Survives a project
    /// reconversion (which can renumber `MprId`s) because it is spelled from `Grammar::mpr_names`
    /// strings, never raw ids. This is what [`crate::model::UserLexEntry::class_key`] stores.
    pub key: String,
    /// The POS value extracted from the class's entries' `syn_fs`, if any (display label; the
    /// `<PartOfSpeech><Name>` text, e.g. `"n"`).
    pub pos: Option<String>,
    /// `Grammar::mpr_names[id]` for each bit set in this class's `MprSet`, sorted.
    pub mpr_names: Vec<String>,
    /// How many `Grammar::entries` fall into this class.
    pub entry_count: usize,
    /// `MorphemeInfo::xml_key` of one representative entry in this class — the anchor
    /// [`crate::augment::augment_xml`] clones from and [`crate::paradigm::disambiguating_forms`]
    /// fabricates a hypothetical stem against.
    pub exemplar_xml_key: String,
    /// The exemplar's `("ID", hvo)` property, if present (FieldWorks-exported grammars carry this;
    /// hand-built grammars may not) — lets the demo look up `lexicalData[exemplar_morph_id]` for a
    /// human headword to show alongside the class in the comparison UI.
    pub exemplar_morph_id: Option<String>,
}

/// Group `grammar.entries` by `(pos-value-of-syn_fs, mpr)`, one [`ClassCandidate`] per distinct
/// pair, sorted by descending `entry_count` (ties broken by `key` for determinism).
pub fn candidate_classes(grammar: &Grammar) -> Vec<ClassCandidate> {
    struct Group {
        pos: Option<String>,
        mpr: MprSet,
        count: usize,
        exemplar: LexEntryId,
    }

    // First-seen order, like `Grammar::entries` itself (document order) -- a plain Vec scan (not a
    // HashMap) keeps the exemplar choice ("first entry seen in this class") deterministic and
    // matches this crate's small-grammar assumption (v1 is a UI-facing, human-scale enumeration,
    // not a hot parse-time path).
    let mut groups: Vec<Group> = Vec::new();
    for (i, entry) in grammar.entries.iter().enumerate() {
        let pos = pos_name_of(grammar, entry);
        match groups
            .iter_mut()
            .find(|g| g.pos == pos && g.mpr == entry.mpr)
        {
            Some(g) => g.count += 1,
            None => groups.push(Group {
                pos,
                mpr: entry.mpr,
                count: 1,
                exemplar: LexEntryId(i as u32),
            }),
        }
    }

    let mut out: Vec<ClassCandidate> = groups
        .into_iter()
        .map(|g| {
            let mut mpr_names: Vec<String> = (0..grammar.mpr_names.len())
                .filter(|&b| g.mpr.contains(MprId(b as u8)))
                .filter_map(|b| grammar.mpr_names.get(b).cloned())
                .collect();
            mpr_names.sort();

            let mut key = g.pos.clone().unwrap_or_default();
            for n in &mpr_names {
                key.push('|');
                key.push_str(n);
            }

            let entry = &grammar.entries[g.exemplar.0 as usize];
            let morpheme = &grammar.morphemes[entry.morpheme.0 as usize];
            let exemplar_morph_id = morpheme
                .properties
                .iter()
                .find(|(k, _)| k == "ID")
                .map(|(_, v)| v.clone());

            ClassCandidate {
                key,
                pos: g.pos,
                mpr_names,
                entry_count: g.count,
                exemplar_xml_key: morpheme.xml_key.clone(),
                exemplar_morph_id,
            }
        })
        .collect();

    out.sort_by(|a, b| b.entry_count.cmp(&a.entry_count).then_with(|| a.key.cmp(&b.key)));
    out
}

/// The `<PartOfSpeech><Name>` text for `entry`'s `syn_fs`, if the POS feature is instantiated and
/// symbolic (it always is for a loaded grammar — see `pg_grammar::model::SynFeatureSystem`'s own
/// doc: POS is always feature 0 and always `Symbolic`). Mirrors how `pg-parse::morpher` reads the
/// same feature (`structured_analysis`'s `w.syn_fs.get(self.g.syn_features.pos)`), just resolving
/// the bit to its declared name instead of leaving it as a bare index.
fn pos_name_of(g: &Grammar, entry: &LexEntryDef) -> Option<String> {
    let fs = g.fs_interner.get(entry.syn_fs);
    match fs.get(g.syn_features.pos)? {
        FeatureValue::Symbolic(bits) => {
            let idx = bits.first()? as usize;
            match &g.syn_features.features[g.syn_features.pos.0 as usize].kind {
                SynFeatureKind::Symbolic { symbols, .. } => {
                    symbols.get(idx).map(|(_, name)| name.clone())
                }
                SynFeatureKind::Complex => None,
            }
        }
        FeatureValue::Complex(_) => None,
    }
}

/// Resolve a [`pg_grammar::model::MorphemeInfo::xml_key`] back to the [`LexEntryId`] that owns it
/// (linear scan; called once per candidate class or per user-lexicon entry, never per-word).
pub(crate) fn resolve_entry_by_xml_key(grammar: &Grammar, xml_key: &str) -> Option<LexEntryId> {
    grammar.entries.iter().enumerate().find_map(|(i, e)| {
        (grammar.morphemes[e.morpheme.0 as usize].xml_key == xml_key).then(|| LexEntryId(i as u32))
    })
}

/// The surface (last, per `pg-parse::morpher`'s own convention) stratum's character-definition
/// table -- the one a user-typed shape is validated/segmented against, matching how
/// `Morpher::parse_word_core_selected` picks its segmentation table.
fn surface_table(grammar: &Grammar) -> Option<&pg_grammar::chardef::CharDefTable> {
    let last = grammar.strata.last()?;
    Some(&grammar.char_tables[last.table.0 as usize])
}

/// Segment `shape` against the grammar's surface-stratum character-definition table, the same way
/// [`crate::paradigm::disambiguating_forms`] and [`crate::augment::augment_xml`] both require
/// before they'll accept a shape. On failure, returns a friendly message listing every distinct
/// character in `shape` this grammar's writing system doesn't define (best-effort: segmentation
/// itself is a greedy multi-character-representation match, so this per-character scan is a
/// diagnostic aid, not a re-implementation of the segmenter).
pub fn validate_shape(grammar: &Grammar, shape: &str) -> Result<(), String> {
    let Some(table) = surface_table(grammar) else {
        return Err("this grammar defines no strata to validate a shape against".to_string());
    };
    if pg_grammar::segment::segment(table, shape).is_ok() {
        return Ok(());
    }

    let mut bad: Vec<char> = Vec::new();
    for c in shape.chars() {
        if table.lookup_nfd(&c.to_string()).is_none() && !bad.contains(&c) {
            bad.push(c);
        }
    }
    if bad.is_empty() {
        // Every individual character is independently defined, but no decomposition of the whole
        // string segments (e.g. an all-boundary shape) -- report the whole shape instead of an
        // empty (and unhelpful) character list.
        return Err(format!(
            "\"{shape}\" doesn't segment against this grammar's writing system"
        ));
    }
    let listed: Vec<String> = bad.iter().map(|c| format!("'{c}'")).collect();
    Err(format!(
        "\"{shape}\" contains characters this grammar's writing system doesn't define: {}",
        listed.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>ClassesTest</Name>
  <PartsOfSpeech><PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
  <MorphologicalPhonologicalRuleFeatures>
    <MorphologicalPhonologicalRuleFeature id="mprC1">C1</MorphologicalPhonologicalRuleFeature>
    <MorphologicalPhonologicalRuleFeature id="mprC2">C2</MorphologicalPhonologicalRuleFeature>
  </MorphologicalPhonologicalRuleFeatures>
  <CharacterDefinitionTable id="t1">
    <Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <Strata>
    <Stratum characterDefinitionTable="t1">
      <Name>S</Name>
      <LexicalEntries>
        <LexicalEntry id="e1" partOfSpeech="posN" ruleFeatures="mprC1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          <Gloss>one</Gloss>
          <Properties><Property name="ID">101</Property></Properties>
        </LexicalEntry>
        <LexicalEntry id="e2" partOfSpeech="posN" ruleFeatures="mprC2">
          <Allomorphs><Allomorph id="a2"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
          <Gloss>two</Gloss>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>
"#;

    #[test]
    fn candidate_classes_groups_by_pos_and_mpr() {
        let g = pg_grammar::load(XML).expect("loads");
        let classes = candidate_classes(&g);
        assert_eq!(classes.len(), 2, "two distinct MPR sets under one POS");
        assert!(classes.iter().all(|c| c.pos.as_deref() == Some("n")));
        assert!(classes.iter().any(|c| c.mpr_names == vec!["C1".to_string()]));
        assert!(classes.iter().any(|c| c.mpr_names == vec!["C2".to_string()]));
        let c1 = classes
            .iter()
            .find(|c| c.mpr_names == vec!["C1".to_string()])
            .unwrap();
        assert_eq!(c1.exemplar_xml_key, "e1");
        assert_eq!(c1.exemplar_morph_id.as_deref(), Some("101"));
        let c2 = classes
            .iter()
            .find(|c| c.mpr_names == vec!["C2".to_string()])
            .unwrap();
        assert_eq!(c2.exemplar_morph_id, None);
    }

    #[test]
    fn validate_shape_accepts_defined_characters_and_rejects_others() {
        let g = pg_grammar::load(XML).expect("loads");
        assert!(validate_shape(&g, "aba").is_ok());
        let err = validate_shape(&g, "azb").unwrap_err();
        assert!(err.contains('z'), "message should list the offending char: {err}");
    }
}
