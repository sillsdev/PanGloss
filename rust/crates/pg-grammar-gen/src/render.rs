//! Assembles a full `<HermitCrabInput>` XML document from a `crate::recipe::Recipe`.
//! Generalizes the hand-verified element shapes in `pg-foma/src/morphotactics.rs`'s
//! `FIXTURE_SLOTS`/`FIXTURE_STRATA`, parameterized over a `Recipe` instead of hardcoded.
//!
//! Determinism: `render`/`render_indexed` are pure functions of `recipe`'s own
//! fields -- `IdMinter` assigns ids purely by CALL ORDER (fixed for a given recipe, never by
//! `Rng` draws) and every character/segment choice below is knob-driven, not randomized, so the
//! same recipe renders byte-identically every time (`tests/self_check.rs` pins this). `Rng` is
//! still seeded and drawn from here (one throwaway draw) so the mechanism is exercised end-to-end
//! even though nothing here currently consumes further draws.
//!
//! ## One "singular construct" active per recipe
//! Every `ConstructKnobs` field beyond `table_count`/`circumfix_count`/
//! `template_slot_optional` (`gated_subrule_count`,
//! `alpha_var_count`, `extra_strata`, `compounding_rule_count`, `quantifier_bound`,
//! `metathesis_rule_count`, `simultaneous_rule_count`, `rtl_rule_count`) REPLACES stratum 0's
//! generic per-root entries with its own construct-specific entries (mirrors `circumfix`'s own
//! ti==0 special case, taken further). Each construct-specific gate's own recipe sets exactly ONE
//! of these at a time (like GATE 1/GATE 2 recipes each set exactly one knob) -- this
//! module does not attempt to render a composite of several such constructs in one grammar.

use crate::build;
use crate::build::tables::TableSpec;
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
/// back out of the loaded `pg_grammar::model::Grammar` by xml id.
#[derive(Debug, Clone)]
pub struct TableIndex {
    pub table_xml_id: String,
    pub stratum_name: String,
    pub roots: Vec<RootIndex>,
    /// Circumfix `MorphologicalRule` xml ids generated on this stratum (empty unless this is
    /// stratum 0 of a `circumfix_count > 0` recipe -- see `render_indexed`).
    pub circumfix_mrule_xml_ids: Vec<String>,
}

/// Bookkeeping for a `gated_subrule_count > 0` recipe.
#[derive(Debug, Clone)]
pub struct GatingIndex {
    /// The `k` gated rules' own xml ids, in `j` order.
    pub rule_xml_ids: Vec<String>,
    /// The `2^k` entries' own xml ids, in bit-pattern order (entry `i` realizes gating key `i`;
    /// `crate::build::gating::bit_set` derives the same bit convention).
    pub entry_xml_ids: Vec<String>,
}

/// Bookkeeping for an `alpha_var_count > 0` recipe.
#[derive(Debug, Clone)]
pub struct AlphaIndex {
    /// The `var_count` independent alpha rules' own xml ids (`crate::build::alpha`'s own doc: one
    /// rule per var, each targeting its own dedicated marker position).
    pub rule_xml_ids: Vec<String>,
    /// The single root entry's own xml id.
    pub root_entry_xml_id: String,
    /// The root entry's own `<PhoneticShape>` text -- also its expected, UNCHANGED post-synthesis
    /// surface (`crate::build::alpha::AlphaBuild::root_shape`'s own doc).
    pub root_shape: String,
}

/// One additional stratum beyond the base scaffold.
#[derive(Debug, Clone)]
pub struct ExtraStratumIndex {
    pub stratum_name: String,
    pub rule_xml_id: String,
}

/// Bookkeeping for a `compounding_rule_count > 0` recipe.
#[derive(Debug, Clone)]
pub struct CompoundingIndex {
    pub rule_xml_ids: Vec<String>,
    pub head_entry_xml_ids: Vec<String>,
    pub nonhead_entry_xml_ids: Vec<String>,
}

/// Bookkeeping for a `quantifier_bound.is_some()` recipe.
#[derive(Debug, Clone)]
pub struct QuantifierIndex {
    pub rule_xml_id: String,
}

/// Bookkeeping for a `chain_rule_count > 0` recipe (Part C, delanguaging: deep standalone-affix
/// chain — `crate::build::chain`'s own module doc).
#[derive(Debug, Clone)]
pub struct ChainIndex {
    /// The `count` standalone suffix rules' own xml ids, in level order.
    pub rule_xml_ids: Vec<String>,
    /// The single generated root entry's own xml id.
    pub root_entry_xml_id: String,
    /// The root entry's own bare (pre-any-rule) `<PhoneticShape>` text.
    pub root_shape: String,
}

/// Bookkeeping for a `metathesis_rule_count > 0` recipe.
#[derive(Debug, Clone)]
pub struct MetathesisIndex {
    pub rule_xml_ids: Vec<String>,
}

/// Bookkeeping for a `simultaneous_rule_count > 0` recipe.
#[derive(Debug, Clone)]
pub struct SimultaneousIndex {
    pub rule_xml_ids: Vec<String>,
}

/// Bookkeeping for an `rtl_rule_count > 0` recipe.
#[derive(Debug, Clone)]
pub struct RightToLeftIndex {
    pub rule_xml_ids: Vec<String>,
}

/// Everything `render_indexed` produces: the XML string itself, plus the bookkeeping a gate
/// needs to resolve its own generated material (roots, rules, natural classes) back out of the
/// `Grammar` `pg_grammar::load` returns -- entries/rules are found by their xml id via
/// `Grammar::morphemes[..].xml_key` (`pg-grammar/src/load.rs`'s convention for every
/// morpheme-bearing element), which is exactly the id this module minted for it.
#[derive(Debug, Clone)]
pub struct RenderedGrammar {
    pub xml: String,
    pub tables: Vec<TableIndex>,
    /// Present iff the recipe declared `table_count >= 2` (GATE 1's shape) -- the demo devoicing
    /// rule's own xml id, plus the two feature-based natural class ids it references (a
    /// detect-wrong construct; see `build::tables`'s module doc for the full mechanism).
    pub devoice_rule_xml_id: Option<String>,
    pub nc_voiced_xml_id: String,
    pub nc_voiceless_xml_id: String,
    pub gating: Option<GatingIndex>,
    pub alpha: Option<AlphaIndex>,
    pub extra_strata: Vec<ExtraStratumIndex>,
    pub compounding: Option<CompoundingIndex>,
    pub quantifier: Option<QuantifierIndex>,
    pub metathesis: Option<MetathesisIndex>,
    pub simultaneous: Option<SimultaneousIndex>,
    pub right_to_left: Option<RightToLeftIndex>,
    pub chain: Option<ChainIndex>,
}

const POS_XML_ID: &str = "posV";

/// A 2-segment slice of `table` starting at `start`, wrapped as a standalone `TableSpec`; lets each rule-building module mint `N` independent instances from one shared table via private pairs.
fn sub_table_pair(table: &TableSpec, start: usize) -> TableSpec {
    TableSpec {
        xml_id: table.xml_id.clone(),
        segments: table.segments[start..start + 2].to_vec(),
    }
}

/// Mint one `<LexicalEntry>` with a single-allomorph `shape` spelling, returning `(xml, entry_xml_id)`; shared by every construct that just needs some valid, loadable root.
fn one_entry_xml(ids: &mut IdMinter, pos: &str, shape: &str, morph_id: &str) -> (String, String) {
    let entry_xml_id = ids.next("entry");
    let allo_xml_id = ids.next("allo");
    let xml = format!(
        "\n          <LexicalEntry id=\"{entry_xml_id}\" partOfSpeech=\"{pos}\">\n            \
         <Allomorphs><Allomorph id=\"{allo_xml_id}\"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs>\n            \
         <MorphemeId>{morph_id}</MorphemeId>\n          </LexicalEntry>"
    );
    (xml, entry_xml_id)
}

/// Render `recipe` into a full `<HermitCrabInput>` XML string. Thin wrapper over
/// `render_indexed` for callers that only need the XML (e.g.
/// `tests/self_check.rs`'s round-trip check) -- gates that need to resolve their own generated
/// material back out of the loaded `Grammar` should call `render_indexed` instead.
pub fn render(recipe: &Recipe) -> String {
    render_indexed(recipe).xml
}

/// `render`'s core: builds the same XML string, but also returns the `RenderedGrammar` index
/// a gate needs.
pub fn render_indexed(recipe: &Recipe) -> RenderedGrammar {
    let mut ids = IdMinter::new();
    // One throwaway draw proves the RNG is wired end-to-end without affecting any correctness-critical choice below, which all stay knob-driven, never RNG-driven.
    let mut rng = Rng::seeded(recipe.name, recipe.seed);
    let _ = rng.next_u64();

    let c = &recipe.construct;
    let table_count = c.table_count.max(1);
    let has_circumfix = c.circumfix_count > 0;
    let has_gating = c.gated_subrule_count > 0;
    let has_alpha = c.alpha_var_count > 0;
    let has_extra_strata = c.extra_strata > 0;
    let has_compounding = c.compounding_rule_count > 0;
    let has_quantifier = c.quantifier_bound.is_some();
    let has_metathesis = c.metathesis_rule_count > 0;
    let has_simultaneous = c.simultaneous_rule_count > 0;
    let has_rtl = c.rtl_rule_count > 0;
    let has_chain = c.chain_rule_count > 0;

    // Circumfix affix material needs characters distinct from root characters, so pad table 0's inventory up (never shrink an explicit larger request); each "replaces stratum 0" construct folds its own requirement into the same max().
    let mut min_inventory_for_table0 = if has_circumfix {
        recipe.scale.entries_per_stratum.max(1) + 2
    } else {
        2
    };
    if has_gating {
        min_inventory_for_table0 = min_inventory_for_table0.max(1 + 2 * c.gated_subrule_count);
    }
    if has_alpha {
        min_inventory_for_table0 = min_inventory_for_table0
            .max(c.alpha_var_count)
            .max(c.alpha_class_size);
    }
    if has_metathesis {
        min_inventory_for_table0 = min_inventory_for_table0.max(2 * c.metathesis_rule_count);
    }
    if has_simultaneous {
        min_inventory_for_table0 = min_inventory_for_table0.max(2 * c.simultaneous_rule_count);
    }
    if has_rtl {
        min_inventory_for_table0 = min_inventory_for_table0.max(2 * c.rtl_rule_count);
    }
    if has_compounding {
        min_inventory_for_table0 = min_inventory_for_table0.max(2 * c.compounding_rule_count);
    }
    if has_extra_strata {
        min_inventory_for_table0 = min_inventory_for_table0.max(c.extra_strata);
    }
    if has_chain {
        min_inventory_for_table0 = min_inventory_for_table0.max(c.chain_rule_count + 1);
    }

    let segment_inventory = recipe
        .scale
        .segment_inventory
        .max(min_inventory_for_table0)
        .max(2);
    let misaligned = table_count >= 2;
    let needs_boundary = has_compounding;

    let tb = build::tables::build(
        table_count,
        segment_inventory,
        misaligned,
        needs_boundary,
        &mut ids,
    );

    // "Replaces stratum 0" construct builders: built once, before the per-table loop, since every one consumes table 0's own segments.
    let gating_build = has_gating
        .then(|| build::gating::build(c.gated_subrule_count, POS_XML_ID, &tb.tables[0], &mut ids));
    let alpha_build =
        has_alpha.then(|| build::alpha::build(c.alpha_var_count, &tb.tables[0], &mut ids));
    let quantifier_build =
        has_quantifier.then(|| build::quantifier::build(&tb.tables[0], &mut ids));
    let chain_build = has_chain
        .then(|| build::chain::build(c.chain_rule_count, POS_XML_ID, &tb.tables[0], &mut ids));
    let metathesis_builds: Vec<_> = (0..c.metathesis_rule_count)
        .map(|n| build::metathesis::build(&sub_table_pair(&tb.tables[0], 2 * n), &mut ids))
        .collect();
    let simultaneous_builds: Vec<_> = (0..c.simultaneous_rule_count)
        .map(|n| build::simultaneous::build(&sub_table_pair(&tb.tables[0], 2 * n), &mut ids))
        .collect();
    let rtl_builds: Vec<_> = (0..c.rtl_rule_count)
        .map(|n| build::right_to_left::build(&sub_table_pair(&tb.tables[0], 2 * n), &mut ids))
        .collect();
    let compounding_builds: Vec<_> = (0..c.compounding_rule_count)
        .map(|n| {
            build::compounding::build(
                POS_XML_ID,
                tb.boundary_xml_id.as_deref().unwrap_or(""),
                &sub_table_pair(&tb.tables[0], 2 * n),
                &mut ids,
            )
        })
        .collect();

    let replaces_stratum0_entries = has_gating
        || has_alpha
        || has_quantifier
        || has_metathesis
        || has_simultaneous
        || has_rtl
        || has_compounding
        || has_chain;

    let mut strata_xml = String::new();
    let mut table_indices = Vec::with_capacity(table_count);
    let mut alpha_root_entry_id: Option<String> = None;
    let mut chain_root_entry_id: Option<String> = None;

    for (ti, table) in tb.tables.iter().enumerate() {
        let mut entries_xml = String::new();
        let mut roots = Vec::new();

        if ti == 0 && replaces_stratum0_entries {
            if let Some(gb) = &gating_build {
                entries_xml.push_str(&gb.entries_xml);
            } else if let Some(ab) = &alpha_build {
                let (xml, entry_id) = one_entry_xml(&mut ids, POS_XML_ID, &ab.root_shape, "ALPHA");
                entries_xml.push_str(&xml);
                alpha_root_entry_id = Some(entry_id);
            } else if let Some(qb) = &quantifier_build {
                let (xml, _id) = one_entry_xml(&mut ids, POS_XML_ID, &qb.root_shape, "QUANT");
                entries_xml.push_str(&xml);
            } else if let Some(cb) = &chain_build {
                let (xml, entry_id) =
                    one_entry_xml(&mut ids, POS_XML_ID, &cb.root_shape, "CHAINROOT");
                entries_xml.push_str(&xml);
                chain_root_entry_id = Some(entry_id);
            } else if !metathesis_builds.is_empty() {
                for (n, mb) in metathesis_builds.iter().enumerate() {
                    let (xml, _id) =
                        one_entry_xml(&mut ids, POS_XML_ID, &mb.root_shape, &format!("META{n}"));
                    entries_xml.push_str(&xml);
                }
            } else if !simultaneous_builds.is_empty() {
                for (n, sb) in simultaneous_builds.iter().enumerate() {
                    let (xml, _id) =
                        one_entry_xml(&mut ids, POS_XML_ID, &sb.root_shape, &format!("SIMUL{n}"));
                    entries_xml.push_str(&xml);
                }
            } else if !rtl_builds.is_empty() {
                for (n, rb) in rtl_builds.iter().enumerate() {
                    let (xml, _id) =
                        one_entry_xml(&mut ids, POS_XML_ID, &rb.root_shape, &format!("RTL{n}"));
                    entries_xml.push_str(&xml);
                }
            } else if !compounding_builds.is_empty() {
                for cb in &compounding_builds {
                    entries_xml.push_str(&cb.head_entry_xml);
                    entries_xml.push_str(&cb.nonhead_entry_xml);
                }
            }
        } else {
            let n_roots = recipe.scale.entries_per_stratum.max(1);
            // Reserve the last two segments of table 0 for circumfix affix material; roots draw from the rest, cycling if `n_roots` exceeds what's left.
            let root_pool_len = if ti == 0 && has_circumfix {
                table.segments.len().saturating_sub(2).max(1)
            } else {
                table.segments.len()
            };

            roots = Vec::with_capacity(n_roots);
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
        }

        let mut mrule_defs_xml = String::new();
        let mut templates_block = String::new();
        let mut circumfix_mrule_xml_ids = Vec::new();
        if ti == 0 && has_circumfix {
            let root_pool_len = table.segments.len().saturating_sub(2).max(1);
            let affix_chars: Vec<char> = table.segments[root_pool_len..]
                .iter()
                .map(|s| s.ch)
                .collect();
            let circs = build::circumfix::build_circumfixes(
                recipe.construct.circumfix_count,
                POS_XML_ID,
                &affix_chars,
                &mut ids,
            );
            let rule_ids: Vec<&str> = circs.iter().map(|c| c.mrule_xml_id.as_str()).collect();
            for circ in &circs {
                mrule_defs_xml.push_str(&circ.xml);
                circumfix_mrule_xml_ids.push(circ.mrule_xml_id.clone());
            }
            let (tmpl_xml, _tmpl_id) = build::template::build_single_slot_template(
                &rule_ids,
                recipe.construct.template_slot_optional,
                &mut ids,
            );
            templates_block =
                format!("\n        <AffixTemplates>{tmpl_xml}\n        </AffixTemplates>");
        }
        if ti == 0 && !compounding_builds.is_empty() {
            for cb in &compounding_builds {
                mrule_defs_xml.push_str(&cb.rule_xml);
            }
        }
        if ti == 0 {
            if let Some(cb) = &chain_build {
                mrule_defs_xml.push_str(&cb.mrule_defs_xml);
            }
        }

        // The demo devoicing rule always sits on the last stratum; every construct-specific phonological rule targets stratum 0, merged into the same attribute (harmless since the two are mutually exclusive per recipe).
        let mut phon_ids: Vec<&str> = Vec::new();
        if ti == table_count - 1 {
            if let Some(id) = tb.devoice_rule_xml_id.as_deref() {
                phon_ids.push(id);
            }
        }
        if ti == 0 {
            if let Some(gb) = &gating_build {
                phon_ids.extend(gb.rule_xml_ids.iter().map(String::as_str));
            }
            if let Some(ab) = &alpha_build {
                phon_ids.extend(ab.rule_xml_ids.iter().map(String::as_str));
            }
            if let Some(qb) = &quantifier_build {
                phon_ids.push(qb.rule_xml_id.as_str());
            }
            for mb in &metathesis_builds {
                phon_ids.push(mb.rule_xml_id.as_str());
            }
            for sb in &simultaneous_builds {
                phon_ids.push(sb.rule_xml_id.as_str());
            }
            for rb in &rtl_builds {
                phon_ids.push(rb.rule_xml_id.as_str());
            }
        }
        let phon_rules_attr = if phon_ids.is_empty() {
            String::new()
        } else {
            format!(" phonologicalRules=\"{}\"", phon_ids.join(" "))
        };

        // `CompoundingRule`s are stratum-attached obligatory rules, never wrapped in an `<AffixTemplate>`: a rule not listed in the owning stratum's `morphologicalRules` id-list is dead XML the synthesis cascade never attempts.
        let mut morph_ids: Vec<&str> = Vec::new();
        if ti == 0 {
            for cb in &compounding_builds {
                morph_ids.push(cb.rule_xml_id.as_str());
            }
            if let Some(cb) = &chain_build {
                morph_ids.extend(cb.rule_xml_ids.iter().map(String::as_str));
            }
        }
        let morph_rules_attr = if morph_ids.is_empty() {
            String::new()
        } else {
            format!(" morphologicalRules=\"{}\"", morph_ids.join(" "))
        };

        let stratum_name = format!("S{ti}");
        strata_xml.push_str(&format!(
            "\n      <Stratum characterDefinitionTable=\"{table_xml_id}\" morphologicalRuleOrder=\"unordered\"{phon_rules_attr}{morph_rules_attr}>\n        \
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

    // Stratum-depth: additional strata reusing table 0, appended after the base per-table loop's own strata (document order = cascade order).
    let extra_strata_build = has_extra_strata.then(|| {
        build::strata::build(
            c.extra_strata,
            table_count,
            POS_XML_ID,
            &tb.tables[0],
            &mut ids,
        )
    });
    if let Some(sb) = &extra_strata_build {
        strata_xml.push_str(&sb.strata_xml);
    }

    let mut prules_block = String::new();
    if let Some(x) = tb.devoice_rule_xml.as_deref() {
        prules_block.push_str(x);
    }
    if let Some(gb) = &gating_build {
        prules_block.push_str(&gb.prules_xml);
    }
    if let Some(ab) = &alpha_build {
        prules_block.push_str(&ab.prules_xml);
    }
    if let Some(qb) = &quantifier_build {
        prules_block.push_str(&qb.prule_xml);
    }
    for mb in &metathesis_builds {
        prules_block.push_str(&mb.prule_xml);
    }
    for sb in &simultaneous_builds {
        prules_block.push_str(&sb.prule_xml);
    }
    for rb in &rtl_builds {
        prules_block.push_str(&rb.prule_xml);
    }

    let mpr_features_block = gating_build
        .as_ref()
        .map(|gb| gb.mpr_features_xml.clone())
        .unwrap_or_default();

    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<HermitCrabInput>\n  <Language>\n    <Name>{name}</Name>\n    \
         <PartsOfSpeech>\n      <PartOfSpeech id=\"{POS_XML_ID}\"><Name>v</Name></PartOfSpeech>\n    </PartsOfSpeech>{mpr_features_block}{feature_system}{tables_xml}\n    \
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
        gating: gating_build.map(|gb| GatingIndex {
            rule_xml_ids: gb.rule_xml_ids,
            entry_xml_ids: gb.entry_xml_ids,
        }),
        alpha: alpha_build.map(|ab| AlphaIndex {
            rule_xml_ids: ab.rule_xml_ids,
            root_entry_xml_id: alpha_root_entry_id
                .expect("alpha_build implies the ti==0 branch minted a root entry"),
            root_shape: ab.root_shape,
        }),
        extra_strata: extra_strata_build
            .map(|sb| {
                sb.strata
                    .into_iter()
                    .map(|s| ExtraStratumIndex {
                        stratum_name: s.stratum_name,
                        rule_xml_id: s.rule_xml_id,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        compounding: (!compounding_builds.is_empty()).then(|| CompoundingIndex {
            rule_xml_ids: compounding_builds
                .iter()
                .map(|cb| cb.rule_xml_id.clone())
                .collect(),
            head_entry_xml_ids: compounding_builds
                .iter()
                .map(|cb| cb.head_entry_xml_id.clone())
                .collect(),
            nonhead_entry_xml_ids: compounding_builds
                .iter()
                .map(|cb| cb.nonhead_entry_xml_id.clone())
                .collect(),
        }),
        quantifier: quantifier_build.map(|qb| QuantifierIndex {
            rule_xml_id: qb.rule_xml_id,
        }),
        metathesis: (!metathesis_builds.is_empty()).then(|| MetathesisIndex {
            rule_xml_ids: metathesis_builds
                .iter()
                .map(|mb| mb.rule_xml_id.clone())
                .collect(),
        }),
        simultaneous: (!simultaneous_builds.is_empty()).then(|| SimultaneousIndex {
            rule_xml_ids: simultaneous_builds
                .iter()
                .map(|sb| sb.rule_xml_id.clone())
                .collect(),
        }),
        right_to_left: (!rtl_builds.is_empty()).then(|| RightToLeftIndex {
            rule_xml_ids: rtl_builds.iter().map(|rb| rb.rule_xml_id.clone()).collect(),
        }),
        chain: chain_build.map(|cb| ChainIndex {
            rule_xml_ids: cb.rule_xml_ids,
            root_entry_xml_id: chain_root_entry_id
                .expect("chain_build implies the ti==0 branch minted a root entry"),
            root_shape: cb.root_shape,
        }),
    }
}
