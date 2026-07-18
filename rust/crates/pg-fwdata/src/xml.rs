//! Streaming `.fwdata` reader: walks the flat sequence of `<rt class="..." guid="..."
//! [ownerguid="..."]>` records one at a time (`quick_xml`'s pull-based `Reader`, never a DOM of
//! the whole document — Sena 3 is ~54MB) and builds a [`RawGraph`] keyed by GUID, containing
//! only records whose `class` is one this crate's extractor understands. Every other class
//! (the bulk of a real project — `ChkRef`, `WfiWordform`, `StText`, Scripture data, ...) is
//! skipped without ever being parsed into a [`crate::node::Node`].

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::node::Node;
use crate::ImportError;

/// One retained `<rt>` record: its LCM class, its own GUID, its owner's GUID (absent for a
/// handful of singleton objects FieldWorks omits `ownerguid` for, e.g. `LexEntry`), and the
/// parsed body. `ownerguid` isn't read by the current extractor (every "which collection does
/// this belong to" question is instead answered by walking the *owner's* named field, which also
/// gives ordering) but is cheap to carry and useful for future diagnostics/debugging.
#[derive(Debug)]
#[allow(dead_code)]
pub struct Record {
    pub class: String,
    pub guid: String,
    pub ownerguid: Option<String>,
    pub node: Node,
}

/// Every retained record, keyed by GUID. Cross-references (`objsur` targets) resolve against
/// this map regardless of whether the link was owning (`t="o"`) or by-reference (`t="r"`) —
/// once parsed, both are just a GUID to look up.
#[derive(Debug, Default)]
pub struct RawGraph {
    pub records: HashMap<String, Record>,
    /// `LexEntry` guids in file-encounter order. `LexEntry` is declared `owner="none"` in the LCM
    /// schema (confirmed empirically too — `<rt class="LexEntry" guid="...">` never carries an
    /// `ownerguid` attribute) and `LexDb` has no ordered `Entries` sequence field either; the only
    /// deterministic order available at all is raw document order, so the streaming parser
    /// records it directly here rather than the extractor discovering entries via
    /// [`RawGraph::by_class`] (hashmap iteration order is per-process-random and would break the
    /// "same file twice → byte-identical JSON" determinism requirement).
    pub lex_entry_order: Vec<String>,
}

impl RawGraph {
    pub fn get(&self, guid: &str) -> Option<&Record> {
        self.records.get(guid)
    }

    /// All retained records of a given class, in arbitrary (hashmap) order — `HashMap` iteration
    /// order is randomized per-process (`SipHash` seeding), so anything built from it would
    /// *not* satisfy "importing the same file twice produces byte-identical JSON" across two
    /// separate process runs (an intra-process determinism test using the same `HashMap` would
    /// pass while still hiding this). The **only** current caller is
    /// [`crate::extract::project::find_lang_project`], which is safe precisely because
    /// `LangProject` is a file singleton — `.next()` over a one-element filtered set can't
    /// observe ordering. Every other ordered output in this crate comes from a named-field
    /// `objsur_list` walk, a `CmPossibilityList`/`SubPossibilities` tree walk, or
    /// [`RawGraph::lex_entry_order`] — never this method. Keep it that way: a new caller of
    /// `by_class` feeding `Snapshot` output would silently reintroduce the hazard.
    pub fn by_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        self.records.values().filter(move |r| r.class == class)
    }
}

/// The LCM classes this crate's extractor reads. Every other `<rt class="...">` in a `.fwdata`
/// file is skipped (subtree not parsed) as soon as its class attribute is seen.
const ALLOWED_CLASSES: &[&str] = &[
    // project / roots
    "LangProject",
    "LexDb",
    "MoMorphData",
    "PhPhonData",
    // feature systems
    "FsFeatureSystem",
    "FsClosedFeature",
    "FsComplexFeature",
    "FsSymFeatVal",
    "FsFeatStruc",
    "FsClosedValue",
    "FsComplexValue",
    // phonology
    "PhPhonemeSet",
    "PhPhoneme",
    "PhCode",
    "PhBdryMarker",
    "PhNCSegments",
    "PhNCFeatures",
    "PhEnvironment",
    "PhFeatureConstraint",
    "PhPhonRuleFeat",
    "PhRegularRule",
    "PhMetathesisRule",
    "PhSegRuleRHS",
    "PhSequenceContext",
    "PhIterationContext",
    "PhSimpleContextSeg",
    "PhSimpleContextNC",
    "PhSimpleContextBdry",
    "PhVariable",
    // morphology
    "CmPossibilityList",
    "CmPossibility",
    "PartOfSpeech",
    "MoInflClass",
    "MoStemName",
    "MoInflAffixSlot",
    "MoInflAffixTemplate",
    "MoMorphType",
    "MoEndoCompound",
    "MoExoCompound",
    "MoAlloAdhocProhib",
    "MoMorphAdhocProhib",
    "LexEntryInflType",
    "LexEntryType",
    // lexicon
    "LexEntry",
    "MoStemAllomorph",
    "MoAffixAllomorph",
    "MoAffixProcess",
    "MoInsertNC",
    "MoCopyFromInput",
    "MoInsertPhones",
    "MoModifyFromInput",
    "MoStemMsa",
    "MoInflAffMsa",
    "MoDerivAffMsa",
    "MoUnclassifiedAffixMsa",
    "LexSense",
    "LexEntryRef",
];

fn class_allowed(class: &str) -> bool {
    ALLOWED_CLASSES.contains(&class)
}

fn get_attr(e: &BytesStart, name: &str) -> Result<Option<String>, ImportError> {
    for a in e.attributes() {
        let a = a.map_err(|err| ImportError::Xml(err.to_string()))?;
        if a.key.as_ref() == name.as_bytes() {
            let v = a
                .unescape_value()
                .map_err(|err| ImportError::Xml(err.to_string()))?;
            return Ok(Some(v.into_owned()));
        }
    }
    Ok(None)
}

/// Parse `path` into a [`RawGraph`]. Hard errors are reserved for I/O failures and XML that
/// isn't well-formed at all (or isn't a `.fwdata` document — no `<languageproject>`/`<rt>`
/// elements found); anything else (dangling references, missing fields, unknown morph types) is
/// the extractor's job to log as a warning, never this layer's.
pub fn parse_fwdata(path: &Path) -> Result<RawGraph, ImportError> {
    let file = File::open(path).map_err(ImportError::Io)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut graph = RawGraph::default();
    let mut saw_any_rt = false;

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| ImportError::Xml(e.to_string()))?;
        match event {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == b"rt" => {
                let class = get_attr(&e, "class")?.unwrap_or_default();
                let guid = get_attr(&e, "guid")?.unwrap_or_default();
                let ownerguid = get_attr(&e, "ownerguid")?;
                saw_any_rt = true;
                if class_allowed(&class) {
                    let node = parse_rt_body(&mut reader)?;
                    if class == "LexEntry" {
                        graph.lex_entry_order.push(guid.clone());
                    }
                    graph.records.insert(
                        guid.clone(),
                        Record {
                            class,
                            guid,
                            ownerguid,
                            node,
                        },
                    );
                } else {
                    let mut skip_buf = Vec::new();
                    reader
                        .read_to_end_into(e.name(), &mut skip_buf)
                        .map_err(|e| ImportError::Xml(e.to_string()))?;
                }
            }
            Event::Empty(e) if e.local_name().as_ref() == b"rt" => {
                // A record with no body at all (e.g. `<rt class="PhVariable" guid="..."
                // ownerguid="..." />`) — still a valid, retainable record with an empty node.
                let class = get_attr(&e, "class")?.unwrap_or_default();
                let guid = get_attr(&e, "guid")?.unwrap_or_default();
                let ownerguid = get_attr(&e, "ownerguid")?;
                saw_any_rt = true;
                if class_allowed(&class) {
                    if class == "LexEntry" {
                        graph.lex_entry_order.push(guid.clone());
                    }
                    graph.records.insert(
                        guid.clone(),
                        Record {
                            class,
                            guid,
                            ownerguid,
                            node: Node::empty(),
                        },
                    );
                }
            }
            _ => {}
        }
        buf.clear();
    }

    if !saw_any_rt {
        return Err(ImportError::NotFwdata);
    }

    Ok(graph)
}

/// Parse everything up to (and including) the matching `</rt>` into a [`Node`] representing the
/// `<rt>` element itself (its `children` are the record's property elements).
fn parse_rt_body(reader: &mut Reader<BufReader<File>>) -> Result<Node, ImportError> {
    let mut stack: Vec<Node> = vec![Node::empty()];
    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| ImportError::Xml(e.to_string()))?;
        match event {
            Event::Start(e) => stack.push(node_from_start(&e)?),
            Event::Empty(e) => {
                let node = node_from_start(&e)?;
                stack
                    .last_mut()
                    .expect("root never popped here")
                    .children
                    .push(node);
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(|e| ImportError::Xml(e.to_string()))?;
                stack
                    .last_mut()
                    .expect("root never popped here")
                    .text
                    .push_str(&s);
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(t.as_ref()).into_owned();
                stack
                    .last_mut()
                    .expect("root never popped here")
                    .text
                    .push_str(&s);
            }
            Event::End(_) => {
                if stack.len() == 1 {
                    // This is the closing `</rt>` for the record itself.
                    break;
                }
                let node = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(node);
            }
            Event::Eof => {
                return Err(ImportError::Xml(
                    "unexpected end of file inside <rt> element".to_string(),
                ))
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(stack.pop().unwrap())
}

fn node_from_start(e: &BytesStart) -> Result<Node, ImportError> {
    let tag = String::from_utf8_lossy(e.local_name().into_inner()).into_owned();
    let mut attrs = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|err| ImportError::Xml(err.to_string()))?;
        let key = String::from_utf8_lossy(a.key.local_name().into_inner()).into_owned();
        let val = a
            .unescape_value()
            .map_err(|err| ImportError::Xml(err.to_string()))?
            .into_owned();
        attrs.push((key, val));
    }
    Ok(Node {
        tag,
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}
