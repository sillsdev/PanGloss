//! Lower bounds on construction units, from the plan and the character table alone, before any net is built.

use super::AdmissionPlan;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::Grammar;

/// Tokens every cascade alphabet has: one per segment and boundary char-def of the table.
pub(super) fn tokens_least(g: &Grammar) -> u64 {
    let Some(table) = g.char_tables.first() else {
        return 0;
    };
    table
        .iter()
        .filter(|(_, def)| matches!(def.kind(), CharDefKind::Segment | CharDefKind::Boundary))
        .count() as u64
}

// A net that accepts a string after each of `m` symbols has `m` arcs and a state; five wrapping steps read and make nets of that kind.
/// Units one piece costs at least with `m` tokens: its regex net, `tokens*`, and the four steps that wrap it in any text.
pub(super) fn piece_least(m: u64) -> u64 {
    let any_text = m + 1;
    let tokens = m + 2;
    4 + tokens + 11 * any_text
}

impl AdmissionPlan {
    // Counts the nets over the token alphabet that every piece needs; the cascade, the compositions and the table read-out are not counted, and a piece whose regex does not compile is assumed absent.
    /// Fewest construction units the admission build for `g` spends, known without building a net.
    pub fn construction_floor(&self, g: &Grammar) -> u64 {
        let m = tokens_least(g);
        let pieces = (self.pieces.len() + self.outer_pieces.len()) as u64;
        2 * m + pieces * piece_least(m)
    }
}
