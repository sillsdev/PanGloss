//! Affix forms written as bracket patterns (`[C^1][V^1]`, `ta[...]`), ported from `HCLoader.LoadFormAffixProcessAllomorph`, `LoadReduplicationPatterns` and `LoadReduplicationOutputActions` (HCLoader.cs:1494-1566, 1664-1710).

use crate::model::{OutputAction, PartRef, Pattern, PatternNode, SimpleContext};

use super::affixes::{insert_segments, Shape};
use super::environment;
use super::Ctx;

/// What a bracket-pattern form builds: LHS patterns, RHS actions, and the warnings C# logs without dropping the allomorph (`UnmatchedReduplicationIndexedClass`).
pub(super) type Built = (Vec<Pattern>, Vec<OutputAction>, Vec<String>);

/// `form.Contains("[")` (HCLoader.cs:1495): the form is a pattern, not literal text.
pub(crate) fn is_bracket_form(form: &str) -> bool {
    form.contains('[')
}

/// `form.Contains("[...]")` (HCLoader.cs:1497): the stem is copied around the literal text rather than read from an environment.
pub(super) fn is_stem_copy_form(form: &str) -> bool {
    form.contains("[...]")
}

/// Whether the form, read as the right context of an environment, validates (HCLoader.cs:1528-1530).
pub(super) fn validate_form(form: &str, ctx: &Ctx) -> Result<(), String> {
    let resolution = environment::resolve_environment_expression("", &format!("/_{form}"), ctx);
    match resolution.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// `form.Substring(0, form.IndexOf('['))` and the text after the first `]`, as `ta[...]` and `[...]tu` split (HCLoader.cs:1506-1516).
pub(super) fn build_stem_copy(form: &str, ctx: &Ctx) -> Result<Built, String> {
    let before = form[..form.find('[').unwrap_or(0)].trim();
    let after = form[form.find(']').map_or(form.len(), |p| p + 1)..].trim();
    let lhs = vec![Pattern {
        nodes: environment::any_plus(ctx),
    }];
    let mut rhs = vec![
        OutputAction::Copy(PartRef::Input(0)),
        insert_segments(&format!("+{before}"), ctx)?,
        OutputAction::Copy(PartRef::Input(0)),
    ];
    if !after.is_empty() {
        rhs.push(insert_segments(after, ctx)?);
    }
    Ok((lhs, rhs, Vec::new()))
}

/// Builds the allomorph for a form that validated as an environment. `left` and `right` are the raw sides of the environment (empty when the pass has none): the left side holds a suffix's patterns, the right side a prefix's.
pub(super) fn build(
    form: &str,
    shape: Shape,
    left: &str,
    right: &str,
    ctx: &Ctx,
) -> Result<Built, String> {
    let context = match shape {
        Shape::Suffix => left,
        _ => right,
    };
    let (patterns, names) = reduplication_patterns(context, ctx)?;
    let mut notes = Vec::new();
    let part = |name_index: usize| OutputAction::Copy(PartRef::Input(name_index as u16));
    let stem_pattern = || Pattern {
        nodes: environment::any_star(ctx),
    };
    let (lhs, rhs) = match shape {
        Shape::Suffix => {
            let mut lhs = vec![stem_pattern()];
            lhs.extend(patterns);
            lhs.push(Pattern {
                nodes: vec![environment::suffix_null(ctx)],
            });
            let null_part = lhs.len() - 1;
            let mut rhs = vec![part(0)];
            rhs.extend(output_actions(left, &names, 1, ctx, &mut notes)?);
            rhs.push(part(null_part));
            rhs.push(insert_segments("+", ctx)?);
            rhs.extend(output_actions(form, &names, 1, ctx, &mut notes)?);
            (lhs, rhs)
        }
        _ => {
            let mut lhs = vec![Pattern {
                nodes: vec![environment::prefix_null(ctx)],
            }];
            lhs.extend(patterns);
            lhs.push(stem_pattern());
            let stem_part = lhs.len() - 1;
            let mut rhs = output_actions(form, &names, 1, ctx, &mut notes)?;
            rhs.push(insert_segments("+", ctx)?);
            rhs.push(part(0));
            rhs.extend(output_actions(right, &names, 1, ctx, &mut notes)?);
            rhs.push(part(stem_part));
            (lhs, rhs)
        }
    };
    Ok((lhs, rhs, notes))
}

/// `LoadReduplicationPatterns`: one pattern per `[Class^n]` token, named by the token text. A token without `^` makes C# throw `ArgumentOutOfRangeException`; here the allomorph is skipped with a warning.
fn reduplication_patterns(context: &str, ctx: &Ctx) -> Result<(Vec<Pattern>, Vec<String>), String> {
    let mut patterns = Vec::new();
    let mut names = Vec::new();
    for token in environment::tokenize(context)? {
        if !token.starts_with('[') {
            continue;
        }
        let inner = token[1..token.len() - 1].trim();
        let (class, _) = inner
            .split_once('^')
            .ok_or_else(|| format!("reduplication context {token:?} has no ^ index"))?;
        let nat_class = ctx
            .natclass_by_name
            .get(class.trim())
            .copied()
            .ok_or_else(|| format!("unknown natural class {:?}", class.trim()))?;
        patterns.push(Pattern {
            nodes: vec![PatternNode::Context(SimpleContext {
                nat_class,
                vars: Vec::new(),
            })],
        });
        names.push(inner.to_string());
    }
    Ok((patterns, names))
}

/// `LoadReduplicationOutputActions`: a `[Class^n]` token copies the pattern of that name (or warns when none matches); any other token is inserted as text. `first_part` is the input part of the first named pattern.
fn output_actions(
    context: &str,
    names: &[String],
    first_part: usize,
    ctx: &Ctx,
    notes: &mut Vec<String>,
) -> Result<Vec<OutputAction>, String> {
    let mut out = Vec::new();
    for token in environment::tokenize(context)? {
        if !token.starts_with('[') {
            out.push(insert_segments(token.trim(), ctx)?);
            continue;
        }
        let inner = token[1..token.len() - 1].trim();
        match names.iter().position(|name| name == inner) {
            Some(index) => out.push(OutputAction::Copy(PartRef::Input(
                (first_part + index) as u16,
            ))),
            None if inner.contains('^') => {
                notes.push(format!("Ill-formed index: {token}"));
            }
            None => return Err(format!("reduplication token {token:?} has no ^ index")),
        }
    }
    Ok(out)
}
