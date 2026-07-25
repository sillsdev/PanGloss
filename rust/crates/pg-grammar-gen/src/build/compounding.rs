//! Compounding-rule scale builder (synthetic-stress-grammar-plan.md §2 row `CompoundingRuleDef`;
//! design doc §6 priority (6), first emit-scale exerciser). Mirrors `machine/conformance/languages/
//! fusional-realizational-morphology/grammar.xml`'s own `mrCompoundHN` shape (the only real `<CompoundingRule>` fixture
//! in this repo): a head root + a non-head root, concatenated with a literal `+` compound-seam
//! marker in between (`CopyFromInput(head) + InsertSegments("+") + CopyFromInput(nonhead)`). That
//! same file's own header comment records a load-bearing finding this builder depends on: the `+`
//! marker must be declared as a `<BoundaryDefinition>` (via [`crate::build::tables::build`]'s own
//! `needs_boundary` flag), NOT a plain `<SegmentDefinition>` -- declaring it as a segment "produced
//! zero parses for every compound word (confirmed by isolated probing)".
//!
//! ## Recall-parity vs. overbudget: two different emitters, deliberately
//! Recall-parity uses the PRODUCTION `pg_foma::emit::emit` path (same as GATE 2/circumfix) --
//! `emit.rs`'s own classification loop sets `has_compounding_rules = true` whenever any stratum
//! declares a `MorphRuleDef::Compounding` rule, routing compound words through its template-less/
//! structural-composite machinery. The `_overbudget` variant instead drives `pg_foma::uflexc::
//! emit_underlying_filtered_with_budget`'s own root-entry line count over a tiny test
//! [`ComposeBudget`] (design doc §6, "first emit-scale exerciser") -- `uflexc.rs`'s own module doc
//! records that it does not even SEE compounding rules ("no `CompoundingRuleDef` allomorph is even
//! visible through `emit::allomorphs_of`, so there is nothing to enumerate wrongly, only something
//! absent"), so this is a scale check on plain root-entry COUNT (the `entries_per_stratum` scale
//! knob) in a grammar that happens to also declare a compounding rule, not a compounding-specific
//! code path -- an honest, deliberate choice recorded here rather than hidden: it is the
//! grammar-wide entry-count vector (V4, synthetic-stress-grammar-plan.md §3) that trips first, and
//! compounding is the construct that motivated giving this vector its own gate.

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// Everything [`build`] produces: one `<CompoundingRule>` plus its head/non-head root entries.
#[derive(Debug, Clone)]
pub struct CompoundingBuild {
    pub rule_xml: String,
    pub rule_xml_id: String,
    pub head_entry_xml: String,
    pub head_entry_xml_id: String,
    pub nonhead_entry_xml: String,
    pub nonhead_entry_xml_id: String,
}

/// Build one compounding rule (`headPartsOfSpeech`/`nonHeadPartsOfSpeech` both `pos_xml_id`,
/// unconstrained -- no MPR/POS gating, module doc's own minimal-shape convention) plus its own head
/// and non-head root entries, drawing head/non-head characters from `table.segments[0]`/`[1]` and
/// the compound seam from `boundary_xml_id` ([`crate::build::tables::build`]'s own
/// `needs_boundary=true` output). Panics if `table` has fewer than 2 segments.
pub fn build(
    pos_xml_id: &str,
    boundary_xml_id: &str,
    table: &TableSpec,
    ids: &mut IdMinter,
) -> CompoundingBuild {
    assert!(
        table.segments.len() >= 2,
        "build_compounding: table needs at least 2 segments (head char + non-head char), has {}",
        table.segments.len()
    );
    let head_seg = &table.segments[0];
    let nonhead_seg = &table.segments[1];

    let rule_xml_id = ids.next("mrCompound");
    let head_stem_id = ids.next("cmpHead");
    let nonhead_stem_id = ids.next("cmpNonHead");
    let rule_xml = format!(
        "\n          <CompoundingRule id=\"{rule_xml_id}\" blockable=\"false\" headPartsOfSpeech=\"{pos_xml_id}\" nonHeadPartsOfSpeech=\"{pos_xml_id}\">\n            \
         <Name>compoundDemo</Name>\n            <CompoundingSubrules>\n              <CompoundingSubrule>\n                \
         <HeadMorphologicalInput><PhoneticSequence id=\"{head_stem_id}\"><OptionalSegmentSequence min=\"1\" max=\"-1\">\
         <SimpleContext naturalClass=\"{nc_any}\" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>\n                \
         <NonHeadMorphologicalInput><PhoneticSequence id=\"{nonhead_stem_id}\"><OptionalSegmentSequence min=\"1\" max=\"-1\">\
         <SimpleContext naturalClass=\"{nc_any}\" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>\n                \
         <MorphologicalOutput><CopyFromInput index=\"{head_stem_id}\" />\
         <InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments>\
         <CopyFromInput index=\"{nonhead_stem_id}\" /></MorphologicalOutput>\n              \
         </CompoundingSubrule>\n            </CompoundingSubrules>\n          </CompoundingRule>",
        nc_any = crate::build::tables::nc_any_xml_id(),
    );
    // boundary_xml_id is validated at load time via the literal "+" representation match, not
    // referenced by xml id anywhere in this rule's own text -- accepted as a parameter purely so a
    // caller is forced to have built one (module doc: a plain segment silently fails to parse any
    // compound word), and so this builder's own doc/signature makes that dependency visible.
    let _ = boundary_xml_id;

    let head_entry_xml_id = ids.next("entryHead");
    let head_allo_id = ids.next("alloHead");
    let head_entry_xml = format!(
        "\n          <LexicalEntry id=\"{head_entry_xml_id}\" partOfSpeech=\"{pos_xml_id}\">\n            \
         <Allomorphs><Allomorph id=\"{head_allo_id}\"><PhoneticShape>{ch}</PhoneticShape></Allomorph></Allomorphs>\n            \
         <MorphemeId>CMPHEAD</MorphemeId>\n          </LexicalEntry>",
        ch = head_seg.ch,
    );

    let nonhead_entry_xml_id = ids.next("entryNonHead");
    let nonhead_allo_id = ids.next("alloNonHead");
    let nonhead_entry_xml = format!(
        "\n          <LexicalEntry id=\"{nonhead_entry_xml_id}\" partOfSpeech=\"{pos_xml_id}\">\n            \
         <Allomorphs><Allomorph id=\"{nonhead_allo_id}\"><PhoneticShape>{ch}</PhoneticShape></Allomorph></Allomorphs>\n            \
         <MorphemeId>CMPNONHEAD</MorphemeId>\n          </LexicalEntry>",
        ch = nonhead_seg.ch,
    );

    CompoundingBuild {
        rule_xml,
        rule_xml_id,
        head_entry_xml,
        head_entry_xml_id,
        nonhead_entry_xml,
        nonhead_entry_xml_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn builds_one_compounding_rule_and_two_roots() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 2, false, true, &mut ids);
        let cb = build(
            "posV",
            tb.boundary_xml_id.as_deref().unwrap(),
            &tb.tables[0],
            &mut ids,
        );
        assert!(cb.rule_xml.contains("CompoundingRule"));
        assert_ne!(cb.head_entry_xml_id, cb.nonhead_entry_xml_id);
    }
}
