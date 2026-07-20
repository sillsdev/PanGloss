//! Assembles a full `<HermitCrabInput>` XML document from a [`crate::recipe::Recipe`] (design doc
//! §2's `render(recipe) -> String`). Generalizes the working precedent this design doc names:
//! `pg-foma/src/gate.rs`'s `sixteen_group_fixture_xml` (a string-built XML fixture
//! `pg_grammar::load` accepts) and `pg-foma/src/morphotactics.rs`'s `FIXTURE_SLOTS`/
//! `FIXTURE_STRATA` -- same hand-verified element shapes, parameterized over a `Recipe` instead of
//! hardcoded.
//!
//! Determinism (design doc §2): `render`/`render_indexed` are pure functions of `recipe`'s own
//! fields -- [`IdMinter`] assigns ids purely by CALL ORDER (fixed for a given recipe, never by
//! `Rng` draws) and every character/segment choice below is knob-driven, not randomized, so the
//! same recipe renders byte-identically every time (`tests/self_check.rs` pins this). [`Rng`] is
//! still seeded and drawn from here (one throwaway draw) so the mechanism is exercised end-to-end;
//! stage 2's scale-sweep builders are expected to be the first REAL consumers of further draws.

use crate::build;
use crate::ids::IdMinter;
use crate::recipe::Recipe;
use crate::rng::Rng;

/// One generated root: its minted `LexicalEntry` xml id, the literal character it spells, and
/// (only meaningful on a multi-table recipe) its voice polarity.
#[derive(Debug, Clone)]
pub struct RootIndex {
    pub entry_xml_id: String,
    pub ch: char,
    pub voice_plus: bool,
}

/// One generated stratum/table pairing, plus everything a gate needs to find its own material
/// back out of the loaded [`pg_grammar::model::Grammar`] by xml id.
#[derive(Debug, Clone)]
pub struct TableIndex {
    pub table_xml_id: String,
    pub stratum_name: String,
    pub roots: Vec<RootIndex>,
    /// Circumfix `MorphologicalRule` xml ids generated on this stratum (empty unless this is
    /// stratum 0 of a `circumfix_count > 0` recipe -- see [`render_indexed`]).
    pub circumfix_mrule_xml_ids: Vec<String>,
}

/// Everything [`render_indexed`] produces: the XML string itself, plus the bookkeeping a gate
/// needs to resolve its own generated material (roots, rules, natural classes) back out of the
/// `Grammar` `pg_grammar::load` returns -- entries/rules are found by their xml id via
/// `Grammar::morphemes[..].xml_key` (`pg-grammar/src/load.rs`'s convention for every
/// morpheme-bearing element), which is exactly the id this module minted for it.
#[derive(Debug, Clone)]
pub struct RenderedGrammar {
    pub xml: String,
    pub tables: Vec<TableIndex>,
    /// Present iff the recipe declared `table_count >= 2` (GATE 1's shape) -- the demo devoicing
    /// rule's own xml id, plus the two feature-based natural class ids it references (design doc
    /// §5's detect-wrong construct; see `build::tables`'s module doc for the full mechanism).
    pub devoice_rule_xml_id: Option<String>,
    pub nc_voiced_xml_id: String,
    pub nc_voiceless_xml_id: String,
}

const POS_XML_ID: &str = "posV";

/// Render `recipe` into a full `<HermitCrabInput>` XML string (design doc §2's `render(recipe) ->
/// String`). Thin wrapper over [`render_indexed`] for callers that only need the XML (e.g.
/// `tests/self_check.rs`'s round-trip check) -- gates that need to resolve their own generated
/// material back out of the loaded `Grammar` should call [`render_indexed`] instead.
pub fn render(recipe: &Recipe) -> String {
    render_indexed(recipe).xml
}

/// [`render`]'s core: builds the same XML string, but also returns the [`RenderedGrammar`] index
/// a gate needs.
pub fn render_indexed(recipe: &Recipe) -> RenderedGrammar {
    let mut ids = IdMinter::new();
    // One throwaway draw: proves the RNG is wired end-to-end (module doc) without letting it
    // affect any correctness-critical choice below (character assignment, voice-polarity
    // alignment, etc. are all knob-driven, never RNG-driven -- GATE 1/GATE 2's own correctness
    // depends on those staying deterministic-by-construction, not just deterministic-by-seed).
    let mut rng = Rng::seeded(recipe.name, recipe.seed);
    let _ = rng.next_u64();

    let table_count = recipe.construct.table_count.max(1);
    let has_circumfix = recipe.construct.circumfix_count > 0;
    // Circumfix affix material needs its own declared characters distinct from root characters
    // (module doc of `build::circumfix`) -- pad the requested segment inventory up so table 0 has
    // enough (roots + 2 affix letters), never SHRINK an explicit larger request.
    let min_inventory_for_table0 = if has_circumfix {
        recipe.scale.entries_per_stratum.max(1) + 2
    } else {
        2
    };
    let segment_inventory = recipe.scale.segment_inventory.max(min_inventory_for_table0).max(2);
    let misaligned = table_count >= 2;

    let tb = build::tables::build(table_count, segment_inventory, misaligned, &mut ids);

    let mut strata_xml = String::new();
    let mut table_indices = Vec::with_capacity(table_count);

    for (ti, table) in tb.tables.iter().enumerate() {
        let n_roots = recipe.scale.entries_per_stratum.max(1);
        // Reserve the LAST two segments of table 0 for circumfix affix material when this
        // stratum carries circumfix rules (module doc); roots draw from the remaining segments,
        // cycling if `n_roots` exceeds what's left.
        let root_pool_len = if ti == 0 && has_circumfix {
            table.segments.len().saturating_sub(2).max(1)
        } else {
            table.segments.len()
        };

        let mut entries_xml = String::new();
        let mut roots = Vec::with_capacity(n_roots);
        for r in 0..n_roots {
            let seg = &table.segments[r % root_pool_len];
            let entry_xml_id = ids.next("entry");
            let allo_xml_id = ids.next("allo");
            let morpheme_id = format!("R{ti}_{r}");
            entries_xml.push_str(&format!(
                "\n          <LexicalEntry id=\"{entry_xml_id}\" partOfSpeech=\"{POS_XML_ID}\">\n            \
                 <Allomorphs><Allomorph id=\"{allo_xml_id}\"><PhoneticShape>{ch}</PhoneticShape></Allomorph></Allomorphs>\n            \
                 <MorphemeId>{morpheme_id}</MorphemeId>\n          </LexicalEntry>",
                ch = seg.ch
            ));
            roots.push(RootIndex {
                entry_xml_id,
                ch: seg.ch,
                voice_plus: seg.voice_plus,
            });
        }

        let mut mrule_defs_xml = String::new();
        let mut templates_block = String::new();
        let mut circumfix_mrule_xml_ids = Vec::new();
        if ti == 0 && has_circumfix {
            let affix_chars: Vec<char> = table.segments[root_pool_len..].iter().map(|s| s.ch).collect();
            let circs = build::circumfix::build_circumfixes(recipe.construct.circumfix_count, POS_XML_ID, &affix_chars, &mut ids);
            let rule_ids: Vec<&str> = circs.iter().map(|c| c.mrule_xml_id.as_str()).collect();
            for c in &circs {
                mrule_defs_xml.push_str(&c.xml);
                circumfix_mrule_xml_ids.push(c.mrule_xml_id.clone());
            }
            let (tmpl_xml, _tmpl_id) =
                build::template::build_single_slot_template(&rule_ids, recipe.construct.template_slot_optional, &mut ids);
            templates_block = format!("\n        <AffixTemplates>{tmpl_xml}\n        </AffixTemplates>");
        }

        // The demo devoicing rule (module doc of `build::tables`) sits on the LAST stratum only
        // (GATE 1's own 2-table shape puts it on table/stratum 1; a `table_count > 2` recipe
        // would still put it on the final one -- stage 2's concern if that ever needs to vary).
        let phon_rules_attr = if ti == table_count - 1 {
            tb.devoice_rule_xml_id.as_deref().unwrap_or("")
        } else {
            ""
        };
        let phon_attr = if phon_rules_attr.is_empty() {
            String::new()
        } else {
            format!(" phonologicalRules=\"{phon_rules_attr}\"")
        };

        let stratum_name = format!("S{ti}");
        strata_xml.push_str(&format!(
            "\n      <Stratum characterDefinitionTable=\"{table_xml_id}\" morphologicalRuleOrder=\"unordered\"{phon_attr}>\n        \
             <Name>{stratum_name}</Name>\n        <MorphologicalRuleDefinitions>{mrule_defs_xml}\n        \
             </MorphologicalRuleDefinitions>{templates_block}\n        <LexicalEntries>{entries_xml}\n        \
             </LexicalEntries>\n      </Stratum>",
            table_xml_id = table.xml_id,
        ));

        table_indices.push(TableIndex {
            table_xml_id: table.xml_id.clone(),
            stratum_name,
            roots,
            circumfix_mrule_xml_ids,
        });
    }

    let prules_block = tb.devoice_rule_xml.as_deref().unwrap_or("");

    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<HermitCrabInput>\n  <Language>\n    <Name>{name}</Name>\n    \
         <PartsOfSpeech>\n      <PartOfSpeech id=\"{POS_XML_ID}\"><Name>v</Name></PartOfSpeech>\n    </PartsOfSpeech>{feature_system}{tables_xml}\n    \
         <NaturalClasses>{natural_classes}\n    </NaturalClasses>\n    <PhonologicalRuleDefinitions>{prules_block}\n    \
         </PhonologicalRuleDefinitions>\n    <Strata>{strata_xml}\n    </Strata>\n  </Language>\n</HermitCrabInput>\n",
        name = recipe.name,
        feature_system = tb.feature_system_xml,
        tables_xml = tb.tables_xml,
        natural_classes = tb.natural_classes_xml,
    );

    RenderedGrammar {
        xml,
        tables: table_indices,
        devoice_rule_xml_id: tb.devoice_rule_xml_id,
        nc_voiced_xml_id: tb.nc_voiced_xml_id,
        nc_voiceless_xml_id: tb.nc_voiceless_xml_id,
    }
}
