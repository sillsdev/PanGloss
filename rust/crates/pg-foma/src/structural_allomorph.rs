//! Circumfix text lowering for the surface-probed emitter's edge-decomposability check: the
//! already-encoded `(prefix, suffix)` token text of a `Role::CircumfixPrefix` allomorph.

use pg_grammar::model::{AffixAllomorphDef, Grammar, OutputAction, PartRef};

use crate::replace::SegAlphabet;

fn encode_insert_actions(
    g: &Grammar,
    alphabet: &SegAlphabet,
    actions: &[OutputAction],
) -> Option<Vec<String>> {
    if actions.is_empty() {
        return None;
    }
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments { table, shape } = action else {
            return None;
        };
        let origin = g.char_tables.get(table.0 as usize)?;
        let pieces = crate::emit::underlying_shape_variants(alphabet, origin, &shape.shape);
        if pieces.is_empty() {
            return None;
        }
        let mut next = Vec::with_capacity(variants.len() * pieces.len());
        for prefix in &variants {
            for piece in &pieces {
                let mut encoded = prefix.clone();
                encoded.push_str(piece);
                next.push(encoded);
            }
        }
        variants = next;
    }
    Some(variants)
}

/// The already-encoded `(prefix, suffix)` token text for `allomorph`, if it matches a single,
/// non-reduplicated `Copy(Input(0))` wrapped by leading and trailing `InsertSegments` over a
/// 1-part LHS (`Role::CircumfixPrefix`'s shape); an interior insert or a repeated copy of the same
/// part needs root-internal splitting or duplication this cannot represent, so stays uncovered.
pub(crate) fn circumfix_texts(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allomorph: &AffixAllomorphDef,
) -> Option<Vec<(String, String)>> {
    if allomorph.lhs.len() != 1 {
        return None;
    }
    let rhs = allomorph.rhs.as_slice();
    let mut copy_positions = rhs
        .iter()
        .enumerate()
        .filter_map(|(i, a)| matches!(a, OutputAction::Copy(PartRef::Input(0))).then_some(i));
    let copy_pos = copy_positions.next()?;
    if copy_positions.next().is_some() {
        return None;
    }
    if copy_pos == 0 || copy_pos == rhs.len() - 1 {
        return None;
    }
    let prefixes = encode_insert_actions(g, alphabet, &rhs[..copy_pos])?;
    let suffixes = encode_insert_actions(g, alphabet, &rhs[copy_pos + 1..])?;
    let mut pairs = Vec::with_capacity(prefixes.len() * suffixes.len());
    for prefix in prefixes {
        for suffix in &suffixes {
            pairs.push((prefix.clone(), suffix.clone()));
        }
    }
    Some(pairs)
}

#[cfg(test)]
mod circumfix_text_tests;
