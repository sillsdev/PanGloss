//! Full-grammar loader (plan §5.5): parse a HermitCrab `*-hc.xml` document into the frozen
//! runtime tables of [`crate::model::Grammar`]. A faithful port of the C#
//! `XmlLanguageLoader.cs` object-model construction.
//!
//! ## Two passes, one file
//! Pass 1 reuses [`crate::load_char_def_table_from_xml`] to build the phonological census
//! (`PhonologicalFeatureSystem` + every `CharacterDefinitionTable`). Pass 2 builds a small
//! read-only DOM of the active `<Language>` element and ports each `Load*` method almost
//! line-for-line against it — the C# is irreducibly DOM-style (`.Element`/`.Elements`/
//! `SingleOrDefault`/recursion in `LoadFeatureStruct`), so a DOM keeps the port faithful and
//! the parity surface small. Both passes use only `quick_xml`.
//!
//! ## Ordering (parity-critical)
//! Strata in document order; a stratum's phonological/morphological rules in the order of its
//! `phonologicalRules`/`morphologicalRules` id-list attributes (ids not found are silently
//! skipped, as C# `TryGetValue`); subrules, template slots, and allomorphs in document order.
//!
//! ## v1 lint surface (plan §8 layer 6; see [`crate::model`] docs)
//! `FootFeatures`, `StemName`, `Family` (with entries), `MetathesisRule`, `RealizationalRule`,
//! `MorphemeCoOccurrenceRule`, `AllomorphCoOccurrenceRule`, `AlphaVariable` in an allomorph
//! environment, >=64 symbols in a symbolic feature, and >64 MPR features all lint
//! [`GrammarError::Unsupported`] → managed fallback. The three reference grammars contain none
//! of these, so a correct loader loads all three without an `Unsupported` error.

use std::fmt::Write as _;

use hashbrown::{HashMap, HashSet};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use hc_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, Interner, SymbolBits};
use hc_shape::NodeKind;

use crate::chardef::{CharDefId, CharDefTable};
use crate::featsys::{FlatIndex, PhonFeatureSystem};
use crate::model::*;
use crate::segment::{segment, segment_with_patterns};
use crate::{load_char_def_table_from_xml, GrammarError, GrammarPhonology};

// =============================================================================================
// Minimal read-only DOM built from quick_xml events (mirrors the XElement subset the C# uses).
// =============================================================================================

#[derive(Debug)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    /// Concatenated direct text (`XElement.Value` for the leaf-text elements HC uses).
    text: String,
    children: Vec<Node>,
}

impl Node {
    /// `(string)elem.Attribute(name)` — `None` if absent.
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// The attribute value only if present and non-empty (mirrors `!string.IsNullOrEmpty`).
    fn attr_ne(&self, name: &str) -> Option<&str> {
        self.attr(name).filter(|s| !s.is_empty())
    }

    /// Port of `XmlLanguageLoader.IsActive`.
    fn is_active(&self) -> bool {
        self.attr("isActive").is_none_or(|v| v == "yes")
    }

    /// `elem.Element(tag)` — the first child element named `tag`.
    fn child(&self, tag: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.tag == tag)
    }

    /// `(string)elem.Element(tag)` — that child's text, or `None` if the element is absent.
    fn text_of(&self, tag: &str) -> Option<&str> {
        self.child(tag).map(|c| c.text.as_str())
    }

    /// `elem.Elements(tag)` — all child elements named `tag`, document order.
    fn elems<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    /// `elem.Elements(outer).Elements(inner)`.
    fn elems2<'a>(&'a self, outer: &'a str, inner: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children
            .iter()
            .filter(move |c| c.tag == outer)
            .flat_map(|c| c.children.iter())
            .filter(move |c| c.tag == inner)
    }

    /// `elem.Elements(outer).Elements()` — every element child of every `outer` block.
    fn under<'a>(&'a self, outer: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children
            .iter()
            .filter(move |c| c.tag == outer)
            .flat_map(|c| c.children.iter())
    }
}

fn xml_err(e: impl std::fmt::Display) -> GrammarError {
    GrammarError::Xml(e.to_string())
}

fn start_node(e: &BytesStart) -> Result<Node, GrammarError> {
    let tag = String::from_utf8_lossy(e.local_name().into_inner()).into_owned();
    let mut attrs = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(xml_err)?;
        let key = String::from_utf8_lossy(a.key.local_name().into_inner()).into_owned();
        let val = a.unescape_value().map_err(xml_err)?.into_owned();
        attrs.push((key, val));
    }
    Ok(Node {
        tag,
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}

/// Parse the whole document into a synthetic root node (its children are the top-level
/// elements, e.g. `HermitCrabInput`).
fn parse_document(xml: &str) -> Result<Node, GrammarError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<Node> = vec![Node {
        tag: String::new(),
        attrs: Vec::new(),
        text: String::new(),
        children: Vec::new(),
    }];

    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) => stack.push(start_node(&e)?),
            Event::Empty(e) => {
                let node = start_node(&e)?;
                stack.last_mut().unwrap().children.push(node);
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(xml_err)?;
                stack.last_mut().unwrap().text.push_str(&s);
            }
            Event::CData(t) => {
                stack
                    .last_mut()
                    .unwrap()
                    .text
                    .push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Event::End(_) => {
                // The synthetic root never receives an End, so the stack always has a parent.
                let node = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(node);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(stack.pop().unwrap())
}

// =============================================================================================
// Read-only resolvers (`Ro`) and mutable accumulators (`Acc`).
// =============================================================================================

/// Read-only resolution tables used while building patterns and feature structs. Held by
/// reference so `Acc` (the mutable side) can be borrowed independently.
struct Ro<'a> {
    phon: &'a GrammarPhonology,
    syn: &'a SynFeatureSystem,
    /// natural-class XML id → dense id.
    natclass: &'a HashMap<String, NatClassId>,
    /// Full natural-class definitions, document order (dense-id-indexed: `NatClassId(i)` is
    /// `natural_class_defs[i]`). Only [`load_root_allomorph`] needs the definitions themselves
    /// (not just the id) — finding N3's `[ClassName]` pattern-language lookup is by `<Name>` text,
    /// a full linear scan over this slice (mirrors C#'s per-table `_naturalClassLookup`, which is
    /// likewise populated from every natural class regardless of which table is being segmented).
    natural_class_defs: &'a [NaturalClass],
    /// character-definition XML id → (owning table, per-table id).
    chardef: &'a HashMap<String, (TableId, CharDefId)>,
    /// character-definition-table XML id → dense id.
    table: &'a HashMap<String, TableId>,
    /// MPR feature XML id → bit position.
    mpr: &'a HashMap<String, MprId>,
    /// `<StemName>` XML id → dense id (W5). Fixed before the strata loop starts (`<StemNames>`
    /// loads before `<Strata>`, `XmlLanguageLoader.cs:280-281`), so a plain `Ro` field — unlike
    /// `families`' entry-membership half, which mutates during the strata loop and lives on
    /// [`Acc`] instead.
    stem_names: &'a HashMap<String, StemNameId>,
    /// `<Family>` XML id → dense id (W5). Fixed before the strata loop the same way; see
    /// [`Acc::families`] for where the mutable `entries` list this indexes into lives.
    families: &'a HashMap<String, FamilyId>,
}

/// Everything the loader appends to as it walks the strata.
struct Acc {
    fs_interner: Interner<FeatureStruct>,
    mrules: Vec<MorphRuleDef>,
    morphemes: Vec<MorphemeInfo>,
    allomorph_owners: Vec<AllomorphOwner>,
    templates: Vec<AffixTemplateDef>,
    entries: Vec<LexEntryDef>,
    /// `<Family>` definitions (W5), pre-seeded (name + empty `entries`) before the strata loop
    /// starts; `try_load_lex_entry` pushes each successfully-loaded entry's [`LexEntryId`] onto
    /// its family's `entries` as it goes (C# `family.Entries.Add(entry)`,
    /// `XmlLanguageLoader.cs:463-465` — see that function's doc for the one documented C# edge
    /// case this does not reproduce).
    families: Vec<FamilyDef>,
    /// XML `id` → [`AllomorphId`] (C#'s `_allomorphs` dict: `<Allomorph id="...">` and
    /// `<MorphologicalSubrule id="...">` share one namespace there, so this does too). Consumed
    /// only by the post-strata `<AllomorphCoOccurrenceRule>` pass in `load()` — mirrors C#'s own
    /// `primaryAllomorph`/`otherAllomorphs` IDREF resolution, which runs after every stratum is
    /// loaded (`XmlLanguageLoader.LoadLanguage`'s `Strata` loop precedes its
    /// `AllomorphCoOccurrenceRules` loop).
    allomorph_xml_index: HashMap<String, AllomorphId>,
}

/// Which morphological input list a captured LHS part belongs to (drives the [`PartRef`] kind).
#[derive(Copy, Clone)]
enum PartKind {
    Input,
    Head,
    NonHead,
}

fn mk_part_ref(kind: PartKind, idx: u16) -> PartRef {
    match kind {
        PartKind::Input => PartRef::Input(idx),
        PartKind::Head => PartRef::Head(idx),
        PartKind::NonHead => PartRef::NonHead(idx),
    }
}

/// A load error that should *drop the current allomorph* (C# error-handler path) rather than
/// abort the whole load. `Unsupported` is never a drop — it must propagate to trigger managed
/// fallback (plan §8 layer 6).
fn is_droppable(e: &GrammarError) -> bool {
    !matches!(e, GrammarError::Unsupported(_))
}

/// C# `XmlLanguageLoader.GetMorphCoOccurrenceAdjacency` (XmlLanguageLoader.cs:137-157): unknown/
/// absent values default to `Anywhere` (also the DTD's own default).
fn load_co_occurrence_adjacency(v: Option<&str>) -> CoOccurrenceAdjacency {
    match v {
        Some("somewhereToLeft") => CoOccurrenceAdjacency::SomewhereToLeft,
        Some("somewhereToRight") => CoOccurrenceAdjacency::SomewhereToRight,
        Some("adjacentToLeft") => CoOccurrenceAdjacency::AdjacentToLeft,
        Some("adjacentToRight") => CoOccurrenceAdjacency::AdjacentToRight,
        _ => CoOccurrenceAdjacency::Anywhere,
    }
}

fn parse_bool(v: Option<&str>, default: bool) -> bool {
    match v {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

// =============================================================================================
// Entry point.
// =============================================================================================

/// Load a full HermitCrab XML grammar into the frozen [`Grammar`] runtime tables.
///
/// Faithful port of `XmlLanguageLoader.Load`. Constructs outside the v1 surface lint
/// [`GrammarError::Unsupported`]; malformed references (unknown feature/symbol/natural-class ids)
/// surface [`GrammarError::Semantic`]; XML errors surface [`GrammarError::Xml`].
pub fn load(xml: &str) -> Result<Grammar, GrammarError> {
    // Pass 1: phonological feature system + character-definition tables.
    let phon = load_char_def_table_from_xml(xml)?;

    // Pass 2: DOM of the active <Language>.
    let root = parse_document(xml)?;
    let lang = root
        .child("HermitCrabInput")
        .and_then(|hc| hc.elems("Language").find(|l| l.is_active()))
        .ok_or_else(|| GrammarError::Xml("no active <Language> element".into()))?;

    // --- top-level lints (constructs the reference grammars never contain) --------------------
    // FootFeatures (F1, HYBRID_FST_RUST_PLAN.md §7.1 item 4): no longer hard-linted — loaded by
    // `build_syn_features` below, mirroring HeadFeatures exactly (see that function + the
    // `SynFeatureSystem::foot` doc for the confirmed-against-C# shared-namespace behavior).
    // StemName / Family / RealizationalRule (plan W5): no longer hard-linted — loaded below
    // (StemNames/Families passes) and inline in `load_stratum`/`try_load_lex_entry`.
    // MorphemeCoOccurrenceRule / AllomorphCoOccurrenceRule (plan W6): no longer hard-linted here —
    // parsed by the post-strata pass near the end of this function (their `primaryMorpheme`/
    // `primaryAllomorph`/`otherMorphemes`/`otherAllomorphs` IDREFs resolve against the
    // `acc.morphemes`/`acc.allomorph_xml_index` registries the strata loop populates, mirroring
    // C#'s `XmlLanguageLoader.LoadLanguage` placing its own `MorphemeCoOccurrenceRules`/
    // `AllomorphCoOccurrenceRules` loops after the `Strata` loop).

    // --- syntactic feature system (POS = feature 0; head complex feature = feature 1) ---------
    let syn = build_syn_features(lang)?;

    // --- grammar-tier FS interner: the empty FS is interned first (FsId 0) --------------------
    let mut fs_interner: Interner<FeatureStruct> = Interner::with_capacity(64);
    let empty = fs_interner.intern(FeatureStruct::EMPTY);
    debug_assert_eq!(empty, hc_featstruct::FsId(0));

    // --- table / char-def id maps (from pass 1, document order) -------------------------------
    let mut table_index: HashMap<String, TableId> = HashMap::new();
    let mut chardef_index: HashMap<String, (TableId, CharDefId)> = HashMap::new();
    for (ti, table) in phon.tables().iter().enumerate() {
        let tid = TableId(ti as u16);
        table_index.insert(table.xml_id().to_string(), tid);
        for (cd_id, cd) in table.iter() {
            chardef_index.insert(cd.xml_id().to_string(), (tid, cd_id));
        }
    }

    // --- MPR features + groups ----------------------------------------------------------------
    let mut mpr_names: Vec<String> = Vec::new();
    let mut mpr_index: HashMap<String, MprId> = HashMap::new();
    {
        let count = lang
            .elems2("MorphologicalPhonologicalRuleFeatures", "MorphologicalPhonologicalRuleFeature")
            .filter(|e| e.is_active())
            .count();
        if count > 64 {
            return Err(GrammarError::Unsupported(format!(
                "{count} MPR features; the bitset representation supports at most 64"
            )));
        }
        for mf in lang
            .elems2("MorphologicalPhonologicalRuleFeatures", "MorphologicalPhonologicalRuleFeature")
            .filter(|e| e.is_active())
        {
            let id = mf.attr("id").unwrap_or("").to_string();
            mpr_index.insert(id, MprId(mpr_names.len() as u8));
            mpr_names.push(mf.text.clone());
        }
    }
    let mut mpr_groups: Vec<MprGroup> = Vec::new();
    for g in lang
        .elems2(
            "MorphologicalPhonologicalRuleFeatures",
            "MorphologicalPhonologicalRuleFeatureGroup",
        )
        .filter(|e| e.is_active())
    {
        let match_type = match g.attr("matchType") {
            Some("all") => MprGroupMatchType::All,
            _ => MprGroupMatchType::Any,
        };
        let output = match g.attr("outputType") {
            Some("append") => MprGroupOutput::Append,
            _ => MprGroupOutput::Overwrite,
        };
        let members = load_mpr_set(g.attr("features"), &mpr_index)?;
        mpr_groups.push(MprGroup {
            name: g.text_of("Name").map(str::to_string),
            match_type,
            output,
            members,
        });
    }

    // --- natural classes ----------------------------------------------------------------------
    let mut natural_classes: Vec<NaturalClass> = Vec::new();
    let mut natclass_index: HashMap<String, NatClassId> = HashMap::new();
    for nc in lang.under("NaturalClasses").filter(|e| e.is_active()) {
        let kind = match nc.tag.as_str() {
            "FeatureNaturalClass" => NaturalClassKind::Feature(load_phon_constraints(nc, &phon)?),
            "SegmentNaturalClass" => {
                let mut segs = Vec::new();
                for se in nc.elems("Segment") {
                    let seg_id = se.attr("segment").unwrap_or("");
                    let (_, cd) = chardef_index.get(seg_id).ok_or_else(|| {
                        GrammarError::Semantic(format!("natural class references unknown segment '{seg_id}'"))
                    })?;
                    segs.push(*cd);
                }
                NaturalClassKind::Segments(segs)
            }
            _ => continue,
        };
        let id = nc.attr("id").unwrap_or("").to_string();
        natclass_index.insert(id.clone(), NatClassId(natural_classes.len() as u32));
        natural_classes.push(NaturalClass {
            xml_id: id,
            name: nc.text_of("Name").map(str::to_string),
            kind,
        });
    }

    // --- stem names (W5; `XmlLanguageLoader.cs:280-281,323-345`) ------------------------------
    let mut stem_names: Vec<StemNameDef> = Vec::new();
    let mut stem_name_index: HashMap<String, StemNameId> = HashMap::new();
    for sn in lang.elems2("StemNames", "StemName") {
        let id = sn.attr("id").unwrap_or("").to_string();
        stem_name_index.insert(id, StemNameId(stem_names.len() as u32));
        stem_names.push(load_stem_name(&mut fs_interner, sn, &syn)?);
    }

    // --- families (W5; `XmlLanguageLoader.cs:289-294`) ----------------------------------------
    // `entries` fills in during lexical-entry loading (`try_load_lex_entry`), mirroring C#'s
    // `family.Entries.Add(entry)` — stored on `Acc` (mutated throughout the strata loop), while
    // the xml-id lookup itself (fixed once families are declared) lives on `Ro`, same split as
    // every other id-index/mutable-accumulator pair in this loader.
    let mut family_defs: Vec<FamilyDef> = Vec::new();
    let mut family_index: HashMap<String, FamilyId> = HashMap::new();
    for fam in lang.elems2("Families", "Family").filter(|e| e.is_active()) {
        let id = fam.attr("id").unwrap_or("").to_string();
        family_index.insert(id, FamilyId(family_defs.len() as u32));
        family_defs.push(FamilyDef { name: Some(fam.text.clone()), entries: Vec::new() });
    }

    let ro = Ro {
        phon: &phon,
        syn: &syn,
        natclass: &natclass_index,
        natural_class_defs: &natural_classes,
        chardef: &chardef_index,
        table: &table_index,
        mpr: &mpr_index,
        stem_names: &stem_name_index,
        families: &family_index,
    };

    // --- phonological rules -------------------------------------------------------------------
    let mut prules: Vec<PhonRuleDef> = Vec::new();
    let mut prule_index: HashMap<String, PRuleId> = HashMap::new();
    for pr in lang.under("PhonologicalRuleDefinitions").filter(|e| e.is_active()) {
        match pr.tag.as_str() {
            "MetathesisRule" => {
                let def = load_metathesis_rule(pr, &ro)?;
                let id = pr.attr("id").unwrap_or("").to_string();
                prule_index.insert(id, PRuleId(prules.len() as u32));
                prules.push(PhonRuleDef::Metathesis(def));
            }
            "PhonologicalRule" => {
                let def = load_rewrite_rule(pr, &ro)?;
                let id = pr.attr("id").unwrap_or("").to_string();
                prule_index.insert(id, PRuleId(prules.len() as u32));
                prules.push(PhonRuleDef::Rewrite(def));
            }
            _ => {}
        }
    }

    // --- strata (morphological rules, templates, lexicon) -------------------------------------
    let mut acc = Acc {
        fs_interner,
        mrules: Vec::new(),
        morphemes: Vec::new(),
        allomorph_owners: Vec::new(),
        templates: Vec::new(),
        entries: Vec::new(),
        families: family_defs,
        allomorph_xml_index: HashMap::new(),
    };
    let mut strata: Vec<StratumDef> = Vec::new();
    for stratum in lang.elems2("Strata", "Stratum").filter(|e| e.is_active()) {
        let stratum_id = StratumId(strata.len() as u8);
        let def = load_stratum(stratum, stratum_id, &ro, &mut acc, &prule_index)?;
        strata.push(def);
    }

    // `IsTemplateRule` post-pass (plan §6 item 6 / W1.6, `AffixProcessRuleDef::is_template_rule`'s
    // doc): tag every affix rule referenced from any template's slot, across every stratum. Must
    // run after the whole strata loop (not inline per-stratum) because `acc.templates`/`acc.mrules`
    // are both flat, grammar-global vectors indexed by ids minted throughout that loop — a template
    // in stratum N could in principle be loaded before or after the rule it references relative to
    // other strata, so only a post-pass over the complete sets is correct.
    let mut is_template_rule = vec![false; acc.mrules.len()];
    for t in &acc.templates {
        for slot in &t.slots {
            for &mid in &slot.rules {
                is_template_rule[mid.0 as usize] = true;
            }
        }
    }
    for (mid, flag) in is_template_rule.into_iter().enumerate() {
        if let MorphRuleDef::AffixProcess(def) = &mut acc.mrules[mid] {
            def.is_template_rule = flag;
        }
    }

    // --- co-occurrence rules (plan W6; post-strata, mirroring `XmlLanguageLoader.LoadLanguage`'s
    // own placement after its `Strata` loop — `primaryMorpheme`/`otherMorphemes`/
    // `primaryAllomorph`/`otherAllomorphs` IDREFs resolve against the morpheme/allomorph
    // registries the strata loop just finished populating) -------------------------------------
    let mut morpheme_xml_index: HashMap<String, MorphemeId> = HashMap::new();
    for (i, m) in acc.morphemes.iter().enumerate() {
        morpheme_xml_index.insert(m.xml_key.clone(), MorphemeId(i as u32));
    }

    for co in lang
        .elems2("MorphemeCoOccurrenceRules", "MorphemeCoOccurrenceRule")
        .filter(|e| e.is_active())
    {
        let primary_xid = co.attr("primaryMorpheme").unwrap_or("");
        let primary = *morpheme_xml_index.get(primary_xid).ok_or_else(|| {
            GrammarError::Semantic(format!(
                "MorphemeCoOccurrenceRule references unknown primaryMorpheme '{primary_xid}'"
            ))
        })?;
        let require = matches!(co.attr("type"), Some("require"));
        let adjacency = load_co_occurrence_adjacency(co.attr("adjacency"));
        let mut others = Vec::new();
        for xid in co.attr("otherMorphemes").unwrap_or("").split_whitespace() {
            let mid = *morpheme_xml_index.get(xid).ok_or_else(|| {
                GrammarError::Semantic(format!(
                    "MorphemeCoOccurrenceRule references unknown otherMorphemes id '{xid}'"
                ))
            })?;
            others.push(mid);
        }
        acc.morphemes[primary.0 as usize]
            .co_occurrence
            .push(MorphemeCoOccurrenceRuleDef { require, others, adjacency });
    }

    for co in lang
        .elems2("AllomorphCoOccurrenceRules", "AllomorphCoOccurrenceRule")
        .filter(|e| e.is_active())
    {
        let primary_xid = co.attr("primaryAllomorph").unwrap_or("");
        let primary = *acc.allomorph_xml_index.get(primary_xid).ok_or_else(|| {
            GrammarError::Semantic(format!(
                "AllomorphCoOccurrenceRule references unknown primaryAllomorph '{primary_xid}'"
            ))
        })?;
        let require = matches!(co.attr("type"), Some("require"));
        let adjacency = load_co_occurrence_adjacency(co.attr("adjacency"));
        let mut others = Vec::new();
        for xid in co.attr("otherAllomorphs").unwrap_or("").split_whitespace() {
            let aid = *acc.allomorph_xml_index.get(xid).ok_or_else(|| {
                GrammarError::Semantic(format!(
                    "AllomorphCoOccurrenceRule references unknown otherAllomorphs id '{xid}'"
                ))
            })?;
            others.push(aid);
        }
        let rule = AllomorphCoOccurrenceRuleDef { require, others, adjacency };
        match acc.allomorph_owners[primary.0 as usize] {
            AllomorphOwner::Root(le, idx) => {
                acc.entries[le.0 as usize].allomorphs[idx as usize].co_occurrence.push(rule);
            }
            AllomorphOwner::Affix(mr, idx) => match &mut acc.mrules[mr.0 as usize] {
                MorphRuleDef::AffixProcess(def) => def.allomorphs[idx as usize].co_occurrence.push(rule),
                MorphRuleDef::Realizational(def) => def.allomorphs[idx as usize].co_occurrence.push(rule),
                MorphRuleDef::Compounding(_) => {
                    unreachable!("compounding rules mint no AllomorphId (no per-allomorph registry entry)")
                }
            },
        }
    }

    // All borrows of `phon` (via `ro`) have ended; the grammar takes ownership of its
    // phonology so `TableId`/`CharDefId`/`FlatIndex` stay resolvable downstream.
    let (phon_features, char_tables) = phon.into_parts();

    Ok(Grammar {
        name: lang.text_of("Name").map(str::to_string),
        phon_features,
        char_tables,
        syn_features: syn,
        fs_interner: acc.fs_interner,
        mpr_names,
        mpr_groups,
        stem_names,
        families: acc.families,
        natural_classes,
        morphemes: acc.morphemes,
        allomorph_owners: acc.allomorph_owners,
        prules,
        mrules: acc.mrules,
        templates: acc.templates,
        entries: acc.entries,
        strata,
    })
}

// =============================================================================================
// Syntactic feature system.
// =============================================================================================

fn build_syn_features(lang: &Node) -> Result<SynFeatureSystem, GrammarError> {
    let mut features: Vec<SynFeature> = Vec::new();

    // Feature 0: parts of speech (C# `AddPartsOfSpeech`). Symbols = <PartOfSpeech> in doc order.
    let pos_symbols: Vec<(String, String)> = lang
        .elems2("PartsOfSpeech", "PartOfSpeech")
        .map(|e| {
            (
                e.attr("id").unwrap_or("").to_string(),
                e.text_of("Name").unwrap_or("").to_string(),
            )
        })
        .collect();
    if pos_symbols.len() >= 64 {
        return Err(GrammarError::Unsupported(format!(
            "{} parts of speech; the symbol bitset supports at most 63",
            pos_symbols.len()
        )));
    }
    features.push(SynFeature {
        xml_id: "__pos__".into(),
        name: "partsOfSpeech".into(),
        kind: SynFeatureKind::Symbolic {
            symbols: pos_symbols,
            default_symbol: None,
        },
    });
    let pos = FeatId(0);

    // Feature 1: the head complex feature, present iff <HeadFeatures> exists (even if empty,
    // as Indonesian's `<HeadFeatures/>` — C# still calls AddHeadFeature).
    let mut head = None;
    if let Some(hf) = lang.child("HeadFeatures") {
        head = Some(FeatId(features.len() as u16));
        features.push(SynFeature {
            xml_id: "__head__".into(),
            name: "head".into(),
            kind: SynFeatureKind::Complex,
        });
        // Head-declared features, document order (SymbolicFeature | ComplexFeature).
        for fd in hf.children.iter().filter(|e| e.is_active()) {
            if let Some(f) = load_syn_feature(fd)? {
                features.push(f);
            }
        }
    }

    // The foot complex feature, present iff <FootFeatures> exists (F1: mirrors <HeadFeatures>
    // exactly, `XmlLanguageLoader.cs:250-255` — `AddFootFeature()` + `LoadSyntacticFeatureSystem
    // (footFeatsElem, SyntacticFeatureType.Foot)`). Foot-declared features are added to the SAME
    // `features` vec as head's — there is one shared syntactic feature namespace in C#, not two —
    // so a real grammar could in principle declare a feature under `<FootFeatures>` that
    // `<AssignedHeadFeatures>` references, and vice versa (confirmed, not assumed: see the
    // `SynFeatureSystem` doc).
    let mut foot = None;
    if let Some(ff) = lang.child("FootFeatures") {
        foot = Some(FeatId(features.len() as u16));
        features.push(SynFeature {
            xml_id: "__foot__".into(),
            name: "foot".into(),
            kind: SynFeatureKind::Complex,
        });
        for fd in ff.children.iter().filter(|e| e.is_active()) {
            if let Some(f) = load_syn_feature(fd)? {
                features.push(f);
            }
        }
    }

    Ok(SynFeatureSystem { features, pos, head, foot })
}

/// Port of `XmlLanguageLoader.LoadFeature` for the syntactic domain.
fn load_syn_feature(elem: &Node) -> Result<Option<SynFeature>, GrammarError> {
    let xml_id = elem.attr("id").unwrap_or("").to_string();
    let name = elem.text_of("Name").unwrap_or("").to_string();
    match elem.tag.as_str() {
        "SymbolicFeature" => {
            let symbols: Vec<(String, String)> = elem
                .elems2("Symbols", "Symbol")
                .map(|s| (s.attr("id").unwrap_or("").to_string(), s.text.clone()))
                .collect();
            if symbols.len() >= 64 {
                return Err(GrammarError::Unsupported(format!(
                    "symbolic feature '{name}' ({xml_id}) has {} symbols; the bitset supports at most 63",
                    symbols.len()
                )));
            }
            let default_symbol = elem.attr_ne("defaultSymbol").and_then(|d| {
                symbols.iter().position(|(id, _)| id == d).map(|i| i as u32)
            });
            Ok(Some(SynFeature {
                xml_id,
                name,
                kind: SynFeatureKind::Symbolic {
                    symbols,
                    default_symbol,
                },
            }))
        }
        "ComplexFeature" => Ok(Some(SynFeature {
            xml_id,
            name,
            kind: SynFeatureKind::Complex,
        })),
        _ => Ok(None),
    }
}

/// `ParsePartsOfSpeech` → the POS symbol bit set.
fn parse_pos_bits(syn: &SynFeatureSystem, ids: &str) -> Result<SymbolBits, GrammarError> {
    let mut bits = SymbolBits::EMPTY;
    for id in ids.split_whitespace() {
        let idx = syn.symbol_index(syn.pos, id).ok_or_else(|| {
            GrammarError::Semantic(format!("unknown part-of-speech id '{id}'"))
        })?;
        bits.set(idx);
    }
    Ok(bits)
}

/// Port of `LoadFeatureStruct` for the *syntactic* feature system (recursive, complex features).
fn load_syn_fs(elem: &Node, syn: &SynFeatureSystem) -> Result<FeatureStruct, GrammarError> {
    let mut b = FeatureStructBuilder::new();
    for fv in elem.elems("FeatureValue").filter(|e| e.is_active()) {
        let feat_xml = fv.attr("feature").unwrap_or("");
        let feat_id = syn
            .feature_by_xml_id(feat_xml)
            .ok_or_else(|| GrammarError::Semantic(format!("unknown syntactic feature '{feat_xml}'")))?;
        match fv.attr_ne("symbolValues") {
            Some(vals) => {
                let mut bits = SymbolBits::EMPTY;
                for sym in vals.split_whitespace() {
                    let idx = syn.symbol_index(feat_id, sym).ok_or_else(|| {
                        GrammarError::Semantic(format!("unknown symbol '{sym}' on feature '{feat_xml}'"))
                    })?;
                    bits.set(idx);
                }
                b.add(feat_id, FeatureValue::Symbolic(bits));
            }
            None => {
                let nested = load_syn_fs(fv, syn)?;
                b.add(feat_id, FeatureValue::Complex(nested));
            }
        }
    }
    Ok(b.build())
}

/// Build a `{POS?, head?, foot?}` syntactic feature struct from an element carrying a POS id-list
/// attribute (`pos_attr`), a head-features child element (`head_elem`), and/or a foot-features
/// child element (`foot_elem`), then intern it. `foot_elem` mirrors `head_elem` exactly (F1,
/// HYBRID_FST_RUST_PLAN.md §7.1 item 4) — both are `None`/absent-element no-ops when the grammar
/// declares no `<FootFeatures>` at all (`syn.foot == None`), matching every pre-F1 caller's
/// behavior bit-for-bit.
fn build_syn_fs(
    elem: &Node,
    syn: &SynFeatureSystem,
    pos_attr: Option<&str>,
    head_elem: Option<&str>,
    foot_elem: Option<&str>,
) -> Result<FeatureStruct, GrammarError> {
    let mut b = FeatureStructBuilder::new();
    if let Some(pa) = pos_attr {
        if let Some(ids) = elem.attr_ne(pa) {
            b.add(syn.pos, FeatureValue::Symbolic(parse_pos_bits(syn, ids)?));
        }
    }
    if let (Some(he), Some(head_fid)) = (head_elem, syn.head) {
        if let Some(hn) = elem.child(he) {
            b.add(head_fid, FeatureValue::Complex(load_syn_fs(hn, syn)?));
        }
    }
    if let (Some(fe), Some(foot_fid)) = (foot_elem, syn.foot) {
        if let Some(fn_) = elem.child(fe) {
            b.add(foot_fid, FeatureValue::Complex(load_syn_fs(fn_, syn)?));
        }
    }
    Ok(b.build())
}

/// `LoadStemName` (`XmlLanguageLoader.cs:323-345`, W5). Each `<Region>` becomes one region FS:
/// the `<StemName>`'s own `partsOfSpeech` attribute (shared by every region) plus that region's
/// own optional `<AssignedHeadFeatures>`/`<AssignedFootFeatures>` — exactly the `{POS, head, foot}`
/// shape `build_syn_fs` produces for `RequiredSyntacticFeatureStruct` elsewhere, so a region FS is
/// directly comparable (via `subsumes`) to a word's accumulated syntactic FS.
fn load_stem_name(
    fs_interner: &mut Interner<FeatureStruct>,
    sn: &Node,
    syn: &SynFeatureSystem,
) -> Result<StemNameDef, GrammarError> {
    let pos_bits = parse_pos_bits(syn, sn.attr("partsOfSpeech").unwrap_or(""))?;
    let mut regions = Vec::new();
    for region in sn.elems2("Regions", "Region") {
        let mut b = FeatureStructBuilder::new();
        b.add(syn.pos, FeatureValue::Symbolic(pos_bits));
        if let (Some(head_fid), Some(hn)) = (syn.head, region.child("AssignedHeadFeatures")) {
            b.add(head_fid, FeatureValue::Complex(load_syn_fs(hn, syn)?));
        }
        // F1: `AssignedFootFeatures` on a StemName `<Region>` (`XmlLanguageLoader.cs:332-337`) —
        // previously dead here because FootFeatures lint made `syn.foot` always `None`.
        if let (Some(foot_fid), Some(fnode)) = (syn.foot, region.child("AssignedFootFeatures")) {
            b.add(foot_fid, FeatureValue::Complex(load_syn_fs(fnode, syn)?));
        }
        regions.push(fs_interner.intern(b.build()));
    }
    Ok(StemNameDef { name: sn.text_of("Name").map(str::to_string), regions })
}

fn intern_syn_fs(
    acc: &mut Acc,
    elem: &Node,
    syn: &SynFeatureSystem,
    pos_attr: Option<&str>,
    head_elem: Option<&str>,
    foot_elem: Option<&str>,
) -> Result<hc_featstruct::FsId, GrammarError> {
    let fs = build_syn_fs(elem, syn, pos_attr, head_elem, foot_elem)?;
    Ok(acc.fs_interner.intern(fs))
}

// =============================================================================================
// MPR, natural-class, and variable helpers.
// =============================================================================================

fn load_mpr_set(ids: Option<&str>, mpr: &HashMap<String, MprId>) -> Result<MprSet, GrammarError> {
    let mut set = MprSet::EMPTY;
    if let Some(ids) = ids.filter(|s| !s.is_empty()) {
        for id in ids.split_whitespace() {
            let m = mpr
                .get(id)
                .ok_or_else(|| GrammarError::Semantic(format!("unknown MPR feature '{id}'")))?;
            set.insert(*m);
        }
    }
    Ok(set)
}

/// `LoadFeatureStruct` for a `FeatureNaturalClass` against the phonological feature system,
/// flattened to sparse `(lane, symbols)` constraints sorted by lane (union on repeats).
fn load_phon_constraints(
    nc: &Node,
    phon: &GrammarPhonology,
) -> Result<Vec<(FlatIndex, SymbolBits)>, GrammarError> {
    let fs = phon.feature_system();
    let mut map: HashMap<u32, SymbolBits> = HashMap::new();
    for fv in nc.elems("FeatureValue").filter(|e| e.is_active()) {
        let feat_xml = fv.attr("feature").unwrap_or("");
        let flat = fs
            .flat_index(feat_xml)
            .ok_or_else(|| GrammarError::Semantic(format!("unknown phonological feature '{feat_xml}'")))?;
        let mut bits = SymbolBits::EMPTY;
        if let Some(vals) = fv.attr_ne("symbolValues") {
            for sym in vals.split_whitespace() {
                let idx = fs.symbol_index(flat, sym).ok_or_else(|| {
                    GrammarError::Semantic(format!("unknown symbol '{sym}' on feature '{feat_xml}'"))
                })?;
                bits.set(idx);
            }
        }
        let entry = map.entry(flat.0).or_insert(SymbolBits::EMPTY);
        entry.0 |= bits.0;
    }
    // Plan §13.1 Tier-1 #1: every `FeatureNaturalClass` unconditionally requires `Type=Segment`,
    // mirroring C# `NaturalClass`'s base-constructor `fs.AddValue(HCFeatureSystem.Type,
    // HCFeatureSystem.Segment)` (`NaturalClass.cs:7-15`), which fires regardless of what
    // `<FeatureValue>`s the class authors. No `<FeatureValue feature="...">` in real HC XML ever
    // names the synthetic `Type` feature, so this can never collide with (or be overridden by) an
    // authored constraint — it is inserted unconditionally, last. `SegmentNaturalClass` needs no
    // equivalent injection: it gets `Type=Segment` "for free" via the lane-union-of-members logic
    // once each member char-def's own lanes correctly carry `Type` (`hc-grammar/src/chardef.rs`).
    map.insert(fs.type_flat().0, SymbolBits::single(crate::featsys::TYPE_SEGMENT_SYMBOL));
    let mut out: Vec<(FlatIndex, SymbolBits)> = map.into_iter().map(|(k, v)| (FlatIndex(k), v)).collect();
    out.sort_by_key(|(f, _)| f.0);
    Ok(out)
}

/// Port of `LoadVariables` — a rule-scoped alpha-variable table over phonological features.
fn load_variables(elem: Option<&Node>, phon: &GrammarPhonology) -> Result<VarTable, GrammarError> {
    let mut vars = Vec::new();
    if let Some(vf) = elem {
        for v in vf.elems("VariableFeature") {
            let id = v.attr("id").unwrap_or("").to_string();
            let name = v.attr("name").unwrap_or("").to_string();
            let feat_xml = v.attr("phonologicalFeature").unwrap_or("");
            let flat = phon.feature_system().flat_index(feat_xml).ok_or_else(|| {
                GrammarError::Semantic(format!("variable references unknown phonological feature '{feat_xml}'"))
            })?;
            vars.push((id, name, flat));
        }
    }
    Ok(VarTable { vars })
}

// =============================================================================================
// Patterns (`LoadPatternNodes` / `LoadSimpleContext` / templates / sequences).
// =============================================================================================

fn load_simple_context(rec: &Node, vars: &VarTable, ro: &Ro) -> Result<SimpleContext, GrammarError> {
    let nc_xml = rec.attr("naturalClass").unwrap_or("");
    let nat_class = ro
        .natclass
        .get(nc_xml)
        .copied()
        .ok_or_else(|| GrammarError::Semantic(format!("unknown natural class '{nc_xml}'")))?;
    let mut alpha = Vec::new();
    for va in rec.elems2("AlphaVariables", "AlphaVariable") {
        let var_xml = va.attr("variableFeature").unwrap_or("");
        // Empty scope (allomorph environment) ⇒ this is the linted case (C# throws KeyNotFound).
        let var = vars.by_xml_id(var_xml).ok_or_else(|| {
            GrammarError::Unsupported(format!(
                "AlphaVariable '{var_xml}' referenced outside a variable scope \
                 (e.g. in an allomorph environment)"
            ))
        })?;
        let feature = vars.vars[var.0 as usize].2;
        let plus = va.attr("polarity").is_none_or(|p| p == "plus");
        alpha.push(AlphaVar { feature, var, plus });
    }
    Ok(SimpleContext {
        nat_class,
        vars: alpha,
    })
}

/// Build one `PatternNode` from a single `<SimpleContext>`/`<Segment>`/`<BoundaryMarker>`/
/// `<OptionalSegmentSequence>`/`<Segments>` element (`None` for any other/unrecognized tag, mirroring
/// C#'s `LoadPatternNodes` switch falling through with no `node` assigned). Factored out of
/// [`load_pattern_nodes`] so `load_metathesis_pattern_nodes` can reuse the exact same per-element
/// logic while additionally checking each element's own `id` attribute for switch-tagging (the DTD's
/// only group-authoring mechanism, used exclusively by `<MetathesisRule>`).
fn load_one_pattern_node(
    rec: &Node,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
) -> Result<Option<PatternNode>, GrammarError> {
    let node = match rec.tag.as_str() {
        "SimpleContext" => PatternNode::Context(load_simple_context(rec, vars, ro)?),
        "Segment" => PatternNode::CharDef(resolve_chardef(ro, rec.attr("segment").unwrap_or(""))?),
        "BoundaryMarker" => PatternNode::CharDef(resolve_chardef(ro, rec.attr("boundary").unwrap_or(""))?),
        "OptionalSegmentSequence" => {
            let min: u32 = match rec.attr_ne("min") {
                Some(s) => s
                    .parse()
                    .map_err(|_| GrammarError::Semantic(format!("bad min '{s}'")))?,
                None => 0,
            };
            let max_raw: i64 = match rec.attr_ne("max") {
                Some(s) => s
                    .parse()
                    .map_err(|_| GrammarError::Semantic(format!("bad max '{s}'")))?,
                None => -1,
            };
            let max = if max_raw < 0 { None } else { Some(max_raw as u32) };
            let children = load_pattern_nodes(rec, vars, default_table, ro)?;
            PatternNode::Quantifier { min, max, children }
        }
        "Segments" => {
            let tid = match rec.attr("characterDefinitionTable") {
                Some(t) => *ro
                    .table
                    .get(t)
                    .ok_or_else(|| GrammarError::Semantic(format!("unknown table '{t}'")))?,
                None => default_table,
            };
            let shape_str = rec.text_of("PhoneticShape").unwrap_or("");
            PatternNode::Segments {
                table: tid,
                shape: segment_text(tid, shape_str, ro.phon)?,
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(node))
}

fn load_pattern_nodes(
    pseq: &Node,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
) -> Result<Vec<PatternNode>, GrammarError> {
    let mut out = Vec::new();
    for rec in &pseq.children {
        if let Some(node) = load_one_pattern_node(rec, vars, default_table, ro)? {
            out.push(node);
        }
    }
    Ok(out)
}

fn resolve_chardef(ro: &Ro, xml_id: &str) -> Result<CharDefId, GrammarError> {
    ro.chardef
        .get(xml_id)
        .map(|(_, cd)| *cd)
        .ok_or_else(|| GrammarError::Semantic(format!("unknown character definition '{xml_id}'")))
}

/// `LoadPhoneticSequence`: empty pattern if the element is absent.
fn load_phonetic_sequence(
    pseq: Option<&Node>,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
) -> Result<Pattern, GrammarError> {
    match pseq {
        None => Ok(Pattern::default()),
        Some(n) => Ok(Pattern {
            nodes: load_pattern_nodes(n, vars, default_table, ro)?,
        }),
    }
}

/// `LoadPhoneticTemplate`: `None` when the template element is absent (C# builds an empty
/// pattern; the model represents "no template on this side" as `None`, matching semantics).
fn load_phonetic_template(
    ptemp: Option<&Node>,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
) -> Result<Option<Pattern>, GrammarError> {
    let Some(pt) = ptemp else { return Ok(None) };
    let mut nodes = Vec::new();
    if pt.attr("initialBoundaryCondition") == Some("true") {
        nodes.push(PatternNode::Anchor(AnchorSide::Left));
    }
    if let Some(ps) = pt.child("PhoneticSequence") {
        nodes.extend(load_pattern_nodes(ps, vars, default_table, ro)?);
    }
    if pt.attr("finalBoundaryCondition") == Some("true") {
        nodes.push(PatternNode::Anchor(AnchorSide::Right));
    }
    Ok(Some(Pattern { nodes }))
}

fn segment_text(
    table: TableId,
    shape_str: &str,
    phon: &GrammarPhonology,
) -> Result<SegmentedText, GrammarError> {
    let t = phon
        .tables()
        .get(table.0 as usize)
        .ok_or_else(|| GrammarError::Semantic(format!("no table {}", table.0)))?;
    let shape = segment(t, shape_str)
        .map_err(|e| GrammarError::Semantic(format!("cannot segment {shape_str:?}: {e}")))?;
    Ok(SegmentedText {
        text: shape_str.to_string(),
        shape,
    })
}

/// [`segment_text`]'s pattern-aware counterpart (finding N3): C# `LoadRootAllomorph` is the
/// **only** `new Segments(...)` call site that passes `allowPattern = true`
/// (`XmlLanguageLoader.cs:501`), so this is used only by [`load_root_allomorph`], never by
/// [`segment_text`]'s other two call sites (rule/environment `<Segments>` patterns).
fn segment_text_with_patterns(
    table: TableId,
    shape_str: &str,
    phon: &GrammarPhonology,
    natural_classes: &[NaturalClass],
) -> Result<SegmentedText, GrammarError> {
    let t = phon
        .tables()
        .get(table.0 as usize)
        .ok_or_else(|| GrammarError::Semantic(format!("no table {}", table.0)))?;
    let shape = segment_with_patterns(t, natural_classes, shape_str)
        .map_err(|e| GrammarError::Semantic(format!("cannot segment {shape_str:?}: {e}")))?;
    Ok(SegmentedText {
        text: shape_str.to_string(),
        shape,
    })
}

/// `LoadAllomorphEnvironments` for one `RequiredEnvironments`/`ExcludedEnvironments` block. The
/// variable scope is always empty (C# `LoadAllomorphEnvironment`), so an `AlphaVariable` here
/// lints `Unsupported` via [`load_simple_context`].
fn load_allomorph_environments(
    envs: Option<&Node>,
    require: bool,
    default_table: TableId,
    ro: &Ro,
) -> Result<Vec<EnvironmentDef>, GrammarError> {
    let mut out = Vec::new();
    let empty_vars = VarTable::default();
    if let Some(block) = envs {
        for env in block.elems("Environment") {
            let left_pt = env.child("LeftEnvironment").and_then(|n| n.child("PhoneticTemplate"));
            let right_pt = env.child("RightEnvironment").and_then(|n| n.child("PhoneticTemplate"));
            out.push(EnvironmentDef {
                require,
                left: load_phonetic_template(left_pt, &empty_vars, default_table, ro)?,
                right: load_phonetic_template(right_pt, &empty_vars, default_table, ro)?,
            });
        }
    }
    Ok(out)
}

// =============================================================================================
// Phonological (rewrite) rules.
// =============================================================================================

fn load_rewrite_rule(pr: &Node, ro: &Ro) -> Result<RewriteRuleDef, GrammarError> {
    let mult = pr.attr("multipleApplicationOrder").unwrap_or("");
    // P13 (rust-optimizations-phase2.md §P13 / rust/docs/p13-simultaneous-design.md): the former
    // stopgap hard lint here (W1.4) has been REMOVED now that `RewriteMode::Simultaneous` has real
    // execution semantics in `hc-rules` (`rewrite::sim_feature`/`sim_narrow` for synthesis;
    // `analyze`/`analyze_cached`'s `self_opaquing` repeat-wrapper for analysis — see the design
    // doc's §4). A grammar authoring `multipleApplicationOrder="simultaneous"` now loads and runs
    // with its authored semantics instead of hard-failing.
    let mode = match mult {
        "simultaneous" => RewriteMode::Simultaneous,
        _ => RewriteMode::Iterative,
    };
    let dir = match mult {
        "rightToLeftIterative" => Dir::RightToLeft,
        _ => Dir::LeftToRight,
    };
    let vars = load_variables(pr.child("VariableFeatures"), ro.phon)?;
    // Phonological rules have no default char-def table context; Segments there carry their own.
    let default_table = TableId(0);
    let lhs = load_phonetic_sequence(
        pr.child("PhoneticInput").and_then(|n| n.child("PhoneticSequence")),
        &vars,
        default_table,
        ro,
    )?;

    let mut subrules = Vec::new();
    for sub in pr
        .elems2("PhonologicalSubrules", "PhonologicalSubrule")
        .filter(|e| e.is_active())
    {
        subrules.push(load_rewrite_subrule(sub, &vars, default_table, ro, mode, &lhs)?);
    }

    Ok(RewriteRuleDef {
        xml_id: pr.attr("id").unwrap_or("").to_string(),
        name: pr.text_of("Name").map(str::to_string),
        mode,
        dir,
        vars,
        lhs,
        subrules,
    })
}

fn load_rewrite_subrule(
    sub: &Node,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
    mode: RewriteMode,
    lhs: &Pattern,
) -> Result<RewriteSubruleDef, GrammarError> {
    // NOT hard-linted, despite `subrule_applicable` (hc-rules) silently treating it as always-false:
    // a stopgap lint here was drafted and REJECTED during W1 development after direct measurement
    // showed Amharic's real grammar authors `requiredPartsOfSpeech` on 3 `<PhonologicalSubrule>`s
    // (`amharic-hc.xml:12151,12169,12188`) — contradicting the audit's "0 across all 5 sample
    // grammars" premise (phase2/B-phonology-parity.md §3's requiredPOS-gate row is wrong; flagged
    // for correction). A hard lint here would drop Amharic's load entirely (532/673 -> 0), a clear
    // regression. The construct stays silently-always-false (today's actual, already-measured
    // 532/673 baseline runs with these 3 subrules effectively disabled) until the real port lands
    // (needs `Word` syntactic-FS threaded into `hc_rules::rewrite`'s rule-level API — tracked as a
    // real, Effort-M port, not a W1 stopgap).
    let required_pos = match sub.attr_ne("requiredPartsOfSpeech") {
        Some(ids) => Some(parse_pos_bits(ro.syn, ids)?),
        None => None,
    };
    let required_mpr = load_mpr_set(sub.attr("requiredMPRFeatures"), ro.mpr)?;
    let excluded_mpr = load_mpr_set(sub.attr("excludedMPRFeatures"), ro.mpr)?;
    let rhs = load_phonetic_sequence(
        sub.child("PhoneticOutput").and_then(|n| n.child("PhoneticSequence")),
        vars,
        default_table,
        ro,
    )?;

    let (left_env, right_env) = match sub.child("Environment") {
        None => (None, None),
        Some(env) => {
            let left_pt = env.child("LeftEnvironment").and_then(|n| n.child("PhoneticTemplate"));
            let right_pt = env.child("RightEnvironment").and_then(|n| n.child("PhoneticTemplate"));
            (
                load_phonetic_template(left_pt, vars, default_table, ro)?,
                load_phonetic_template(right_pt, vars, default_table, ro)?,
            )
        }
    };

    let self_opaquing =
        compute_self_opaquing(ro, default_table, mode, lhs, &rhs, left_env.as_ref(), right_env.as_ref());

    Ok(RewriteSubruleDef {
        required_pos,
        required_mpr,
        excluded_mpr,
        rhs,
        left_env,
        right_env,
        self_opaquing,
    })
}

// ---------------------------------------------------------------------------------------------
// P13 §4.3: `RewriteSubruleDef::self_opaquing` -- computed once at load time from the rule's own
// (already-loaded) patterns; see that field's doc for the exact per-kind formula.
// ---------------------------------------------------------------------------------------------

/// C# `AnalysisRewriteRule.cs:106-120,75-80`'s self-opaquing precheck. See
/// `RewriteSubruleDef::self_opaquing`'s doc for the per-kind dispatch this implements.
fn compute_self_opaquing(
    ro: &Ro,
    table_id: TableId,
    mode: RewriteMode,
    lhs: &Pattern,
    rhs: &Pattern,
    left_env: Option<&Pattern>,
    right_env: Option<&Pattern>,
) -> bool {
    if mode != RewriteMode::Simultaneous {
        return false;
    }
    if lhs.nodes.is_empty() {
        // Epenthesis: unconditional whenever Simultaneous, no unifiability precheck
        // (`AnalysisRewriteRule.cs:75-80`).
        return true;
    }
    if lhs.nodes.len() != rhs.nodes.len() {
        // Narrow/Expansion: irrelevant field, this kind's analysis is always unconditionally
        // Simultaneous+Deletion regardless of `rule.mode` (§2.2/§1.3 of the design doc).
        return false;
    }
    // Feature: self-opaquing iff some RHS constraint is not feature-unifiable with every
    // Segment-typed node of EITHER environment (`IsUnifiable`, `AnalysisRewriteRule.cs:106-120`).
    let phon = ro.phon.feature_system();
    let table = &ro.phon.tables()[table_id.0 as usize];
    rhs.nodes.iter().any(|rhs_node| {
        let rhs_pins = pattern_node_pin_bits(phon, table, ro.natural_class_defs, rhs_node);
        !env_unifiable(phon, table, ro.natural_class_defs, &rhs_pins, left_env)
            || !env_unifiable(phon, table, ro.natural_class_defs, &rhs_pins, right_env)
    })
}

/// `IsUnifiable(rhsConstraint, environment)`: every Segment-typed (`Context`/`CharDef`) node
/// inside `environment`'s pattern (recursing into quantifiers, mirroring
/// `strip_boundary_nodes`'s own recursion) must be feature-unifiable with `rhs_pins` -- i.e. for
/// every phonological feature BOTH sides pin, their symbol-bit sets must overlap (a feature
/// pinned by only one side never blocks -- the same "is_unifiable" convention this port already
/// uses throughout, e.g. `hc-featstruct/src/ops.rs::is_unifiable`). No environment (`None`) or one
/// with no Segment-typed nodes is vacuously unifiable (nothing to violate).
fn env_unifiable(
    phon: &PhonFeatureSystem,
    table: &CharDefTable,
    natural_classes: &[NaturalClass],
    rhs_pins: &[(usize, u64)],
    env: Option<&Pattern>,
) -> bool {
    let Some(env) = env else { return true };
    env_nodes_unifiable(phon, table, natural_classes, rhs_pins, &env.nodes)
}

fn env_nodes_unifiable(
    phon: &PhonFeatureSystem,
    table: &CharDefTable,
    natural_classes: &[NaturalClass],
    rhs_pins: &[(usize, u64)],
    nodes: &[PatternNode],
) -> bool {
    nodes.iter().all(|n| match n {
        PatternNode::Quantifier { children, .. } => {
            env_nodes_unifiable(phon, table, natural_classes, rhs_pins, children)
        }
        PatternNode::Context(_) | PatternNode::CharDef(_) => {
            let node_pins = pattern_node_pin_bits(phon, table, natural_classes, n);
            rhs_pins.iter().all(|&(f, bits)| match node_pins.iter().find(|&&(nf, _)| nf == f) {
                Some(&(_, nbits)) => bits & nbits != 0,
                None => true,
            })
        }
        // Anchor/Segments: no phonological feature pin to violate.
        _ => true,
    })
}

/// Local mirror of `hc_rules::rewrite::node_pins` -- kept as a duplicate in THIS crate (not
/// imported) because `hc-grammar` cannot depend on `hc-rules` (the dependency runs the other
/// way: `hc-rules` depends on `hc-grammar`). Used ONLY to compute `self_opaquing` above; MUST
/// stay semantically identical to `node_pins` (same alpha-variable exclusion via `sc.vars`, same
/// `Feature`/`Segments` natural-class dispatch) -- see `node_pins`'s own doc for the exact
/// contract this mirrors, and `self_opaquing_pin_semantics_match_node_pins` (below) for the
/// pinned parity check.
fn pattern_node_pin_bits(
    phon: &PhonFeatureSystem,
    table: &CharDefTable,
    natural_classes: &[NaturalClass],
    node: &PatternNode,
) -> Vec<(usize, u64)> {
    let w = phon.len();
    match node {
        PatternNode::Context(sc) => {
            let alpha: HashSet<usize> = sc.vars.iter().map(|v| v.feature.0 as usize).collect();
            match &natural_classes[sc.nat_class.0 as usize].kind {
                NaturalClassKind::Feature(pairs) => pairs
                    .iter()
                    .filter(|(f, _)| !alpha.contains(&(f.0 as usize)))
                    .map(|(f, b)| (f.0 as usize, b.0))
                    .collect(),
                NaturalClassKind::Segments(segs) => (0..w)
                    .filter_map(|f| {
                        let bits = segs.iter().fold(0u64, |acc, cd| acc | table.get(*cd).feature_lanes()[f]);
                        (bits != phon.mask(FlatIndex(f as u32))).then_some((f, bits))
                    })
                    .collect(),
            }
        }
        PatternNode::CharDef(cd) => {
            let lanes = table.get(*cd).feature_lanes();
            (0..w).filter(|&f| lanes[f] != phon.mask(FlatIndex(f as u32))).map(|f| (f, lanes[f])).collect()
        }
        _ => Vec::new(),
    }
}

/// `LoadMetathesisRule` (`XmlLanguageLoader.cs:826-850`). No `VariableFeatures` scope (the DTD's
/// `<MetathesisRule>` has none) and no default char-def table context (same convention
/// `load_rewrite_rule` documents for `<PhonologicalRule>`'s LHS/RHS: `Segment`/`BoundaryMarker`
/// reference char-defs by a table-independent global IDREF; only a nested `<Segments>` element's own
/// optional `characterDefinitionTable` attribute needs a fallback, and C# passes `null` there too —
/// ported as `TableId(0)`, matching every reference/fixture grammar's single-table convention).
///
/// Group-authoring: the DTD has no `<Group>` element. `XmlLanguageLoader.LoadMetathesisRule` instead
/// builds a `groupNames` dictionary mapping {the id the `leftSwitch` attribute references → an
/// internal name, the id the `rightSwitch` attribute references → another internal name} and has
/// `LoadPatternNodes` wrap whichever single pattern element carries a matching `id` attribute in a
/// `Group` of that name — so `MetathesisRule.LeftSwitchName` ends up bound to whatever
/// `leftSwitch`'s IDREF points at (C# names that internal group `"r"`; the naming is an
/// implementation detail — see the `MetathesisRuleDef` doc for why "left" doesn't mean "physically
/// left"). This port skips minting a `Group` pattern-node kind entirely: since a real grammar's
/// `<MetathesisRule>` can only ever validly switch-tag a *single* `<SimpleContext>`/`<Segment>`/
/// `<BoundaryMarker>` element (a `<Segments>`/`<OptionalSegmentSequence>` switch group is DTD-legal
/// but fails to compile against the real C# engine — see
/// `rust/conformance/metathesis/complex_rule/README.md`'s finding), recording each switch as a plain
/// index into `pattern.nodes` is sufficient and avoids adding an authored-`Group` pattern-node kind
/// (and the matching `hc_rules::bridge::PatternBridge` case) that would only ever wrap one node.
fn load_metathesis_rule(pr: &Node, ro: &Ro) -> Result<MetathesisRuleDef, GrammarError> {
    let left_switch_xml = pr
        .attr("leftSwitch")
        .ok_or_else(|| GrammarError::Semantic("MetathesisRule missing required 'leftSwitch' attribute".into()))?;
    let right_switch_xml = pr
        .attr("rightSwitch")
        .ok_or_else(|| GrammarError::Semantic("MetathesisRule missing required 'rightSwitch' attribute".into()))?;
    // DTD: `multipleApplicationOrder (leftToRightIterative | rightToLeftIterative)` — no
    // `simultaneous` option here (unlike `<PhonologicalRule>`), so no W1.4-style lint is needed.
    let dir = match pr.attr("multipleApplicationOrder") {
        Some("rightToLeftIterative") => Dir::RightToLeft,
        _ => Dir::LeftToRight,
    };
    let default_table = TableId(0);
    let ptemp = pr
        .child("StructuralDescription")
        .and_then(|n| n.child("PhoneticTemplate"))
        .ok_or_else(|| {
            GrammarError::Semantic("MetathesisRule missing StructuralDescription/PhoneticTemplate".into())
        })?;

    let mut nodes = Vec::new();
    if ptemp.attr("initialBoundaryCondition") == Some("true") {
        nodes.push(PatternNode::Anchor(AnchorSide::Left));
    }
    let empty_vars = VarTable::default();
    let mut left_switch = None;
    let mut right_switch = None;
    if let Some(ps) = ptemp.child("PhoneticSequence") {
        for rec in &ps.children {
            let Some(node) = load_one_pattern_node(rec, &empty_vars, default_table, ro)? else {
                continue;
            };
            let idx = nodes.len() as u32;
            match rec.attr("id") {
                Some(id) if id == left_switch_xml => left_switch = Some(idx),
                Some(id) if id == right_switch_xml => right_switch = Some(idx),
                _ => {}
            }
            nodes.push(node);
        }
    }
    if ptemp.attr("finalBoundaryCondition") == Some("true") {
        nodes.push(PatternNode::Anchor(AnchorSide::Right));
    }

    let left_switch = left_switch.ok_or_else(|| {
        GrammarError::Semantic(format!(
            "MetathesisRule leftSwitch '{left_switch_xml}' does not reference any element in its own \
             StructuralDescription"
        ))
    })?;
    let right_switch = right_switch.ok_or_else(|| {
        GrammarError::Semantic(format!(
            "MetathesisRule rightSwitch '{right_switch_xml}' does not reference any element in its own \
             StructuralDescription"
        ))
    })?;
    if left_switch == right_switch {
        return Err(GrammarError::Semantic(
            "MetathesisRule leftSwitch and rightSwitch must reference different elements".into(),
        ));
    }

    Ok(MetathesisRuleDef {
        xml_id: pr.attr("id").unwrap_or("").to_string(),
        name: pr.text_of("Name").map(str::to_string),
        dir,
        pattern: Pattern { nodes },
        left_switch,
        right_switch,
    })
}

// =============================================================================================
// Strata (morphological rules, templates, lexicon).
// =============================================================================================

fn load_stratum(
    stratum: &Node,
    stratum_id: StratumId,
    ro: &Ro,
    acc: &mut Acc,
    prule_index: &HashMap<String, PRuleId>,
) -> Result<StratumDef, GrammarError> {
    let table_xml = stratum.attr("characterDefinitionTable").unwrap_or("");
    let table = *ro
        .table
        .get(table_xml)
        .ok_or_else(|| GrammarError::Semantic(format!("stratum references unknown table '{table_xml}'")))?;
    let mrule_order = match stratum.attr("morphologicalRuleOrder") {
        Some("unordered") => MorphRuleOrder::Unordered,
        _ => MorphRuleOrder::Linear,
    };

    // Phonological rules in the stratum's `phonologicalRules` id-list order (skip unknown).
    let mut prules = Vec::new();
    if let Some(ids) = stratum.attr_ne("phonologicalRules") {
        for id in ids.split_whitespace() {
            if let Some(pid) = prule_index.get(id) {
                prules.push(*pid);
            }
        }
    }

    // Morphological rule definitions (document order), building a local xml-id → MRuleId map.
    let mut local_mr: HashMap<String, MRuleId> = HashMap::new();
    for mr in stratum.under("MorphologicalRuleDefinitions").filter(|e| e.is_active()) {
        let loaded = match mr.tag.as_str() {
            "MorphologicalRule" => try_load_affix_process_rule(mr, table, stratum_id, ro, acc)?,
            "RealizationalRule" => try_load_realizational_rule(mr, table, stratum_id, ro, acc)?,
            "CompoundingRule" => try_load_compounding_rule(mr, table, ro, acc)?,
            _ => None,
        };
        if let Some(id) = loaded {
            local_mr.insert(mr.attr("id").unwrap_or("").to_string(), id);
        }
    }

    // Morphological rules in the stratum's `morphologicalRules` id-list order (skip unknown).
    let mut mrules = Vec::new();
    if let Some(ids) = stratum.attr_ne("morphologicalRules") {
        for id in ids.split_whitespace() {
            if let Some(mid) = local_mr.get(id) {
                mrules.push(*mid);
            }
        }
    }

    // Affix templates (document order).
    let mut templates = Vec::new();
    for temp in stratum.elems2("AffixTemplates", "AffixTemplate").filter(|e| e.is_active()) {
        let def = load_affix_template(temp, &local_mr, ro, acc)?;
        let tid = TemplateId(acc.templates.len() as u32);
        acc.templates.push(def);
        templates.push(tid);
    }

    // Lexical entries (document order; entries with zero loadable allomorphs are dropped).
    let mut entries = Vec::new();
    for entry in stratum.elems2("LexicalEntries", "LexicalEntry").filter(|e| e.is_active()) {
        if let Some(eid) = try_load_lex_entry(entry, table, stratum_id, ro, acc)? {
            entries.push(eid);
        }
    }

    Ok(StratumDef {
        name: stratum.text_of("Name").map(str::to_string),
        table,
        mrule_order,
        prules,
        mrules,
        templates,
        entries,
    })
}

fn try_load_affix_process_rule(
    mr: &Node,
    default_table: TableId,
    stratum_id: StratumId,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<Option<MRuleId>, GrammarError> {
    let mrule_id = MRuleId(acc.mrules.len() as u32);

    let required_syn_fs = intern_syn_fs(acc, mr, ro.syn, Some("requiredPartsOfSpeech"), Some("RequiredHeadFeatures"), Some("RequiredFootFeatures"))?;
    let out_syn_fs = intern_syn_fs(acc, mr, ro.syn, Some("outputPartOfSpeech"), Some("OutputHeadFeatures"), Some("OutputFootFeatures"))?;

    let mut obligatory_features = Vec::new();
    if let Some(ids) = mr.attr_ne("outputObligatoryFeatures") {
        for id in ids.split_whitespace() {
            let fid = ro
                .syn
                .feature_by_xml_id(id)
                .ok_or_else(|| GrammarError::Semantic(format!("unknown obligatory feature '{id}'")))?;
            obligatory_features.push(fid);
        }
    }

    let mut allomorphs = Vec::new();
    for sub in mr
        .elems2("MorphologicalSubrules", "MorphologicalSubrule")
        .filter(|e| e.is_active())
    {
        let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
        match load_affix_allomorph(sub, default_table, allo_id, ro, acc) {
            Ok(def) => {
                acc.allomorph_owners
                    .push(AllomorphOwner::Affix(mrule_id, allomorphs.len() as u16));
                // C#'s `_allomorphs[(string)subruleElem.Attribute("id")] = allomorph`
                // (XmlLanguageLoader.cs:925) — the id `<AllomorphCoOccurrenceRule
                // otherAllomorphs="...">` resolves against for an affix allomorph.
                if let Some(xid) = sub.attr("id") {
                    acc.allomorph_xml_index.insert(xid.to_string(), allo_id);
                }
                allomorphs.push(def);
            }
            Err(e) if is_droppable(&e) => {}
            Err(e) => return Err(e),
        }
    }

    if allomorphs.is_empty() {
        return Ok(None);
    }

    let morpheme = MorphemeId(acc.morphemes.len() as u32);
    acc.morphemes.push(MorphemeInfo {
        xml_key: mr.attr("id").unwrap_or("").to_string(),
        morph_id: mr.text_of("MorphemeId").map(str::to_string),
        gloss: mr.text_of("Gloss").map(str::to_string),
        stratum: stratum_id,
        properties: load_properties(mr.child("Properties")),
        // Filled by the post-strata co-occurrence-rule pass in `load()`.
        co_occurrence: Vec::new(),
    });

    let max_apps = match mr.attr_ne("multipleApplication") {
        Some(s) => s
            .parse()
            .map_err(|_| GrammarError::Semantic(format!("bad multipleApplication '{s}'")))?,
        None => 1,
    };

    // `requiredStemName` (W5, `XmlLanguageLoader.cs:908-910`).
    let required_stem_name = match mr.attr_ne("requiredStemName") {
        Some(sid) => Some(*ro.stem_names.get(sid).ok_or_else(|| {
            GrammarError::Semantic(format!("MorphologicalRule references unknown requiredStemName '{sid}'"))
        })?),
        None => None,
    };

    acc.mrules.push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme,
        name: mr.text_of("Name").map(str::to_string),
        blockable: parse_bool(mr.attr("blockable"), true),
        partial: parse_bool(mr.attr("partial"), false),
        max_apps,
        required_syn_fs,
        out_syn_fs,
        obligatory_features,
        required_stem_name,
        allomorphs,
        // Set by the post-pass in `load()` once every stratum's templates are known — default
        // `false` here, same as every other not-yet-known-at-construction-time field pattern in
        // this loader.
        is_template_rule: false,
    }));
    Ok(Some(mrule_id))
}

/// `TryLoadRealizationalRule` (`XmlLanguageLoader.cs:947-1014`, W5). Shares `load_affix_allomorph`
/// with the regular affix-process loader above (C#'s `LoadAffixProcessAllomorph` is the one method
/// both call) — see [`hc_grammar::model::MorphRuleDef::affix_allomorphs`]'s doc for why that's
/// exact, not coincidental.
fn try_load_realizational_rule(
    real: &Node,
    default_table: TableId,
    stratum_id: StratumId,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<Option<MRuleId>, GrammarError> {
    let mrule_id = MRuleId(acc.mrules.len() as u32);

    // No `requiredPartsOfSpeech`/POS attribute on `<RealizationalRule>` (DTD + loader both omit
    // it) — head/foot only, foot dead as everywhere else.
    let required_syn_fs = intern_syn_fs(acc, real, ro.syn, None, Some("RequiredHeadFeatures"), Some("RequiredFootFeatures"))?;

    // `<RealizationalFeatures>` wrapped in the head feature (`XmlLanguageLoader.cs:972-980`):
    // `FeatureStruct.New().Feature(_headFeature).EqualTo(LoadFeatureStruct(realFeatElem, ...))`.
    // Empty (unwrapped) FS if the element is absent, matching `RealizationalAffixProcessRule`'s
    // ctor default (`FeatureStruct.New().Value`).
    let real_fs = match (ro.syn.head, real.child("RealizationalFeatures")) {
        (Some(head_fid), Some(rf)) => {
            let mut b = FeatureStructBuilder::new();
            b.add(head_fid, FeatureValue::Complex(load_syn_fs(rf, ro.syn)?));
            acc.fs_interner.intern(b.build())
        }
        _ => acc.fs_interner.intern(FeatureStruct::default()),
    };

    let mut allomorphs = Vec::new();
    for sub in real
        .elems2("MorphologicalSubrules", "MorphologicalSubrule")
        .filter(|e| e.is_active())
    {
        let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
        match load_affix_allomorph(sub, default_table, allo_id, ro, acc) {
            Ok(def) => {
                acc.allomorph_owners
                    .push(AllomorphOwner::Affix(mrule_id, allomorphs.len() as u16));
                if let Some(xid) = sub.attr("id") {
                    acc.allomorph_xml_index.insert(xid.to_string(), allo_id);
                }
                allomorphs.push(def);
            }
            Err(e) if is_droppable(&e) => {}
            Err(e) => return Err(e),
        }
    }

    if allomorphs.is_empty() {
        return Ok(None);
    }

    let morpheme = MorphemeId(acc.morphemes.len() as u32);
    acc.morphemes.push(MorphemeInfo {
        xml_key: real.attr("id").unwrap_or("").to_string(),
        morph_id: real.text_of("MorphemeId").map(str::to_string),
        gloss: real.text_of("Gloss").map(str::to_string),
        stratum: stratum_id,
        properties: load_properties(real.child("Properties")),
        co_occurrence: Vec::new(),
    });

    acc.mrules.push(MorphRuleDef::Realizational(RealizationalRuleDef {
        morpheme,
        name: real.text_of("Name").map(str::to_string),
        blockable: parse_bool(real.attr("blockable"), true),
        required_syn_fs,
        real_fs,
        allomorphs,
    }));
    Ok(Some(mrule_id))
}

fn load_affix_allomorph(
    sub: &Node,
    default_table: TableId,
    allo_id: AllomorphId,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<AffixAllomorphDef, GrammarError> {
    let mut environments = load_allomorph_environments(
        sub.child("RequiredEnvironments"),
        true,
        default_table,
        ro,
    )?;
    environments.extend(load_allomorph_environments(
        sub.child("ExcludedEnvironments"),
        false,
        default_table,
        ro,
    )?);

    // Subrule-level requirement FS carries head/foot only (no POS), per LoadAffixProcessAllomorph.
    let required_syn_fs = intern_syn_fs(acc, sub, ro.syn, None, Some("RequiredHeadFeatures"), Some("RequiredFootFeatures"))?;

    let vars = load_variables(sub.child("VariableFeatures"), ro.phon)?;

    let input = sub
        .child("MorphologicalInput")
        .ok_or_else(|| GrammarError::Semantic("MorphologicalSubrule without MorphologicalInput".into()))?;
    let required_mpr = load_mpr_set(input.attr("requiredMPRFeatures"), ro.mpr)?;
    let excluded_mpr = load_mpr_set(input.attr("excludedMPRFeatures"), ro.mpr)?;

    let mut lhs = Vec::new();
    let mut part_names: HashMap<String, PartRef> = HashMap::new();
    load_morph_lhs(input, &vars, default_table, ro, PartKind::Input, &mut lhs, &mut part_names)?;

    let output = sub
        .child("MorphologicalOutput")
        .ok_or_else(|| GrammarError::Semantic("MorphologicalSubrule without MorphologicalOutput".into()))?;
    let out_mpr = load_mpr_set(output.attr("MPRFeatures"), ro.mpr)?;
    let redup_hint = match output.attr("redupMorphType") {
        Some("prefix") => ReduplicationHint::Prefix,
        Some("suffix") => ReduplicationHint::Suffix,
        _ => ReduplicationHint::Implicit,
    };
    let rhs = load_morph_rhs(output, &vars, &part_names, default_table, ro)?;

    Ok(AffixAllomorphDef {
        id: allo_id,
        environments,
        // See `load_root_allomorph`'s matching comment: filled by the post-strata pass in `load()`.
        co_occurrence: Vec::new(),
        required_syn_fs,
        vars,
        required_mpr,
        excluded_mpr,
        out_mpr,
        redup_hint,
        lhs,
        rhs,
        properties: load_properties(sub.child("Properties")),
    })
}

#[allow(clippy::too_many_arguments)]
fn load_morph_lhs(
    input: &Node,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
    kind: PartKind,
    lhs: &mut Vec<Pattern>,
    part_names: &mut HashMap<String, PartRef>,
) -> Result<(), GrammarError> {
    for (idx, pseq) in input.elems("PhoneticSequence").enumerate() {
        if let Some(id) = pseq.attr_ne("id") {
            part_names.insert(id.to_string(), mk_part_ref(kind, idx as u16));
        }
        lhs.push(load_phonetic_sequence(Some(pseq), vars, default_table, ro)?);
    }
    Ok(())
}

fn load_morph_rhs(
    output: &Node,
    vars: &VarTable,
    part_names: &HashMap<String, PartRef>,
    default_table: TableId,
    ro: &Ro,
) -> Result<Vec<OutputAction>, GrammarError> {
    let mut rhs = Vec::new();
    for part in &output.children {
        match part.tag.as_str() {
            "CopyFromInput" => {
                let idx = part.attr("index").unwrap_or("");
                let pr = *part_names
                    .get(idx)
                    .ok_or_else(|| GrammarError::Semantic(format!("CopyFromInput unknown part '{idx}'")))?;
                rhs.push(OutputAction::Copy(pr));
            }
            "InsertSimpleContext" => {
                let sc = part
                    .child("SimpleContext")
                    .ok_or_else(|| GrammarError::Semantic("InsertSimpleContext without SimpleContext".into()))?;
                rhs.push(OutputAction::InsertContext(load_simple_context(sc, vars, ro)?));
            }
            "ModifyFromInput" => {
                let idx = part.attr("index").unwrap_or("");
                let pr = *part_names
                    .get(idx)
                    .ok_or_else(|| GrammarError::Semantic(format!("ModifyFromInput unknown part '{idx}'")))?;
                let sc = part
                    .child("SimpleContext")
                    .ok_or_else(|| GrammarError::Semantic("ModifyFromInput without SimpleContext".into()))?;
                rhs.push(OutputAction::Modify(pr, load_simple_context(sc, vars, ro)?));
            }
            "InsertSegments" => {
                let tid = match part.attr("characterDefinitionTable") {
                    Some(t) => *ro
                        .table
                        .get(t)
                        .ok_or_else(|| GrammarError::Semantic(format!("unknown table '{t}'")))?,
                    None => default_table,
                };
                let shape_str = part.text_of("PhoneticShape").unwrap_or("");
                rhs.push(OutputAction::InsertSegments {
                    table: tid,
                    shape: segment_text(tid, shape_str, ro.phon)?,
                });
            }
            _ => {}
        }
    }
    Ok(rhs)
}

fn try_load_compounding_rule(
    comp: &Node,
    default_table: TableId,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<Option<MRuleId>, GrammarError> {
    let mrule_id = MRuleId(acc.mrules.len() as u32);

    let head_required_syn_fs = intern_syn_fs(
        acc,
        comp,
        ro.syn,
        Some("headPartsOfSpeech"),
        Some("HeadRequiredHeadFeatures"),
        Some("HeadRequiredFootFeatures"),
    )?;
    let non_head_required_syn_fs = intern_syn_fs(
        acc,
        comp,
        ro.syn,
        Some("nonHeadPartsOfSpeech"),
        Some("NonHeadRequiredHeadFeatures"),
        Some("NonHeadRequiredFootFeatures"),
    )?;
    let out_syn_fs =
        intern_syn_fs(acc, comp, ro.syn, Some("outputPartOfSpeech"), Some("OutputHeadFeatures"), Some("OutputFootFeatures"))?;

    let head_prod_restrictions_mpr = load_mpr_set(comp.attr("headProdRestrictionsMprFeatures"), ro.mpr)?;
    let non_head_prod_restrictions_mpr =
        load_mpr_set(comp.attr("nonHeadProdRestrictionsMprFeatures"), ro.mpr)?;
    let output_prod_restrictions_mpr = load_mpr_set(comp.attr("outputProdRestrictionsMprFeatures"), ro.mpr)?;

    let mut obligatory_features = Vec::new();
    if let Some(ids) = comp.attr_ne("outputObligatoryFeatures") {
        for id in ids.split_whitespace() {
            let fid = ro
                .syn
                .feature_by_xml_id(id)
                .ok_or_else(|| GrammarError::Semantic(format!("unknown obligatory feature '{id}'")))?;
            obligatory_features.push(fid);
        }
    }

    let max_apps = match comp.attr_ne("multipleApplication") {
        Some(s) => s
            .parse()
            .map_err(|_| GrammarError::Semantic(format!("bad multipleApplication '{s}'")))?,
        None => 1,
    };

    let mut subrules = Vec::new();
    for sub in comp
        .elems2("CompoundingSubrules", "CompoundingSubrule")
        .filter(|e| e.is_active())
    {
        match load_compounding_subrule(sub, default_table, ro) {
            Ok(def) => subrules.push(def),
            Err(e) if is_droppable(&e) => {}
            Err(e) => return Err(e),
        }
    }

    if subrules.is_empty() {
        return Ok(None);
    }

    acc.mrules.push(MorphRuleDef::Compounding(CompoundingRuleDef {
        xml_id: comp.attr("id").unwrap_or("").to_string(),
        name: comp.text_of("Name").map(str::to_string),
        blockable: parse_bool(comp.attr("blockable"), true),
        max_apps,
        head_required_syn_fs,
        non_head_required_syn_fs,
        out_syn_fs,
        head_prod_restrictions_mpr,
        non_head_prod_restrictions_mpr,
        output_prod_restrictions_mpr,
        obligatory_features,
        subrules,
    }));
    Ok(Some(mrule_id))
}

fn load_compounding_subrule(
    sub: &Node,
    default_table: TableId,
    ro: &Ro,
) -> Result<CompoundingSubruleDef, GrammarError> {
    let vars = load_variables(sub.child("VariableFeatures"), ro.phon)?;

    let head = sub
        .child("HeadMorphologicalInput")
        .ok_or_else(|| GrammarError::Semantic("CompoundingSubrule without HeadMorphologicalInput".into()))?;
    let required_mpr = load_mpr_set(head.attr("requiredMPRFeatures"), ro.mpr)?;
    let excluded_mpr = load_mpr_set(head.attr("excludedMPRFeatures"), ro.mpr)?;

    let mut head_lhs = Vec::new();
    let mut non_head_lhs = Vec::new();
    let mut part_names: HashMap<String, PartRef> = HashMap::new();
    load_morph_lhs(head, &vars, default_table, ro, PartKind::Head, &mut head_lhs, &mut part_names)?;

    let non_head = sub
        .child("NonHeadMorphologicalInput")
        .ok_or_else(|| GrammarError::Semantic("CompoundingSubrule without NonHeadMorphologicalInput".into()))?;
    load_morph_lhs(non_head, &vars, default_table, ro, PartKind::NonHead, &mut non_head_lhs, &mut part_names)?;

    let output = sub
        .child("MorphologicalOutput")
        .ok_or_else(|| GrammarError::Semantic("CompoundingSubrule without MorphologicalOutput".into()))?;
    let out_mpr = load_mpr_set(output.attr("MPRFeatures"), ro.mpr)?;
    let rhs = load_morph_rhs(output, &vars, &part_names, default_table, ro)?;

    Ok(CompoundingSubruleDef {
        vars,
        required_mpr,
        excluded_mpr,
        out_mpr,
        head_lhs,
        non_head_lhs,
        rhs,
    })
}

fn load_affix_template(
    temp: &Node,
    local_mr: &HashMap<String, MRuleId>,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<AffixTemplateDef, GrammarError> {
    let required_syn_fs = intern_syn_fs(acc, temp, ro.syn, Some("requiredPartsOfSpeech"), None, None)?;

    let mut slots = Vec::new();
    for slot in temp.elems("Slot").filter(|e| e.is_active()) {
        let mut rules = Vec::new();
        if let Some(ids) = slot.attr_ne("morphologicalRules") {
            for id in ids.split_whitespace() {
                if let Some(mid) = local_mr.get(id) {
                    rules.push(*mid);
                }
            }
        }
        slots.push(SlotDef {
            name: slot.text_of("Name").map(str::to_string),
            optional: parse_bool(slot.attr("optional"), false),
            rules,
        });
    }

    Ok(AffixTemplateDef {
        name: temp.text_of("Name").map(str::to_string),
        is_final: parse_bool(temp.attr("final"), true),
        required_syn_fs,
        slots,
    })
}

fn try_load_lex_entry(
    entry: &Node,
    default_table: TableId,
    stratum_id: StratumId,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<Option<LexEntryId>, GrammarError> {
    // `LexicalEntry@family` (W5, `XmlLanguageLoader.cs:460-465`): resolved to a `FamilyId` now;
    // the entry is pushed onto that family's `entries` below, once its own id is known and it has
    // been confirmed to load at least one allomorph (see `Acc::families`'s doc for the one
    // documented C# edge case — a family reference on an entry that ends up with zero allomorphs —
    // this does not reproduce).
    let family = match entry.attr_ne("family") {
        Some(fid) => Some(*ro.families.get(fid).ok_or_else(|| {
            GrammarError::Semantic(format!("LexicalEntry references unknown family '{fid}'"))
        })?),
        None => None,
    };

    let syn_fs = intern_syn_fs(acc, entry, ro.syn, Some("partOfSpeech"), Some("AssignedHeadFeatures"), Some("AssignedFootFeatures"))?;
    let mpr = load_mpr_set(entry.attr("ruleFeatures"), ro.mpr)?;
    let partial = parse_bool(entry.attr("partial"), false);

    let lex_id = LexEntryId(acc.entries.len() as u32);
    let mut allomorphs = Vec::new();
    for allo in entry.elems2("Allomorphs", "Allomorph").filter(|e| e.is_active()) {
        let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
        match load_root_allomorph(allo, default_table, allo_id, ro) {
            Ok(def) => {
                acc.allomorph_owners
                    .push(AllomorphOwner::Root(lex_id, allomorphs.len() as u16));
                // C#'s `_allomorphs[(string)alloElem.Attribute("id")] = allomorph`
                // (XmlLanguageLoader.cs:477).
                if let Some(xid) = allo.attr("id") {
                    acc.allomorph_xml_index.insert(xid.to_string(), allo_id);
                }
                allomorphs.push(def);
            }
            Err(e) if is_droppable(&e) => {}
            Err(e) => return Err(e),
        }
    }

    if allomorphs.is_empty() {
        return Ok(None);
    }

    let morpheme = MorphemeId(acc.morphemes.len() as u32);
    acc.morphemes.push(MorphemeInfo {
        xml_key: entry.attr("id").unwrap_or("").to_string(),
        morph_id: entry.text_of("MorphemeId").map(str::to_string),
        gloss: entry.text_of("Gloss").map(str::to_string),
        stratum: stratum_id,
        properties: load_properties(entry.child("Properties")),
        // Filled by the post-strata co-occurrence-rule pass in `load()`.
        co_occurrence: Vec::new(),
    });

    acc.entries.push(LexEntryDef {
        morpheme,
        syn_fs,
        mpr,
        partial,
        allomorphs,
        family,
    });
    if let Some(fam) = family {
        acc.families[fam.0 as usize].entries.push(lex_id);
    }
    Ok(Some(lex_id))
}

fn load_root_allomorph(
    allo: &Node,
    default_table: TableId,
    allo_id: AllomorphId,
    ro: &Ro,
) -> Result<RootAllomorphDef, GrammarError> {
    let shape_str = allo.text_of("PhoneticShape").unwrap_or("");
    // Finding N3: `LoadRootAllomorph` is C#'s one `allowPattern = true` call site — root-allomorph
    // shapes fall back to the `[NatClass]`/`([NatClass])`/`[NatClass]*` pattern language wherever a
    // literal character-definition match fails, instead of erroring the whole allomorph out.
    let shape = segment_text_with_patterns(default_table, shape_str, ro.phon, ro.natural_class_defs)?;
    // C# throws InvalidShapeException (→ dropped) if the shape is entirely boundary markers.
    if shape.shape.interior().all(|(_, k, _, _)| k == NodeKind::Boundary) {
        return Err(GrammarError::Semantic(format!(
            "root allomorph shape {shape_str:?} is all boundaries"
        )));
    }

    let mut environments = load_allomorph_environments(
        allo.child("RequiredEnvironments"),
        true,
        default_table,
        ro,
    )?;
    environments.extend(load_allomorph_environments(
        allo.child("ExcludedEnvironments"),
        false,
        default_table,
        ro,
    )?);

    // `Allomorph@stemName` (W5, `XmlLanguageLoader.cs:513-515`).
    let stem_name = match allo.attr_ne("stemName") {
        Some(sid) => Some(*ro.stem_names.get(sid).ok_or_else(|| {
            GrammarError::Semantic(format!("Allomorph references unknown stemName '{sid}'"))
        })?),
        None => None,
    };

    // P11 §4.2: C# `RootAllomorph` ctor rule (`RootAllomorph.cs:16-29`) — any interior node that
    // is iterative, or optional-and-not-a-boundary, makes this a lexical pattern. A bare mandatory
    // `[Class]` node does NOT qualify (that's a normal trie-indexed root, e.g. `b[Vowel]t`), and
    // ordinary boundary-optional shapes (every `pi+t`-style root, since `+` is always Optional
    // after segmentation) don't qualify either — the `kind != Boundary` guard is exactly why.
    let is_pattern = shape.shape.interior().any(|(_, kind, _, flags)| {
        flags.is_iterative() || (flags.is_optional() && kind != NodeKind::Boundary)
    });

    Ok(RootAllomorphDef {
        id: allo_id,
        shape,
        is_bound: parse_bool(allo.attr("isBound"), false),
        environments,
        // Populated by the post-strata co-occurrence-rule pass in `load()`; empty at construction
        // (mirrors C#'s `RootAllomorph` ctor, whose `AllomorphCoOccurrenceRules` set starts empty
        // and is only filled later by `LoadAllomorphCoOccurrenceRule`, itself run after every
        // stratum is loaded).
        co_occurrence: Vec::new(),
        properties: load_properties(allo.child("Properties")),
        stem_name,
        is_pattern,
    })
}

fn load_properties(props: Option<&Node>) -> Vec<(String, String)> {
    match props {
        None => Vec::new(),
        Some(p) => p
            .elems("Property")
            .map(|pr| (pr.attr("name").unwrap_or("").to_string(), pr.text.clone()))
            .collect(),
    }
}

// =============================================================================================
// Deterministic loader dump (plan §8 layer-1 gate).
// =============================================================================================

impl Grammar {
    /// A normalized, deterministic, human-readable structural inventory of the grammar, for the
    /// plan §8 layer-1 loader gate (diffed against counts derived independently from the XML).
    /// Mirrors the style of [`crate::GrammarPhonology::dump_char_defs`]; iterates only `Vec`
    /// order and interner id order (never a `HashMap`), so it is stable across re-loads.
    pub fn dump_grammar(&self) -> String {
        let mut out = String::new();

        // Syntactic feature system.
        let _ = writeln!(
            out,
            "syn_features={} pos={} head={:?}",
            self.syn_features.features.len(),
            self.syn_features.pos.0,
            self.syn_features.head.map(|f| f.0)
        );
        for (i, f) in self.syn_features.features.iter().enumerate() {
            let kind = match &f.kind {
                SynFeatureKind::Symbolic {
                    symbols,
                    default_symbol,
                } => format!("Symbolic symbols={} default={default_symbol:?}", symbols.len()),
                SynFeatureKind::Complex => "Complex".to_string(),
            };
            let _ = writeln!(out, "  feat[{i}] id={} name={} {kind}", f.xml_id, f.name);
        }

        // MPR features and groups.
        let _ = writeln!(out, "mpr_features={}", self.mpr_names.len());
        for (i, n) in self.mpr_names.iter().enumerate() {
            let _ = writeln!(out, "  mpr[{i}] {n}");
        }
        let _ = writeln!(out, "mpr_groups={}", self.mpr_groups.len());
        for (i, g) in self.mpr_groups.iter().enumerate() {
            let _ = writeln!(
                out,
                "  group[{i}] name={:?} match={:?} output={:?} members={:#b}",
                g.name, g.match_type, g.output, g.members.0
            );
        }

        // Natural classes.
        let _ = writeln!(out, "natural_classes={}", self.natural_classes.len());
        for (i, nc) in self.natural_classes.iter().enumerate() {
            let kind = match &nc.kind {
                NaturalClassKind::Feature(v) => format!("Feature lanes={}", v.len()),
                NaturalClassKind::Segments(v) => format!("Segments segs={}", v.len()),
            };
            let _ = writeln!(out, "  nc[{i}] id={} name={:?} {kind}", nc.xml_id, nc.name);
        }

        // Phonological rules.
        let _ = writeln!(out, "prules={}", self.prules.len());
        for (i, p) in self.prules.iter().enumerate() {
            match p {
                PhonRuleDef::Rewrite(p) => {
                    let _ = writeln!(
                        out,
                        "  prule[{i}] id={} mode={:?} dir={:?} lhs_nodes={} subrules={}",
                        p.xml_id,
                        p.mode,
                        p.dir,
                        p.lhs.nodes.len(),
                        p.subrules.len()
                    );
                }
                PhonRuleDef::Metathesis(p) => {
                    let _ = writeln!(
                        out,
                        "  prule[{i}] id={} Metathesis dir={:?} pattern_nodes={} left_switch={} right_switch={}",
                        p.xml_id,
                        p.dir,
                        p.pattern.nodes.len(),
                        p.left_switch,
                        p.right_switch
                    );
                }
            }
        }

        // Morphological rules.
        let _ = writeln!(out, "mrules={}", self.mrules.len());
        for (i, m) in self.mrules.iter().enumerate() {
            match m {
                MorphRuleDef::AffixProcess(a) => {
                    let _ = writeln!(
                        out,
                        "  mrule[{i}] AffixProcess morpheme={} allomorphs={}",
                        a.morpheme.0,
                        a.allomorphs.len()
                    );
                }
                MorphRuleDef::Compounding(c) => {
                    let _ = writeln!(
                        out,
                        "  mrule[{i}] Compounding id={} subrules={}",
                        c.xml_id,
                        c.subrules.len()
                    );
                }
                MorphRuleDef::Realizational(r) => {
                    let _ = writeln!(
                        out,
                        "  mrule[{i}] Realizational morpheme={} allomorphs={}",
                        r.morpheme.0,
                        r.allomorphs.len()
                    );
                }
            }
        }

        // Affix templates.
        let _ = writeln!(out, "templates={}", self.templates.len());
        for (i, t) in self.templates.iter().enumerate() {
            let _ = writeln!(
                out,
                "  template[{i}] name={:?} final={} slots={}",
                t.name,
                t.is_final,
                t.slots.len()
            );
            for (j, s) in t.slots.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "    slot[{j}] name={:?} optional={} rules={}",
                    s.name,
                    s.optional,
                    s.rules.len()
                );
            }
        }

        // Lexicon (totals; per-entry lines would be O(thousands) for Sena).
        let total_root_allos: usize = self.entries.iter().map(|e| e.allomorphs.len()).sum();
        let _ = writeln!(
            out,
            "entries={} root_allomorphs={}",
            self.entries.len(),
            total_root_allos
        );

        // Strata (per-section counts).
        let _ = writeln!(out, "strata={}", self.strata.len());
        for (i, s) in self.strata.iter().enumerate() {
            let _ = writeln!(
                out,
                "  stratum[{i}] name={:?} order={:?} table={} prules={} mrules={} templates={} entries={}",
                s.name,
                s.mrule_order,
                s.table.0,
                s.prules.len(),
                s.mrules.len(),
                s.templates.len(),
                s.entries.len()
            );
        }

        // Registries.
        let _ = writeln!(out, "morphemes={}", self.morphemes.len());
        let _ = writeln!(out, "allomorphs={}", self.allomorph_owners.len());
        let _ = writeln!(out, "fs_interned={}", self.fs_interner.len());

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Same self-skipping corpus-file locator as the lib.rs segmentation tests: the `*-hc.xml`
    /// grammars are untracked local corpus files (`rust-conversion.md` §8), present on this dev
    /// machine but not guaranteed elsewhere. Returns `None` if absent so callers self-skip.
    fn sample_path(name: &str) -> Option<PathBuf> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn pos_symbol_count(g: &Grammar) -> usize {
        match &g.syn_features.features[g.syn_features.pos.0 as usize].kind {
            SynFeatureKind::Symbolic { symbols, .. } => symbols.len(),
            SynFeatureKind::Complex => 0,
        }
    }

    /// Load a real grammar and assert exact structural counts. Every expected count was obtained
    /// **independently of this loader** by grepping the raw XML (the exact greps are stated per
    /// call site), so a match is real evidence the loader isn't dropping/double-counting, not an
    /// echo of its own output.
    #[allow(clippy::too_many_arguments)]
    fn check(
        xml_name: &str,
        expect_syn_features: usize,
        expect_pos: usize,
        expect_nat_classes: usize,
        expect_prules: usize,
        expect_mrules: usize,
        expect_templates: usize,
        expect_entries: usize,
        expect_strata: usize,
    ) {
        let Some(path) = sample_path(xml_name) else {
            eprintln!("skipping {xml_name}: sample grammar not present on disk");
            return;
        };
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        let g = load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"));

        assert_eq!(g.syn_features.features.len(), expect_syn_features, "{xml_name}: syn features");
        assert_eq!(pos_symbol_count(&g), expect_pos, "{xml_name}: POS symbols");
        assert_eq!(g.natural_classes.len(), expect_nat_classes, "{xml_name}: natural classes");
        assert_eq!(g.prules.len(), expect_prules, "{xml_name}: phonological rules");
        assert_eq!(g.mrules.len(), expect_mrules, "{xml_name}: morphological rules");
        assert_eq!(g.templates.len(), expect_templates, "{xml_name}: templates");
        assert_eq!(g.entries.len(), expect_entries, "{xml_name}: lexical entries");
        assert_eq!(g.strata.len(), expect_strata, "{xml_name}: strata");

        // Every morphological AffixProcess rule and every lexical entry is a morpheme.
        let affix_mrules = g
            .mrules
            .iter()
            .filter(|m| matches!(m, MorphRuleDef::AffixProcess(_)))
            .count();
        assert_eq!(
            g.morphemes.len(),
            affix_mrules + g.entries.len(),
            "{xml_name}: morpheme registry = affix-process rules + lexical entries"
        );

        // The empty FS is always interned first (FsId 0).
        assert!(!g.fs_interner.is_empty(), "{xml_name}: empty FS must be interned");
    }

    #[test]
    fn loads_indonesian() {
        // Independently confirmed via `grep -c` on indonesian-hc.xml:
        //   syn features = 1 POS + 1 head (empty <HeadFeatures/> still adds it) + 0 head-declared = 2
        //   POS symbols  = 6  (`<PartOfSpeech `)
        //   natural classes = 10 `<FeatureNaturalClass ` + 4 `<SegmentNaturalClass ` = 14
        //   prules = 5 `<PhonologicalRule ` ; mrules = 13 `<MorphologicalRule ` + 2 `<CompoundingRule ` = 15
        //   templates = 0 `<AffixTemplate ` ; entries = 66 `<LexicalEntry ` ; strata = 3 `<Stratum `
        check("indonesian-hc.xml", 2, 6, 14, 5, 15, 0, 66, 3);
    }

    #[test]
    fn loads_amharic() {
        // Independently confirmed via `grep -c` on amharic-hc.xml:
        //   syn features = 1 POS + 1 head + 16 head-declared (11 SymbolicFeature + 5 ComplexFeature
        //                  inside the <HeadFeatures> block) = 18
        //   POS symbols = 16 ; natural classes = 14 Feature + 3 Segment = 17
        //   prules = 7 ; mrules = 87 `<MorphologicalRule ` + 1 `<CompoundingRule ` = 88
        //   templates = 15 ; entries = 76 ; strata = 3
        check("amharic-hc.xml", 18, 16, 17, 7, 88, 15, 76, 3);
    }

    #[test]
    fn loads_sena() {
        // Independently confirmed via `grep -c` on sena-hc.xml:
        //   syn features = 1 POS + 1 head + 3 head-declared = 5 ; POS symbols = 37
        //   natural classes = 1 Feature + 12 Segment = 13
        //   prules = 0 ; mrules = 132 `<MorphologicalRule ` + 8 `<CompoundingRule ` = 140
        //   templates = 24 ; entries = 1371 `<LexicalEntry ` ; strata = 3
        check("sena-hc.xml", 5, 37, 13, 0, 140, 24, 1371, 3);
    }

    #[test]
    fn dump_grammar_is_deterministic() {
        for name in ["indonesian-hc.xml", "amharic-hc.xml", "sena-hc.xml"] {
            let Some(path) = sample_path(name) else {
                eprintln!("skipping {name}: not present");
                continue;
            };
            let xml = std::fs::read_to_string(&path).unwrap();
            let d1 = load(&xml).unwrap().dump_grammar();
            let d2 = load(&xml).unwrap().dump_grammar();
            assert_eq!(d1, d2, "{name}: dump must be deterministic across re-loads");
        }
    }

    #[test]
    fn dump_grammar_reports_headline_counts() {
        let Some(path) = sample_path("indonesian-hc.xml") else {
            eprintln!("skipping: indonesian not present");
            return;
        };
        let xml = std::fs::read_to_string(&path).unwrap();
        let dump = load(&xml).unwrap().dump_grammar();
        assert!(dump.contains("strata=3"), "dump:\n{dump}");
        assert!(dump.contains("mrules=15"));
        assert!(dump.contains("prules=5"));
        assert!(dump.contains("entries=66"));
    }

    /// A hand-built minimal grammar exercises the loader end-to-end without the corpus files,
    /// so it runs in CI too: POS + empty head, one MPR feature, one natural class, one stratum
    /// with one affix-process rule (one allomorph) and one lexical entry.
    #[test]
    fn loads_hand_built_minimal_grammar() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Mini</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures />
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cA" /><Segment segment="cB" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr2 bogus mr1">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-b</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput MPRFeatures="mprA">
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mr2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-a</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub2">
                <MorphologicalInput>
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem2" />
                  <InsertSegments><PhoneticShape>+a</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>do</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = load(XML).unwrap();
        assert_eq!(g.name.as_deref(), Some("Mini"));
        assert_eq!(g.syn_features.features.len(), 2); // POS + head (empty HeadFeatures still added)
        assert_eq!(pos_symbol_count(&g), 2);
        assert_eq!(g.mpr_names, vec!["Alpha".to_string()]);
        assert_eq!(g.natural_classes.len(), 1);
        assert_eq!(g.strata.len(), 1);
        assert_eq!(g.mrules.len(), 2);
        assert_eq!(g.entries.len(), 1);
        assert_eq!(g.morphemes.len(), 3); // two affix rules + one entry
        assert_eq!(g.allomorph_owners.len(), 3);
        assert_eq!(g.strata[0].mrule_order, MorphRuleOrder::Unordered);
        // Parity-critical: stratum rules follow the `morphologicalRules` id-list ORDER, not
        // document order, and unknown ids ("bogus") are silently skipped. mr1/mr2 are assigned
        // MRuleId 0/1 in document order, but the attribute lists them "mr2 bogus mr1".
        assert_eq!(g.strata[0].mrules, vec![MRuleId(1), MRuleId(0)]);
        // Affix rule references its output MPR feature and has one allomorph.
        let MorphRuleDef::AffixProcess(a) = &g.mrules[0] else {
            panic!("expected affix process rule");
        };
        assert_eq!(a.allomorphs.len(), 1);
        assert!(a.allomorphs[0].out_mpr.contains(MprId(0)));
        // The RHS copies the captured input part then inserts "+b".
        assert!(matches!(a.allomorphs[0].rhs[0], OutputAction::Copy(PartRef::Input(0))));
    }

    /// Finding N3: a root-allomorph `<PhoneticShape>` whose text doesn't literally match a
    /// character definition at some position must fall back to the `[NatClass]` pattern language
    /// (`load_root_allomorph` -> `segment_text_with_patterns` -> `segment::segment_with_patterns`)
    /// instead of erroring the whole allomorph out. Pre-fix (plain `segment_text`/`segment::segment`,
    /// no pattern awareness at all), `load("b[Vowel]t")` would return `GrammarError::Semantic`
    /// from `segment()`'s `InvalidShape` at the `[`, which `load_stratum`'s allomorph-loading loop
    /// treats as droppable (`is_droppable`); this fixture's entry has only that one allomorph, so
    /// dropping it drops the *entry* too (confirmed empirically by reverting the fix locally:
    /// `g.entries.len()` goes to 0, not just `allomorphs.len()`). This test's assertions are the
    /// red-on-revert signal for that: reverting `load_root_allomorph`'s call back to
    /// `segment_text` makes both fail (the entry and its allomorph silently disappear, exactly
    /// the class of bug the audit flags).
    #[test]
    fn root_allomorph_shape_falls_back_to_pattern_language_natural_class_reference() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N3Test</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncVowel"><Name>Vowel</Name><Segment segment="cA" /><Segment segment="cE" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="n">
            <Allomorphs>
              <Allomorph id="a1"><PhoneticShape>b[Vowel]t</PhoneticShape></Allomorph>
            </Allomorphs>
            <Gloss>bVt</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
        assert_eq!(g.entries.len(), 1, "the lexical entry itself must survive");
        let entry = &g.entries[0];
        assert_eq!(
            entry.allomorphs.len(),
            1,
            "the allomorph must NOT be silently dropped (pattern-language fallback missing?)"
        );
        let shape = &entry.allomorphs[0].shape.shape;
        let interior: Vec<_> = shape.interior().collect();
        assert_eq!(interior.len(), 3, "b, [Vowel], t");
        let a_id = g.char_tables[0].lookup_nfd("a").unwrap();
        let e_id = g.char_tables[0].lookup_nfd("e").unwrap();
        assert_eq!(interior[0].2, g.char_tables[0].lookup_nfd("b").unwrap().0);
        assert_eq!(interior[2].2, g.char_tables[0].lookup_nfd("t").unwrap().0);
        assert_eq!(interior[1].2, hc_shape::NO_CHAR_DEF, "the class reference is an abstract node");
        match shape.node_cd_set(interior[1].0) {
            hc_shape::EffectiveCdSet::Members(b) => {
                assert!(b.contains(a_id.0) && b.contains(e_id.0));
                assert_eq!(b.count(), 2);
            }
            other => panic!("expected Members({{a,e}}), got {other:?}"),
        }
    }

    /// P11 §4.2: `RootAllomorphDef.is_pattern` must match C#'s `RootAllomorph` ctor rule exactly
    /// (`RootAllomorph.cs:16-29`) — any interior node that is iterative, or optional-and-not-a-
    /// boundary, classifies the whole allomorph as a lexical pattern (diverted out of the trie,
    /// P11 chunk 2). One grammar, one allomorph per shape variant, checked by entry order.
    #[test]
    fn is_pattern_matches_the_csharp_root_allomorph_classification_rule() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PatternClassTest</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncVowel"><Name>Vowel</Name><Segment segment="cA" /><Segment segment="cE" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="cB" /><Segment segment="cP" /><Segment segment="cI" /><Segment segment="cT" /><Segment segment="cA" /><Segment segment="cE" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e_star" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_star"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>star</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_opt" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_opt"><PhoneticShape>([Vowel])</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>opt</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_mandatory_class" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_mandatory_class"><PhoneticShape>b[Vowel]t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>bVt</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_plain" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_plain"><PhoneticShape>pit</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pit</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_boundary" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_boundary"><PhoneticShape>pi+t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>piPlusT</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
        assert_eq!(g.entries.len(), 5, "every entry must survive with its one allomorph");
        let is_pattern = |i: usize| {
            let e = &g.entries[i];
            assert_eq!(e.allomorphs.len(), 1, "entry {i}: exactly one allomorph");
            e.allomorphs[0].is_pattern
        };
        assert!(is_pattern(0), "[Any]* : iterative -> pattern");
        assert!(is_pattern(1), "([Vowel]) : optional, not a boundary -> pattern");
        assert!(!is_pattern(2), "b[Vowel]t : mandatory (non-optional, non-iterative) class -> NOT a pattern");
        assert!(!is_pattern(3), "pit : plain literal shape -> NOT a pattern");
        assert!(
            !is_pattern(4),
            "pi+t : the only optional node is the boundary ('+' is always Optional after \
             segmentation) -> NOT a pattern (the kind != Boundary guard)"
        );
    }

    /// F1 (HYBRID_FST_RUST_PLAN.md §7.1 item 4): `<FootFeatures>` is no longer hard-linted
    /// unsupported — the `fst-advisor-toys/HermitCrabTestBase.shared.xml` fixture needs it (an
    /// empty `<FootFeatures/>` plus `AssignedFootFeatures` referencing head-declared features).
    /// This test now pins the *positive* behavior: `<FootFeatures>` loads, adds a foot complex
    /// feature (mirroring `<HeadFeatures>`/`syn.head` exactly), and its own declared features join
    /// the shared syntactic feature namespace (confirmed against `XmlLanguageLoader.cs:244-256`
    /// which adds both under the SAME `SyntacticFeatureSystem` — see the `SynFeatureSystem::foot`
    /// doc). Was `foot_features_lints_unsupported` pre-F1.
    #[test]
    fn foot_features_loads_as_a_complex_feature_mirroring_head() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
          <FootFeatures><SymbolicFeature id="f"><Name>x</Name><Symbols><Symbol id="s">+</Symbol></Symbols></SymbolicFeature></FootFeatures>
        </Language></HermitCrabInput>"#;
        let g = load(XML).expect("FootFeatures must load, not lint unsupported");
        assert!(g.syn_features.foot.is_some(), "foot complex feature must be present");
        assert!(
            g.syn_features.feature_by_xml_id("f").is_some(),
            "foot-declared feature 'f' must join the syntactic feature namespace"
        );
    }

    /// An absent `<FootFeatures>` element must still leave `syn.foot == None` (no spurious complex
    /// feature invented) — the exact `<HeadFeatures>`-absent behavior already pinned elsewhere.
    #[test]
    fn absent_foot_features_element_leaves_foot_none() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
        </Language></HermitCrabInput>"#;
        let g = load(XML).expect("grammar with no FootFeatures must still load");
        assert!(g.syn_features.foot.is_none());
    }

    /// Regression test for the `<AffixTemplate final>` bug (rust-conversion.md §13.1 T1-#2):
    /// C#'s DTD-validating XML reader (`XmlLanguageLoader.cs:209-218` + `HermitCrabInput.dtd:259`,
    /// `final (true | false) "true"`) materializes the DTD default into the parsed tree, so an
    /// omitted `final` attribute reads as `true` in C#. The loader must match. This test also
    /// pins every other DTD `ATTLIST` default reachable from `load.rs` (full sweep against
    /// `HermitCrabInput.dtd`, beyond the single `final` spot-check), so a future change can't
    /// silently reintroduce a mismatch of this class:
    ///
    /// - `AffixTemplate final` "true" (the bug fixed here)
    /// - `Slot optional` "false"
    /// - `MorphologicalRule blockable` "true", `partial` "false", `multipleApplication` "1"
    /// - `Allomorph isBound` "false"
    /// - `MorphologicalOutput redupMorphType` "implicit"
    /// - `MorphologicalPhonologicalRuleFeatureGroup matchType` "any", `outputType` "overwrite"
    /// - `Stratum morphologicalRuleOrder` "linear"
    /// - `PhonologicalRule multipleApplicationOrder` "leftToRightIterative"
    ///
    /// (`isActive` "yes" is exercised pervasively elsewhere via `Node::is_active`; `Stratum`'s
    /// `cyclicity`/`phonologicalRuleOrder` attributes are not modeled by this loader at all — a
    /// separate architectural scope-cut, not a wrong-default bug, and neither attribute appears
    /// in any of the three reference grammars.)
    #[test]
    fn dtd_attribute_defaults_match_spec() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Defaults</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures />
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup features="mprA"><Name>G</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cA" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="pr1">
        <Name>PR</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-a</Name>
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
        <AffixTemplates>
          <AffixTemplate>
            <Name>T1</Name>
            <Slot morphologicalRules="mr1"><Name>Sl1</Name></Slot>
          </AffixTemplate>
          <AffixTemplate final="false">
            <Name>T2</Name>
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
        let g = load(XML).unwrap();

        // AffixTemplate final: omitted -> true (the bug), explicit "false" -> false.
        assert!(g.templates[0].is_final, "AffixTemplate final defaults to true per DTD");
        assert!(!g.templates[1].is_final, "explicit final=\"false\" must still be honored");

        // Slot optional: omitted -> false.
        assert!(!g.templates[0].slots[0].optional);

        // MorphologicalRule blockable/partial/multipleApplication: all omitted.
        let MorphRuleDef::AffixProcess(mr1) = &g.mrules[0] else {
            panic!("expected affix process rule");
        };
        assert!(mr1.blockable, "MorphologicalRule blockable defaults to true per DTD");
        assert!(!mr1.partial, "MorphologicalRule partial defaults to false per DTD");
        assert_eq!(mr1.max_apps, 1, "MorphologicalRule multipleApplication defaults to 1 per DTD");

        // MorphologicalOutput redupMorphType: omitted -> Implicit.
        assert_eq!(mr1.allomorphs[0].redup_hint, ReduplicationHint::Implicit);

        // Allomorph isBound: omitted -> false.
        assert!(!g.entries[0].allomorphs[0].is_bound, "Allomorph isBound defaults to false per DTD");

        // MorphologicalPhonologicalRuleFeatureGroup matchType/outputType: both omitted.
        assert_eq!(g.mpr_groups[0].match_type, MprGroupMatchType::Any);
        assert_eq!(g.mpr_groups[0].output, MprGroupOutput::Overwrite);

        // Stratum morphologicalRuleOrder: omitted -> Linear.
        assert_eq!(g.strata[0].mrule_order, MorphRuleOrder::Linear);

        // PhonologicalRule multipleApplicationOrder: omitted -> leftToRightIterative.
        let PhonRuleDef::Rewrite(pr0) = &g.prules[0] else {
            panic!("expected a rewrite rule");
        };
        assert_eq!(pr0.mode, RewriteMode::Iterative);
        assert_eq!(pr0.dir, Dir::LeftToRight);
    }

    /// P13: the former W1.4 stopgap lint (`RewriteMode::Simultaneous` hard-failed at load because
    /// it had zero readers in `hc-rules`) is gone now that Simultaneous has real execution
    /// semantics (`rust/docs/p13-simultaneous-design.md`). This test now asserts the POSITIVE
    /// case the lint used to block: a `multipleApplicationOrder="simultaneous"` rule loads
    /// successfully and round-trips into `RewriteRuleDef.mode`.
    #[test]
    fn rewrite_mode_simultaneous_loads_and_round_trips() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
        let g = load(XML).unwrap_or_else(|e| panic!("simultaneous rule must load: {e}"));
        let PhonRuleDef::Rewrite(pr0) = &g.prules[0] else {
            panic!("expected a rewrite rule");
        };
        assert_eq!(pr0.mode, RewriteMode::Simultaneous, "multipleApplicationOrder=\"simultaneous\" must round-trip");

        // Sanity: the same rule with `leftToRightIterative` (the default) must still load fine and
        // round-trip Iterative — confirms this isn't a blanket "always Simultaneous now" bug.
        let iterative_xml = XML.replace(r#" multipleApplicationOrder="simultaneous""#, "");
        let g2 = load(&iterative_xml).expect("an ordinary iterative rule must still load");
        let PhonRuleDef::Rewrite(pr0b) = &g2.prules[0] else {
            panic!("expected a rewrite rule");
        };
        assert_eq!(pr0b.mode, RewriteMode::Iterative);
    }

    /// P13 §4.3 / §7 open question 5's own ask ("Unit-test the `IsUnifiable`-equivalent
    /// computation directly against hand-built patterns"): pin `RewriteSubruleDef::self_opaquing`'s
    /// per-kind formula directly, mirroring C#'s own dispatch (`AnalysisRewriteRule.cs:26-104`)
    /// rather than just eyeballing agreement against the design doc's prose. Five rules, one
    /// subrule each:
    /// - `prA` (Feature, Simultaneous): RHS pin `Voiced` (voi+) IS feature-unifiable with its
    ///   RightEnvironment `Voiced` (voi+ again, bits overlap) -> `self_opaquing` must be `false`.
    /// - `prB` (Feature, Simultaneous): RHS pin `Voiced` (voi+) is NOT unifiable with its
    ///   RightEnvironment `Voiceless` (voi-, disjoint bits on the SAME feature) -> `true`.
    /// - `prC`: identical patterns to `prB` but `multipleApplicationOrder` omitted (Iterative) ->
    ///   `false` (the mode gate short-circuits before the unifiability check ever runs).
    /// - `prD` (Epenthesis: empty LHS, Simultaneous): `true` unconditionally, no unifiability
    ///   precheck for this branch (`AnalysisRewriteRule.cs:75-80`).
    /// - `prE` (Narrow: 2-node LHS, 1-node RHS, Simultaneous): `false` -- irrelevant field, this
    ///   kind is always unconditionally Simultaneous+Deletion regardless of `rule.mode`.
    #[test]
    fn self_opaquing_pin_semantics_match_node_pins() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SelfOpaquingProbe</Name>
          <PhonologicalFeatureSystem>
            <SymbolicFeature id="featCons"><Name>cons</Name>
              <Symbols><Symbol id="symConsP">+</Symbol><Symbol id="symConsM">-</Symbol></Symbols>
            </SymbolicFeature>
            <SymbolicFeature id="featVoi"><Name>voi</Name>
              <Symbols><Symbol id="symVoiP">+</Symbol><Symbol id="symVoiM">-</Symbol></Symbols>
            </SymbolicFeature>
          </PhonologicalFeatureSystem>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations>
                <FeatureValue feature="featCons" symbolValues="symConsP" />
                <FeatureValue feature="featVoi" symbolValues="symVoiM" />
              </SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <FeatureNaturalClass id="ncStop"><Name>Stop</Name>
              <FeatureValue feature="featCons" symbolValues="symConsP" />
            </FeatureNaturalClass>
            <FeatureNaturalClass id="ncVoiced"><Name>Voiced</Name>
              <FeatureValue feature="featVoi" symbolValues="symVoiP" />
            </FeatureNaturalClass>
            <FeatureNaturalClass id="ncVoiceless"><Name>Voiceless</Name>
              <FeatureValue feature="featVoi" symbolValues="symVoiM" />
            </FeatureNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prA" multipleApplicationOrder="simultaneous"><Name>ruleA</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prB" multipleApplicationOrder="simultaneous"><Name>ruleB</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prC"><Name>ruleC</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prD" multipleApplicationOrder="simultaneous"><Name>ruleD</Name>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prE" multipleApplicationOrder="simultaneous"><Name>ruleE</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
        let g = load(XML).unwrap_or_else(|e| panic!("self-opaquing probe grammar must load: {e}"));
        let rewrite = |i: usize| -> &RewriteRuleDef {
            let PhonRuleDef::Rewrite(r) = &g.prules[i] else { panic!("expected a rewrite rule at {i}") };
            r
        };
        assert!(
            !rewrite(0).subrules[0].self_opaquing,
            "prA: RHS pin unifiable with its environment -> Normal reapply, not self-opaquing"
        );
        assert!(
            rewrite(1).subrules[0].self_opaquing,
            "prB: RHS pin NOT unifiable with its environment (disjoint voi bits) -> self-opaquing"
        );
        assert!(
            !rewrite(2).subrules[0].self_opaquing,
            "prC: same patterns as prB but Iterative mode -> mode gate short-circuits to false"
        );
        assert!(rewrite(3).subrules[0].self_opaquing, "prD: Epenthesis + Simultaneous is unconditionally self-opaquing");
        assert!(
            !rewrite(4).subrules[0].self_opaquing,
            "prE: Narrow/Expansion is irrelevant/always false regardless of rule.mode"
        );
    }

    /// Plan §6 item 6 (audit phase2/C-loader-parity.md §item 6): pin the exact `>= 64` vs `> 64`
    /// boundary for the two symbol-count caps a real grammar is most likely to hit. Neither edge
    /// was previously exercised by a unit test (all 3 reference grammars sit comfortably under 64).
    fn pos_grammar_with_n_symbols(n: usize) -> String {
        let mut pos = String::new();
        for i in 0..n {
            pos.push_str(&format!(r#"<PartOfSpeech id="p{i}"><Name>p{i}</Name></PartOfSpeech>"#));
        }
        format!(r#"<HermitCrabInput><Language><Name>X</Name><PartsOfSpeech>{pos}</PartsOfSpeech></Language></HermitCrabInput>"#)
    }

    #[test]
    fn pos_symbol_cap_63_ok_64_rejected() {
        let ok = load(&pos_grammar_with_n_symbols(63));
        assert!(ok.is_ok(), "63 parts of speech must load fine: {:?}", ok.err());
        let bad = load(&pos_grammar_with_n_symbols(64));
        assert!(
            matches!(bad, Err(GrammarError::Unsupported(_))),
            "64 parts of speech must be rejected at the 64-symbol boundary, got {bad:?}"
        );
    }

    fn phon_feature_grammar_with_n_symbols(n: usize) -> String {
        let mut syms = String::new();
        for i in 0..n {
            syms.push_str(&format!(r#"<Symbol id="s{i}">v{i}</Symbol>"#));
        }
        format!(
            r#"<HermitCrabInput><Language><Name>X</Name>
              <PhonologicalFeatureSystem>
                <SymbolicFeature id="f1"><Name>f</Name><Symbols>{syms}</Symbols></SymbolicFeature>
              </PhonologicalFeatureSystem>
            </Language></HermitCrabInput>"#
        )
    }

    #[test]
    fn phonological_symbolic_feature_cap_63_ok_64_rejected() {
        let ok = load(&phon_feature_grammar_with_n_symbols(63));
        assert!(ok.is_ok(), "a 63-symbol phonological feature must load fine: {:?}", ok.err());
        let bad = load(&phon_feature_grammar_with_n_symbols(64));
        assert!(
            matches!(bad, Err(GrammarError::Unsupported(_))),
            "a 64-symbol phonological feature must be rejected at the 64-symbol boundary, got {bad:?}"
        );
    }
}
