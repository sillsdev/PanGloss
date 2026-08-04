//! Stratum-depth scale builder. Adds `extra_strata` ADDITIONAL strata beyond the base single-table/single-stratum
//! scaffold [`crate::render::render_indexed`] always builds, each REUSING the SAME table 0 (never a
//! new table) -- deliberately sidesteps the multi-table threading question
//! (`pg_foma::replace::table_of`/`resolve_alpha_tuples`'s handling of `char_tables[0]`)
//! rather than exercising it a second time: GATE 1 already covers multi-TABLE wrongness; this gate
//! is about multi-STRATUM CASCADING specifically, which is orthogonal and provably correct against
//! a single, always-table-0 grammar.
//!
//! Mirrors `pg-foma/src/morphotactics.rs`'s own `FIXTURE_STRATA` shape: each extra stratum declares ONE OBLIGATORY `<MorphologicalRule>`, wired via the
//! `<Stratum>` element's OWN `morphologicalRules="..."` attribute (NOT an `<AffixTemplate>` slot --
//! that attribute is what makes the rule apply UNCONDITIONALLY to every word entering that stratum,
//! no optionality, so a root's surface form after `N` extra strata is mechanically `markerN-1 ...
//! marker1 marker0 root` -- prefix-inserting, innermost stratum's marker closest to the root).

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// One additional stratum: its own `<Stratum>` XML fragment (to be appended, in order, after the
/// base stratum inside `<Strata>`), name, and obligatory rule's xml id.
#[derive(Debug, Clone)]
pub struct ExtraStratum {
    pub stratum_name: String,
    pub rule_xml_id: String,
}

/// Everything [`build`] produces: `extra_strata` additional `<Stratum>` elements (concatenated XML,
/// document order = cascade order, applied AFTER the base stratum) and their own bookkeeping.
#[derive(Debug, Clone)]
pub struct StrataBuild {
    pub strata_xml: String,
    pub strata: Vec<ExtraStratum>,
}

/// Build `extra_strata` (`>= 1`) additional strata, all sharing `table`'s own xml id, referencing
/// part of speech `pos_xml_id`, numbered starting at `first_index` (so a caller building strata
/// `S0..S{first_index-1}` itself can continue the naming sequence without collision -- existing
/// recipes always use `first_index = 1`, the base stratum being `S0`). Needs at least
/// `extra_strata` distinct segments in `table` (one dedicated marker character per extra stratum,
/// mirroring [`crate::build::gating`]/[`crate::build::alpha`]'s own "one dedicated position"
/// convention) -- panics otherwise.
pub fn build(
    extra_strata: usize,
    first_index: usize,
    pos_xml_id: &str,
    table: &TableSpec,
    ids: &mut IdMinter,
) -> StrataBuild {
    assert!(extra_strata >= 1, "build_strata: extra_strata must be >= 1");
    assert!(
        table.segments.len() >= extra_strata,
        "build_strata: table has {} segments, needs at least {extra_strata} (one dedicated marker char per extra stratum)",
        table.segments.len()
    );

    let nc_any = crate::build::tables::nc_any_xml_id();
    let mut strata_xml = String::new();
    let mut strata = Vec::with_capacity(extra_strata);
    for i in 0..extra_strata {
        let seg = &table.segments[i];
        let rule_xml_id = ids.next("mrStrata");
        let sub_xml_id = ids.next("subStrata");
        let stem_xml_id = ids.next("stemStrata");
        let stratum_index = first_index + i;
        let stratum_name = format!("S{stratum_index}");

        strata_xml.push_str(&format!(
            "\n      <Stratum characterDefinitionTable=\"{table_xml_id}\" morphologicalRuleOrder=\"unordered\" morphologicalRules=\"{rule_xml_id}\">\n        \
             <Name>{stratum_name}</Name>\n        <MorphologicalRuleDefinitions>\n          \
             <MorphologicalRule id=\"{rule_xml_id}\" requiredPartsOfSpeech=\"{pos_xml_id}\" outputPartOfSpeech=\"{pos_xml_id}\">\n            \
             <Name>strata{i}</Name>\n            <MorphologicalSubrules>\n              <MorphologicalSubrule id=\"{sub_xml_id}\">\n                \
             <MorphologicalInput><PhoneticSequence id=\"{stem_xml_id}\"><OptionalSegmentSequence min=\"1\" max=\"-1\">\
             <SimpleContext naturalClass=\"{nc_any}\" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>\n                \
             <MorphologicalOutput><InsertSegments><PhoneticShape>{ch}</PhoneticShape></InsertSegments>\
             <CopyFromInput index=\"{stem_xml_id}\" /></MorphologicalOutput>\n              </MorphologicalSubrule>\n            \
             </MorphologicalSubrules>\n            <MorphemeId>STRATA{i}</MorphemeId>\n          </MorphologicalRule>\n        \
             </MorphologicalRuleDefinitions>\n        <LexicalEntries></LexicalEntries>\n      </Stratum>",
            table_xml_id = table.xml_id,
            ch = seg.ch,
        ));
        strata.push(ExtraStratum {
            stratum_name,
            rule_xml_id,
        });
    }

    StrataBuild { strata_xml, strata }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn builds_n_extra_strata_each_with_own_rule() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 3, false, false, &mut ids);
        let sb = build(2, 1, "posV", &tb.tables[0], &mut ids);
        assert_eq!(sb.strata.len(), 2);
        assert_eq!(sb.strata[0].stratum_name, "S1");
        assert_eq!(sb.strata[1].stratum_name, "S2");
        assert_eq!(sb.strata_xml.matches("<Stratum ").count(), 2);
    }
}
