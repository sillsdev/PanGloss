//! Parser for HermitCrab pattern text (`[V] (/k/) #`), the query notation of `pangloss serve` op `match`.
//!
//! The text is untrusted input: every failure is a typed [`PatternError`] naming the token, its
//! byte offset and what would have been accepted. Offsets are UTF-8 byte offsets into the text.
//!
//! Supported grammar (whitespace between items is ignored):
//!
//! ```text
//! pattern := items | items? '/' items? '_' items?      the second form is a rule environment
//! items   := item*
//! item    := '#' | '+' | class | literal | group
//! class   := '[' natural-class-name ']'
//! literal := '/' segment-text '/'          segmented against the surface table
//! group   := '(' item+ ')' [ '*' ]         optional, or optional-and-repeated
//! ```
//!
//! `#` is a word edge. In a plain pattern it may open or close the pattern; in an environment it may
//! open the left side or close the right side, and the target takes none. `+` is a morpheme boundary
//! and needs the caller to name a boundary character definition. In an environment the `/` that
//! separates the target from its sides is followed by whitespace or `_`, which tells it from the
//! `/` that opens a literal; an empty target is allowed (an insertion point), and so is an empty side.
//! Feature bundles (`[+voice -son]`) and alpha variables (`[αback]`) parse far enough to be named,
//! and then come back as [`PatternError::Unsupported`] until conformance fixtures exist.

use pg_grammar_model::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar_model::model::{
    AnchorSide, Grammar, NatClassId, Pattern, PatternNode, SimpleContext,
};
use pg_grammar_model::segment::segment_phonemes_only;

/// What the text can contain, for `pattern_parse` errors and `--describe`.
pub const EXPECTED_ITEM_KINDS: [&str; 5] = ["'#'", "'+'", "'['", "'/'", "'('"];

/// Why pattern text was refused. Every variant carries a byte `offset` into the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternError {
    /// Syntax error at `offset`; `token` is the text found there (or `end of pattern`).
    Parse {
        token: String,
        offset: usize,
        expected: Vec<String>,
    },
    /// A `[name]` that is no natural class of the grammar; `near` lists likely intended names.
    UnknownNaturalClass {
        name: String,
        offset: usize,
        near: Vec<String>,
    },
    /// A feature in a bundle that the grammar does not define; `near` lists likely intended names.
    UnknownFeature {
        name: String,
        offset: usize,
        near: Vec<String>,
    },
    /// A construct that parses but is not served yet.
    Unsupported {
        construct: &'static str,
        offset: usize,
    },
    /// The pattern contains `+` and the caller marks no boundaries in its words.
    BoundaryNeedsMarkedInput { offset: usize },
}

/// Pattern text parsed: the target pattern and, for `target / left _ right`, the two sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPattern {
    /// The whole pattern for plain text; the part before `/` for an environment (possibly empty).
    pub target: Pattern,
    /// `Some` when the text has the `target / left _ right` form.
    pub environment: Option<EnvironmentPattern>,
}

/// The two sides of `target / left _ right`; a side with no items is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentPattern {
    pub left: Option<Pattern>,
    pub right: Option<Pattern>,
}

/// Which part of the text `Parser::items` is reading.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    /// The whole of a plain pattern, or the target before it is known to be an environment's.
    Top,
    Group,
    Left,
    Right,
}

struct Parser<'a> {
    g: &'a Grammar,
    table: &'a CharDefTable,
    text: &'a str,
    pos: usize,
    boundary: Option<CharDefId>,
    /// Offset of the first `#` the parser accepted in the top-level section, so an environment can refuse it in the target.
    anchor_at: Option<usize>,
}

/// Parses `text` against the grammar and the character table the words will be segmented with. `boundary` is the character definition `+` stands for, when the caller marks boundaries in its words.
pub fn parse_pattern(
    g: &Grammar,
    table: &CharDefTable,
    text: &str,
    boundary: Option<CharDefId>,
) -> Result<ParsedPattern, PatternError> {
    let mut p = Parser {
        g,
        table,
        text,
        pos: 0,
        boundary,
        anchor_at: None,
    };
    let mut target = Vec::new();
    p.items(&mut target, Section::Top)?;
    match p.peek() {
        None => {
            if target.is_empty() {
                return Err(p.unexpected(&EXPECTED_ITEM_KINDS));
            }
            Ok(ParsedPattern {
                target: Pattern { nodes: target },
                environment: None,
            })
        }
        Some('/') => {
            if let Some(anchor) = p
                .anchor_at
                .filter(|_| target.iter().any(|n| matches!(n, PatternNode::Anchor(_))))
            {
                p.pos = anchor;
                return Err(p.unexpected(&["'['", "'/'", "'('", "'+'"]));
            }
            p.pos += 1; // '/'
            let mut left = Vec::new();
            p.items(&mut left, Section::Left)?;
            if p.peek() != Some('_') {
                return Err(p.unexpected(&["'_'"]));
            }
            p.pos += 1; // '_'
            let mut right = Vec::new();
            p.items(&mut right, Section::Right)?;
            if p.peek().is_some() {
                return Err(p.unexpected(&["end of pattern"]));
            }
            let side = |nodes: Vec<PatternNode>| (!nodes.is_empty()).then_some(Pattern { nodes });
            Ok(ParsedPattern {
                target: Pattern { nodes: target },
                environment: Some(EnvironmentPattern {
                    left: side(left),
                    right: side(right),
                }),
            })
        }
        Some('_') => Err(p.unexpected(&["end of pattern", "'/'"])),
        Some(_) => Err(p.unexpected(&["end of pattern"])),
    }
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += self.peek().map_or(0, char::len_utf8);
        }
    }

    fn unexpected(&self, expected: &[&str]) -> PatternError {
        let token = self
            .peek()
            .map_or_else(|| "end of pattern".to_string(), |c| c.to_string());
        PatternError::Parse {
            token,
            offset: self.pos,
            expected: expected.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    /// Whether the `/` at the cursor separates an environment's target from its sides: it is followed by whitespace, `_` or the end, where a literal's `/` is followed by segment text.
    fn at_environment_slash(&self) -> bool {
        self.text[self.pos + 1..]
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '_')
    }

    /// Reads items until the end of the section: `)` closes a group, `_` ends a left side, and an environment's `/` ends the target. `#` is allowed where `section` says.
    fn items(&mut self, out: &mut Vec<PatternNode>, section: Section) -> Result<(), PatternError> {
        let mut seen_item = false;
        loop {
            self.skip_space();
            let Some(c) = self.peek() else { return Ok(()) };
            match c {
                ')' | '_' => return Ok(()),
                '/' if self.at_environment_slash() => return Ok(()),
                '#' => {
                    let at = self.pos;
                    let opens = matches!(section, Section::Top | Section::Left) && !seen_item;
                    let closes =
                        section == Section::Right || (section == Section::Top && seen_item);
                    if !opens && !closes {
                        return Err(self.unexpected(&["'['", "'/'", "'('", "'+'", "')'"]));
                    }
                    self.pos += 1;
                    if opens {
                        if section == Section::Top {
                            self.anchor_at.get_or_insert(at);
                        }
                        out.push(PatternNode::Anchor(AnchorSide::Left));
                        seen_item = true;
                        continue;
                    }
                    self.skip_space();
                    if self.peek().is_some() {
                        self.pos = at;
                        return Err(self.unexpected(&["'['", "'/'", "'('", "'+'"]));
                    }
                    out.push(PatternNode::Anchor(AnchorSide::Right));
                    return Ok(());
                }
                '+' => {
                    let at = self.pos;
                    self.pos += 1;
                    match self.boundary {
                        Some(cd) => out.push(PatternNode::CharDef(cd)),
                        None => return Err(PatternError::BoundaryNeedsMarkedInput { offset: at }),
                    }
                }
                '[' => out.push(self.class()?),
                '/' => self.literal(out)?,
                '(' => out.push(self.group()?),
                _ => return Err(self.unexpected(&EXPECTED_ITEM_KINDS)),
            }
            seen_item = true;
        }
    }

    fn group(&mut self) -> Result<PatternNode, PatternError> {
        self.pos += 1; // '('
        let mut children = Vec::new();
        self.items(&mut children, Section::Group)?;
        if self.peek() != Some(')') {
            return Err(self.unexpected(&["')'"]));
        }
        if children.is_empty() {
            return Err(self.unexpected(&["'['", "'/'", "'+'", "'('"]));
        }
        self.pos += 1; // ')'
        let max = if self.peek() == Some('*') {
            self.pos += 1;
            None
        } else {
            Some(1)
        };
        Ok(PatternNode::Quantifier {
            min: 0,
            max,
            children,
        })
    }

    fn literal(&mut self, out: &mut Vec<PatternNode>) -> Result<(), PatternError> {
        let open = self.pos;
        self.pos += 1; // '/'
        let body_start = self.pos;
        let Some(len) = self.text[body_start..].find('/') else {
            self.pos = self.text.len();
            return Err(self.unexpected(&["'/'"]));
        };
        let body = &self.text[body_start..body_start + len];
        self.pos = body_start + len + 1;
        let bad = |offset: usize| PatternError::Parse {
            token: self.text[open..self.pos].to_string(),
            offset,
            expected: vec!["segment of the surface character table".to_string()],
        };
        if body.is_empty() {
            return Err(bad(open));
        }
        let shape = segment_phonemes_only(self.table, body).map_err(|_| bad(open))?;
        for (_, _, cd, _) in shape.interior() {
            out.push(PatternNode::CharDef(CharDefId(cd)));
        }
        Ok(())
    }

    fn class(&mut self) -> Result<PatternNode, PatternError> {
        let open = self.pos;
        self.pos += 1; // '['
        let Some(len) = self.text[self.pos..].find(']') else {
            self.pos = self.text.len();
            return Err(self.unexpected(&["']'"]));
        };
        let body_at = self.pos;
        let body = &self.text[body_at..body_at + len];
        self.pos = body_at + len + 1;
        let trimmed = body.trim();
        if trimmed.is_empty() {
            self.pos = body_at;
            return Err(self.unexpected(&["natural class name", "feature bundle"]));
        }
        if is_bundle(trimmed) {
            return Err(self.bundle(body, body_at, open));
        }
        let name_at = body_at + (body.len() - body.trim_start().len());
        match find_class(self.g, trimmed) {
            Some(id) => Ok(PatternNode::Context(SimpleContext {
                nat_class: id,
                vars: Vec::new(),
            })),
            None => Err(PatternError::UnknownNaturalClass {
                name: trimmed.to_string(),
                offset: name_at,
                near: near(class_names(self.g), trimmed),
            }),
        }
    }

    /// Names a feature bundle's first problem: an unknown feature, else the gate.
    fn bundle(&self, body: &str, body_at: usize, open: usize) -> PatternError {
        let mut alpha = false;
        let mut idx = 0usize;
        for word in body.split_whitespace() {
            let at = body_at + body[idx..].find(word).map_or(idx, |i| idx + i);
            idx = at - body_at + word.len();
            let name = word.trim_start_matches(['+', '-']);
            let is_alpha = name.chars().next().is_some_and(is_greek);
            if is_alpha {
                alpha = true;
            }
            let feature = if is_alpha {
                name.trim_start_matches(is_greek)
            } else {
                name
            };
            if !feature.is_empty() && !feature_names(self.g).iter().any(|f| f == feature) {
                return PatternError::UnknownFeature {
                    name: feature.to_string(),
                    offset: at + (word.len() - feature.len()),
                    near: near(feature_names(self.g), feature),
                };
            }
        }
        PatternError::Unsupported {
            construct: if alpha {
                "alphaVariable"
            } else {
                "featureBundle"
            },
            offset: open,
        }
    }
}

fn is_greek(c: char) -> bool {
    ('\u{3b1}'..='\u{3c9}').contains(&c) || ('\u{391}'..='\u{3a9}').contains(&c)
}

/// A `[...]` whose content starts with a sign or a Greek variable is a bundle, not a class name.
fn is_bundle(trimmed: &str) -> bool {
    trimmed
        .chars()
        .next()
        .is_some_and(|c| c == '+' || c == '-' || is_greek(c))
}

fn class_names(g: &Grammar) -> Vec<String> {
    g.natural_classes
        .iter()
        .map(|c| c.name.clone().unwrap_or_else(|| c.xml_id.clone()))
        .collect()
}

fn feature_names(g: &Grammar) -> Vec<String> {
    g.phon_features
        .iter()
        .map(|(_, name, _)| name.to_string())
        .collect()
}

/// The natural class a `[token]` names: the first class whose name is `name`, else the first whose XML id is `name`. A name outranks an id when both exist, and the first of two equal names wins.
fn find_class(g: &Grammar, name: &str) -> Option<NatClassId> {
    let by = |f: &dyn Fn(&pg_grammar_model::model::NaturalClass) -> bool| {
        g.natural_classes.iter().position(f)
    };
    by(&|c| c.name.as_deref() == Some(name))
        .or_else(|| by(&|c| c.xml_id == name))
        .map(|i| NatClassId(i as u32))
}

/// The text that selects class `id` as `[token]` in pattern text: its name, or its XML id when the name is empty, repeated by an earlier class, or not writable inside brackets; `None` when no spelling reaches it. The token round-trips: parsing `[token]` gives back `id`.
pub fn class_token(g: &Grammar, id: NatClassId) -> Option<String> {
    let class = g.natural_classes.get(id.0 as usize)?;
    let reaches = |text: &str| {
        let t = text.trim();
        t == text
            && !t.is_empty()
            && !t.contains(']')
            && !is_bundle(t)
            && find_class(g, t) == Some(id)
    };
    class
        .name
        .iter()
        .chain(std::iter::once(&class.xml_id))
        .find(|text| reaches(text))
        .cloned()
}

/// Up to five names equal to `wanted` ignoring case, or containing it or contained by it, sorted.
fn near(names: Vec<String>, wanted: &str) -> Vec<String> {
    let w = wanted.to_lowercase();
    let mut found: Vec<String> = names
        .into_iter()
        .filter(|n| {
            let l = n.to_lowercase();
            l == w || (w.len() >= 3 && l.contains(&w)) || (l.len() >= 3 && w.contains(&l))
        })
        .collect();
    found.sort();
    found.dedup();
    found.truncate(5);
    found
}

/// The boundary character definition `marker` names in `table`, if it is one.
pub fn boundary_char_def(table: &CharDefTable, marker: &str) -> Option<CharDefId> {
    let id = table.lookup_nfd(&pg_grammar_model::nfd::nfd(marker))?;
    (table.get(id).kind() == CharDefKind::Boundary).then_some(id)
}
