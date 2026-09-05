//! Streaming `.fwdata` reader: pulls `<rt class="..." guid="...">` records one at a time (never a DOM of the whole document) into a `RawGraph`, skipping any class this crate's extractor doesn't understand before it is ever parsed into a `Node`.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use sha2::{Digest, Sha256};

use pg_snapshot::{ConversionIssue, IssueClass, RawSourceCensus, SourceRef};

use crate::extract::codes;
use crate::node::Node;
use crate::ImportError;

/// One retained `<rt>` record; ownership comes from walking the owner's named field, not the optional `ownerguid` XML attribute.
#[derive(Debug)]
pub struct Record {
    pub class: String,
    pub guid: String,
    pub node: Node,
}

/// One `<rt class="..." guid="...">` header, recorded in document order regardless of whether its class is understood or its guid is present.
#[derive(Debug, Clone)]
pub struct RawRecordHeader {
    pub class: String,
    pub guid: String,
}

/// Every retained record, keyed by GUID; owning (`t="o"`) and by-reference (`t="r"`) links both resolve here the same way, as a GUID lookup.
#[derive(Debug, Default)]
pub struct RawGraph {
    pub records: HashMap<String, Record>,
    /// `LexEntry` guids in file-encounter order — the only deterministic order available, since `LexEntry` has no `ownerguid` and `LexDb` has no ordered entries field to fall back on.
    pub lex_entry_order: Vec<String>,
    /// Every `<rt>` header in document order, allowed and unknown classes alike.
    pub headers: Vec<RawRecordHeader>,
    /// Structural integrity issues found while parsing: duplicate or missing guids.
    pub issues: Vec<ConversionIssue>,
}

impl RawGraph {
    pub fn get(&self, guid: &str) -> Option<&Record> {
        self.records.get(guid)
    }

    /// Arbitrary (per-process-random) hashmap order; safe only for `find_lang_project`'s singleton lookup — a new ordered-output caller would break cross-run JSON determinism.
    pub fn by_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        self.records.values().filter(move |r| r.class == class)
    }

    /// Tallies every header in document order; `ordered_header_sha256` hashes the canonical `"{class}\t{guid}\n"` byte stream per header, in order, as lowercase hex SHA-256.
    pub fn census(&self) -> RawSourceCensus {
        let mut class_occurrences = BTreeMap::new();
        let mut unhandled_class_occurrences = BTreeMap::new();
        let mut hasher = Sha256::new();
        for header in &self.headers {
            *class_occurrences.entry(header.class.clone()).or_insert(0u64) += 1;
            if !class_allowed(&header.class) {
                *unhandled_class_occurrences
                    .entry(header.class.clone())
                    .or_insert(0u64) += 1;
            }
            hasher.update(format!("{}\t{}\n", header.class, header.guid).as_bytes());
        }
        RawSourceCensus {
            total_occurrences: self.headers.len() as u64,
            class_occurrences,
            unhandled_class_occurrences,
            ordered_header_sha256: format!("{:x}", hasher.finalize()),
        }
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
    parse_fwdata_reader(BufReader::new(file))
}

/// As `parse_fwdata`, but from any buffered byte source, so a zip entry needs no temp file.
pub fn parse_fwdata_reader<R: BufRead>(reader: R) -> Result<RawGraph, ImportError> {
    let mut reader = Reader::from_reader(reader);
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
                let ordinal = graph.headers.len() as u64 + 1;
                graph.headers.push(RawRecordHeader {
                    class: class.clone(),
                    guid: guid.clone(),
                });
                if class_allowed(&class) {
                    if guid.is_empty() {
                        let mut skip_buf = Vec::new();
                        reader
                            .read_to_end_into(e.name(), &mut skip_buf)
                            .map_err(|e| ImportError::Xml(e.to_string()))?;
                        graph.issues.push(missing_guid_issue(&class, ordinal));
                    } else if graph.records.contains_key(&guid) {
                        // A recognized record already holds this guid; keep it and just advance past this duplicate's body.
                        let mut skip_buf = Vec::new();
                        reader
                            .read_to_end_into(e.name(), &mut skip_buf)
                            .map_err(|e| ImportError::Xml(e.to_string()))?;
                    } else {
                        let node = parse_rt_body(&mut reader)?;
                        if class == "LexEntry" {
                            graph.lex_entry_order.push(guid.clone());
                        }
                        graph
                            .records
                            .insert(guid.clone(), Record { class, guid, node });
                    }
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
                let ordinal = graph.headers.len() as u64 + 1;
                graph.headers.push(RawRecordHeader {
                    class: class.clone(),
                    guid: guid.clone(),
                });
                if class_allowed(&class) {
                    if guid.is_empty() {
                        graph.issues.push(missing_guid_issue(&class, ordinal));
                    } else if !graph.records.contains_key(&guid) {
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
            }
            _ => {}
        }
        buf.clear();
    }

    if !saw_any_rt {
        return Err(ImportError::NotFwdata);
    }

    push_duplicate_guid_issues(&mut graph);

    Ok(graph)
}

/// The fatal issue for a tracked (allowed-class) `<rt>` record with no guid; the record itself is never inserted. `ordinal` is 1-based: the record's position among every `<rt>` header in the document.
fn missing_guid_issue(class: &str, ordinal: u64) -> ConversionIssue {
    ConversionIssue {
        code: codes::MISSING_GUID.to_string(),
        class: IssueClass::InvalidSource,
        source: Some(SourceRef {
            kind: class.to_string(),
            id: format!("rt#{ordinal}"),
        }),
        fatal: true,
        message: format!("{class} record at rt#{ordinal} has no guid; dropped"),
    }
}

/// One fatal issue per guid shared by two or more headers (recognized or unknown class), naming every occurrence.
fn push_duplicate_guid_issues(graph: &mut RawGraph) {
    let mut by_guid: HashMap<String, Vec<(u64, String)>> = HashMap::new();
    for (index, header) in graph.headers.iter().enumerate() {
        if header.guid.is_empty() {
            continue;
        }
        by_guid
            .entry(header.guid.clone())
            .or_default()
            .push((index as u64 + 1, header.class.clone()));
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
            code: codes::DUPLICATE_GUID.to_string(),
            class: IssueClass::InvalidSource,
            source: Some(SourceRef {
                kind: first_class,
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

/// Parse up to and including the matching `</rt>` into a `Node`, whose `children` are the record's property elements.
fn parse_rt_body<R: BufRead>(reader: &mut Reader<R>) -> Result<Node, ImportError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fwdata_reader_accepts_in_memory_bytes() {
        let xml = br#"<?xml version="1.0"?><languageproject><rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/></languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        assert!(graph.get("00000000-0000-0000-0000-000000000001").is_some());
    }

    #[test]
    fn census_counts_every_header_including_unknown_classes() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/>
<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000003"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let census = graph.census();
        assert_eq!(census.total_occurrences, 3);
        assert_eq!(census.class_occurrences.get("LangProject"), Some(&1));
        assert_eq!(census.class_occurrences.get("ZzUnknown"), Some(&2));
        assert_eq!(census.unhandled_class_occurrences.get("ZzUnknown"), Some(&2));
        assert!(census.unhandled_class_occurrences.get("LangProject").is_none());
        assert_eq!(census.ordered_header_sha256.len(), 64);
        assert!(census
            .ordered_header_sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn census_is_stable_across_two_parses_of_the_same_bytes() {
        let xml = br#"<?xml version="1.0"?><languageproject><rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/></languageproject>"#;
        let a = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let b = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        assert_eq!(a.census().ordered_header_sha256, b.census().ordered_header_sha256);
    }

    #[test]
    fn duplicate_guid_on_an_allowed_class_keeps_the_first_and_reports_one_issue() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexDb" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="LexDb" guid="00000000-0000-0000-0000-000000000002"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let duplicate_issues: Vec<_> = graph
            .issues
            .iter()
            .filter(|i| i.code == "invalid-source.duplicate-guid")
            .collect();
        assert_eq!(duplicate_issues.len(), 1);
        assert!(duplicate_issues[0].fatal);
        assert!(graph.get("00000000-0000-0000-0000-000000000002").is_some());
    }

    #[test]
    fn first_recognized_occurrence_wins_over_an_earlier_unknown_class_duplicate() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="LexDb" guid="00000000-0000-0000-0000-000000000002"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let duplicate_issues: Vec<_> = graph
            .issues
            .iter()
            .filter(|i| i.code == "invalid-source.duplicate-guid")
            .collect();
        assert_eq!(duplicate_issues.len(), 1);
        let source = duplicate_issues[0].source.as_ref().unwrap();
        assert_eq!(source.kind, "ZzUnknown");
        assert_eq!(source.id, "00000000-0000-0000-0000-000000000002");
        let record = graph
            .get("00000000-0000-0000-0000-000000000002")
            .expect("the recognized LexDb occurrence must be kept");
        assert_eq!(record.class, "LexDb");
    }

    #[test]
    fn duplicate_guid_across_two_different_allowed_classes_keeps_the_first() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexEntry" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="MoStemMsa" guid="00000000-0000-0000-0000-000000000002"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let duplicate_issues: Vec<_> = graph
            .issues
            .iter()
            .filter(|i| i.code == "invalid-source.duplicate-guid")
            .collect();
        assert_eq!(duplicate_issues.len(), 1);
        let source = duplicate_issues[0].source.as_ref().unwrap();
        assert_eq!(source.kind, "LexEntry");
        assert_eq!(source.id, "00000000-0000-0000-0000-000000000002");
        let record = graph
            .get("00000000-0000-0000-0000-000000000002")
            .expect("the first occurrence must be kept");
        assert_eq!(record.class, "LexEntry");
    }

    #[test]
    fn two_allowed_class_records_missing_guid_each_get_their_own_issue() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexDb"/>
<rt class="MoStemMsa"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let missing_issues: Vec<_> = graph
            .issues
            .iter()
            .filter(|i| i.code == "invalid-source.missing-guid")
            .collect();
        assert_eq!(missing_issues.len(), 2);
        let duplicate_issues = graph
            .issues
            .iter()
            .filter(|i| i.code == "invalid-source.duplicate-guid")
            .count();
        assert_eq!(duplicate_issues, 0);
        assert!(!graph.records.contains_key(""));
    }

    #[test]
    fn missing_guid_on_an_allowed_class_is_a_fatal_issue_and_is_not_inserted() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexDb"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        let missing_issues: Vec<_> = graph
            .issues
            .iter()
            .filter(|i| i.code == "invalid-source.missing-guid")
            .collect();
        assert_eq!(missing_issues.len(), 1);
        assert!(missing_issues[0].fatal);
        assert_eq!(graph.records.len(), 0);
    }

    #[test]
    fn missing_guid_on_an_unknown_class_is_census_only_with_no_issue() {
        let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="ZzUnknown"/>
</languageproject>"#;
        let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
        assert!(graph.issues.is_empty());
        assert_eq!(graph.census().unhandled_class_occurrences.get("ZzUnknown"), Some(&1));
    }
}
