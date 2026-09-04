//! A tiny read-only DOM for a single `.fwdata` `<rt>` record; we never DOM the whole 54MB document (see `crate::xml`), building a `Node` tree for one `<rt>` element at a time and dropping it once extracted.

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

/// A generic element node: tag name, attributes, direct text, and child elements, enough to represent the handful of `.fwdata` field shapes this crate cares about.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn empty() -> Self {
        Node::default()
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, tag: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.tag == tag)
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
            .filter_map(|child| match child.tag.as_str() {
                "AUni" => Some(WsForm {
                    ws: child.attr("ws")?.to_string(),
                    form: child.text.clone(),
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
        Some(self.child(tag)?.child("Uni")?.text.clone())
    }

    /// `<Tag><Str><Run ws="en">text</Run>...</Str></Tag>` — a single rich-text field collapsed to plain text.
    pub fn str_text(&self, tag: &str) -> Option<String> {
        Some(concat_runs(self.child(tag)?.child("Str")?))
    }

    /// Tolerant boolean parse of a child element's own text content, as opposed to `val_bool`'s `val="..."` attribute.
    pub fn child_bool_text(&self, tag: &str) -> Option<bool> {
        let raw = self.child(tag)?.text.trim();
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

fn node_from_start(e: &quick_xml::events::BytesStart<'_>) -> Result<Node, DocumentError> {
    let tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
    let mut attrs = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|error| document_error(error.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
        let value = attr
            .unescape_value()
            .map_err(|error| document_error(error.to_string()))?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(Node {
        tag,
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}

/// Parses a small, complete XML document into a synthetic root `Node`; unlike
/// `crate::xml::parse_fwdata` this builds a full DOM, safe since the input is always small. The
/// result is strict: exactly one element root is required, all tags must balance, and only
/// whitespace/comments/declarations may occur outside it.
pub fn parse_full_document(xml: &str) -> Result<Node, DocumentError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_comments = true;
    let mut stack: Vec<Node> = vec![Node::empty()];
    loop {
        let event = reader
            .read_event()
            .map_err(|error| document_error(error.to_string()))?;
        match event {
            Event::Start(e) => {
                if stack.len() == 1 && stack[0].children.len() == 1 {
                    return Err(document_error("multiple root elements"));
                }
                stack.push(node_from_start(&e)?);
            }
            Event::Empty(e) => {
                if stack.len() == 1 && stack[0].children.len() == 1 {
                    return Err(document_error("multiple root elements"));
                }
                let node = node_from_start(&e)?;
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
                    .ok_or_else(|| document_error("parser stack is empty"))?;
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
                    return Err(document_error("document must have exactly one root element"));
                }
                return Ok(stack
                    .pop()
                    .ok_or_else(|| document_error("parser stack is empty"))?);
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {}
            Event::DocType(_) => return Err(document_error("DOCTYPE is not supported")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace_documents_are_rejected() {
        assert!(parse_full_document("").is_err());
        assert!(parse_full_document(" \n\t").is_err());
    }

    #[test]
    fn second_root_and_trailing_junk_are_rejected() {
        assert!(parse_full_document("<a/><b/>").is_err());
        assert!(parse_full_document("<a/>junk").is_err());
    }

    #[test]
    fn malformed_attributes_are_rejected_but_valid_attributes_are_kept() {
        assert!(parse_full_document(r#"<a bad="x></a>"#).is_err());
        let root = parse_full_document(r#"<a answer="1" note="a&amp;b"/>"#).unwrap();
        let node = &root.children[0];
        assert_eq!(node.attr("answer"), Some("1"));
        assert_eq!(node.attr("note"), Some("a&b"));
    }

    #[test]
    fn text_and_cdata_are_preserved() {
        let root = parse_full_document("<a>text<![CDATA[<raw>]]>tail</a>").unwrap();
        assert_eq!(root.children[0].text, "text<raw>tail");
    }

    #[test]
    fn comments_do_not_change_document_contents() {
        let root =
            parse_full_document("<!-- before --><a><!-- inside -->ok</a><!-- after -->").unwrap();
        assert_eq!(root.children[0].text, "ok");
    }

    #[test]
    fn malformed_comments_are_rejected() {
        assert!(parse_full_document("<a><!-- invalid -- comment --></a>").is_err());
    }
}

/// Strips the FieldWorks placeholder dotted-circle (U+25CC), used to mark a diacritic-only grapheme's "base" position.
pub fn strip_dotted_circles(s: &str) -> String {
    s.chars().filter(|&c| c != '\u{25CC}').collect()
}
