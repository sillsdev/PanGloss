//! Streaming `.fwdata` reader: pulls `<rt class="..." guid="...">` records one at a time (never a DOM of the whole document) into a `RawGraph`, skipping any class this crate's extractor doesn't understand before it is ever parsed into a `Node`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use sha2::{Digest, Sha256};

use pg_snapshot::{
    canonical_guid, ConversionIssue, FwClass, IssueClass, RawSourceCensus, RawSourceObject,
    SourceRef,
};

use crate::extract::codes;
use crate::node::{Attr, Node, KEPT_ATTRIBUTES};
use crate::ImportError;

/// Multiply-rotate hasher for the fixed set of recognized class names.
#[derive(Default, Clone, Copy)]
pub(crate) struct FastHasher(u64);

const FAST_HASH_K: u64 = 0x517c_c1b7_2722_0a95;

impl FastHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(FAST_HASH_K);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            self.add(u64::from_le_bytes(bytes[..8].try_into().unwrap()));
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            self.add(u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64);
            bytes = &bytes[4..];
        }
        for b in bytes {
            self.add(*b as u64);
        }
    }

    #[inline]
    fn write_u8(&mut self, byte: u8) {
        self.add(byte as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

type FastBuild = BuildHasherDefault<FastHasher>;

/// One retained `<rt>` record; ownership comes from walking the owner's named field, not the optional `ownerguid` XML attribute.
#[derive(Debug)]
pub struct Record {
    pub class: &'static str,
    pub guid: Arc<str>,
    pub node: Node,
}

/// One `<rt class="..." guid="...">` header, recorded in document order regardless of whether its class is understood or its guid is present.
#[derive(Debug, Clone)]
pub struct RawRecordHeader {
    pub ordinal: u32,
    pub class: Arc<str>,
    pub guid: Arc<str>,
    pub retained: bool,
    pub duplicate: bool,
}

const NO_RECORD: u32 = u32::MAX;

/// Retained records in document order, found by guid; owning (`t="o"`) and by-reference (`t="r"`) links both resolve here the same way, as a GUID lookup.
#[derive(Debug, Default)]
pub struct RecordStore {
    records: Vec<Record>,
    /// Every non-empty guid seen on any header: the index of its record, or `NO_RECORD` while only unrecognized-class headers carry it.
    index: HashMap<Arc<str>, u32>,
}

impl RecordStore {
    pub fn get(&self, guid: &str) -> Option<&Record> {
        match self.index.get(guid) {
            Some(&at) if at != NO_RECORD => Some(&self.records[at as usize]),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn contains_key(&self, guid: &str) -> bool {
        self.get(guid).is_some()
    }

    /// Document order.
    pub fn values(&self) -> std::slice::Iter<'_, Record> {
        self.records.iter()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.records.len()
    }
}

/// Every retained record, keyed by GUID.
#[derive(Debug, Default)]
pub struct RawGraph {
    pub records: RecordStore,
    /// `LexEntry` guids in file-encounter order — the only deterministic order available, since `LexEntry` has no `ownerguid` and `LexDb` has no ordered entries field to fall back on.
    pub lex_entry_order: Vec<Arc<str>>,
    /// Every `<rt>` header in document order, allowed and unknown classes alike.
    pub headers: Vec<RawRecordHeader>,
    /// Structural integrity issues found while parsing: duplicate or missing guids.
    pub issues: Vec<ConversionIssue>,
}

impl RawGraph {
    pub fn get(&self, guid: &str) -> Option<&Record> {
        self.records.get(guid)
    }

    /// Document order.
    pub fn by_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        self.records.values().filter(move |r| r.class == class)
    }

    /// Tallies every header in document order; `ordered_header_sha256` hashes the canonical `"{class}\t{guid}\n"` byte stream per header, in order, as lowercase hex SHA-256.
    pub fn census(&self) -> RawSourceCensus {
        let mut class_occurrences = BTreeMap::<&str, u64>::new();
        let mut hasher = Sha256::new();
        for header in &self.headers {
            *class_occurrences.entry(&*header.class).or_default() += 1;
            hasher.update(header.class.as_bytes());
            hasher.update(b"\t");
            hasher.update(header.guid.as_bytes());
            hasher.update(b"\n");
        }
        let unhandled_class_occurrences = class_occurrences
            .iter()
            .filter(|(class, _)| !class_allowed(class))
            .map(|(class, count)| (class.to_string(), *count))
            .collect();
        let class_occurrences = class_occurrences
            .into_iter()
            .map(|(class, count)| (class.to_string(), count))
            .collect();
        RawSourceCensus {
            total_occurrences: self.headers.len() as u64,
            class_occurrences,
            unhandled_class_occurrences,
            ordered_header_sha256: format!("{:x}", hasher.finalize()),
        }
    }

    /// Returns every source occurrence in document order with the parser's retention decision.
    pub fn source_objects(&self) -> Vec<RawSourceObject> {
        self.headers
            .iter()
            .map(|header| RawSourceObject {
                ordinal: header.ordinal as u64,
                class_name: header.class.to_string(),
                raw_guid: header.guid.to_string(),
                canonical_guid: canonical_guid(&header.guid),
                handled: class_allowed(&header.class),
                inventory_kind: crate::extract::tracked_kind(&header.class),
                retained: header.retained,
                duplicate: header.duplicate,
            })
            .collect()
    }
}

/// The LCM classes this crate's extractor reads; every other `<rt class="...">` is skipped unparsed.
pub(crate) const ALLOWED_CLASSES: &[&str] = &[
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
    // read only by liblcm's `EqualsMsa` port; HCLoader never loads them
    "FsNegatedValue",
    "FsDisjunctiveValue",
    "FsOpenValue",
    "FsSharedValue",
    "FsFeatStrucDisj",
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
    "MoAdhocProhibGr",
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
    // stored analyses
    "WfiWordform",
    "WfiAnalysis",
    "WfiMorphBundle",
];

fn allowed_class(class: &str) -> Option<&'static str> {
    static ALLOWED: OnceLock<HashSet<&'static str, FastBuild>> = OnceLock::new();
    ALLOWED
        .get_or_init(|| ALLOWED_CLASSES.iter().copied().collect())
        .get(class)
        .copied()
}

fn class_allowed(class: &str) -> bool {
    allowed_class(class).is_some()
}

/// Skip past a recognized `<rt>` record's body, discarding it without building a `Node`.
fn skip_rt_body<R: BufRead>(
    reader: &mut Reader<R>,
    name: QName,
    scratch: &mut Vec<u8>,
) -> Result<(), ImportError> {
    reader
        .read_to_end_into(name, scratch)
        .map_err(|e| ImportError::Xml(e.to_string()))?;
    scratch.clear();
    Ok(())
}

type RtHeader = (Option<Arc<str>>, Option<Arc<str>>);

/// The `class` and `guid` attributes of an `<rt>` header, each stopping at its first occurrence.
fn rt_header(e: &BytesStart, names: &mut NameInterner) -> Result<RtHeader, ImportError> {
    let (mut class, mut guid) = (None, None);
    for a in e.attributes() {
        let a = a.map_err(|err| ImportError::Xml(err.to_string()))?;
        let key = a.key.as_ref();
        if class.is_none() && key == b"class" {
            let v = a
                .unescape_value()
                .map_err(|err| ImportError::Xml(err.to_string()))?;
            class = Some(names.intern(v.as_bytes()));
        } else if guid.is_none() && key == b"guid" {
            let v = a
                .unescape_value()
                .map_err(|err| ImportError::Xml(err.to_string()))?;
            guid = Some(Arc::<str>::from(&*v));
        }
        if class.is_some() && guid.is_some() {
            break;
        }
    }
    Ok((class, guid))
}

/// Parse `path` into a `RawGraph`; hard errors are reserved for I/O and malformed/non-`.fwdata` XML, everything else is the extractor's job to warn on.
pub fn parse_fwdata(path: &Path) -> Result<RawGraph, ImportError> {
    let file = File::open(path).map_err(ImportError::Io)?;
    parse_fwdata_reader(BufReader::new(file))
}

/// Reusable buffers for building record bodies.
#[derive(Default)]
struct Scratch {
    children: Vec<Node>,
    frames: Vec<Frame>,
    attrs: Vec<Attr>,
    skip: Vec<u8>,
}

/// An open element inside a record: its children sit on `Scratch::children` from `child_start`.
struct Frame {
    tag: Arc<str>,
    attrs: Box<[Attr]>,
    text: String,
    child_start: usize,
}

/// As `parse_fwdata`, but from any buffered byte source, so a zip entry needs no temp file.
pub fn parse_fwdata_reader<R: BufRead>(reader: R) -> Result<RawGraph, ImportError> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut graph = RawGraph::default();
    let mut names = NameInterner::default();
    let mut scratch = Scratch::default();
    let mut saw_any_rt = false;

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| ImportError::Xml(e.to_string()))?;
        let (e, empty) = match event {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == b"rt" => (e, false),
            // A self-closed `<rt .../>` with no body is still a valid record, with an empty node.
            Event::Empty(e) if e.local_name().as_ref() == b"rt" => (e, true),
            _ => {
                buf.clear();
                continue;
            }
        };
        let (class, guid) = rt_header(&e, &mut names)?;
        let class_arc = class.unwrap_or_else(|| names.intern(b""));
        let guid_arc = guid.unwrap_or_else(|| Arc::from(""));
        saw_any_rt = true;
        let ordinal = u32::try_from(graph.headers.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| ImportError::Xml("too many record headers".into()))?;
        let allowed = allowed_class(&class_arc);
        let next_record = u32::try_from(graph.records.records.len())
            .ok()
            .filter(|n| *n != NO_RECORD)
            .ok_or_else(|| ImportError::Xml("too many retained records".into()))?;
        let (duplicate, holds_record) = if guid_arc.is_empty() {
            (false, false)
        } else {
            match graph.records.index.entry(guid_arc.clone()) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(if allowed.is_some() {
                        next_record
                    } else {
                        NO_RECORD
                    });
                    (false, false)
                }
                std::collections::hash_map::Entry::Occupied(mut slot) => {
                    let held = *slot.get() != NO_RECORD;
                    if allowed.is_some() && !held {
                        slot.insert(next_record);
                    }
                    (true, held)
                }
            }
        };
        let retained = allowed.is_some() && !guid_arc.is_empty() && !holds_record;
        graph.headers.push(RawRecordHeader {
            ordinal,
            class: class_arc.clone(),
            guid: guid_arc.clone(),
            retained,
            duplicate,
        });
        match allowed {
            Some(class_name) if guid_arc.is_empty() => {
                if !empty {
                    skip_rt_body(&mut reader, e.name(), &mut scratch.skip)?;
                }
                graph
                    .issues
                    .push(missing_guid_issue(class_name, ordinal as u64));
            }
            // A recognized record already holds this guid; keep it and just advance past this duplicate's body.
            Some(_) if holds_record => {
                if !empty {
                    skip_rt_body(&mut reader, e.name(), &mut scratch.skip)?;
                }
            }
            Some(class_name) => {
                let node = if empty {
                    Node::record_root(class_arc, "".into(), Box::new([]))
                } else {
                    parse_rt_body(&mut reader, class_name, class_arc, &mut names, &mut scratch)?
                };
                if class_name == "LexEntry" {
                    graph.lex_entry_order.push(guid_arc.clone());
                }
                graph.records.records.push(Record {
                    class: class_name,
                    guid: guid_arc,
                    node,
                });
            }
            None => {
                if !empty {
                    skip_rt_body(&mut reader, e.name(), &mut scratch.skip)?;
                }
            }
        }
        buf.clear();
    }

    if !saw_any_rt {
        return Err(ImportError::NotFwdata);
    }

    graph.headers.shrink_to_fit();
    graph.records.records.shrink_to_fit();
    graph.lex_entry_order.shrink_to_fit();

    push_duplicate_guid_issues(&mut graph);

    Ok(graph)
}

/// The fatal issue for a tracked (allowed-class) `<rt>` record with no guid; the record itself is never inserted. `ordinal` is 1-based: the record's position among every `<rt>` header in the document.
fn missing_guid_issue(class: &str, ordinal: u64) -> ConversionIssue {
    ConversionIssue {
        code: codes::MISSING_GUID,
        class: IssueClass::InvalidSource,
        source: Some(SourceRef {
            kind: FwClass::from_wire(class),
            id: format!("rt#{ordinal}"),
        }),
        fatal: true,
        message: format!("{class} record at rt#{ordinal} has no guid; dropped"),
    }
}

/// One fatal issue per guid shared by two or more headers (recognized or unknown class), naming every occurrence.
fn push_duplicate_guid_issues(graph: &mut RawGraph) {
    // The parser marks every repeat occurrence of a guid, so only marked guids can have two headers.
    let duplicated: HashSet<&str> = graph
        .headers
        .iter()
        .filter(|header| header.duplicate)
        .map(|header| &*header.guid)
        .collect();
    if duplicated.is_empty() {
        return;
    }
    let mut by_guid: HashMap<String, Vec<(u64, String)>> = HashMap::new();
    for (index, header) in graph.headers.iter().enumerate() {
        if header.guid.is_empty() || !duplicated.contains(&*header.guid) {
            continue;
        }
        by_guid
            .entry(header.guid.to_string())
            .or_default()
            .push((index as u64 + 1, header.class.to_string()));
    }

    let mut duplicates: Vec<(String, Vec<(u64, String)>)> = by_guid
        .into_iter()
        .filter(|(_, occurrences)| occurrences.len() > 1)
        .collect();
    duplicates.sort_by_key(|(_, occurrences)| occurrences[0].0);

    for (guid, occurrences) in duplicates {
        let first_class = occurrences[0].1.clone();
        let detail = occurrences
            .iter()
            .map(|(ordinal, class)| format!("rt#{ordinal} ({class})"))
            .collect::<Vec<_>>()
            .join(", ");
        graph.issues.push(ConversionIssue {
            code: codes::DUPLICATE_GUID,
            class: IssueClass::InvalidSource,
            source: Some(SourceRef {
                kind: FwClass::from_wire(&first_class),
                id: guid.clone(),
            }),
            fatal: true,
            message: format!(
                "guid {guid} appears on {} records: {detail}",
                occurrences.len()
            ),
        });
    }
}

/// Parse up to and including the matching `</rt>` into a record root `Node`, whose `children` are the record's property elements.
fn parse_rt_body<R: BufRead>(
    reader: &mut Reader<R>,
    class: &str,
    class_arc: Arc<str>,
    names: &mut NameInterner,
    scratch: &mut Scratch,
) -> Result<Node, ImportError> {
    let mut buf = Vec::new();
    let mut root_text = String::new();
    debug_assert!(scratch.children.is_empty() && scratch.frames.is_empty());
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| ImportError::Xml(e.to_string()))?;
        match event {
            Event::Start(e) => {
                let tag = names.intern(e.local_name().into_inner());
                if scratch.frames.is_empty() && pruned_field(class, &tag) {
                    check_attrs(&e)?;
                    reader
                        .read_to_end_into(e.name(), &mut scratch.skip)
                        .map_err(|err| ImportError::Xml(err.to_string()))?;
                    scratch.skip.clear();
                } else {
                    let attrs = kept_attrs(&e, names, &mut scratch.attrs)?;
                    scratch.frames.push(Frame {
                        tag,
                        attrs,
                        text: String::new(),
                        child_start: scratch.children.len(),
                    });
                }
            }
            Event::Empty(e) => {
                let tag = names.intern(e.local_name().into_inner());
                if scratch.frames.is_empty() && pruned_field(class, &tag) {
                    check_attrs(&e)?;
                } else {
                    let attrs = kept_attrs(&e, names, &mut scratch.attrs)?;
                    scratch.children.push(leaf(tag, attrs));
                }
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(|e| ImportError::Xml(e.to_string()))?;
                let slot = scratch
                    .frames
                    .last_mut()
                    .map_or(&mut root_text, |frame| &mut frame.text);
                slot.push_str(&s);
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(t.as_ref());
                let slot = scratch
                    .frames
                    .last_mut()
                    .map_or(&mut root_text, |frame| &mut frame.text);
                slot.push_str(&s);
            }
            Event::End(_) => {
                let Some(frame) = scratch.frames.pop() else {
                    // This is the closing `</rt>` for the record itself.
                    break;
                };
                let children: Box<[Node]> = scratch.children.drain(frame.child_start..).collect();
                scratch.children.push(Node {
                    tag: frame.tag,
                    attrs: frame.attrs,
                    text: frame.text.into_boxed_str(),
                    children,
                    flags: crate::node::FLAG_ATTRS_FILTERED,
                });
            }
            Event::Eof => {
                scratch.children.clear();
                scratch.frames.clear();
                return Err(ImportError::Xml(
                    "unexpected end of file inside <rt> element".to_string(),
                ));
            }
            _ => {}
        }
        buf.clear();
    }
    let children: Box<[Node]> = scratch.children.drain(..).collect();
    Ok(Node::record_root(
        class_arc,
        root_text.into_boxed_str(),
        children,
    ))
}

fn leaf(tag: Arc<str>, attrs: Box<[Attr]>) -> Node {
    Node {
        tag,
        attrs,
        text: "".into(),
        children: Box::new([]),
        flags: crate::node::FLAG_ATTRS_FILTERED,
    }
}

/// Reject a malformed attribute exactly as a full decode would, without keeping anything.
fn check_attrs(e: &BytesStart) -> Result<(), ImportError> {
    for a in e.attributes() {
        let a = a.map_err(|err| ImportError::Xml(err.to_string()))?;
        a.unescape_value()
            .map_err(|err| ImportError::Xml(err.to_string()))?;
    }
    Ok(())
}

/// Decode `e`'s attributes (every one is validated), keeping only the names in `KEPT_ATTRIBUTES`.
fn kept_attrs(
    e: &BytesStart,
    names: &mut NameInterner,
    buf: &mut Vec<Attr>,
) -> Result<Box<[Attr]>, ImportError> {
    for a in e.attributes() {
        let a = a.map_err(|err| ImportError::Xml(err.to_string()))?;
        let value = a
            .unescape_value()
            .map_err(|err| ImportError::Xml(err.to_string()))?;
        let key = a.key.local_name().into_inner();
        if KEPT_ATTRIBUTES.iter().any(|kept| kept.as_bytes() == key) {
            buf.push((names.intern(key), Box::from(&*value)));
        }
    }
    Ok(buf.drain(..).collect())
}

/// An interned name, hashed and compared as its UTF-8 bytes so a lookup needs no decoding.
struct Name(Arc<str>);

impl std::hash::Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Name {}

impl std::borrow::Borrow<[u8]> for Name {
    fn borrow(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Share repeated names while bounding the temporary table on unusual input.
#[derive(Default)]
struct NameInterner {
    names: HashSet<Name>,
    accounted_bytes: usize,
}

// Includes conservative per-entry table and Arc overhead, not just string bytes.
const NAME_INTERNER_BUDGET: usize = 1024 * 1024;

impl NameInterner {
    fn intern(&mut self, name: &[u8]) -> Arc<str> {
        if let Some(known) = self.names.get(name) {
            return Arc::clone(&known.0);
        }
        let known: Arc<str> = Arc::from(String::from_utf8_lossy(name).as_ref());
        let cost = known.len().saturating_add(96);
        if let Some(total) = self.accounted_bytes.checked_add(cost) {
            if total <= NAME_INTERNER_BUDGET {
                self.names.insert(Name(Arc::clone(&known)));
                self.accounted_bytes = total;
            }
        }
        known
    }
}

/// The table form of `pruned_field`: top-level fields of whole records whose subtrees are skipped unparsed because no importer code reads them. Nested fields are never pruned.
#[cfg(test)]
const PRUNED_FIELDS: &[(&str, &str)] = &[
    ("LexSense", "ImportResidue"),
    ("LexSense", "GrammarNote"),
    ("LexSense", "Examples"),
    ("LexSense", "Exemplar"),
    ("LexSense", "ScientificName"),
    ("LexSense", "GeneralNote"),
    ("LexEntry", "DateCreated"),
    ("LexEntry", "DateModified"),
    ("LexEntry", "DoNotUseForParsing"),
    ("LexEntry", "HomographNumber"),
    ("LexEntry", "Comment"),
    ("LexEntry", "Etymology"),
    ("LexEntry", "ImportResidue"),
    ("LexEntryRef", "HideMinorEntry"),
    ("LexEntryRef", "RefType"),
    ("WfiWordform", "Checksum"),
    ("WfiWordform", "SpellingStatus"),
];

pub(crate) fn pruned_field(class: &str, field: &str) -> bool {
    match class {
        "LexSense" => matches!(
            field,
            "ImportResidue"
                | "GrammarNote"
                | "Examples"
                | "Exemplar"
                | "ScientificName"
                | "GeneralNote"
        ),
        "LexEntry" => matches!(
            field,
            "DateCreated"
                | "DateModified"
                | "DoNotUseForParsing"
                | "HomographNumber"
                | "Comment"
                | "Etymology"
                | "ImportResidue"
        ),
        "LexEntryRef" => matches!(field, "HideMinorEntry" | "RefType"),
        "WfiWordform" => matches!(field, "Checksum" | "SpellingStatus"),
        _ => false,
    }
}

#[cfg(test)]
mod pruned_table_tests {
    #[test]
    fn match_agrees_with_the_table() {
        for (_, field) in super::PRUNED_FIELDS {
            for class in super::ALLOWED_CLASSES {
                assert_eq!(
                    super::pruned_field(class, field),
                    super::PRUNED_FIELDS.contains(&(class, field)),
                    "{class}.{field}"
                );
            }
        }
        assert!(!super::pruned_field("MoStemMsa", "Examples"));
        assert!(!super::pruned_field("LexSense", "Gloss"));
    }
}

#[cfg(test)]
mod prune_parity_tests;
#[cfg(test)]
mod tests;
