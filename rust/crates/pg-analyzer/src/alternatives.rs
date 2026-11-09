//! Segmentation alternatives: other divisions of one Occurrence's source span (ADR 0011).

use crate::text::Span;

/// Alternatives per Occurrence when the request names no bound.
pub(crate) const DEFAULT_MAX_ALTERNATIVES: usize = 16;

/// Candidates examined per allowed alternative before giving up; bounds duplicate-heavy runs.
const CANDIDATES_PER_ALTERNATIVE: usize = 64;

/// A character range of the Occurrence, half-open, in code points.
pub(crate) type PartRange = (usize, usize);

pub(crate) struct Generated {
    /// Distinct divisions other than the primary, fewest changes first.
    pub(crate) divisions: Vec<Vec<PartRange>>,
    /// More divisions exist than the bound allowed.
    pub(crate) capped: bool,
}

fn is_dual_role(c: char) -> bool {
    matches!(
        c,
        '\'' | '\u{2018}' | '\u{2019}' | '-' | '\u{2010}' | '\u{2011}'
    )
}

#[derive(Clone, Copy)]
enum Mode {
    KeepLeft,
    Drop,
}

struct Search<'a> {
    length: usize,
    positions: &'a [usize],
    limit: usize,
    budget: usize,
    out: Generated,
    seen: std::collections::BTreeSet<Vec<PartRange>>,
}

pub(crate) fn generate(word: &str, limit: usize) -> Generated {
    let positions: Vec<usize> = word
        .chars()
        .enumerate()
        .filter_map(|(i, c)| is_dual_role(c).then_some(i))
        .collect();
    let length = word.chars().count();
    let mut search = Search {
        length,
        positions: &positions,
        limit,
        budget: limit.saturating_mul(CANDIDATES_PER_ALTERNATIVE) + CANDIDATES_PER_ALTERNATIVE,
        out: Generated {
            divisions: Vec::new(),
            capped: false,
        },
        seen: std::collections::BTreeSet::new(),
    };
    search.seen.insert(vec![(0, length)]);
    for changed in 1..=positions.len() {
        if !search.choose(changed, 0, &mut Vec::new()) {
            break;
        }
    }
    search.out
}

impl Search<'_> {
    /// Picks `left` more changed positions from `positions[from..]`; false once the search is over.
    fn choose(&mut self, left: usize, from: usize, picked: &mut Vec<(usize, Mode)>) -> bool {
        if left == 0 {
            return self.offer(picked);
        }
        for index in from..self.positions.len() {
            for mode in [Mode::KeepLeft, Mode::Drop] {
                picked.push((self.positions[index], mode));
                let go = self.choose(left - 1, index + 1, picked);
                picked.pop();
                if !go {
                    return false;
                }
            }
        }
        true
    }

    fn offer(&mut self, picked: &[(usize, Mode)]) -> bool {
        if self.budget == 0 {
            self.out.capped = true;
            return false;
        }
        self.budget -= 1;
        let division = divide(self.length, picked);
        if division.is_empty() || !self.seen.insert(division.clone()) {
            return true;
        }
        if self.out.divisions.len() == self.limit {
            self.out.capped = true;
            return false;
        }
        self.out.divisions.push(division);
        true
    }
}

fn divide(length: usize, picked: &[(usize, Mode)]) -> Vec<PartRange> {
    let mut parts = Vec::new();
    let mut start = 0;
    for &(at, mode) in picked {
        match mode {
            Mode::KeepLeft => {
                parts.push((start, at + 1));
                start = at + 1;
            }
            Mode::Drop => {
                parts.push((start, at));
                start = at + 1;
            }
        }
    }
    parts.push((start, length));
    parts.retain(|(a, b)| a < b);
    parts
}

/// Source spans of every character range of a word that starts at `origin`.
pub(crate) fn part_span(word: &str, origin: &Span, range: PartRange) -> Span {
    let mut offsets = Vec::with_capacity(word.len() + 1);
    let (mut byte, mut unit) = (0, 0);
    for ch in word.chars() {
        offsets.push((byte, unit));
        byte += ch.len_utf8();
        unit += ch.len_utf16();
    }
    offsets.push((byte, unit));
    let (a, b) = (offsets[range.0], offsets[range.1]);
    Span {
        code_points: [
            origin.code_points[0] + range.0,
            origin.code_points[0] + range.1,
        ],
        utf8: [origin.utf8[0] + a.0, origin.utf8[0] + b.0],
        utf16: [origin.utf16[0] + a.1, origin.utf16[0] + b.1],
    }
}
