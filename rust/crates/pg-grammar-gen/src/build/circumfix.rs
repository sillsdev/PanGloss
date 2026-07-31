//! Circumfix `MorphologicalRule` builder (design doc §2 "circumfix"; synthetic-stress-grammar-
//! plan.md §2's "Circumfix / null-morph roles" row -- UNPROVEN, dormant on every reference
//! grammar this repo has). Mirrors `machine/conformance/languages/fusional-realizational-morphology/grammar.xml`'s
//! own `mrCircumfixGeT` shape exactly (the only REAL circumfix fixture in this repo): a single
//! `MorphologicalInput` capturing the whole root, an output of `InsertSegments` (prefix) +
//! `CopyFromInput` (the captured root) + `InsertSegments` (suffix) -- a LEADING and a TRAILING
//! insert around one copied span. `pg-foma/src/emit.rs`'s `classify_affix` reads exactly this
//! shape as `Role::CircumfixPrefix` and (that module's own doc, `is_structural_rule`) ALWAYS
//! routes it through the "structural composite" (Morpher-driven synthesis) path, never literal-
//! lexc concatenation -- this is GATE 2's reason for existing: it is the one construct that forces
//! the full `emit()`/`FomaProposer` pipeline (not `pg-foma/src/uflexc.rs`'s simpler, circumfix-
//! skipping emitter) end to end.

use crate::ids::IdMinter;

/// One circumfix rule: `prefix` + captured root + `suffix`, POS-preserving
/// (`requiredPartsOfSpeech == outputPartOfSpeech == pos`), no `OutputHeadFeatures` (kept minimal
/// -- the oracle's `real_fs` argument, [`crate::oracle`]'s own doc, stays unconstrained).
#[derive(Debug, Clone)]
pub struct CircumfixSpec {
    pub mrule_xml_id: String,
    pub morpheme_id: String,
    pub xml: String,
}

/// Build `count` circumfix rules referencing part of speech `pos_xml_id`, whose prefix/suffix
/// affix material is drawn from `affix_chars` (must be non-empty; every character must already be
/// declared as a `<SegmentDefinition>` representation in the SAME `CharacterDefinitionTable` the
/// owning stratum uses -- `<PhoneticShape>` text is segmented against that table at load time, so
/// an undeclared character fails to load, not just fails to mean what you'd expect). Each rule's
/// own prefix is `affix_chars[i % len]` doubled and its suffix is `affix_chars[(i+1) % len]` --
/// distinct-enough per rule (when `count > 1`) to keep generated words visually distinguishable
/// when debugging a failing gate, not load-bearing for correctness.
pub fn build_circumfixes(
    count: usize,
    pos_xml_id: &str,
    affix_chars: &[char],
    ids: &mut IdMinter,
) -> Vec<CircumfixSpec> {
    assert!(
        !affix_chars.is_empty(),
        "build_circumfixes: affix_chars must be non-empty"
    );
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let mrule_xml_id = ids.next("mrCirc");
        let sub_xml_id = ids.next("subCirc");
        let stem_xml_id = ids.next("stemCirc");
        let morpheme_id = format!("CIRC{i}");
        let prefix_ch = affix_chars[i % affix_chars.len()];
        let suffix_ch = affix_chars[(i + 1) % affix_chars.len()];
        let prefix: String = std::iter::repeat_n(prefix_ch, 2).collect();
        let suffix: String = std::iter::once(suffix_ch).collect();
        let xml = format!(
            "\n          <MorphologicalRule id=\"{mrule_xml_id}\" requiredPartsOfSpeech=\"{pos_xml_id}\" outputPartOfSpeech=\"{pos_xml_id}\">\n            \
             <Name>circ{i}</Name>\n            <MorphologicalSubrules>\n              <MorphologicalSubrule id=\"{sub_xml_id}\">\n                \
             <MorphologicalInput><PhoneticSequence id=\"{stem_xml_id}\"><OptionalSegmentSequence min=\"1\" max=\"-1\">\
             <SimpleContext naturalClass=\"{nc_any}\" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>\n                \
             <MorphologicalOutput>\n                  <InsertSegments><PhoneticShape>{prefix}</PhoneticShape></InsertSegments>\n                  \
             <CopyFromInput index=\"{stem_xml_id}\" />\n                  \
             <InsertSegments><PhoneticShape>{suffix}</PhoneticShape></InsertSegments>\n                </MorphologicalOutput>\n              \
             </MorphologicalSubrule>\n            </MorphologicalSubrules>\n            <MorphemeId>{morpheme_id}</MorphemeId>\n          \
             </MorphologicalRule>",
            nc_any = crate::build::tables::nc_any_xml_id(),
        );
        out.push(CircumfixSpec {
            mrule_xml_id,
            morpheme_id,
            xml,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_rules_get_distinct_affix_material() {
        let mut ids = IdMinter::new();
        let specs = build_circumfixes(2, "posV", &['x', 'y', 'z'], &mut ids);
        assert_eq!(specs.len(), 2);
        assert_ne!(specs[0].mrule_xml_id, specs[1].mrule_xml_id);
        assert!(specs[0].xml.contains("xx"));
        assert!(specs[1].xml.contains("yy"));
    }
}
