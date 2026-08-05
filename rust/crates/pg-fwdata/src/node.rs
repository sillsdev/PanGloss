//! A tiny read-only DOM for a single `.fwdata` `<rt>` record, plus accessor helpers that mirror
//! the handful of shapes FieldWorks uses for scalar/multi-lingual/reference-link fields.
//!
//! We never DOM the whole 54MB `.fwdata` document (see `crate::xml`); a `Node` tree is built
//! for one `<rt>...</rt>` element at a time and dropped once its class has been extracted, or
//! (for classes outside our allowlist) never built at all — the reader skips straight past the
//! closing tag.

use pg_snapshot::WsForm;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// A generic element node: tag name, attributes, direct text, and child elements — enough to
/// represent the handful of `.fwdata` field shapes we care about (`AUni`/`AStr` multi-lingual
/// strings, `Uni` plain strings, `Str`/`Run` rich strings, `objsur` reference/ownership links,
/// and `val="..."` scalar attributes).
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

    /// Tolerant boolean parse of a `val="..."` attribute: FieldWorks data in the wild has been
    /// observed to use both `"True"`/`"False"` (typical .NET `ToString()`) and lowercase
    /// `"true"` (e.g. `MoInflAffixTemplate.Final` in Sena 3), so we accept both cases plus
    /// `"1"`/`"0"`.
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

    /// `<Tag><objsur guid="..."/> <objsur guid="..."/> ...</Tag>` — the ordered-list shape
    /// (used for both `seq` and `col` LCM cardinalities; see the crate-level docs for why
    /// preserving encounter order is sufficient for either).
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

    /// `<Tag><AUni ws="en">text</AUni><AStr ws="pt"><Run ws="pt">text</Run></AStr>...</Tag>` —
    /// a `MultiUnicode`/`MultiString` field, one `WsForm` per writing system, in document
    /// order.
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

    /// `<Tag><Uni>plain text</Uni></Tag>` — a single plain-text field with no writing-system
    /// tagging (e.g. `CurVernWss`, the raw `ParserParameters` XML blob).
    pub fn uni_text(&self, tag: &str) -> Option<String> {
        Some(self.child(tag)?.child("Uni")?.text.clone())
    }

    /// `<Tag><Str><Run ws="en">text</Run>...</Str></Tag>` — a single rich-text field collapsed
    /// to plain text (used where the format only wants one string regardless of writing system,
    /// e.g. `PhEnvironment.StringRepresentation`).
    pub fn str_text(&self, tag: &str) -> Option<String> {
        Some(concat_runs(self.child(tag)?.child("Str")?))
    }

    /// Tolerant boolean parse of a child element's own *text content* (as opposed to `val_bool`
    /// which reads a `val="..."` attribute) — the shape used by the nested `<ParserParameters>`
    /// XML blob (e.g. `<NotOnClitics>false</NotOnClitics>`).
    ///
    /// `val_bool`: Node::val_bool
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

/// Parse a small, complete XML document (e.g. the decoded `<ParserParameters>` blob, a few
/// hundred bytes) into a synthetic root `Node` whose children are the document's top-level
/// elements. Unlike `crate::xml::parse_fwdata` this *does* build a full DOM — safe here because
/// the input is always small (never the 54MB `.fwdata` file itself).
pub fn parse_full_document(xml: &str) -> Option<Node> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Node> = vec![Node::empty()];
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => {
                let tag = String::from_utf8_lossy(e.local_name().into_inner()).into_owned();
                stack.push(Node {
                    tag,
                    attrs: e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .map(|a| {
                            (
                                String::from_utf8_lossy(a.key.local_name().into_inner())
                                    .into_owned(),
                                a.unescape_value().unwrap_or_default().into_owned(),
                            )
                        })
                        .collect(),
                    text: String::new(),
                    children: Vec::new(),
                });
            }
            Event::Empty(e) => {
                let tag = String::from_utf8_lossy(e.local_name().into_inner()).into_owned();
                let node = Node {
                    tag,
                    attrs: e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .map(|a| {
                            (
                                String::from_utf8_lossy(a.key.local_name().into_inner())
                                    .into_owned(),
                                a.unescape_value().unwrap_or_default().into_owned(),
                            )
                        })
                        .collect(),
                    text: String::new(),
                    children: Vec::new(),
                };
                stack.last_mut()?.children.push(node);
            }
            Event::Text(t) => {
                let s = t.unescape().ok()?;
                stack.last_mut()?.text.push_str(&s);
            }
            Event::End(_) => {
                let node = stack.pop()?;
                stack.last_mut()?.children.push(node);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    stack.pop()
}

/// Strip the FieldWorks placeholder dotted-circle (U+25CC), used to mark a diacritic-only
/// grapheme's "base" position, from a phoneme/boundary-marker representation.
/// ← `HCLoader.RemoveDottedCircles`, HCLoader.cs:2678-2680.
pub fn strip_dotted_circles(s: &str) -> String {
    s.chars().filter(|&c| c != '\u{25CC}').collect()
}
