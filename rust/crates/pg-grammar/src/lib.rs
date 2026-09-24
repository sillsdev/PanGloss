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

pub mod compile;
pub mod grammar_health;
pub(crate) mod grammar_health_presentation;
pub mod lint;
pub mod load;
pub mod stats_identity;

pub use pg_grammar_model::{chardef, featsys, model, nfd, segment};

pub use compile::{compile_project, compile_project_measured, compile_project_with};
pub use load::load;

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::reader::Reader;
use thiserror::Error;

use chardef::{CharDefKind, CharDefTable, RawCharDef, RawFeatureValue};
use featsys::{PhonFeatureSystem, RawFeature};
use pg_snapshot::ConversionIssue;

use compile::issues::ConversionError;

/// Errors surfaced by grammar loading. `Unsupported` drives managed fallback (plan §8 layer 6).
#[derive(Debug, Error)]
pub enum GrammarError {
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("unsupported grammar construct: {0}")]
    Unsupported(String),
    #[error("grammar semantic error: {0}")]
    Semantic(String),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error("duplicate character-definition representation: {0}")]
    DuplicateRepresentation(String),
    #[error("cannot compile compounding: {0}")]
    UnsegmentableBoundary(String),
}

impl From<pg_grammar_model::ModelError> for GrammarError {
    fn from(error: pg_grammar_model::ModelError) -> Self {
        match error {
            pg_grammar_model::ModelError::Unsupported(message) => Self::Unsupported(message),
            pg_grammar_model::ModelError::Semantic(message) => Self::Semantic(message),
            pg_grammar_model::ModelError::DuplicateRepresentation(message) => {
                Self::DuplicateRepresentation(message)
            }
        }
    }
}

impl GrammarError {
    /// Every [`ConversionIssue`] this error carries, or `&[]` for a variant that carries none.
    pub fn issues(&self) -> &[ConversionIssue] {
        match self {
            GrammarError::Conversion(e) => &e.issues,
            _ => &[],
        }
    }
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

    /// Consume this phonology into its parts (for embedding in `model::Grammar`, which owns
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
    // quick_xml defaults this to false, so `--` inside a comment was silently tolerated.
    reader.config_mut().check_comments = true;

    let mut raw_features: Vec<RawFeature> = Vec::new();
    let mut raw_tables: Vec<(String, Option<String>, Vec<RawCharDef>)> = Vec::new();
    // C# selects the one active `<PhonologicalFeatureSystem>` block among possibly several (`SingleOrDefault(IsActive)`); `phon_feat_sys_selected` gates on `is_active` and latches after the first active block, leniently keeping the first if more than one is active rather than throwing.
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
                // `<PhonologicalFeatureSystem/>` with no children: zero features, same as omitting the element entirely; still respects isActive/first-wins so an inactive empty block can't block a later active one.
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

/// HC XML has no namespaces, so local name == qualified name; standardize on `local_name()` everywhere.
#[inline]
fn local<'a>(e: &'a BytesStart<'a>) -> &'a [u8] {
    e.local_name().into_inner()
}

/// `local`, for `Event::End`'s `BytesEnd` (a distinct type from `BytesStart` in quick-xml).
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

/// Read text content (concatenating `Text`/`CData` events) up to and including the matching `End(tag)`; caller has already consumed the opening `Start(tag)`.
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

/// Drain a subtree whose opening `Start(tag)` has already been consumed, counting nested same-named elements so the matching `End(tag)` is identified correctly.
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
                    // `defaultSymbol` must be read here, from the start tag itself, since by the time `parse_symbolic_feature` returns the `BytesStart` borrow has ended.
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
                        source_guid: None,
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
                        source_guid: None,
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
                    // else: a nested-ComplexFeature FeatureValue (no symbolValues); silently dropped since this pass doesn't need nested/complex char-def features, rather than failing the whole grammar load over an unused field.
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
mod tests;
