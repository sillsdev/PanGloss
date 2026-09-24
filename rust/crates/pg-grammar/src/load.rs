//! Full-grammar loader (plan §5.5): parse a HermitCrab `*-hc.xml` document into the frozen
//! runtime tables of `crate::model::Grammar`. A faithful port of the C#
//! `XmlLanguageLoader.cs` object-model construction.
//!
//! ## Two passes, one file
//! Pass 1 reuses `crate::load_char_def_table_from_xml` to build the phonological census
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
//! ## v1 lint surface (plan §8 layer 6; see `crate::model` docs)
//! `FootFeatures`, `StemName`, `Family` (with entries), `MetathesisRule`, `RealizationalRule`,
//! `MorphemeCoOccurrenceRule`, `AllomorphCoOccurrenceRule`, `AlphaVariable` in an allomorph
//! environment, >=64 symbols in a symbolic feature, and >64 MPR features all lint
//! `GrammarError::Unsupported` → managed fallback. The three reference grammars contain none
//! of these, so a correct loader loads all three without an `Unsupported` error.

use hashbrown::{HashMap, HashSet};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use pg_featstruct::{
    FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, Interner, SymbolBits,
};
use pg_shape::NodeKind;

use crate::chardef::{CharDefId, CharDefTable};
use crate::featsys::{FlatIndex, PhonFeatureSystem};
use crate::model::*;
use crate::segment::{segment, segment_with_patterns};
use crate::{load_char_def_table_from_xml, GrammarError, GrammarPhonology};

// Minimal read-only DOM built from quick_xml events (mirrors the XElement subset the C# uses).

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
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
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

/// Parse the whole document into a synthetic root node (its children are the top-level elements, e.g. `HermitCrabInput`).
fn parse_document(xml: &str) -> Result<Node, GrammarError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    // quick_xml defaults this to false, so `--` inside a comment was silently tolerated.
    reader.config_mut().check_comments = true;

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

// Read-only resolvers (`Ro`) and mutable accumulators (`Acc`).

/// Read-only resolution tables used while building patterns and feature structs, held by reference so `Acc` (the mutable side) can be borrowed independently.
struct Ro<'a> {
    phon: &'a GrammarPhonology,
    syn: &'a SynFeatureSystem,
    /// natural-class XML id → dense id.
    natclass: &'a HashMap<String, NatClassId>,
    /// Full natural-class definitions, document order (dense-id-indexed); only `load_root_allomorph` needs the definitions themselves, for its `[ClassName]` pattern-language lookup by `<Name>` text.
    natural_class_defs: &'a [NaturalClass],
    /// character-definition XML id → (owning table, per-table id).
    chardef: &'a HashMap<String, (TableId, CharDefId)>,
    /// character-definition-table XML id → dense id.
    table: &'a HashMap<String, TableId>,
    /// MPR feature XML id → bit position.
    mpr: &'a HashMap<String, MprId>,
    /// `<StemName>` XML id → dense id; fixed before the strata loop starts, so a plain `Ro` field unlike `families`' entry-membership half, which mutates during the strata loop and lives on `Acc`.
    stem_names: &'a HashMap<String, StemNameId>,
    /// `<Family>` XML id → dense id, fixed the same way; see `Acc::families` for where the mutable `entries` list this indexes into lives.
    families: &'a HashMap<String, FamilyId>,
}

/// Everything the loader appends to as it walks the strata.
struct Acc {
    fs_interner: Interner<FeatureStruct>,
    mrules: Vec<MorphRuleDef>,
    morphemes: Vec<MorphemeInfo>,
    allomorph_owners: Vec<AllomorphOwner>,
    allomorph_sources: Vec<AllomorphSource>,
    templates: Vec<AffixTemplateDef>,
    entries: Vec<LexEntryDef>,
    /// `<Family>` definitions, pre-seeded (name + empty `entries`) before the strata loop starts; `try_load_lex_entry` pushes each successfully-loaded entry's `LexEntryId` onto its family's `entries` as it goes.
    families: Vec<FamilyDef>,
    /// XML `id` → `AllomorphId` (`<Allomorph id="...">` and `<MorphologicalSubrule id="...">` share one namespace, so this does too); consumed only by the post-strata `<AllomorphCoOccurrenceRule>` pass in `load()`.
    allomorph_xml_index: HashMap<String, AllomorphId>,
}

/// Which morphological input list a captured LHS part belongs to (drives the `PartRef` kind).
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

/// A load error that should drop the current allomorph rather than abort the whole load; `Unsupported` is never a drop, since it must propagate to trigger managed fallback.
fn is_droppable(e: &GrammarError) -> bool {
    !matches!(e, GrammarError::Unsupported(_))
}

/// C# `XmlLanguageLoader.GetMorphCoOccurrenceAdjacency`: unknown/absent values default to `Anywhere` (also the DTD's own default).
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

// Entry point.

/// Load a full HermitCrab XML grammar into the frozen `Grammar` runtime tables.
///
/// Faithful port of `XmlLanguageLoader.Load`. Constructs outside the v1 surface lint
/// `GrammarError::Unsupported`; malformed references (unknown feature/symbol/natural-class ids)
/// surface `GrammarError::Semantic`; XML errors surface `GrammarError::Xml`.
pub fn load(xml: &str) -> Result<Grammar, GrammarError> {
    // Pass 1: phonological feature system + character-definition tables.
    let phon = load_char_def_table_from_xml(xml)?;

    // Pass 2: DOM of the active <Language>.
    let root = parse_document(xml)?;
    let lang = root
        .child("HermitCrabInput")
        .and_then(|hc| hc.elems("Language").find(|l| l.is_active()))
        .ok_or_else(|| GrammarError::Xml("no active <Language> element".into()))?;

    // Top-level lints (constructs the reference grammars never contain): FootFeatures, StemName/Family/RealizationalRule, and MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule are all loaded below rather than hard-linted here; the co-occurrence rules run in a post-strata pass since their IDREFs resolve against registries the strata loop populates.

    // --- syntactic feature system (POS = feature 0; head complex feature = feature 1) ---------
    let syn = build_syn_features(lang)?;

    // --- grammar-tier FS interner: the empty FS is interned first (FsId 0) --------------------
    let mut fs_interner: Interner<FeatureStruct> = Interner::with_capacity(64);
    let empty = fs_interner.intern(FeatureStruct::EMPTY);
    debug_assert_eq!(empty, pg_featstruct::FsId(0));

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
    let mut mpr_features: Vec<MprFeatureDef> = Vec::new();
    let mut mpr_index: HashMap<String, MprId> = HashMap::new();
    {
        let count = lang
            .elems2(
                "MorphologicalPhonologicalRuleFeatures",
                "MorphologicalPhonologicalRuleFeature",
            )
            .filter(|e| e.is_active())
            .count();
        if count > 64 {
            return Err(GrammarError::Unsupported(format!(
                "{count} MPR features; the bitset representation supports at most 64"
            )));
        }
        for mf in lang
            .elems2(
                "MorphologicalPhonologicalRuleFeatures",
                "MorphologicalPhonologicalRuleFeature",
            )
            .filter(|e| e.is_active())
        {
            let id = mf.attr("id").unwrap_or("").to_string();
            mpr_index.insert(id.clone(), MprId(mpr_names.len() as u8));
            mpr_features.push(MprFeatureDef {
                xml_id: id,
                name: mf.text.clone(),
            });
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
                        GrammarError::Semantic(format!(
                            "natural class references unknown segment '{seg_id}'"
                        ))
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

    // Families: `entries` fills in during lexical-entry loading and is stored on `Acc` (mutated throughout the strata loop), while the fixed xml-id lookup lives on `Ro`, same split as every other id-index/mutable-accumulator pair in this loader.
    let mut family_defs: Vec<FamilyDef> = Vec::new();
    let mut family_index: HashMap<String, FamilyId> = HashMap::new();
    for fam in lang.elems2("Families", "Family").filter(|e| e.is_active()) {
        let id = fam.attr("id").unwrap_or("").to_string();
        family_index.insert(id, FamilyId(family_defs.len() as u32));
        family_defs.push(FamilyDef {
            name: Some(fam.text.clone()),
            entries: Vec::new(),
        });
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
    for pr in lang
        .under("PhonologicalRuleDefinitions")
        .filter(|e| e.is_active())
    {
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
        allomorph_sources: Vec::new(),
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

    // `IsTemplateRule` post-pass: tag every affix rule referenced from any template's slot, across every stratum. Must run after the whole strata loop, since a template in stratum N could be loaded before or after the rule it references relative to other strata.
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

    // Co-occurrence rules, post-strata: their IDREFs resolve against the morpheme/allomorph registries the strata loop just finished populating.
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
            .push(MorphemeCoOccurrenceRuleDef {
                require,
                others,
                adjacency,
            });
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
        let rule = AllomorphCoOccurrenceRuleDef {
            require,
            others,
            adjacency,
        };
        match acc.allomorph_owners[primary.0 as usize] {
            AllomorphOwner::Root(le, idx) => {
                acc.entries[le.0 as usize].allomorphs[idx as usize]
                    .co_occurrence
                    .push(rule);
            }
            AllomorphOwner::Affix(mr, idx) => {
                match &mut acc.mrules[mr.0 as usize] {
                    MorphRuleDef::AffixProcess(def) => {
                        def.allomorphs[idx as usize].co_occurrence.push(rule)
                    }
                    MorphRuleDef::Realizational(def) => {
                        def.allomorphs[idx as usize].co_occurrence.push(rule)
                    }
                    MorphRuleDef::Compounding(_) => {
                        unreachable!("compounding rules mint no AllomorphId (no per-allomorph registry entry)")
                    }
                }
            }
        }
    }

    // All borrows of `phon` (via `ro`) have ended; the grammar takes ownership of its phonology so ids stay resolvable downstream.
    let (phon_features, char_tables) = phon.into_parts();

    let grammar = Grammar {
        name: lang.text_of("Name").map(str::to_string),
        phon_features,
        char_tables,
        syn_features: syn,
        fs_interner: acc.fs_interner,
        mpr_names,
        mpr_features,
        mpr_groups,
        stem_names,
        families: acc.families,
        natural_classes,
        morphemes: acc.morphemes,
        allomorph_owners: acc.allomorph_owners,
        allomorph_sources: acc.allomorph_sources,
        prules,
        mrules: acc.mrules,
        templates: acc.templates,
        entries: acc.entries,
        strata,
    };
    grammar.final_template_prune_facts()?;
    Ok(grammar)
}

// Syntactic feature system.

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
    // HermitCrabInput.dtd requires `Language (..., PartsOfSpeech, ...)` and `PartsOfSpeech (PartOfSpeech+)`.
    if pos_symbols.is_empty() {
        return Err(GrammarError::Xml(
            "<Language> has no <PartsOfSpeech><PartOfSpeech> entries; HermitCrab's DTD requires \
             at least one part of speech"
                .into(),
        ));
    }
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

    // Feature 1: the head complex feature, present iff <HeadFeatures> exists, even if empty.
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

    // The foot complex feature, present iff <FootFeatures> exists, mirrors <HeadFeatures> exactly; foot-declared features join the SAME `features` vec as head's, since C# has one shared syntactic feature namespace, not two (see `SynFeatureSystem`'s doc).
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

    Ok(SynFeatureSystem {
        features,
        pos,
        head,
        foot,
    })
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
            let default_symbol = elem
                .attr_ne("defaultSymbol")
                .and_then(|d| symbols.iter().position(|(id, _)| id == d).map(|i| i as u32));
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
        let idx = syn
            .symbol_index(syn.pos, id)
            .ok_or_else(|| GrammarError::Semantic(format!("unknown part-of-speech id '{id}'")))?;
        bits.set(idx);
    }
    Ok(bits)
}

/// Port of `LoadFeatureStruct` for the *syntactic* feature system (recursive, complex features).
fn load_syn_fs(elem: &Node, syn: &SynFeatureSystem) -> Result<FeatureStruct, GrammarError> {
    let mut b = FeatureStructBuilder::new();
    for fv in elem.elems("FeatureValue").filter(|e| e.is_active()) {
        let feat_xml = fv.attr("feature").unwrap_or("");
        let feat_id = syn.feature_by_xml_id(feat_xml).ok_or_else(|| {
            GrammarError::Semantic(format!("unknown syntactic feature '{feat_xml}'"))
        })?;
        match fv.attr_ne("symbolValues") {
            Some(vals) => {
                let mut bits = SymbolBits::EMPTY;
                for sym in vals.split_whitespace() {
                    let idx = syn.symbol_index(feat_id, sym).ok_or_else(|| {
                        GrammarError::Semantic(format!(
                            "unknown symbol '{sym}' on feature '{feat_xml}'"
                        ))
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

/// Build a `{POS?, head?, foot?}` syntactic feature struct from an element carrying a POS id-list attribute, a head-features child element, and/or a foot-features child element, then intern it; `foot_elem` mirrors `head_elem` exactly, a no-op when the grammar declares no `<FootFeatures>`.
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

/// `LoadStemName`: each `<Region>` becomes one region FS, in the same `{POS, head, foot}` shape `build_syn_fs` produces elsewhere, so a region FS is directly comparable (via `subsumes`) to a word's accumulated syntactic FS.
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
        // `AssignedFootFeatures` on a StemName `<Region>`.
        if let (Some(foot_fid), Some(fnode)) = (syn.foot, region.child("AssignedFootFeatures")) {
            b.add(foot_fid, FeatureValue::Complex(load_syn_fs(fnode, syn)?));
        }
        regions.push(fs_interner.intern(b.build()));
    }
    Ok(StemNameDef {
        name: sn.text_of("Name").map(str::to_string),
        regions,
    })
}

fn intern_syn_fs(
    acc: &mut Acc,
    elem: &Node,
    syn: &SynFeatureSystem,
    pos_attr: Option<&str>,
    head_elem: Option<&str>,
    foot_elem: Option<&str>,
) -> Result<pg_featstruct::FsId, GrammarError> {
    let fs = build_syn_fs(elem, syn, pos_attr, head_elem, foot_elem)?;
    Ok(acc.fs_interner.intern(fs))
}

// MPR, natural-class, and variable helpers.

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

/// `LoadFeatureStruct` for a `FeatureNaturalClass` against the phonological feature system, flattened to sparse `(lane, symbols)` constraints sorted by lane (union on repeats).
fn load_phon_constraints(
    nc: &Node,
    phon: &GrammarPhonology,
) -> Result<Vec<(FlatIndex, SymbolBits)>, GrammarError> {
    let fs = phon.feature_system();
    let mut map: HashMap<u32, SymbolBits> = HashMap::new();
    for fv in nc.elems("FeatureValue").filter(|e| e.is_active()) {
        let feat_xml = fv.attr("feature").unwrap_or("");
        let flat = fs.flat_index(feat_xml).ok_or_else(|| {
            GrammarError::Semantic(format!("unknown phonological feature '{feat_xml}'"))
        })?;
        let mut bits = SymbolBits::EMPTY;
        if let Some(vals) = fv.attr_ne("symbolValues") {
            for sym in vals.split_whitespace() {
                let idx = fs.symbol_index(flat, sym).ok_or_else(|| {
                    GrammarError::Semantic(format!(
                        "unknown symbol '{sym}' on feature '{feat_xml}'"
                    ))
                })?;
                bits.set(idx);
            }
        }
        let entry = map.entry(flat.0).or_insert(SymbolBits::EMPTY);
        entry.0 |= bits.0;
    }
    // Every `FeatureNaturalClass` unconditionally requires `Type=Segment`, inserted last, mirroring C#'s base-constructor `fs.AddValue`. `SegmentNaturalClass` needs no equivalent injection, since it gets `Type=Segment` for free via the lane-union-of-members logic.
    map.insert(
        fs.type_flat().0,
        SymbolBits::single(crate::featsys::TYPE_SEGMENT_SYMBOL),
    );
    let mut out: Vec<(FlatIndex, SymbolBits)> =
        map.into_iter().map(|(k, v)| (FlatIndex(k), v)).collect();
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
                GrammarError::Semantic(format!(
                    "variable references unknown phonological feature '{feat_xml}'"
                ))
            })?;
            vars.push((id, name, flat));
        }
    }
    Ok(VarTable { vars })
}

// Patterns (`LoadPatternNodes` / `LoadSimpleContext` / templates / sequences).

fn load_simple_context(
    rec: &Node,
    vars: &VarTable,
    ro: &Ro,
) -> Result<SimpleContext, GrammarError> {
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

/// Build one `PatternNode` from a single `<SimpleContext>`/`<Segment>`/`<BoundaryMarker>`/`<OptionalSegmentSequence>`/`<Segments>` element (`None` for any other tag). Factored out of `load_pattern_nodes` so `load_metathesis_pattern_nodes` can reuse the same per-element logic while additionally checking each element's `id` attribute for switch-tagging.
fn load_one_pattern_node(
    rec: &Node,
    vars: &VarTable,
    default_table: TableId,
    ro: &Ro,
) -> Result<Option<PatternNode>, GrammarError> {
    let node = match rec.tag.as_str() {
        "SimpleContext" => PatternNode::Context(load_simple_context(rec, vars, ro)?),
        "Segment" => PatternNode::CharDef(resolve_chardef(ro, rec.attr("segment").unwrap_or(""))?),
        "BoundaryMarker" => {
            PatternNode::CharDef(resolve_chardef(ro, rec.attr("boundary").unwrap_or(""))?)
        }
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
            let max = if max_raw < 0 {
                None
            } else {
                Some(max_raw as u32)
            };
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

/// `LoadPhoneticTemplate`: `None` when the template element is absent, matching C#'s semantics ("no template on this side").
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

/// `segment_text`'s pattern-aware counterpart: C# `LoadRootAllomorph` is the only `new Segments(...)` call site that passes `allowPattern = true`, so this is used only by `load_root_allomorph`.
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

/// `LoadAllomorphEnvironments` for one `RequiredEnvironments`/`ExcludedEnvironments` block; the variable scope is always empty, so an `AlphaVariable` here lints `Unsupported` via `load_simple_context`.
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
            let left_pt = env
                .child("LeftEnvironment")
                .and_then(|n| n.child("PhoneticTemplate"));
            let right_pt = env
                .child("RightEnvironment")
                .and_then(|n| n.child("PhoneticTemplate"));
            out.push(EnvironmentDef {
                require,
                left: load_phonetic_template(left_pt, &empty_vars, default_table, ro)?,
                right: load_phonetic_template(right_pt, &empty_vars, default_table, ro)?,
            });
        }
    }
    Ok(out)
}

// Phonological (rewrite) rules.

fn load_rewrite_rule(pr: &Node, ro: &Ro) -> Result<RewriteRuleDef, GrammarError> {
    let mult = pr.attr("multipleApplicationOrder").unwrap_or("");
    // The former stopgap hard lint here has been removed now that `RewriteMode::Simultaneous` has real execution semantics in `pg-rules`.
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
        pr.child("PhoneticInput")
            .and_then(|n| n.child("PhoneticSequence")),
        &vars,
        default_table,
        ro,
    )?;

    let mut subrules = Vec::new();
    for sub in pr
        .elems2("PhonologicalSubrules", "PhonologicalSubrule")
        .filter(|e| e.is_active())
    {
        subrules.push(load_rewrite_subrule(
            sub,
            &vars,
            default_table,
            ro,
            mode,
            &lhs,
        )?);
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
    // NOT hard-linted, despite `subrule_applicable` (pg-rules) silently treating it as always-false: Amharic's real grammar authors `requiredPartsOfSpeech` on 3 subrules, so a hard lint here would drop Amharic's load entirely. The construct stays silently-always-false until the real port (needing `Word` syntactic-FS threaded into `pg_rules::rewrite`'s rule-level API) lands.
    let required_pos = match sub.attr_ne("requiredPartsOfSpeech") {
        Some(ids) => Some(parse_pos_bits(ro.syn, ids)?),
        None => None,
    };
    let required_mpr = load_mpr_set(sub.attr("requiredMPRFeatures"), ro.mpr)?;
    let excluded_mpr = load_mpr_set(sub.attr("excludedMPRFeatures"), ro.mpr)?;
    let rhs = load_phonetic_sequence(
        sub.child("PhoneticOutput")
            .and_then(|n| n.child("PhoneticSequence")),
        vars,
        default_table,
        ro,
    )?;

    let (left_env, right_env) = match sub.child("Environment") {
        None => (None, None),
        Some(env) => {
            let left_pt = env
                .child("LeftEnvironment")
                .and_then(|n| n.child("PhoneticTemplate"));
            let right_pt = env
                .child("RightEnvironment")
                .and_then(|n| n.child("PhoneticTemplate"));
            (
                load_phonetic_template(left_pt, vars, default_table, ro)?,
                load_phonetic_template(right_pt, vars, default_table, ro)?,
            )
        }
    };

    let self_opaquing = compute_self_opaquing(
        ro,
        default_table,
        mode,
        lhs,
        &rhs,
        left_env.as_ref(),
        right_env.as_ref(),
    );

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

// `RewriteSubruleDef::self_opaquing` -- computed once at load time from the rule's own already-loaded patterns; see that field's doc for the exact per-kind formula.

/// C# `AnalysisRewriteRule`'s self-opaquing precheck. See `RewriteSubruleDef::self_opaquing`'s doc for the per-kind dispatch this implements.
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
        // Epenthesis: unconditional whenever Simultaneous, no unifiability precheck.
        return true;
    }
    if lhs.nodes.len() != rhs.nodes.len() {
        // Narrow/Expansion: irrelevant field, this kind's analysis is always unconditionally Simultaneous+Deletion regardless of rule.mode.
        return false;
    }
    // Feature: self-opaquing iff some RHS constraint is not feature-unifiable with every Segment-typed node of either environment.
    let phon = ro.phon.feature_system();
    let table = &ro.phon.tables()[table_id.0 as usize];
    rhs.nodes.iter().any(|rhs_node| {
        let rhs_pins = pattern_node_pin_bits(phon, table, ro.natural_class_defs, rhs_node);
        !env_unifiable(phon, table, ro.natural_class_defs, &rhs_pins, left_env)
            || !env_unifiable(phon, table, ro.natural_class_defs, &rhs_pins, right_env)
    })
}

/// `IsUnifiable(rhsConstraint, environment)`: every Segment-typed node inside `environment`'s pattern (recursing into quantifiers) must be feature-unifiable with `rhs_pins`, i.e. for every phonological feature both sides pin, their symbol-bit sets must overlap. No environment, or one with no Segment-typed nodes, is vacuously unifiable.
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
            rhs_pins.iter().all(
                |&(f, bits)| match node_pins.iter().find(|&&(nf, _)| nf == f) {
                    Some(&(_, nbits)) => bits & nbits != 0,
                    None => true,
                },
            )
        }
        // Anchor/Segments: no phonological feature pin to violate.
        _ => true,
    })
}

/// Local mirror of `pg_rules::rewrite::node_pins`, kept as a duplicate since `pg-grammar` cannot depend on `pg-rules` (the dependency runs the other way). Must stay semantically identical; see `self_opaquing_pin_semantics_match_node_pins` below for the pinned parity check.
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
                        let bits = segs
                            .iter()
                            .fold(0u64, |acc, cd| acc | table.get(*cd).feature_lanes()[f]);
                        (bits != phon.mask(FlatIndex(f as u32))).then_some((f, bits))
                    })
                    .collect(),
            }
        }
        PatternNode::CharDef(cd) => {
            let lanes = table.get(*cd).feature_lanes();
            (0..w)
                .filter(|&f| lanes[f] != phon.mask(FlatIndex(f as u32)))
                .map(|f| (f, lanes[f]))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// `LoadMetathesisRule`: no `VariableFeatures` scope and no default char-def table context; each switch is recorded as a plain index into `pattern.nodes` rather than minting a `Group` pattern-node kind.
/// See docs/research/pg-grammar-metathesis-load-design-notes.md for why no `Group` kind is needed.
fn load_metathesis_rule(pr: &Node, ro: &Ro) -> Result<MetathesisRuleDef, GrammarError> {
    let left_switch_xml = pr.attr("leftSwitch").ok_or_else(|| {
        GrammarError::Semantic("MetathesisRule missing required 'leftSwitch' attribute".into())
    })?;
    let right_switch_xml = pr.attr("rightSwitch").ok_or_else(|| {
        GrammarError::Semantic("MetathesisRule missing required 'rightSwitch' attribute".into())
    })?;
    // DTD: no `simultaneous` option here (unlike `<PhonologicalRule>`), so no such lint is needed.
    let dir = match pr.attr("multipleApplicationOrder") {
        Some("rightToLeftIterative") => Dir::RightToLeft,
        _ => Dir::LeftToRight,
    };
    let default_table = TableId(0);
    let ptemp = pr
        .child("StructuralDescription")
        .and_then(|n| n.child("PhoneticTemplate"))
        .ok_or_else(|| {
            GrammarError::Semantic(
                "MetathesisRule missing StructuralDescription/PhoneticTemplate".into(),
            )
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

// Strata (morphological rules, templates, lexicon).

fn load_stratum(
    stratum: &Node,
    stratum_id: StratumId,
    ro: &Ro,
    acc: &mut Acc,
    prule_index: &HashMap<String, PRuleId>,
) -> Result<StratumDef, GrammarError> {
    let table_xml = stratum.attr("characterDefinitionTable").unwrap_or("");
    let table = *ro.table.get(table_xml).ok_or_else(|| {
        GrammarError::Semantic(format!("stratum references unknown table '{table_xml}'"))
    })?;
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
    for mr in stratum
        .under("MorphologicalRuleDefinitions")
        .filter(|e| e.is_active())
    {
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
    for temp in stratum
        .elems2("AffixTemplates", "AffixTemplate")
        .filter(|e| e.is_active())
    {
        let def = load_affix_template(temp, &local_mr, ro, acc)?;
        let tid = TemplateId(acc.templates.len() as u32);
        acc.templates.push(def);
        templates.push(tid);
    }

    // Lexical entries (document order; entries with zero loadable allomorphs are dropped).
    let mut entries = Vec::new();
    for entry in stratum
        .elems2("LexicalEntries", "LexicalEntry")
        .filter(|e| e.is_active())
    {
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

    let required_syn_fs = intern_syn_fs(
        acc,
        mr,
        ro.syn,
        Some("requiredPartsOfSpeech"),
        Some("RequiredHeadFeatures"),
        Some("RequiredFootFeatures"),
    )?;
    let out_syn_fs = intern_syn_fs(
        acc,
        mr,
        ro.syn,
        Some("outputPartOfSpeech"),
        Some("OutputHeadFeatures"),
        Some("OutputFootFeatures"),
    )?;

    let mut obligatory_features = Vec::new();
    if let Some(ids) = mr.attr_ne("outputObligatoryFeatures") {
        for id in ids.split_whitespace() {
            let fid = ro.syn.feature_by_xml_id(id).ok_or_else(|| {
                GrammarError::Semantic(format!("unknown obligatory feature '{id}'"))
            })?;
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
                let placement = source_morph_placement(&def.rhs);
                acc.allomorph_sources.push(AllomorphSource {
                    // XML id is only a structural handle; source provenance is unavailable here.
                    form_guids: vec![None],
                    omitted: false,
                    placement,
                });
                acc.allomorph_owners
                    .push(AllomorphOwner::Affix(mrule_id, allomorphs.len() as u16));
                // The id `<AllomorphCoOccurrenceRule otherAllomorphs="...">` resolves against for an affix allomorph.
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
        source_msa_guid: None,
        source_msa_class: None,
        source_infl_type_guid: None,
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
            GrammarError::Semantic(format!(
                "MorphologicalRule references unknown requiredStemName '{sid}'"
            ))
        })?),
        None => None,
    };

    let partial_reason =
        parse_bool(mr.attr("partial"), false).then_some(PartialMorphemeReason::Unspecified);
    acc.mrules
        .push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
            morpheme,
            name: mr.text_of("Name").map(str::to_string),
            blockable: parse_bool(mr.attr("blockable"), true),
            partial_reason,
            max_apps,
            required_syn_fs,
            out_syn_fs,
            obligatory_features,
            required_stem_name,
            allomorphs,
            // Set by the post-pass in `load()` once every stratum's templates are known; default `false` here.
            is_template_rule: false,
        }));
    Ok(Some(mrule_id))
}

/// `TryLoadRealizationalRule`. Shares `load_affix_allomorph` with the regular affix-process loader above; see `crate::model::MorphRuleDef::affix_allomorphs`'s doc for why that's exact, not coincidental.
fn try_load_realizational_rule(
    real: &Node,
    default_table: TableId,
    stratum_id: StratumId,
    ro: &Ro,
    acc: &mut Acc,
) -> Result<Option<MRuleId>, GrammarError> {
    let mrule_id = MRuleId(acc.mrules.len() as u32);

    // No `requiredPartsOfSpeech`/POS attribute on `<RealizationalRule>`: head/foot only.
    let required_syn_fs = intern_syn_fs(
        acc,
        real,
        ro.syn,
        None,
        Some("RequiredHeadFeatures"),
        Some("RequiredFootFeatures"),
    )?;

    // `<RealizationalFeatures>` wrapped in the head feature; empty (unwrapped) FS if the element is absent.
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
                let placement = source_morph_placement(&def.rhs);
                acc.allomorph_sources.push(AllomorphSource {
                    form_guids: vec![None],
                    omitted: false,
                    placement,
                });
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
        source_msa_guid: None,
        source_msa_class: None,
        source_infl_type_guid: None,
        morph_id: real.text_of("MorphemeId").map(str::to_string),
        gloss: real.text_of("Gloss").map(str::to_string),
        stratum: stratum_id,
        properties: load_properties(real.child("Properties")),
        co_occurrence: Vec::new(),
    });

    acc.mrules
        .push(MorphRuleDef::Realizational(RealizationalRuleDef {
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
    let mut environments =
        load_allomorph_environments(sub.child("RequiredEnvironments"), true, default_table, ro)?;
    environments.extend(load_allomorph_environments(
        sub.child("ExcludedEnvironments"),
        false,
        default_table,
        ro,
    )?);

    // Subrule-level requirement FS carries head/foot only (no POS), per LoadAffixProcessAllomorph.
    let required_syn_fs = intern_syn_fs(
        acc,
        sub,
        ro.syn,
        None,
        Some("RequiredHeadFeatures"),
        Some("RequiredFootFeatures"),
    )?;

    let vars = load_variables(sub.child("VariableFeatures"), ro.phon)?;

    let input = sub.child("MorphologicalInput").ok_or_else(|| {
        GrammarError::Semantic("MorphologicalSubrule without MorphologicalInput".into())
    })?;
    let required_mpr = load_mpr_set(input.attr("requiredMPRFeatures"), ro.mpr)?;
    let excluded_mpr = load_mpr_set(input.attr("excludedMPRFeatures"), ro.mpr)?;

    let mut lhs = Vec::new();
    let mut part_names: HashMap<String, PartRef> = HashMap::new();
    load_morph_lhs(
        input,
        &vars,
        default_table,
        ro,
        PartKind::Input,
        &mut lhs,
        &mut part_names,
    )?;

    let output = sub.child("MorphologicalOutput").ok_or_else(|| {
        GrammarError::Semantic("MorphologicalSubrule without MorphologicalOutput".into())
    })?;
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
                let pr = *part_names.get(idx).ok_or_else(|| {
                    GrammarError::Semantic(format!("CopyFromInput unknown part '{idx}'"))
                })?;
                rhs.push(OutputAction::Copy(pr));
            }
            "InsertSimpleContext" => {
                let sc = part.child("SimpleContext").ok_or_else(|| {
                    GrammarError::Semantic("InsertSimpleContext without SimpleContext".into())
                })?;
                rhs.push(OutputAction::InsertContext(load_simple_context(
                    sc, vars, ro,
                )?));
            }
            "ModifyFromInput" => {
                let idx = part.attr("index").unwrap_or("");
                let pr = *part_names.get(idx).ok_or_else(|| {
                    GrammarError::Semantic(format!("ModifyFromInput unknown part '{idx}'"))
                })?;
                let sc = part.child("SimpleContext").ok_or_else(|| {
                    GrammarError::Semantic("ModifyFromInput without SimpleContext".into())
                })?;
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

fn source_morph_placement(rhs: &[OutputAction]) -> SourceMorphPlacement {
    let inserts_medially = rhs.iter().enumerate().any(|(index, action)| {
        index > 0
            && index + 1 < rhs.len()
            && matches!(
                action,
                OutputAction::InsertSegments { .. } | OutputAction::InsertContext(_)
            )
    });
    if inserts_medially {
        SourceMorphPlacement::InsertBeforeLast
    } else {
        SourceMorphPlacement::Append
    }
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
    let out_syn_fs = intern_syn_fs(
        acc,
        comp,
        ro.syn,
        Some("outputPartOfSpeech"),
        Some("OutputHeadFeatures"),
        Some("OutputFootFeatures"),
    )?;

    let head_prod_restrictions_mpr =
        load_mpr_set(comp.attr("headProdRestrictionsMprFeatures"), ro.mpr)?;
    let non_head_prod_restrictions_mpr =
        load_mpr_set(comp.attr("nonHeadProdRestrictionsMprFeatures"), ro.mpr)?;
    let output_prod_restrictions_mpr =
        load_mpr_set(comp.attr("outputProdRestrictionsMprFeatures"), ro.mpr)?;

    let mut obligatory_features = Vec::new();
    if let Some(ids) = comp.attr_ne("outputObligatoryFeatures") {
        for id in ids.split_whitespace() {
            let fid = ro.syn.feature_by_xml_id(id).ok_or_else(|| {
                GrammarError::Semantic(format!("unknown obligatory feature '{id}'"))
            })?;
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

    acc.mrules
        .push(MorphRuleDef::Compounding(CompoundingRuleDef {
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

    let head = sub.child("HeadMorphologicalInput").ok_or_else(|| {
        GrammarError::Semantic("CompoundingSubrule without HeadMorphologicalInput".into())
    })?;
    let required_mpr = load_mpr_set(head.attr("requiredMPRFeatures"), ro.mpr)?;
    let excluded_mpr = load_mpr_set(head.attr("excludedMPRFeatures"), ro.mpr)?;

    let mut head_lhs = Vec::new();
    let mut non_head_lhs = Vec::new();
    let mut part_names: HashMap<String, PartRef> = HashMap::new();
    load_morph_lhs(
        head,
        &vars,
        default_table,
        ro,
        PartKind::Head,
        &mut head_lhs,
        &mut part_names,
    )?;

    let non_head = sub.child("NonHeadMorphologicalInput").ok_or_else(|| {
        GrammarError::Semantic("CompoundingSubrule without NonHeadMorphologicalInput".into())
    })?;
    load_morph_lhs(
        non_head,
        &vars,
        default_table,
        ro,
        PartKind::NonHead,
        &mut non_head_lhs,
        &mut part_names,
    )?;

    let output = sub.child("MorphologicalOutput").ok_or_else(|| {
        GrammarError::Semantic("CompoundingSubrule without MorphologicalOutput".into())
    })?;
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
    let required_syn_fs =
        intern_syn_fs(acc, temp, ro.syn, Some("requiredPartsOfSpeech"), None, None)?;

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
            zone: crate::model::TemplateSlotZone::LegacyUnspecified,
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
    // `LexicalEntry@family`: resolved to a `FamilyId`; the entry is pushed onto that family's `entries` below once its own id is known and it has loaded at least one allomorph.
    let family = match entry.attr_ne("family") {
        Some(fid) => Some(*ro.families.get(fid).ok_or_else(|| {
            GrammarError::Semantic(format!("LexicalEntry references unknown family '{fid}'"))
        })?),
        None => None,
    };

    let syn_fs = intern_syn_fs(
        acc,
        entry,
        ro.syn,
        Some("partOfSpeech"),
        Some("AssignedHeadFeatures"),
        Some("AssignedFootFeatures"),
    )?;
    let mpr = load_mpr_set(entry.attr("ruleFeatures"), ro.mpr)?;
    let partial = parse_bool(entry.attr("partial"), false);

    let lex_id = LexEntryId(acc.entries.len() as u32);
    let mut allomorphs = Vec::new();
    for allo in entry
        .elems2("Allomorphs", "Allomorph")
        .filter(|e| e.is_active())
    {
        let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
        match load_root_allomorph(allo, default_table, allo_id, ro) {
            Ok(def) => {
                acc.allomorph_sources.push(AllomorphSource {
                    // XML id is only a structural handle; source provenance is unavailable here.
                    form_guids: vec![None],
                    omitted: false,
                    placement: crate::model::SourceMorphPlacement::Append,
                });
                acc.allomorph_owners
                    .push(AllomorphOwner::Root(lex_id, allomorphs.len() as u16));
                // Same allomorph-id registry `try_load_lex_entry`'s AllomorphCoOccurrenceRule resolution reads.
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
        source_msa_guid: None,
        source_msa_class: None,
        source_infl_type_guid: None,
        morph_id: entry.text_of("MorphemeId").map(str::to_string),
        gloss: entry.text_of("Gloss").map(str::to_string),
        stratum: stratum_id,
        properties: load_properties(entry.child("Properties")),
        // Filled by the post-strata co-occurrence-rule pass in `load()`.
        co_occurrence: Vec::new(),
    });

    acc.entries.push(LexEntryDef {
        authored_id: entry.attr("id").unwrap_or("").to_string(),
        morpheme,
        syn_fs,
        mpr,
        partial_reason: partial.then_some(PartialMorphemeReason::StemWithoutCategory),
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
    // `LoadRootAllomorph` is C#'s one `allowPattern = true` call site: root-allomorph shapes fall back to the `[NatClass]`/`([NatClass])`/`[NatClass]*` pattern language wherever a literal character-definition match fails.
    let shape =
        segment_text_with_patterns(default_table, shape_str, ro.phon, ro.natural_class_defs)?;
    // C# throws InvalidShapeException (→ dropped) if the shape is entirely boundary markers.
    if shape
        .shape
        .interior()
        .all(|(_, k, _, _)| k == NodeKind::Boundary)
    {
        return Err(GrammarError::Semantic(format!(
            "root allomorph shape {shape_str:?} is all boundaries"
        )));
    }

    let mut environments =
        load_allomorph_environments(allo.child("RequiredEnvironments"), true, default_table, ro)?;
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

    // C# `RootAllomorph` ctor rule: any interior node that is iterative, or optional-and-not-a-boundary, makes this a lexical pattern. A bare mandatory `[Class]` node does not qualify (a normal trie-indexed root), nor does an ordinary boundary-optional shape -- the `kind != Boundary` guard is exactly why.
    let is_pattern = shape.shape.interior().any(|(_, kind, _, flags)| {
        flags.is_iterative() || (flags.is_optional() && kind != NodeKind::Boundary)
    });

    Ok(RootAllomorphDef {
        id: allo_id,
        shape,
        is_bound: parse_bool(allo.attr("isBound"), false),
        environments,
        // Populated by the post-strata co-occurrence-rule pass in `load()`; empty at construction.
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

#[cfg(test)]
mod tests;
