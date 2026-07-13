//! HermitCrab XML grammar loading, linting, and compilation to immutable runtime tables
//! (plan §3, §5). The Rust engine consumes the same `*-hc.xml` file FieldWorks' `HCLoader`
//! writes and `XmlLanguageLoader` reads — that shared file is the A/B-switch contract (§3, D1).
//!
//! Loader lint: any construct outside the implemented surface produces a structured
//! `Unsupported` error so the C# facade can fall back to the managed engine rather than
//! risk a wrong parse (plan §8 layer 6).
//!
//! ## Scope of this milestone (M1)
//! Only the **segmentation gate** slice: `PhonologicalFeatureSystem` and
//! `CharacterDefinitionTable` (feature census + segment/boundary inventory + word segmentation).
//! Strata, rules, templates, lexicon, and the FST are later milestones; nothing here reads or
//! writes those XML sections.
#![forbid(unsafe_code)]

pub mod chardef;
pub mod featsys;
pub mod lint;
pub mod load;
pub mod model;
pub mod nfd;
pub mod segment;

pub use load::load;

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::reader::Reader;
use thiserror::Error;

use chardef::{CharDefKind, CharDefTable, RawCharDef, RawFeatureValue};
use featsys::{PhonFeatureSystem, RawFeature};

/// Errors surfaced by grammar loading. `Unsupported` drives managed fallback (plan §8 layer 6).
#[derive(Debug, Error)]
pub enum GrammarError {
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("unsupported grammar construct: {0}")]
    Unsupported(String),
    #[error("grammar semantic error: {0}")]
    Semantic(String),
    #[error("duplicate character-definition representation: {0}")]
    DuplicateRepresentation(String),
}

/// The compiled phonological census of a grammar: its symbolic feature system plus every
/// `CharacterDefinitionTable` it declares.
///
/// A grammar may declare more than one `CharacterDefinitionTable` (each stratum can reference
/// its own — `XmlLanguageLoader.cs` keys them by XML id in `_tables`); this milestone loads all
/// of them (plan scope note) but only exercises segmentation against a single table at a time
/// via [`table_by_id`](Self::table_by_id) / [`main_table`](Self::main_table). All three reference
/// grammars (Indonesian, Amharic, Sena) declare exactly one.
pub struct GrammarPhonology {
    feature_system: PhonFeatureSystem,
    tables: Vec<CharDefTable>,
}

impl GrammarPhonology {
    #[inline]
    pub fn feature_system(&self) -> &PhonFeatureSystem {
        &self.feature_system
    }

    #[inline]
    pub fn tables(&self) -> &[CharDefTable] {
        &self.tables
    }

    /// Consume this phonology into its parts (for embedding in [`model::Grammar`], which owns
    /// them so that `TableId`/`CharDefId`/`FlatIndex` remain resolvable off the compiled
    /// grammar alone). Tables are in document order — `TableId(i)` indexes the returned `Vec`.
    pub fn into_parts(self) -> (PhonFeatureSystem, Vec<CharDefTable>) {
        (self.feature_system, self.tables)
    }

    /// A table by its XML `id` attribute.
    pub fn table_by_id(&self, xml_id: &str) -> Option<&CharDefTable> {
        self.tables.iter().find(|t| t.xml_id() == xml_id)
    }

    /// The first declared `CharacterDefinitionTable` — the common case for a whole-grammar
    /// segmentation gate. Grammars with multiple tables (multiple strata) should resolve the
    /// table they need via [`table_by_id`](Self::table_by_id) instead.
    pub fn main_table(&self) -> Option<&CharDefTable> {
        self.tables.first()
    }

    /// A normalized, deterministic inventory string for the plan §8 layer-1 loader-dump gate:
    /// feature count and per-feature symbol counts, then per-table char-def count and each char
    /// def's xml-id, kind, and sorted representations. Stable across re-loads of the same XML.
    pub fn dump_char_defs(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "features={}", self.feature_system.len());
        for (flat, name, symbol_count) in self.feature_system.iter() {
            let _ = writeln!(
                out,
                "  feature[{}] {} symbols={}",
                flat.0, name, symbol_count
            );
        }
        for table in &self.tables {
            let _ = writeln!(
                out,
                "table {} name={:?} char_defs={}",
                table.xml_id(),
                table.name(),
                table.len()
            );
            for (id, cd) in table.iter() {
                let kind = match cd.kind() {
                    CharDefKind::Segment => "Segment",
                    CharDefKind::Boundary => "Boundary",
                };
                let mut reps: Vec<&str> = cd.representations().iter().map(String::as_str).collect();
                reps.sort_unstable();
                let _ = writeln!(
                    out,
                    "  char_def[{}] id={} kind={} reps={:?}",
                    id.0,
                    cd.xml_id(),
                    kind,
                    reps
                );
            }
        }
        out
    }
}

/// Load a grammar's phonological feature system and character-definition table(s) from a
/// HermitCrab XML document (`*-hc.xml`, the format `XmlLanguageLoader` reads and FieldWorks'
/// `HCLoader` writes).
///
/// Only `PhonologicalFeatureSystem` and `CharacterDefinitionTable` elements are consulted;
/// everything else in the document (parts of speech, strata, rules, templates, lexicon) is
/// ignored at this milestone. Both element kinds are direct children of `<Language>` in the C#
/// object model (`XmlLanguageLoader.LoadLanguage`), so a flat scan over the whole event stream —
/// recursing only when one of these two tag names is seen — is sufficient and matches
/// `Elements("...")`'s direct-children semantics without tracking a full ancestor stack.
pub fn load_char_def_table_from_xml(xml: &str) -> Result<GrammarPhonology, GrammarError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut raw_features: Vec<RawFeature> = Vec::new();
    let mut raw_tables: Vec<(String, Option<String>, Vec<RawCharDef>)> = Vec::new();
    // Finding N1 (phase2 audit C): C# `XmlLanguageLoader.LoadLanguage` selects
    // `langElem.Elements("PhonologicalFeatureSystem").SingleOrDefault(IsActive)`
    // (`XmlLanguageLoader.cs:239`) — the one active block among possibly several (the DTD
    // comment on `PhonologicalFeatureSystem@isActive` explicitly anticipates dialect-variant
    // drafts: "at most one should be active at a time"). This loop used to parse *every*
    // `<PhonologicalFeatureSystem>` block unconditionally and let the last one win, with no
    // `isActive` check at all (unlike the very next `CharacterDefinitionTable` branch below,
    // which always checked it) — silently loading whichever block happened to appear last in
    // the file. `phon_feat_sys_selected` now gates on `is_active` and latches after the first
    // active block is taken, mirroring "first active one, else none" (C#'s `SingleOrDefault`
    // additionally throws if *more than one* block is active; Rust is lenient there and simply
    // keeps the first, matching the `Language@isActive`/load.rs:241-244 precedent of leniency
    // in this crate for the analogous "more than one active" case).
    let mut phon_feat_sys_selected = false;

    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Eof => break,
            Event::Start(e) if local(&e) == b"PhonologicalFeatureSystem" => {
                if is_active(&e)? && !phon_feat_sys_selected {
                    raw_features = parse_phon_feature_system(&mut reader)?;
                    phon_feat_sys_selected = true;
                } else {
                    skip_to_end(&mut reader, b"PhonologicalFeatureSystem")?;
                }
            }
            Event::Empty(e) if local(&e) == b"PhonologicalFeatureSystem" => {
                // `<PhonologicalFeatureSystem/>` with no children: zero features, same as a
                // grammar (like Sena) that omits the element entirely. Still must respect
                // isActive/first-wins so an inactive empty block can't block a later active one.
                if is_active(&e)? && !phon_feat_sys_selected {
                    phon_feat_sys_selected = true;
                }
            }
            Event::Start(e) if local(&e) == b"CharacterDefinitionTable" => {
                if is_active(&e)? {
                    let table_id = get_attr(&e, "id")?.unwrap_or_default();
                    let (name, defs) = parse_char_def_table(&mut reader)?;
                    raw_tables.push((table_id, name, defs));
                }
            }
            Event::Empty(e) if local(&e) == b"CharacterDefinitionTable" && is_active(&e)? => {
                let table_id = get_attr(&e, "id")?.unwrap_or_default();
                raw_tables.push((table_id, None, Vec::new()));
            }
            _ => {}
        }
    }

    let feature_system = PhonFeatureSystem::from_raw(raw_features)?;
    let mut tables = Vec::with_capacity(raw_tables.len());
    for (id, name, defs) in raw_tables {
        tables.push(CharDefTable::from_raw(id, name, defs, &feature_system)?);
    }
    Ok(GrammarPhonology {
        feature_system,
        tables,
    })
}

// --- XML plumbing (private) --------------------------------------------------------------

fn xml_err(e: impl std::fmt::Display) -> GrammarError {
    GrammarError::Xml(e.to_string())
}

/// HC XML has no namespaces, so local name == qualified name; standardize on `local_name()`
/// everywhere (its `LocalName::into_inner()` hands back the tag's raw bytes for `==` comparison).
#[inline]
fn local<'a>(e: &'a BytesStart<'a>) -> &'a [u8] {
    e.local_name().into_inner()
}

/// [`local`], for `Event::End`'s `BytesEnd` (a distinct type from `BytesStart` in quick-xml).
#[inline]
fn local_end<'a>(e: &'a BytesEnd<'a>) -> &'a [u8] {
    e.local_name().into_inner()
}

fn get_attr(e: &BytesStart, name: &str) -> Result<Option<String>, GrammarError> {
    match e.try_get_attribute(name).map_err(xml_err)? {
        Some(a) => Ok(Some(a.unescape_value().map_err(xml_err)?.into_owned())),
        None => Ok(None),
    }
}

/// Port of C# `XmlLanguageLoader.IsActive`: `(string)elem.Attribute("isActive") ?? "yes" == "yes"`.
fn is_active(e: &BytesStart) -> Result<bool, GrammarError> {
    match get_attr(e, "isActive")? {
        Some(v) => Ok(v == "yes"),
        None => Ok(true),
    }
}

/// Read text content (concatenating `Text`/`CData` events) up to and including the matching
/// `End(tag)`. Caller has already consumed the opening `Start(tag)`.
fn read_text_until(reader: &mut Reader<&[u8]>, tag: &[u8]) -> Result<String, GrammarError> {
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Text(t) => text.push_str(&t.unescape().map_err(xml_err)?),
            Event::CData(t) => text.push_str(&String::from_utf8_lossy(t.as_ref())),
            Event::End(e) if local_end(&e) == tag => break,
            Event::Eof => return Err(unexpected_eof(tag)),
            _ => {}
        }
    }
    Ok(text)
}

/// Drain a subtree whose opening `Start(tag)` has already been consumed, counting nested
/// same-named elements so the matching `End(tag)` is identified correctly.
fn skip_to_end(reader: &mut Reader<&[u8]>, tag: &[u8]) -> Result<(), GrammarError> {
    let mut depth = 0u32;
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == tag => depth += 1,
            Event::End(e) if local_end(&e) == tag => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Event::Eof => return Err(unexpected_eof(tag)),
            _ => {}
        }
    }
    Ok(())
}

fn unexpected_eof(tag: &[u8]) -> GrammarError {
    GrammarError::Xml(format!(
        "unexpected end of document inside <{}>",
        String::from_utf8_lossy(tag)
    ))
}

// --- PhonologicalFeatureSystem ------------------------------------------------------------

fn parse_phon_feature_system(reader: &mut Reader<&[u8]>) -> Result<Vec<RawFeature>, GrammarError> {
    let mut features = Vec::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == b"SymbolicFeature" => {
                if is_active(&e)? {
                    let xml_id = get_attr(&e, "id")?.unwrap_or_default();
                    // Finding N2: `defaultSymbol` must be read here (from the `<SymbolicFeature>`
                    // start tag itself), not inside `parse_symbolic_feature`, since by the time
                    // that function returns the `BytesStart` borrow has ended.
                    let default_symbol = get_attr(&e, "defaultSymbol")?.filter(|s| !s.is_empty());
                    features.push(parse_symbolic_feature(reader, xml_id, default_symbol)?);
                }
            }
            Event::Empty(e) if local(&e) == b"SymbolicFeature" => {
                if is_active(&e)? {
                    let xml_id = get_attr(&e, "id")?.unwrap_or_default();
                    let default_symbol = get_attr(&e, "defaultSymbol")?.filter(|s| !s.is_empty());
                    features.push(RawFeature {
                        xml_id,
                        name: String::new(),
                        symbols: Vec::new(),
                        default_symbol,
                    });
                }
            }
            Event::End(e) if local_end(&e) == b"PhonologicalFeatureSystem" => break,
            Event::Eof => return Err(unexpected_eof(b"PhonologicalFeatureSystem")),
            _ => {}
        }
    }
    Ok(features)
}

fn parse_symbolic_feature(
    reader: &mut Reader<&[u8]>,
    xml_id: String,
    default_symbol: Option<String>,
) -> Result<RawFeature, GrammarError> {
    let mut name = String::new();
    let mut symbols = Vec::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == b"Name" => name = read_text_until(reader, b"Name")?,
            Event::Empty(e) if local(&e) == b"Name" => {
                let _ = e;
            }
            Event::Start(e) if local(&e) == b"Symbols" => {
                symbols = parse_symbols(reader)?;
            }
            Event::Empty(e) if local(&e) == b"Symbols" => {
                let _ = e;
            }
            Event::End(e) if local_end(&e) == b"SymbolicFeature" => break,
            Event::Eof => return Err(unexpected_eof(b"SymbolicFeature")),
            _ => {}
        }
    }
    Ok(RawFeature {
        xml_id,
        name,
        symbols,
        default_symbol,
    })
}

fn parse_symbols(reader: &mut Reader<&[u8]>) -> Result<Vec<(String, String)>, GrammarError> {
    let mut symbols = Vec::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == b"Symbol" => {
                let id = get_attr(&e, "id")?.unwrap_or_default();
                let name = read_text_until(reader, b"Symbol")?;
                symbols.push((id, name));
            }
            Event::Empty(e) if local(&e) == b"Symbol" => {
                let id = get_attr(&e, "id")?.unwrap_or_default();
                symbols.push((id, String::new()));
            }
            Event::End(e) if local_end(&e) == b"Symbols" => break,
            Event::Eof => return Err(unexpected_eof(b"Symbols")),
            _ => {}
        }
    }
    Ok(symbols)
}

// --- CharacterDefinitionTable --------------------------------------------------------------

fn parse_char_def_table(
    reader: &mut Reader<&[u8]>,
) -> Result<(Option<String>, Vec<RawCharDef>), GrammarError> {
    let mut name = None;
    let mut defs = Vec::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == b"Name" => {
                name = Some(read_text_until(reader, b"Name")?)
            }
            Event::Empty(e) if local(&e) == b"Name" => {
                let _ = e;
                name = Some(String::new());
            }
            Event::Start(e) if local(&e) == b"SegmentDefinitions" => {
                parse_definitions_block(
                    reader,
                    b"SegmentDefinitions",
                    b"SegmentDefinition",
                    CharDefKind::Segment,
                    &mut defs,
                )?;
            }
            Event::Empty(e) if local(&e) == b"SegmentDefinitions" => {
                let _ = e;
            }
            Event::Start(e) if local(&e) == b"BoundaryDefinitions" => {
                parse_definitions_block(
                    reader,
                    b"BoundaryDefinitions",
                    b"BoundaryDefinition",
                    CharDefKind::Boundary,
                    &mut defs,
                )?;
            }
            Event::Empty(e) if local(&e) == b"BoundaryDefinitions" => {
                let _ = e;
            }
            Event::End(e) if local_end(&e) == b"CharacterDefinitionTable" => break,
            Event::Eof => return Err(unexpected_eof(b"CharacterDefinitionTable")),
            _ => {}
        }
    }
    Ok((name, defs))
}

fn parse_definitions_block(
    reader: &mut Reader<&[u8]>,
    close_tag: &[u8],
    item_tag: &[u8],
    kind: CharDefKind,
    out: &mut Vec<RawCharDef>,
) -> Result<(), GrammarError> {
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == item_tag => {
                if is_active(&e)? {
                    let xml_id = get_attr(&e, "id")?.unwrap_or_default();
                    let (representations, feature_values) =
                        parse_definition_body(reader, item_tag)?;
                    out.push(RawCharDef {
                        xml_id,
                        kind,
                        representations,
                        feature_values,
                    });
                } else {
                    skip_to_end(reader, item_tag)?;
                }
            }
            Event::Empty(e) if local(&e) == item_tag => {
                if is_active(&e)? {
                    let xml_id = get_attr(&e, "id")?.unwrap_or_default();
                    out.push(RawCharDef {
                        xml_id,
                        kind,
                        representations: Vec::new(),
                        feature_values: Vec::new(),
                    });
                }
            }
            Event::End(e) if local_end(&e) == close_tag => break,
            Event::Eof => return Err(unexpected_eof(close_tag)),
            _ => {}
        }
    }
    Ok(())
}

fn parse_definition_body(
    reader: &mut Reader<&[u8]>,
    close_tag: &[u8],
) -> Result<(Vec<String>, Vec<RawFeatureValue>), GrammarError> {
    let mut representations = Vec::new();
    let mut feature_values = Vec::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == b"Representations" => {
                representations = parse_representations(reader)?;
            }
            Event::Empty(e) if local(&e) == b"Representations" => {
                let _ = e;
            }
            Event::Empty(e) if local(&e) == b"FeatureValue" => {
                if is_active(&e)? {
                    if let Some(fv) = feature_value_from_attrs(&e)? {
                        feature_values.push(fv);
                    }
                    // else: a nested-ComplexFeature FeatureValue (no `symbolValues`). None of the
                    // three reference grammars' char-def tables ever omit `symbolValues` (verified
                    // by corpus scan); M1 doesn't need nested/complex char-def features (they only
                    // matter to the M3 FST/rules pipeline), so it is silently dropped here rather
                    // than failing the whole grammar load over an unused field.
                }
            }
            Event::Start(e) if local(&e) == b"FeatureValue" => {
                if is_active(&e)? {
                    if let Some(fv) = feature_value_from_attrs(&e)? {
                        feature_values.push(fv);
                    }
                }
                skip_to_end(reader, b"FeatureValue")?;
            }
            Event::End(e) if local_end(&e) == close_tag => break,
            Event::Eof => return Err(unexpected_eof(close_tag)),
            _ => {}
        }
    }
    Ok((representations, feature_values))
}

fn feature_value_from_attrs(e: &BytesStart) -> Result<Option<RawFeatureValue>, GrammarError> {
    let feature_xml_id = get_attr(e, "feature")?.unwrap_or_default();
    match get_attr(e, "symbolValues")? {
        Some(vals) if !vals.is_empty() => Ok(Some(RawFeatureValue {
            feature_xml_id,
            symbol_xml_ids: vals.split_whitespace().map(str::to_string).collect(),
        })),
        _ => Ok(None),
    }
}

fn parse_representations(reader: &mut Reader<&[u8]>) -> Result<Vec<String>, GrammarError> {
    let mut reps = Vec::new();
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) if local(&e) == b"Representation" => {
                reps.push(read_text_until(reader, b"Representation")?);
            }
            Event::Empty(e) if local(&e) == b"Representation" => {
                let _ = e;
                reps.push(String::new());
            }
            Event::End(e) if local_end(&e) == b"Representations" => break,
            Event::Eof => return Err(unexpected_eof(b"Representations")),
            _ => {}
        }
    }
    Ok(reps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn crate_builds() {
        assert_eq!(2 + 2, 4);
    }

    const HAND_BUILT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Test</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat1">
        <Name>voice</Name>
        <Symbols>
          <Symbol id="symP">+</Symbol>
          <Symbol id="symM">-</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char1">
          <Representations>
            <Representation>s</Representation>
          </Representations>
          <FeatureValue feature="feat1" symbolValues="symM" />
        </SegmentDefinition>
        <SegmentDefinition id="char2">
          <Representations>
            <Representation>y</Representation>
          </Representations>
        </SegmentDefinition>
        <SegmentDefinition id="char3">
          <Representations>
            <Representation>sy</Representation>
          </Representations>
        </SegmentDefinition>
        <SegmentDefinition id="char4">
          <Representations>
            <Representation>m</Representation>
            <Representation>n</Representation>
          </Representations>
        </SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char5">
          <Representations>
            <Representation>+</Representation>
          </Representations>
        </BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn loads_hand_built_grammar() {
        let g = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
        // 1 authored feature + the always-appended synthetic `Type` feature (plan §13.1 Tier-1 #1).
        assert_eq!(g.feature_system().len(), 2);
        assert_eq!(
            g.feature_system().flat_index("feat1"),
            Some(featsys::FlatIndex(0))
        );
        let table = g.main_table().unwrap();
        assert_eq!(table.len(), 5); // 4 segments + 1 boundary
        assert!(table.lookup_nfd("m").is_some());
        assert_eq!(table.lookup_nfd("m"), table.lookup_nfd("n"));
    }

    #[test]
    fn hand_built_grammar_greedy_segmentation_prefers_two_char_rep() {
        let g = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
        let table = g.main_table().unwrap();
        let shape = segment::segment(table, "sy").unwrap();
        assert_eq!(
            shape.interior().count(),
            1,
            "must match the 2-char 'sy' def, not 's'+'y'"
        );
    }

    // --- Finding N1 (phase2 audit C): PhonologicalFeatureSystem@isActive -----------------------

    /// Two `<PhonologicalFeatureSystem>` blocks, the SECOND (inactive, `isActive="no"`) listed
    /// last with a different feature set than the first (active, default `isActive`). Mirrors
    /// C# `XmlLanguageLoader.LoadLanguage`'s `Elements("PhonologicalFeatureSystem")
    /// .SingleOrDefault(IsActive)` (`XmlLanguageLoader.cs:239`): the active block must win
    /// regardless of document position. Pre-fix, this loop had no `isActive` check at all and
    /// simply let the *last* block win — with this fixture's ordering that bug would select the
    /// inactive block (`feat_b`, not `feat_a`), which this test's assertions on `flat_index`
    /// would catch immediately (`feat_a` absent, `feat_b` present — the reverse of what follows).
    const TWO_PHON_FEATURE_SYSTEMS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N1Test</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_a">
        <Name>active</Name>
        <Symbols>
          <Symbol id="a_p">+</Symbol>
          <Symbol id="a_m">-</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <PhonologicalFeatureSystem isActive="no">
      <SymbolicFeature id="feat_b">
        <Name>inactive draft</Name>
        <Symbols>
          <Symbol id="b_p">+</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char1">
          <Representations>
            <Representation>x</Representation>
          </Representations>
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn phon_feature_system_honors_is_active_not_last_block() {
        let g = load_char_def_table_from_xml(TWO_PHON_FEATURE_SYSTEMS_XML).unwrap();
        assert!(
            g.feature_system().flat_index("feat_a").is_some(),
            "the active (first) block's feature must be loaded"
        );
        assert!(
            g.feature_system().flat_index("feat_b").is_none(),
            "the inactive (last) block's feature must NOT be loaded (last-block-wins bug)"
        );
    }

    /// A single inactive block (no active block at all) must behave like no
    /// `<PhonologicalFeatureSystem>` at all — zero authored features (`SingleOrDefault` -> null).
    #[test]
    fn phon_feature_system_all_inactive_yields_no_features() {
        let xml = TWO_PHON_FEATURE_SYSTEMS_XML.replacen(
            "<PhonologicalFeatureSystem>",
            "<PhonologicalFeatureSystem isActive=\"no\">",
            1,
        );
        let g = load_char_def_table_from_xml(&xml).unwrap();
        assert!(g.feature_system().flat_index("feat_a").is_none());
        assert!(g.feature_system().flat_index("feat_b").is_none());
    }

    #[test]
    fn dump_char_defs_is_deterministic() {
        let g = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
        let d1 = g.dump_char_defs();
        let g2 = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
        let d2 = g2.dump_char_defs();
        assert_eq!(d1, d2);
        // 1 authored feature + the always-appended synthetic `Type` feature.
        assert!(d1.contains("features=2"));
        assert!(d1.contains("char_defs=5"));
    }

    /// Locates a real sample grammar file on disk. These are untracked corpus files (per
    /// `rust-conversion.md` §8's "corpora stay untracked local files with self-skipping
    /// `[Explicit]`/`#[ignore]` tests" process rule) — present on this development machine but
    /// not guaranteed present elsewhere (CI, a fresh clone). Returns `None` if absent so callers
    /// can self-skip rather than fail.
    fn sample_path(name: &str) -> Option<PathBuf> {
        // CARGO_MANIFEST_DIR = .../rust/crates/hc-grammar ; samples live at repo_root/samples/data.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_words(name: &str) -> Option<Vec<String>> {
        let path = sample_path(name)?;
        let text = std::fs::read_to_string(path).ok()?;
        Some(
            text.lines()
                .map(str::to_string)
                .filter(|l| !l.is_empty())
                .collect(),
        )
    }

    /// Shared body for the three real-grammar tests: load, sanity-check the feature system and
    /// char-def table (against **exact counts independently confirmed by scanning the raw XML**,
    /// not just non-empty — a loader that silently dropped char defs would still pass a
    /// non-empty check as long as any survived), segment every requested word without error, and
    /// check determinism + node-count sanity.
    ///
    /// `expected_feature_count`/`expected_char_def_count` were obtained independently of this
    /// crate (`grep -c '<SymbolicFeature id'` scoped to the `<PhonologicalFeatureSystem>` block,
    /// and `grep -c '<SegmentDefinition id\|<BoundaryDefinition id'`), so a match here is real
    /// evidence the loader isn't dropping elements, not just an echo of its own output.
    /// `expected_feature_count` is the *authored* (XML-visible) count; the assertion below adds 1
    /// for the always-appended synthetic `Type` feature (plan §13.1 Tier-1 #1), which is not an
    /// XML-grep-able quantity and so is kept out of each call site's grep-verified number.
    fn check_grammar(
        xml_name: &str,
        words_name: &str,
        sample_words: &[&str],
        expected_feature_count: usize,
        expected_char_def_count: usize,
    ) {
        let Some(xml_path) = sample_path(xml_name) else {
            eprintln!("skipping {xml_name}: sample grammar not present on disk");
            return;
        };
        let xml = std::fs::read_to_string(&xml_path).expect("read sample grammar");
        let grammar = load_char_def_table_from_xml(&xml)
            .unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"));

        assert_eq!(
            grammar.feature_system().len(),
            expected_feature_count + 1,
            "{xml_name}: phonological feature count mismatch (authored count + the always-appended \
             Type feature) — loader may be dropping or double-counting <SymbolicFeature> elements"
        );

        let table = grammar.main_table().unwrap_or_else(|| {
            panic!("{xml_name}: expected at least one CharacterDefinitionTable")
        });
        assert_eq!(
            table.len(),
            expected_char_def_count,
            "{xml_name}: char-def count mismatch — loader may be dropping or double-counting \
             <SegmentDefinition>/<BoundaryDefinition> elements"
        );

        let words = load_words(words_name).unwrap_or_else(|| panic!("failed to read {words_name}"));
        for &w in sample_words {
            assert!(
                words.iter().any(|line| line == w),
                "{w:?} not found in {words_name} — test data assumption stale"
            );
        }

        for &word in sample_words {
            let nfd_len = nfd::nfd(word).chars().count();
            let shape1 = segment::segment(table, word)
                .unwrap_or_else(|e| panic!("{xml_name}: failed to segment {word:?}: {e}"));
            let interior_len = shape1.interior().count();
            assert!(
                (1..=nfd_len).contains(&interior_len),
                "{xml_name}: segmenting {word:?} produced {interior_len} interior nodes, expected \
                 1..={nfd_len} (each match step consumes >=1 char and emits exactly 1 node)"
            );
            let shape2 = segment::segment(table, word).unwrap();
            assert_eq!(
                shape1, shape2,
                "{xml_name}: segmenting {word:?} must be deterministic"
            );
        }
    }

    #[test]
    fn loads_and_segments_indonesian() {
        // Counts independently confirmed via `grep -c` scoped to the relevant XML blocks:
        // 14 <SymbolicFeature> under <PhonologicalFeatureSystem>; 29 <SegmentDefinition> + 3
        // <BoundaryDefinition> = 32 char defs under <CharacterDefinitionTable>.
        check_grammar(
            "indonesian-hc.xml",
            "indonesian-words.txt",
            &["ajar", "amat", "ambil", "baca", "bagi"],
            14,
            32,
        );
    }

    #[test]
    fn loads_and_segments_amharic() {
        // amharic-words.txt's first few lines are English glosses (leftover header content, not
        // target-language forms); the actual Amharic surface words are in Ge'ez/Ethiopic script.
        // Counts independently confirmed via `grep -c`: 22 <SymbolicFeature> under
        // <PhonologicalFeatureSystem>; 417 <SegmentDefinition> + 3 <BoundaryDefinition> = 420.
        check_grammar(
            "amharic-hc.xml",
            "amharic-words.txt",
            &["ሂዱ", "ሄደ", "ሆድ"],
            22,
            420,
        );
    }

    #[test]
    fn loads_and_segments_sena() {
        // Sena's real XML has no <PhonologicalFeatureSystem> element at all (its SymbolicFeatures
        // live under HeadFeatures, the syntactic feature system, out of this milestone's scope) —
        // `expected_feature_count: 0` documents that as a verified data reality, not a bug (the
        // loader still reports `feature_system().len() == 1`: `check_grammar`'s `+ 1` for the
        // always-appended synthetic `Type` feature, plan §13.1 Tier-1 #1).
        // Char-def count independently confirmed via `grep -c`: 40 <SegmentDefinition> + 3
        // <BoundaryDefinition> = 43.
        check_grammar(
            "sena-hc.xml",
            "sena-words.txt",
            &["pibubu", "piratu", "mbali", "atawirambo"],
            0,
            43,
        );
    }

    /// Diagnostic, not a required gate (the full-corpus golden-parity gate is plan §8 layer 3,
    /// a later milestone): segments *every* word in each corpus and reports the failure rate.
    /// `#[ignore]`d so `cargo test` stays fast; run explicitly with `--ignored --nocapture`.
    #[test]
    #[ignore = "diagnostic full-corpus survey, not part of the M1 acceptance gate"]
    fn full_corpus_segmentation_survey() {
        for (xml_name, words_name) in [
            ("indonesian-hc.xml", "indonesian-words.txt"),
            ("amharic-hc.xml", "amharic-words.txt"),
            ("sena-hc.xml", "sena-words.txt"),
        ] {
            let Some(xml_path) = sample_path(xml_name) else {
                eprintln!("skipping {xml_name}: not present on disk");
                continue;
            };
            let xml = std::fs::read_to_string(&xml_path).unwrap();
            let grammar = load_char_def_table_from_xml(&xml).unwrap();
            let table = grammar.main_table().unwrap();
            let words = load_words(words_name).unwrap();
            let mut ok = 0usize;
            let mut failures: Vec<(String, usize)> = Vec::new();
            for w in &words {
                match segment::segment(table, w) {
                    Ok(_) => ok += 1,
                    Err(e) => failures.push((w.clone(), e.position)),
                }
            }
            eprintln!(
                "{xml_name}: {ok}/{} segmented without error ({} failures)",
                words.len(),
                failures.len()
            );
            for (w, pos) in failures.iter().take(10) {
                eprintln!("  fail: {w:?} at position {pos}");
            }
        }
    }
}
