//! Deep standalone-affix-chain builder (synthetic-stress-grammar-plan.md §2's "AffixTemplate
//! morphotactics" row's sibling case — a STANDALONE, non-template chain of optional rules — and
//! the direct generalization of `docs/fst-plan/p6-deep-truncation-chain-report.md`'s own root
//! cause). Part C of the delanguaging effort: this is the synthetic reproduction of Aweti's
//! "11-rule prefix / 24-rule suffix STANDALONE sets" that fed `pg_foma::emit`'s
//! `build_deriv_chain` under `TextMode::SurfaceProbed` (the legacy, MAINLINE `emit()` strategy,
//! still unchanged today — that report's chain-restriction fix is `TextMode::UnderlyingTokens`
//! (P6/Aweti-templated-emit) ONLY): "EVERY level offers EVERY rule in `rules`; depth =
//! `rules.len()`" — a single epsilon-yielding standalone rule's tag is choosable at any of
//! `rules.len()` levels, independently each time, which is exactly the mechanism the report found
//! behind both Aweti's `PATHCOUNT_OVERFLOW`-scale ambiguity and `apply_up`'s pre-fix non-
//! termination.
//!
//! `count` INDEPENDENT standalone `MorphologicalRule`s, each `Copy(whole input) +
//! InsertSegments(one dedicated suffix character)` — `pg_foma::emit::classify_affix` reads this
//! shape as plain `Role::Suffix` (a leading `Copy` with only a TRAILING insert; contrast
//! `crate::build::circumfix`'s LEADING-and-trailing shape, which classifies as
//! `Role::CircumfixPrefix` and routes through the structural-composite path instead). Declared
//! directly in the stratum's `<MorphologicalRuleDefinitions>` and referenced by the STRATUM's own
//! `morphologicalRules` attribute (mirrors `crate::build::compounding`'s own "stratum-attached,
//! not template-wrapped" convention, found empirically there to be load-bearing: a rule absent
//! from the owning stratum's own list is dead XML `pg_rules` never attempts) — NOT wrapped in an
//! `<AffixTemplate>`, so `pg_foma::emit::emit`'s "Standalone (stratum-attached) derivation rules"
//! loop (that module's own doc, mirrors `trie.rs::run()`) picks up every one of them into its
//! `deriv_suffix` vector, one call to `build_deriv_chain` per zone, `rules.len() == count` levels.
//!
//! Needs `count + 1` distinct segments from its own table (1 root character + one dedicated
//! suffix marker per rule) — [`crate::render::render_indexed`]'s caller is responsible for sizing
//! `segment_inventory` up to at least this (mirrors `crate::build::circumfix`'s own "pad the
//! table" convention). `crate::build::tables::build`'s own 26-ASCII-letter ceiling
//! (`table_count * segment_inventory <= 26`) caps `count` at 25 for a single-table recipe — which
//! comfortably covers Aweti's real real-grammar scale (11/24 rules per zone), so no stage-2 scale
//! knob beyond a single `usize` count is needed here.

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// Everything [`build`] produces: the `count` standalone suffix rules' own XML (already inside a
/// `<MorphologicalRuleDefinitions>` element — the caller splices it in) and their minted xml ids,
/// in document/level order (`rule_xml_ids[i]` == rule `i`, the SAME order `build_deriv_chain`'s
/// `rules` slice will see, since strata list rules in `morphologicalRules` attribute order and
/// `pg_grammar::load` preserves it), plus the single generated root's own bare spelling.
#[derive(Debug, Clone)]
pub struct ChainBuild {
    /// `count` `<MorphologicalRule>` elements (module doc).
    pub mrule_defs_xml: String,
    /// The rules' own minted xml ids, in level order.
    pub rule_xml_ids: Vec<String>,
    /// The single root entry's own required `<PhoneticShape>` text (the BARE, pre-any-rule
    /// spelling — every rule here is optional, so this is itself a valid word too).
    pub root_shape: String,
}

/// Build `count` (`>= 1`) independent standalone suffix rules over `table`'s own segments (needs
/// at least `count + 1` — module doc; panics otherwise, mirroring
/// [`crate::build::circumfix::build_circumfixes`]'s own non-empty-affix-material precondition).
pub fn build(count: usize, pos_xml_id: &str, table: &TableSpec, ids: &mut IdMinter) -> ChainBuild {
    assert!(count >= 1, "build_chain: count must be >= 1");
    let needed = count + 1;
    assert!(
        table.segments.len() >= needed,
        "build_chain: table has {} segments, needs at least {needed} (1 root + count={count} suffix markers)",
        table.segments.len()
    );

    let root_ch = table.segments[0].ch;
    let nc_any = crate::build::tables::nc_any_xml_id();

    let mut mrule_defs_xml = String::new();
    let mut rule_xml_ids = Vec::with_capacity(count);
    for i in 0..count {
        let suffix_ch = table.segments[1 + i].ch;
        let mrule_xml_id = ids.next("mrChain");
        let sub_xml_id = ids.next("subChain");
        let stem_xml_id = ids.next("stemChain");
        let morpheme_id = format!("CHAIN{i}");
        mrule_defs_xml.push_str(&format!(
            "\n          <MorphologicalRule id=\"{mrule_xml_id}\" requiredPartsOfSpeech=\"{pos_xml_id}\" outputPartOfSpeech=\"{pos_xml_id}\">\n            \
             <Name>chain{i}</Name>\n            <MorphologicalSubrules>\n              <MorphologicalSubrule id=\"{sub_xml_id}\">\n                \
             <MorphologicalInput><PhoneticSequence id=\"{stem_xml_id}\"><OptionalSegmentSequence min=\"1\" max=\"-1\">\
             <SimpleContext naturalClass=\"{nc_any}\" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>\n                \
             <MorphologicalOutput>\n                  <CopyFromInput index=\"{stem_xml_id}\" />\n                  \
             <InsertSegments><PhoneticShape>{suffix_ch}</PhoneticShape></InsertSegments>\n                </MorphologicalOutput>\n              \
             </MorphologicalSubrule>\n            </MorphologicalSubrules>\n            <MorphemeId>{morpheme_id}</MorphemeId>\n          \
             </MorphologicalRule>"
        ));
        rule_xml_ids.push(mrule_xml_id);
    }

    ChainBuild {
        mrule_defs_xml,
        rule_xml_ids,
        root_shape: root_ch.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn distinct_rules_get_distinct_suffix_material() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 6, false, false, &mut ids);
        let cb = build(5, "posV", &tb.tables[0], &mut ids);
        assert_eq!(cb.rule_xml_ids.len(), 5);
        // Every rule's suffix character must be distinct from every other's and from the root's.
        let mut chars: Vec<char> = tb.tables[0].segments[0..6].iter().map(|s| s.ch).collect();
        chars.dedup();
        assert_eq!(
            chars.len(),
            6,
            "build_chain draws 6 distinct chars (1 root + 5 suffixes)"
        );
    }

    #[test]
    #[should_panic(expected = "needs at least")]
    fn panics_when_table_too_small() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 3, false, false, &mut ids);
        let _ = build(5, "posV", &tb.tables[0], &mut ids);
    }
}
