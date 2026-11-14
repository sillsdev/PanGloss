//! Compact read-only trees for retained `.fwdata` records, built by `crate::xml` for extraction.

use std::sync::Arc;

use pg_snapshot::WsForm;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentError(String);

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DocumentError {}

/// Set on a record's root node, whose `tag` is the record class.
pub(crate) const FLAG_RECORD_ROOT: u8 = 1;
/// Set on every node built by the record parser, whose attributes are limited to `KEPT_ATTRIBUTES`.
pub(crate) const FLAG_ATTRS_FILTERED: u8 = 2;

/// Attribute names the importer ever reads from a record's elements (`val`, `guid` and `ws`); every other attribute is validated and dropped at parse time.
pub(crate) const KEPT_ATTRIBUTES: &[&str] = &["val", "guid", "ws"];

pub(crate) type Attr = (Arc<str>, Box<str>);

/// A generic element node: tag name, attributes, direct text, and child elements, enough to represent the handful of `.fwdata` field shapes this crate cares about. Every collection is an exactly sized boxed slice.
#[derive(Debug, Clone)]
pub struct Node {
    pub tag: Arc<str>,
    pub attrs: Box<[Attr]>,
    pub text: Box<str>,
    pub children: Box<[Node]>,
    pub(crate) flags: u8,
}

impl Default for Node {
    fn default() -> Self {
        Node::empty()
    }
}

/// An element under construction.
#[derive(Default)]
pub(crate) struct NodeBuilder {
    pub tag: Option<Arc<str>>,
    pub attrs: Vec<Attr>,
    pub text: String,
    pub children: Vec<Node>,
}

impl NodeBuilder {
    pub(crate) fn finish(self) -> Node {
        Node {
            tag: self.tag.unwrap_or_else(|| Arc::from("")),
            attrs: self.attrs.into_boxed_slice(),
            text: self.text.into_boxed_str(),
            children: self.children.into_boxed_slice(),
            flags: 0,
        }
    }
}

impl Node {
    pub fn empty() -> Self {
        Node {
            tag: Arc::from(""),
            attrs: Box::new([]),
            text: "".into(),
            children: Box::new([]),
            flags: 0,
        }
    }

    /// The root of one `<rt>` record of `class`.
    pub(crate) fn record_root(class: Arc<str>, text: Box<str>, children: Box<[Node]>) -> Self {
        Node {
            tag: class,
            attrs: Box::new([]),
            text,
            children,
            flags: FLAG_RECORD_ROOT | FLAG_ATTRS_FILTERED,
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        assert!(
            self.flags & FLAG_ATTRS_FILTERED == 0 || KEPT_ATTRIBUTES.contains(&name),
            "attribute `{name}` is dropped at parse time because no importer code reads it; add it to `node::KEPT_ATTRIBUTES` before reading it"
        );
        self.attrs
            .iter()
            .find(|(k, _)| &**k == name)
            .map(|(_, v)| &**v)
    }

    pub fn child<'a>(&'a self, tag: &'a str) -> Option<&'a Node> {
        self.children_named(tag).next()
    }

    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        assert!(
            self.flags & FLAG_RECORD_ROOT == 0 || !crate::xml::pruned_field(&self.tag, tag),
            "field `{tag}` of `{}` is skipped at parse time because no importer code read it; remove it from `xml::pruned_field` before reading it",
            self.tag
        );
        self.children.iter().filter(move |c| &*c.tag == tag)
    }

    /// Tolerant boolean parse of a `val="..."` attribute: FieldWorks data mixes `"True"`/`"False"` and lowercase `"true"`, so both cases plus `"1"`/`"0"` are accepted.
    pub fn val_bool(&self, tag: &str) -> Option<bool> {
        let raw = self.child(tag)?.attr("val")?;
        match raw {
            "1" => Some(true),
            "0" => Some(false),
            _ if raw.eq_ignore_ascii_case("true") => Some(true),
            _ if raw.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }

    pub fn val_int(&self, tag: &str) -> Option<i64> {
        self.child(tag)?.attr("val")?.parse().ok()
    }

    /// `<Tag><objsur guid="..." t="o|r"/></Tag>` — the single-value (atomic) shape.
    pub fn objsur_one(&self, tag: &str) -> Option<String> {
        self.child(tag)?
            .children_named("objsur")
            .next()
            .and_then(|o| o.attr("guid"))
            .map(str::to_string)
    }

    /// `<Tag><objsur guid="..."/> <objsur guid="..."/> ...</Tag>` — the ordered-list shape, used for both `seq` and `col` LCM cardinalities.
    pub fn objsur_list(&self, tag: &str) -> Vec<String> {
        match self.child(tag) {
            Some(c) => c
                .children_named("objsur")
                .filter_map(|o| o.attr("guid"))
                .map(str::to_string)
                .collect(),
            None => Vec::new(),
        }
    }

    /// `<Tag><AUni ws="en">text</AUni><AStr ws="pt"><Run ws="pt">text</Run></AStr>...</Tag>` — a `MultiUnicode`/`MultiString` field, one `WsForm` per writing system, in document order.
    pub fn ws_forms(&self, tag: &str) -> Vec<WsForm> {
        let Some(c) = self.child(tag) else {
            return Vec::new();
        };
        c.children
            .iter()
            .filter_map(|child| match &*child.tag {
                "AUni" => Some(WsForm {
                    ws: child.attr("ws")?.to_string(),
                    form: child.text.to_string(),
                }),
                "AStr" => Some(WsForm {
                    ws: child.attr("ws")?.to_string(),
                    form: concat_runs(child),
                }),
                _ => None,
            })
            .collect()
    }

    /// `<Tag><Uni>plain text</Uni></Tag>` — a single plain-text field with no writing-system tagging.
    pub fn uni_text(&self, tag: &str) -> Option<String> {
        Some(self.child(tag)?.child("Uni")?.text.to_string())
    }

    /// `<Tag><Str><Run ws="en">text</Run>...</Str></Tag>` — a single rich-text field collapsed to plain text.
    pub fn str_text(&self, tag: &str) -> Option<String> {
        Some(concat_runs(self.child(tag)?.child("Str")?))
    }

    /// Parses this node's text as a FieldWorks boolean value (`true`/`false`, case-insensitive, or `1`/`0`).
    pub fn bool_text(&self) -> Option<bool> {
        let raw = self.text.trim();
        match raw {
            "1" => Some(true),
            "0" => Some(false),
            _ if raw.eq_ignore_ascii_case("true") => Some(true),
            _ if raw.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }
}

/// Concatenate every direct `<Run>` child's text (an `AStr`/`Str` element's rich-text content).
fn concat_runs(rich_text_elem: &Node) -> String {
    let mut s = String::new();
    for run in rich_text_elem.children_named("Run") {
        s.push_str(&run.text);
    }
    s
}

fn document_error(message: impl Into<String>) -> DocumentError {
    DocumentError(message.into())
}

fn builder_from_start(e: &quick_xml::events::BytesStart<'_>) -> Result<NodeBuilder, DocumentError> {
    let tag = Arc::from(String::from_utf8_lossy(e.local_name().as_ref()).as_ref());
    let mut attrs = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|error| document_error(error.to_string()))?;
        let key = Arc::from(String::from_utf8_lossy(attr.key.local_name().as_ref()).as_ref());
        let value = attr
            .unescape_value()
            .map_err(|error| document_error(error.to_string()))?
            .into_owned()
            .into_boxed_str();
        attrs.push((key, value));
    }
    Ok(NodeBuilder {
        tag: Some(tag),
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}

/// Strict full-DOM parse of a small XML document: one balanced element root, nothing else outside it.
pub fn parse_full_document(xml: &str) -> Result<Node, DocumentError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_comments = true;
    let mut stack: Vec<NodeBuilder> = vec![NodeBuilder::default()];
    loop {
        let event = reader
            .read_event()
            .map_err(|error| document_error(error.to_string()))?;
        match event {
            Event::Start(e) => {
                if stack.len() == 1 && stack[0].children.len() == 1 {
                    return Err(document_error("multiple root elements"));
                }
                stack.push(builder_from_start(&e)?);
            }
            Event::Empty(e) => {
                if stack.len() == 1 && stack[0].children.len() == 1 {
                    return Err(document_error("multiple root elements"));
                }
                let node = builder_from_start(&e)?.finish();
                stack
                    .last_mut()
                    .ok_or_else(|| document_error("parser stack is empty"))?
                    .children
                    .push(node);
            }
            Event::Text(t) => {
                let s = t
                    .unescape()
                    .map_err(|error| document_error(error.to_string()))?;
                if stack.len() == 1 && !s.trim().is_empty() {
                    return Err(document_error("non-whitespace text outside root"));
                }
                stack
                    .last_mut()
                    .ok_or_else(|| document_error("parser stack is empty"))?
                    .text
                    .push_str(&s);
            }
            Event::CData(data) => {
                let s = std::str::from_utf8(data.as_ref())
                    .map_err(|error| document_error(error.to_string()))?;
                if stack.len() == 1 && !s.trim().is_empty() {
                    return Err(document_error("non-whitespace CDATA outside root"));
                }
                stack
                    .last_mut()
                    .ok_or_else(|| document_error("parser stack is empty"))?
                    .text
                    .push_str(s);
            }
            Event::End(end) => {
                if stack.len() == 1 {
                    return Err(document_error("unexpected closing tag"));
                }
                let node = stack
                    .pop()
                    .ok_or_else(|| document_error("parser stack is empty"))?
                    .finish();
                if node.tag.as_bytes() != end.local_name().as_ref() {
                    return Err(document_error(format!(
                        "closing tag {:?} does not match {:?}",
                        end.local_name().as_ref(),
                        node.tag
                    )));
                }
                stack
                    .last_mut()
                    .ok_or_else(|| document_error("parser stack is empty"))?
                    .children
                    .push(node);
            }
            Event::Eof => {
                if stack.len() != 1 {
                    return Err(document_error("unclosed element"));
                }
                if stack[0].children.len() != 1 {
                    return Err(document_error(
                        "document must have exactly one root element",
                    ));
                }
                return stack
                    .pop()
                    .map(NodeBuilder::finish)
                    .ok_or_else(|| document_error("parser stack is empty"));
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {}
            Event::DocType(_) => return Err(document_error("DOCTYPE is not supported")),
        }
    }
}

/// Strips the FieldWorks placeholder dotted-circle (U+25CC), used to mark a diacritic-only grapheme's "base" position.
pub fn strip_dotted_circles(s: &str) -> String {
    s.chars().filter(|&c| c != '\u{25CC}').collect()
}

#[cfg(test)]
mod tests;
