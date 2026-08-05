//! Char-def table + shared feature-system builder. Every table shares
//! ONE grammar-global `<PhonologicalFeatureSystem>` (a single binary "voice" feature, `+`/`-`) --
//! `pg_grammar::load`'s pass 1 (`load.rs`'s `load_char_def_table_from_xml`) loads exactly one
//! feature system for the whole document, so per-table feature systems are not a shape the loader
//! supports; what varies PER TABLE is only which characters/feature-values each table's own
//! `<SegmentDefinition>`s declare.
//!
//! ## Why GATE 1's tables are deliberately "out of phase"
//! `pg_foma::replace` used to hardcode `&g.char_tables[0]` for EVERY natural-class resolution
//! (the former `table_of`/`resolve_alpha_tuples` sites, that crate's own module doc's "SILENT
//! MIS-MAP" case), regardless of which table the rule's own stratum
//! actually uses, while `SegAlphabet::token` (`pg-foma/src/replace.rs`) is a PURE function of a
//! `CharDefId`'s raw numeric index (`PUA_BASE + cd.0`) that never looks at which table that id
//! came from. Composing those two facts: a natural class resolved against table 0 yielded
//! table-0-local `CharDefId`s, but the CALLER's alphabet (built, correctly, from whichever table
//! the rule's own stratum actually uses) converted those same raw indices into ITS OWN table's
//! tokens -- silently naming whatever segment happens to sit at that same positional index in the
//! OTHER table. If every table assigned the same feature to the same index (the `build`'s
//! `misaligned = false` case), this mix-up would coincidentally still name the linguistically-right
//! segment -- useless for a detect-wrong gate. `misaligned = true` gives table 1 (and beyond) the
//! OPPOSITE index/feature alignment table 0 uses, so the mix-up provably named the WRONG segment.
//!
//! Both hardcoded sites were fixed (each rewrite rule
//! now resolves against its OWN owning stratum's table, via `pg_foma::replace::owning_table`); this
//! module's `misaligned = true` recipe is UNCHANGED -- it is now what proves the fix is real (a
//! misaligned rule that resolved correctly by ACCIDENT, e.g. because both tables happened to agree,
//! would be a much weaker witness than one that provably names the wrong segment pre-fix and the
//! right one post-fix). `tests/phase_c_multi_table.rs` (formerly a DETECT-WRONG gate, now inverted)
//! derives (and pins) exactly which segment is which.

//! ## Why every segment ALSO gets a globally-unique `featId` value
//! Found empirically while building GATE 2 (circumfix, single table, `misaligned = false`): with
//! ONLY the alternating `featVoice` feature declared, a 3-root single-table recipe put roots 0 and
//! 2 on the SAME feature vector (both `voice+`, at indices 0 and 2) -- and `pg_parse::Morpher::
//! generate_words` then produced IDENTICAL surface forms for both roots (root 2's own "bare" word
//! came back as root 0's own spelling), because some internal indexing keys on the segment's
//! FEATURE STRUCT, not (only) its literal spelling/char-def identity. A second, per-segment-unique
//! feature closes this: no two segments in the whole grammar share a feature vector, so this
//! collision can never recur regardless of `segment_inventory`/`entries_per_stratum`. Mirrors
//! `machine/conformance/languages/fusional-realizational-morphology/grammar.xml`'s own `featId` feature and its own
//! header comment explaining exactly this need (a fully-specified feature system, one unique value
//! per segment, wherever a segment's identity must be recoverable from its feature struct alone).

use crate::ids::IdMinter;

/// One segment: its minted xml id, the literal character HermitCrab sees, and its voice-feature
/// polarity (`true` = `+`).
#[derive(Debug, Clone)]
pub struct SegmentSpec {
    pub xml_id: String,
    pub ch: char,
    pub voice_plus: bool,
}

/// One `<CharacterDefinitionTable>`, plus enough bookkeeping for a caller to build a
/// `pg_foma::replace::SegAlphabet` over exactly this table and pick its own segments by role.
#[derive(Debug, Clone)]
pub struct TableSpec {
    pub xml_id: String,
    pub segments: Vec<SegmentSpec>,
}

impl TableSpec {
    /// The (first) segment of this table whose polarity is `voice_plus`. Panics if this table has
    /// none -- every table `build` produces has both polarities present (alternating), so this
    /// only fires if a caller asks for a table shape this module doesn't build.
    pub fn segment_with_polarity(&self, voice_plus: bool) -> &SegmentSpec {
        self.segments
            .iter()
            .find(|s| s.voice_plus == voice_plus)
            .unwrap_or_else(|| {
                panic!(
                    "table {} has no voice{} segment",
                    self.xml_id,
                    if voice_plus { "+" } else { "-" }
                )
            })
    }
}

/// Everything `build` produces: the shared feature system, the tables themselves, the shared
/// (feature-based, table-agnostic) natural classes every recipe can reference, and -- when
/// `table_count >= 2` -- the demo devoicing rule GATE 1 composes against.
#[derive(Debug, Clone)]
pub struct TablesBuild {
    pub feature_system_xml: String,
    pub tables: Vec<TableSpec>,
    pub tables_xml: String,
    pub natural_classes_xml: String,
    /// XML id of `<FeatureNaturalClass>` matching voice `+` (the shared, feature-based class both
    /// tables' segments participate in -- see module doc).
    pub nc_voiced_xml_id: String,
    /// XML id of `<FeatureNaturalClass>` matching voice `-`.
    pub nc_voiceless_xml_id: String,
    /// The demo unconditional devoicing rule (`ncVoicedAny -> ncVoicelessAny`, no environment),
    /// present iff `table_count >= 2` (module doc; GATE 1's own reason for existing). `None` for
    /// single-table recipes (GATE 2) -- there is nothing to demonstrate the bug with.
    pub devoice_rule_xml: Option<String>,
    pub devoice_rule_xml_id: Option<String>,
    /// Present iff `build` was called with `needs_boundary = true` -- table 0's own
    /// `<BoundaryDefinition>` xml id (`crate::build::compounding`'s compound-seam marker; see
    /// `build`'s own doc for why this must be a boundary, not a plain segment).
    pub boundary_xml_id: Option<String>,
}

const NC_ANY_XML_ID: &str = "ncAny";
const FEAT_VOICE_XML_ID: &str = "featVoice";
const SYM_VOICE_PLUS_XML_ID: &str = "symVoicePlus";
const SYM_VOICE_MINUS_XML_ID: &str = "symVoiceMinus";
// Every segment ALSO gets a globally-unique value on this second feature (module doc addendum
// below `build`'s own doc: found empirically, not anticipated in the original design pass). Only
// `featVoice` is meaningful to any natural class this module declares (`ncVoicedAny`/
// `ncVoicelessAny` only ever pin `featVoice`) -- `featId` exists purely so no two segments in the
// WHOLE grammar ever share an identical feature VECTOR, mirroring `machine/conformance/languages/
// fusional-realizational-morphology/grammar.xml`'s own `featId` ("id") feature and its own comment explaining exactly
// this need (that file's header: a fully-specified feature system, one unique value per segment,
// is required wherever a segment's identity must be recoverable from its feature struct alone).
const FEAT_ID_XML_ID: &str = "featId";

/// Build `table_count` tables (minimum 1), `segment_inventory` segments each (minimum 2, so every
/// table has both voice polarities to draw on), every table's own characters disjoint from every
/// other table's (module doc: needed so GATE 1's cross-table wrongness is unambiguously
/// observable rather than accidentally-correct-by-coincidence). `misaligned`: when `true`, every
/// table after the first starts its index-0 segment at the OPPOSITE voice polarity from table 0
/// (module doc's "out of phase" reasoning) -- callers building a single-table recipe (GATE 2) or a
/// same-phase multi-table sanity check should pass `false`.
///
/// `needs_boundary`: when `true`, table 0 also declares a single `<BoundaryDefinition>` (xml id
/// returned as `TablesBuild::boundary_xml_id`) whose representation is the literal `"+"`
/// character -- `crate::build::compounding`'s own compound-seam marker. Found empirically by
/// reading `machine/conformance/languages/fusional-realizational-morphology/grammar.xml`'s own header comment on its
/// `cBnd` declaration: a compounding rule's `InsertSegments` boundary text must be declared as a
/// `<BoundaryDefinition>`, NOT a plain `<SegmentDefinition>` -- that file's own comment records that
/// declaring it as a plain segment "produced zero parses for every compound word (confirmed by
/// isolated probing)", i.e. this is load-bearing, not stylistic.
///
/// Panics if `table_count * segment_inventory` exceeds the 26 available disjoint ASCII letters --
/// existing recipes stay far under this by construction, to keep the oracle cheap.
pub fn build(
    table_count: usize,
    segment_inventory: usize,
    misaligned: bool,
    needs_boundary: bool,
    ids: &mut IdMinter,
) -> TablesBuild {
    let table_count = table_count.max(1);
    let segment_inventory = segment_inventory.max(2);
    assert!(
        table_count * segment_inventory <= 26,
        "build_tables: only 26 disjoint ASCII letters available ({table_count} tables x {segment_inventory} \
         segments requested) -- stage-1 recipes should stay well under this"
    );

    // `featId`'s symbols (module doc addendum): one per segment this call will ever mint, named
    // purely from a running global counter -- assigned to segments below in the same order, so
    // segment `k` (across every table) always gets `symId{k}`.
    let total_segments = table_count * segment_inventory;
    let mut feat_id_symbols_xml = String::new();
    for k in 0..total_segments {
        feat_id_symbols_xml.push_str(&format!("<Symbol id=\"symId{k}\">{k}</Symbol>"));
    }

    let mut tables = Vec::with_capacity(table_count);
    let mut tables_xml = String::new();
    let mut letter = b'a';
    let mut global_seg_index = 0usize;
    let mut boundary_xml_id: Option<String> = None;
    for t in 0..table_count {
        let table_xml_id = ids.next("tbl");
        // Table 0 always starts index 0 at voice+; later tables start at voice- when misaligned
        // (module doc), or also at voice+ (same phase, coincidentally-correct mix-up) otherwise.
        let start_plus = !(misaligned && t > 0);

        let mut segments = Vec::with_capacity(segment_inventory);
        let mut segment_defs_xml = String::new();
        for i in 0..segment_inventory {
            let seg_xml_id = ids.next("seg");
            let ch = letter as char;
            letter += 1;
            let voice_plus = if i % 2 == 0 { start_plus } else { !start_plus };
            let sym = if voice_plus {
                SYM_VOICE_PLUS_XML_ID
            } else {
                SYM_VOICE_MINUS_XML_ID
            };
            segment_defs_xml.push_str(&format!(
                "\n        <SegmentDefinition id=\"{seg_xml_id}\"><Representations><Representation>{ch}</Representation></Representations>\
                 <FeatureValue feature=\"{FEAT_VOICE_XML_ID}\" symbolValues=\"{sym}\" />\
                 <FeatureValue feature=\"{FEAT_ID_XML_ID}\" symbolValues=\"symId{global_seg_index}\" /></SegmentDefinition>"
            ));
            segments.push(SegmentSpec {
                xml_id: seg_xml_id,
                ch,
                voice_plus,
            });
            global_seg_index += 1;
        }

        let (boundary_block, this_table_boundary_id) = if needs_boundary && t == 0 {
            let bnd_xml_id = ids.next("bnd");
            (
                format!(
                    "\n      <BoundaryDefinitions>\n        <BoundaryDefinition id=\"{bnd_xml_id}\">\
                     <Representations><Representation>+</Representation></Representations></BoundaryDefinition>\n      \
                     </BoundaryDefinitions>"
                ),
                Some(bnd_xml_id),
            )
        } else {
            (String::new(), None)
        };
        if let Some(id) = this_table_boundary_id {
            boundary_xml_id = Some(id);
        }

        tables_xml.push_str(&format!(
            "\n    <CharacterDefinitionTable id=\"{table_xml_id}\">\n      <Name>{table_xml_id}</Name>\n      \
             <SegmentDefinitions>{segment_defs_xml}\n      </SegmentDefinitions>{boundary_block}\n    </CharacterDefinitionTable>"
        ));
        tables.push(TableSpec {
            xml_id: table_xml_id,
            segments,
        });
    }

    let feature_system_xml = format!(
        "\n    <PhonologicalFeatureSystem>\n      <SymbolicFeature id=\"{FEAT_VOICE_XML_ID}\">\n        <Name>voice</Name>\n        \
         <Symbols><Symbol id=\"{SYM_VOICE_PLUS_XML_ID}\">+</Symbol><Symbol id=\"{SYM_VOICE_MINUS_XML_ID}\">-</Symbol></Symbols>\n      \
         </SymbolicFeature>\n      <SymbolicFeature id=\"{FEAT_ID_XML_ID}\">\n        <Name>id</Name>\n        \
         <Symbols>{feat_id_symbols_xml}</Symbols>\n      </SymbolicFeature>\n    </PhonologicalFeatureSystem>"
    );

    let nc_voiced_xml_id = "ncVoicedAny".to_string();
    let nc_voiceless_xml_id = "ncVoicelessAny".to_string();
    let natural_classes_xml = format!(
        "\n      <FeatureNaturalClass id=\"{NC_ANY_XML_ID}\"><Name>Any</Name></FeatureNaturalClass>\n      \
         <FeatureNaturalClass id=\"{nc_voiced_xml_id}\"><Name>VoicedAny</Name>\n        \
         <FeatureValue feature=\"{FEAT_VOICE_XML_ID}\" symbolValues=\"{SYM_VOICE_PLUS_XML_ID}\" />\n      </FeatureNaturalClass>\n      \
         <FeatureNaturalClass id=\"{nc_voiceless_xml_id}\"><Name>VoicelessAny</Name>\n        \
         <FeatureValue feature=\"{FEAT_VOICE_XML_ID}\" symbolValues=\"{SYM_VOICE_MINUS_XML_ID}\" />\n      </FeatureNaturalClass>"
    );

    let (devoice_rule_xml, devoice_rule_xml_id) = if table_count >= 2 {
        let rule_xml_id = ids.next("pruleDevoice");
        let sub_xml_id = ids.next("subDevoice");
        let xml = format!(
            "\n      <PhonologicalRule id=\"{rule_xml_id}\">\n        <Name>devoiceDemo</Name>\n        \
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass=\"{nc_voiced_xml_id}\" /></PhoneticSequence></PhoneticInput>\n        \
             <PhonologicalSubrules>\n          <PhonologicalSubrule id=\"{sub_xml_id}\">\n            \
             <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass=\"{nc_voiceless_xml_id}\" /></PhoneticSequence></PhoneticOutput>\n          \
             </PhonologicalSubrule>\n        </PhonologicalSubrules>\n      </PhonologicalRule>"
        );
        (Some(xml), Some(rule_xml_id))
    } else {
        (None, None)
    };

    TablesBuild {
        feature_system_xml,
        tables,
        tables_xml,
        natural_classes_xml,
        nc_voiced_xml_id,
        nc_voiceless_xml_id,
        devoice_rule_xml,
        devoice_rule_xml_id,
        boundary_xml_id,
    }
}

/// Xml id of the always-present universal wildcard natural class (matches every segment,
/// regardless of table) -- every builder that captures a root span
/// (`MorphologicalInput`'s `OptionalSegmentSequence`) references this, mirroring every existing
/// fixture's own `ncAny`/`Any` convention (e.g. `machine/conformance/languages/fusional-realizational-morphology/
/// grammar.xml`'s `ncAny`).
pub fn nc_any_xml_id() -> &'static str {
    NC_ANY_XML_ID
}

/// Xml id of the per-segment-unique `featId` `SymbolicFeature` `build` always declares (module
/// doc addendum) -- `crate::build::alpha` reuses this existing feature as the phonological
/// feature every alpha variable it declares binds to (module doc: since `ncAny` matches every
/// segment in the table regardless of feature, and each segment's own `featId` value is unique and
/// always set, this gives alpha-tuple resolution exactly `segment_inventory`-many self-agreeing
/// candidates per occurrence with no need for a second, alpha-specific feature system).
pub fn feat_id_xml_id() -> &'static str {
    FEAT_ID_XML_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_tables_share_polarity_per_index() {
        let mut ids = IdMinter::new();
        let tb = build(2, 2, false, false, &mut ids);
        assert_eq!(
            tb.tables[0].segments[0].voice_plus,
            tb.tables[1].segments[0].voice_plus
        );
    }

    #[test]
    fn misaligned_tables_flip_polarity_per_index_after_the_first() {
        let mut ids = IdMinter::new();
        let tb = build(2, 2, true, false, &mut ids);
        assert_ne!(
            tb.tables[0].segments[0].voice_plus,
            tb.tables[1].segments[0].voice_plus
        );
    }

    #[test]
    fn tables_never_share_a_character() {
        let mut ids = IdMinter::new();
        let tb = build(3, 2, true, false, &mut ids);
        let mut seen = std::collections::HashSet::new();
        for t in &tb.tables {
            for s in &t.segments {
                assert!(
                    seen.insert(s.ch),
                    "character {:?} reused across tables",
                    s.ch
                );
            }
        }
    }

    #[test]
    fn single_table_has_no_devoice_demo_rule() {
        let mut ids = IdMinter::new();
        let tb = build(1, 2, false, false, &mut ids);
        assert!(tb.devoice_rule_xml.is_none());
        assert!(tb.devoice_rule_xml_id.is_none());
    }
}
