//! Streaming `.fwdata` reader: pulls `<rt class="..." guid="...">` records one at a time (never a DOM of the whole document) into a `RawGraph`, skipping any class this crate's extractor doesn't understand before it is ever parsed into a `Node`.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::node::Node;
use crate::ImportError;

/// One retained `<rt>` record. Ownership comes from walking the owner's named field rather than
/// from the optional `ownerguid` XML attribute.
#[derive(Debug)]
pub struct Record {
    pub class: String,
    pub guid: String,
    pub node: Node,
}

/// Every retained record, keyed by GUID; owning (`t="o"`) and by-reference (`t="r"`) links both resolve here the same way, as a GUID lookup.
#[derive(Debug, Default)]
pub struct RawGraph {
    pub records: HashMap<String, Record>,
    /// `LexEntry` guids in file-encounter order — the only deterministic order available, since `LexEntry` has no `ownerguid` and `LexDb` has no ordered entries field to fall back on.
    pub lex_entry_order: Vec<String>,
}

impl RawGraph {
    pub fn get(&self, guid: &str) -> Option<&Record> {
        self.records.get(guid)
    }

    /// Arbitrary (per-process-random) hashmap order; safe only for `find_lang_project`'s singleton lookup — a new ordered-output caller would break cross-run JSON determinism.
    pub fn by_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        self.records.values().filter(move |r| r.class == class)
    }
}

/// The LCM classes this crate's extractor reads; every other `<rt class="...">` is skipped unparsed.
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

/// Parse `path` into a `RawGraph`; hard errors are reserved for I/O and malformed/non-`.fwdata` XML, everything else is the extractor's job to warn on.
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
                saw_any_rt = true;
                if class_allowed(&class) {
                    let node = parse_rt_body(&mut reader)?;
                    if class == "LexEntry" {
                        graph.lex_entry_order.push(guid.clone());
                    }
                    graph
                        .records
                        .insert(guid.clone(), Record { class, guid, node });
                } else {
                    let mut skip_buf = Vec::new();
                    reader
                        .read_to_end_into(e.name(), &mut skip_buf)
                        .map_err(|e| ImportError::Xml(e.to_string()))?;
                }
            }
            Event::Empty(e) if e.local_name().as_ref() == b"rt" => {
                // A self-closed `<rt .../>` with no body is still a valid record, with an empty node.
                let class = get_attr(&e, "class")?.unwrap_or_default();
                let guid = get_attr(&e, "guid")?.unwrap_or_default();
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

/// Parse up to and including the matching `</rt>` into a `Node`, whose `children` are the record's property elements.
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
