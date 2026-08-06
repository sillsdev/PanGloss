//! Environment-string tokenization and pattern building (`TokenizeContext`/`LoadPatternNodes`/`LoadEnvironmentPattern`/`SplitEnvironment`, HCLoader.cs:2260-2457): re-tokenizes and validates a hand-authored string like `/_[UnVDent]` at compile time exactly as HCLoader does at load time -- lazily and tolerantly, since a malformed environment is a warning, never a hard failure.

use crate::model::{AnchorSide, Pattern, PatternNode, SimpleContext};

use super::Ctx;

/// `SplitEnvironment` + `IsValidEnvironment` + `LoadEnvironmentPattern` folded into one tokenize-and-build pass: parses `/left_right` into `(leftPattern, rightPattern)`, `Err` for anything `IsValidEnvironment` would reject so the caller can fall back to treating the environment as absent.
pub(crate) fn parse_environment(
    representation: &str,
    ctx: &Ctx,
) -> Result<(Option<Pattern>, Option<Pattern>), String> {
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
    let left_str = parts[0].trim();
    let right_str = parts[1].trim();
    let left = load_environment_pattern(left_str, true, ctx)?;
    let right = load_environment_pattern(right_str, false, ctx)?;
    Ok((left, right))
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

/// `LoadEnvironmentPattern` (HCLoader.cs:2268-2281): `left` selects which edge a bare `#` anchors (start for the left context, end for the right).
pub(crate) fn load_environment_pattern(
    s: &str,
    left: bool,
    ctx: &Ctx,
) -> Result<Option<Pattern>, String> {
    if s.is_empty() {
        return Ok(None);
    }
    let tokens = tokenize(s)?;
    let mut nodes = Vec::new();
    if left && s.starts_with('#') {
        nodes.push(PatternNode::Anchor(AnchorSide::Left));
    }
    nodes.extend(nodes_from_tokens(&tokens, ctx)?);
    if !left && s.ends_with('#') {
        nodes.push(PatternNode::Anchor(AnchorSide::Right));
    }
    Ok(Some(Pattern { nodes }))
}

/// `TokenizeContext` (HCLoader.cs:2420-2457): splits a context string into `#`, `[...]` (natural-class reference), `(...)` (optional group), and plain-text tokens.
pub(crate) fn tokenize(s: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < chars.len() {
        match chars[pos] {
            '#' => {
                out.push("#".to_string());
                pos += 1;
            }
            '[' => {
                let end = find_from(&chars, pos, ']')
                    .ok_or_else(|| format!("missing closing ']' in {s:?} at position {pos}"))?;
                out.push(chars[pos..=end].iter().collect());
                pos = end + 1;
            }
            '(' => {
                let end = find_matching_paren(&chars, pos)
                    .ok_or_else(|| format!("missing closing ')' in {s:?} at position {pos}"))?;
                out.push(chars[pos..=end].iter().collect());
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
                out.push(chars[pos..end].iter().collect());
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

/// `LoadPatternNodes` (HCLoader.cs:2391-2418): builds pattern nodes from already-tokenized context text; a bare `#` token contributes no node here since the edge anchor, if any, was already pushed by `load_environment_pattern`.
fn nodes_from_tokens(tokens: &[String], ctx: &Ctx) -> Result<Vec<PatternNode>, String> {
    let mut out = Vec::new();
    for tok in tokens {
        let mut chars = tok.chars();
        match chars.next() {
            Some('#') => {}
            Some('[') => {
                let name = tok[1..tok.len() - 1].trim();
                let nc = ctx
                    .natclass_by_name
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("unknown natural class {name:?}"))?;
                out.push(PatternNode::Context(SimpleContext {
                    nat_class: nc,
                    vars: Vec::new(),
                }));
            }
            Some('(') => {
                let inner = tok[1..tok.len() - 1].trim();
                let inner_tokens = tokenize(inner)?;
                let children = nodes_from_tokens(&inner_tokens, ctx)?;
                out.push(PatternNode::Quantifier {
                    min: 0,
                    max: Some(1),
                    children,
                });
            }
            Some(_) => {
                let text = tok.trim();
                let shape = crate::segment::segment_phonemes_only(ctx.table, text)
                    .map_err(|e| format!("cannot segment {text:?}: {e}"))?;
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

/// `LoadPatternNodes(patternStr)` for a plain (non-environment-split) context string -- used by the infix LHS builder, which runs it directly on each side, not through `LoadEnvironmentPattern`, so no edge-anchor handling here.
pub(crate) fn pattern_nodes(s: &str, ctx: &Ctx) -> Result<Vec<PatternNode>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let tokens = tokenize(s)?;
    nodes_from_tokens(&tokens, ctx)
}

/// `IsValidEnvironment` (HCLoader.cs:1205-1271): the whole-string validity check every environment goes through before any pattern is built, as a dry run of the same tokenize/build machinery so verdicts cannot drift from what construction would accept.
/// See `docs/research/pg-grammar-environment-validation-granularity.md` for why the check must fail the environment as a whole rather than per side.
pub(crate) fn validate_environment(representation: &str, ctx: &Ctx) -> Result<(), String> {
    let (left, right) = split_environment_string(representation)?;
    for side in [left, right] {
        if side.is_empty() {
            continue;
        }
        let tokens = tokenize(&side)?;
        nodes_from_tokens(&tokens, ctx)?;
    }
    Ok(())
}
