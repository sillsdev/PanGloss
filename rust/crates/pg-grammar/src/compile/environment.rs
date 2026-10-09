//! Selected environment patterns, literal substrate inputs, and conversion issues for active restrictions.

use pg_snapshot::{InventoryKey, InventoryKind, IssueClass, SourceRef};

use crate::chardef::CharDefId;
use crate::model::{
    AnchorSide, EnvironmentDef, EnvironmentSource, NatClassId, Pattern, PatternNode, SimpleContext,
    TableId,
};

use super::{issue_codes, roles, Ctx};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentResolutionStatus {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentClassToken {
    pub side: String,
    pub token_path: String,
    pub token_text: String,
    pub source_start: usize,
    pub source_end: usize,
    pub natural_class_guid: Option<String>,
    pub natural_class_index: Option<u32>,
}

/// One element of a resolved environment side, in the order the compiler matches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentSideElement {
    /// A `#` the compiler kept as an anchor; a `#` away from the side's outer edge is dropped.
    WordBoundary,
    /// A literal segment, identified by its character definition in the environment's table.
    Segment(CharDefId),
    /// A `[class]` reference, by the natural class the compiler resolved it to.
    NaturalClass(NatClassId),
    /// A `( ... )` group, matched between `min` and `max` times.
    Optional {
        min: u32,
        max: Option<u32>,
        children: Vec<EnvironmentSideElement>,
    },
}

/// The compiler's resolved elements for one side of an environment; empty when the side is absent.
pub fn environment_side_elements(pattern: Option<&Pattern>) -> Vec<EnvironmentSideElement> {
    pattern.map_or_else(Vec::new, |pattern| side_elements(&pattern.nodes))
}

fn side_elements(nodes: &[PatternNode]) -> Vec<EnvironmentSideElement> {
    nodes.iter().flat_map(side_element).collect()
}

fn side_element(node: &PatternNode) -> Vec<EnvironmentSideElement> {
    match node {
        PatternNode::Anchor(_) => vec![EnvironmentSideElement::WordBoundary],
        PatternNode::Context(context) => {
            vec![EnvironmentSideElement::NaturalClass(context.nat_class)]
        }
        PatternNode::CharDef(id) => vec![EnvironmentSideElement::Segment(*id)],
        PatternNode::Quantifier { min, max, children } => vec![EnvironmentSideElement::Optional {
            min: *min,
            max: *max,
            children: side_elements(children),
        }],
        // Literal environment text is segmented by `segment_phonemes_only`, so each node has a char def.
        PatternNode::Segments { shape, .. } => shape
            .shape
            .interior()
            .map(|(_, _, char_def, _)| EnvironmentSideElement::Segment(CharDefId(char_def)))
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct EnvironmentResolution {
    pub environment_guid: String,
    /// The character-definition table the side's segments and class members are read from.
    pub table: TableId,
    pub status: EnvironmentResolutionStatus,
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub left_text: Option<String>,
    pub right_text: Option<String>,
    pub left: Option<Pattern>,
    pub right: Option<Pattern>,
    pub class_tokens: Vec<EnvironmentClassToken>,
}

#[derive(Debug, Clone)]
struct SpannedToken {
    text: String,
    start: usize,
    end: usize,
}

/// Retains authored expression text and only nonblank snapshot environment identifiers.
pub(crate) fn snapshot_environment_source(
    env: &pg_snapshot::phonology::Environment,
) -> EnvironmentSource {
    EnvironmentSource {
        id: (!env.guid.trim().is_empty()).then(|| env.guid.clone()),
        text: (!env.representation.trim().is_empty()).then(|| env.representation.clone()),
    }
}

pub(crate) fn record_unattempted(snapshot: &pg_snapshot::Snapshot, ctx: &Ctx) {
    for environment in &snapshot.phonology.environments {
        let key = InventoryKey::object(InventoryKind::Environment, environment.guid.clone());
        if ctx.has_load_decision(&key) {
            continue;
        }
        let referenced = snapshot.lexicon.entries.iter().any(|entry| {
            entry.allomorphs.iter().any(|allomorph| {
                allomorph
                    .environments
                    .iter()
                    .chain(&allomorph.positions)
                    .any(|guid| guid == &environment.guid)
            })
        });
        ctx.considered(key.clone());
        ctx.not_considered(
            key,
            if referenced {
                pg_snapshot::LoadReasonCode::OwnerNotLoaded
            } else {
                pg_snapshot::LoadReasonCode::Unreferenced
            },
        );
    }
}

/// Resolves environment guids into `EnvironmentDef`s, recording a fatal issue for unresolved restrictions and a warning for invalid expressions; shared by `build_root_allomorph` and `build_circumfix_allomorphs`.
pub(crate) fn resolve_environment_defs<'a>(
    guids: impl IntoIterator<Item = &'a str>,
    ctx: &Ctx,
    allo_guid: &str,
) -> Vec<EnvironmentDef> {
    let mut environments = Vec::new();
    for env_guid in guids {
        let attachment = InventoryKey::attachment(
            InventoryKind::Environment,
            allo_guid.to_string(),
            env_guid.to_string(),
            roles::ENVIRONMENT,
        );
        ctx.authored(attachment.clone());
        ctx.considered(attachment.clone());
        let Some(env) = ctx.env_by_guid.get(env_guid) else {
            // HCLoader skips it silently; the report names the allomorph that holds the dangling reference.
            ctx.selected(attachment.clone());
            ctx.refuse_with_source(
                attachment,
                issue_codes::ENVIRONMENT_UNRESOLVED,
                IssueClass::InvalidSource,
                Some(SourceRef {
                    kind: pg_snapshot::FwClass::MoForm,
                    id: allo_guid.to_string(),
                }),
                format!("environment {env_guid:?} does not resolve"),
            );
            continue;
        };
        ctx.selected(attachment.clone());
        let env_object = InventoryKey::object(InventoryKind::Environment, env.guid.clone());
        ctx.considered(env_object.clone());
        ctx.selected(env_object.clone());
        let resolved = ctx.environment_resolution(env);
        match resolved.status {
            EnvironmentResolutionStatus::Valid => {
                environments.push(EnvironmentDef {
                    require: true,
                    left: resolved.left,
                    right: resolved.right,
                    source: Some(snapshot_environment_source(env)),
                });
                ctx.represented(attachment);
                ctx.represented(env_object);
            }
            EnvironmentResolutionStatus::Invalid => {
                let cause = resolved
                    .error
                    .unwrap_or_else(|| "invalid environment".into());
                let source = Some(SourceRef {
                    kind: pg_snapshot::FwClass::PhEnvironment,
                    id: env.guid.clone(),
                });
                ctx.reject_with_source(
                    attachment,
                    issue_codes::ENVIRONMENT_INVALID,
                    IssueClass::InvalidSource,
                    source.clone(),
                    format!("environment validation failed: {cause}"),
                );
                ctx.reject_with_source(
                    env_object,
                    issue_codes::ENVIRONMENT_INVALID,
                    IssueClass::InvalidSource,
                    source,
                    "environment representation failed to parse",
                );
            }
        }
    }
    environments
}

pub(crate) fn resolve_environment_expression(
    guid: &str,
    representation: &str,
    ctx: &Ctx,
) -> EnvironmentResolution {
    let (left_text, right_text) = match split_environment_string(representation) {
        Ok(parts) => parts,
        Err(error) => {
            return EnvironmentResolution {
                environment_guid: guid.into(),
                table: ctx.table_id,
                status: EnvironmentResolutionStatus::Invalid,
                error: Some(error),
                error_code: Some("invalid_environment".into()),
                left_text: None,
                right_text: None,
                left: None,
                right: None,
                class_tokens: Vec::new(),
            }
        }
    };
    let mut class_tokens = Vec::new();
    let left = match resolve_environment_side(&left_text, true, "left", ctx, &mut class_tokens) {
        Ok(pattern) => pattern,
        Err(error) => {
            return EnvironmentResolution {
                environment_guid: guid.into(),
                table: ctx.table_id,
                status: EnvironmentResolutionStatus::Invalid,
                error: Some(error),
                error_code: Some("invalid_environment".into()),
                left_text: Some(left_text),
                right_text: Some(right_text),
                left: None,
                right: None,
                class_tokens,
            }
        }
    };
    let right = match resolve_environment_side(&right_text, false, "right", ctx, &mut class_tokens)
    {
        Ok(pattern) => pattern,
        Err(error) => {
            return EnvironmentResolution {
                environment_guid: guid.into(),
                table: ctx.table_id,
                status: EnvironmentResolutionStatus::Invalid,
                error: Some(error),
                error_code: Some("invalid_environment".into()),
                left_text: Some(left_text),
                right_text: Some(right_text),
                left: None,
                right: None,
                class_tokens,
            }
        }
    };
    EnvironmentResolution {
        environment_guid: guid.into(),
        table: ctx.table_id,
        status: EnvironmentResolutionStatus::Valid,
        error: None,
        error_code: None,
        left_text: Some(left_text),
        right_text: Some(right_text),
        left,
        right,
        class_tokens,
    }
}

/// `SplitEnvironment` (HCLoader.cs:2260-2266) alone, without building patterns -- needed wherever a concatenative affix rule embeds one side's raw context tokens directly into its LHS pattern.
pub(crate) fn split_environment_string(representation: &str) -> Result<(String, String), String> {
    let body = representation
        .trim()
        .strip_prefix('/')
        .ok_or_else(|| format!("environment string {representation:?} must start with '/'"))?;
    let parts: Vec<&str> = body.split('_').collect();
    if parts.len() != 2 {
        return Err(format!(
            "environment string {representation:?} must contain exactly one '_'"
        ));
    }
    Ok((parts[0].trim().to_string(), parts[1].trim().to_string()))
}

/// `TokenizeContext` (HCLoader.cs:2420-2457): splits a context string into `#`, `[...]` (natural-class reference), `(...)` (optional group), and plain-text tokens.
pub(crate) fn tokenize(s: &str) -> Result<Vec<String>, String> {
    Ok(tokenize_spanned(s)?
        .into_iter()
        .map(|token| token.text)
        .collect())
}

fn tokenize_spanned(s: &str) -> Result<Vec<SpannedToken>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < chars.len() {
        match chars[pos] {
            '#' => {
                out.push(SpannedToken {
                    text: "#".into(),
                    start: pos,
                    end: pos + 1,
                });
                pos += 1;
            }
            '[' => {
                let end = find_from(&chars, pos, ']')
                    .ok_or_else(|| format!("missing closing ']' in {s:?} at position {pos}"))?;
                out.push(SpannedToken {
                    text: chars[pos..=end].iter().collect(),
                    start: pos,
                    end: end + 1,
                });
                pos = end + 1;
            }
            '(' => {
                let end = find_matching_paren(&chars, pos)
                    .ok_or_else(|| format!("missing closing ')' in {s:?} at position {pos}"))?;
                out.push(SpannedToken {
                    text: chars[pos..=end].iter().collect(),
                    start: pos,
                    end: end + 1,
                });
                pos = end + 1;
            }
            ')' => return Err(format!("unmatched ')' in {s:?} at position {pos}")),
            ' ' => pos += 1,
            _ => {
                let end = chars[pos..]
                    .iter()
                    .position(|&c| matches!(c, '#' | '[' | '(' | ')' | ' '))
                    .map(|d| pos + d)
                    .unwrap_or(chars.len());
                out.push(SpannedToken {
                    text: chars[pos..end].iter().collect(),
                    start: pos,
                    end,
                });
                pos = end;
            }
        }
    }
    Ok(out)
}

fn find_from(chars: &[char], from: usize, target: char) -> Option<usize> {
    chars[from..]
        .iter()
        .position(|&c| c == target)
        .map(|d| from + d)
}

/// Balanced-parenthesis scan for a `(` starting at `open`; nested optional groups aren't valid HC syntax, but scanning to the matching depth-0 `)` is a harmless superset that still flags an unbalanced string as invalid.
fn find_matching_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn resolve_environment_side(
    text: &str,
    left: bool,
    side: &str,
    ctx: &Ctx,
    class_tokens: &mut Vec<EnvironmentClassToken>,
) -> Result<Option<Pattern>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let tokens = tokenize_spanned(text)?;
    let mut nodes = Vec::new();
    if left && text.starts_with('#') {
        nodes.push(PatternNode::Anchor(AnchorSide::Left));
    }
    nodes.extend(nodes_from_spanned(
        &tokens,
        side,
        &[],
        0,
        ctx,
        class_tokens,
    )?);
    if !left && text.ends_with('#') {
        nodes.push(PatternNode::Anchor(AnchorSide::Right));
    }
    Ok(Some(Pattern { nodes }))
}

/// Resolves each class token through the compiler's winner map and records that winner.
fn nodes_from_spanned(
    tokens: &[SpannedToken],
    side: &str,
    parent_path: &[usize],
    source_offset: usize,
    ctx: &Ctx,
    class_tokens: &mut Vec<EnvironmentClassToken>,
) -> Result<Vec<PatternNode>, String> {
    let mut out = Vec::new();
    for (ordinal, token) in tokens.iter().enumerate() {
        let tok = &token.text;
        let mut path = parent_path.to_vec();
        path.push(ordinal);
        let mut chars = tok.chars();
        match chars.next() {
            Some('#') => {}
            Some('[') => {
                let name = tok[1..tok.len() - 1].trim();
                let nc = ctx.natclass_by_name.get(name).copied();
                let natural_class_guid = nc
                    .and_then(|id| ctx.natural_class_defs.get(id.0 as usize))
                    .map(|definition| definition.xml_id.clone());
                class_tokens.push(EnvironmentClassToken {
                    side: side.into(),
                    token_path: path
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join("/"),
                    token_text: tok.clone(),
                    source_start: source_offset + token.start,
                    source_end: source_offset + token.end,
                    natural_class_guid,
                    natural_class_index: nc.map(|id| id.0),
                });
                let nc = nc.ok_or_else(|| format!("unknown natural class {name:?}"))?;
                out.push(PatternNode::Context(SimpleContext {
                    nat_class: nc,
                    vars: Vec::new(),
                }));
            }
            Some('(') => {
                let content = &tok[1..tok.len() - 1];
                let leading = content.chars().take_while(|ch| ch.is_whitespace()).count();
                let inner = content.trim();
                let inner_tokens = tokenize_spanned(inner)?;
                let children = nodes_from_spanned(
                    &inner_tokens,
                    side,
                    &path,
                    source_offset + token.start + 1 + leading,
                    ctx,
                    class_tokens,
                )?;
                out.push(PatternNode::Quantifier {
                    min: 0,
                    max: Some(1),
                    children,
                });
            }
            Some(_) => {
                let text = tok.trim();
                let shape = crate::segment::segment_phonemes_only(ctx.table, text)
                    .map_err(|e| e.to_string())?;
                out.push(PatternNode::Segments {
                    table: ctx.table_id,
                    shape: crate::model::SegmentedText {
                        text: text.to_string(),
                        shape,
                    },
                });
            }
            None => {}
        }
    }
    Ok(out)
}

// --- `AnyPlus`/`AnyStar`/`PrefixNull`/`SuffixNull` (HCLoader.cs:2283-2311) ------------------------

pub(crate) fn prefix_null(ctx: &Ctx) -> PatternNode {
    PatternNode::Quantifier {
        min: 0,
        max: None,
        children: vec![
            PatternNode::CharDef(ctx.null_bdry),
            PatternNode::CharDef(ctx.morph_bdry),
        ],
    }
}

pub(crate) fn suffix_null(ctx: &Ctx) -> PatternNode {
    PatternNode::Quantifier {
        min: 0,
        max: None,
        children: vec![
            PatternNode::CharDef(ctx.morph_bdry),
            PatternNode::CharDef(ctx.null_bdry),
        ],
    }
}

fn any_context(ctx: &Ctx) -> PatternNode {
    PatternNode::Context(SimpleContext {
        nat_class: ctx.any_nc,
        vars: Vec::new(),
    })
}

pub(crate) fn any_plus(ctx: &Ctx) -> Vec<PatternNode> {
    vec![
        prefix_null(ctx),
        PatternNode::Quantifier {
            min: 1,
            max: None,
            children: vec![any_context(ctx)],
        },
        suffix_null(ctx),
    ]
}

pub(crate) fn any_star(ctx: &Ctx) -> Vec<PatternNode> {
    vec![
        prefix_null(ctx),
        PatternNode::Quantifier {
            min: 0,
            max: None,
            children: vec![any_context(ctx)],
        },
        suffix_null(ctx),
    ]
}

/// Publishes attached environment literals after the allomorph owner selects a form.
pub(crate) fn collect_text_uses<'a>(
    snapshot: &pg_snapshot::Snapshot,
    guids: impl IntoIterator<Item = &'a String>,
    recorder: &mut pg_snapshot::SelectionRecorder,
) {
    for guid in guids {
        if let Some(env) = snapshot
            .phonology
            .environments
            .iter()
            .find(|env| env.guid == *guid)
        {
            for literal in literal_text_elements(&env.representation) {
                recorder.record_text_use(
                    SourceRef {
                        kind: pg_snapshot::FwClass::PhEnvironment,
                        id: env.guid.clone(),
                    },
                    &literal,
                );
            }
        }
    }
}

/// Literal grapheme text only: excludes `_`/`#`/class brackets, descends into optional-group parens.
pub(crate) fn literal_text_elements(representation: &str) -> Vec<String> {
    let body = representation
        .trim()
        .strip_prefix('/')
        .unwrap_or_else(|| representation.trim());
    let mut out = Vec::new();
    for side in body.splitn(2, '_') {
        collect_literal_tokens(side, &mut out);
    }
    out
}

fn collect_literal_tokens(s: &str, out: &mut Vec<String>) {
    let Ok(tokens) = tokenize(s) else { return };
    for tok in &tokens {
        match tok.chars().next() {
            Some('#') | Some('[') => {}
            Some('(') => collect_literal_tokens(&tok[1..tok.len() - 1], out),
            Some(_) => out.push(tok.clone()),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests;
