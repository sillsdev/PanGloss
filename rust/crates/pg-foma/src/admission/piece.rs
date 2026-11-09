//! Renders an inserted literal as a regular expression over tokens that tolerates later edits.

use super::plan::{Cut, Literal, Mat, Max, Model, Tok};
use crate::replace::SegAlphabet;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::PatternNode;
use std::collections::BTreeSet;

const MAX_NESTING: usize = 3;
const MAX_GAP_CHARS: usize = 4000;

pub(super) struct Ctx<'a> {
    model: &'a Model,
    matches: &'a dyn Fn(&PatternNode, char) -> bool,
    boundaries: Vec<char>,
}

impl<'a> Ctx<'a> {
    pub(super) fn new(
        model: &'a Model,
        table: &CharDefTable,
        matches: &'a dyn Fn(&PatternNode, char) -> bool,
    ) -> Self {
        let alphabet = SegAlphabet::new(table);
        let boundaries = table
            .iter()
            .filter(|(_, def)| def.kind() == CharDefKind::Boundary)
            .map(|(id, _)| alphabet.token(id))
            .collect();
        Ctx {
            model,
            matches,
            boundaries,
        }
    }

    fn reaches(&self, left_depth: usize, right_depth: usize, left: Max, right: Max) -> bool {
        let (dl, dr) = match self.model.deletion {
            Some(d) => (left_depth.saturating_sub(d), right_depth.saturating_sub(d)),
            None => (0, 0),
        };
        left.is_none_or(|b| dl <= b) && right.is_none_or(|b| dr <= b)
    }

    fn zone_hit(&self, dl: usize, dr: usize, ch: char) -> bool {
        self.model.zones.iter().any(|z| {
            self.reaches(dl, dr, z.left, z.right)
                && z.node.as_ref().is_none_or(|node| (self.matches)(node, ch))
        })
    }

    fn edge_hit(&self, dl: usize, dr: usize) -> bool {
        self.model
            .edges
            .iter()
            .any(|e| self.reaches(dl, dr, e.left, e.right))
    }

    fn boundary_union(&self) -> String {
        let members: Vec<String> = self.boundaries.iter().map(char::to_string).collect();
        format!("[ {} ]", members.join(" | "))
    }

    fn wild(&self, min: usize, max: Max) -> String {
        if max == Some(0) {
            return String::new();
        }
        let (bstar, segment) = if self.boundaries.is_empty() {
            (String::new(), "?".to_owned())
        } else {
            let union = self.boundary_union();
            (format!("{union}*"), format!("\\{union}"))
        };
        match max {
            None if min == 0 => "?*".to_owned(),
            None => format!("[ [ {bstar} {segment} ]^>{} {bstar} ]", min - 1),
            Some(m) => format!("[ [ {bstar} {segment} ]^{{{min},{m}}} {bstar} ]"),
        }
    }

    fn gap(&self, dl: usize, dr: usize, level: usize) -> Option<String> {
        let cuts: Vec<&Cut> = self
            .model
            .cuts
            .iter()
            .filter(|c| self.reaches(dl, dr, c.left, c.right))
            .collect();
        if cuts.is_empty() {
            return None;
        }
        if level >= MAX_NESTING {
            return Some("?*".to_owned());
        }
        let materials: BTreeSet<String> = cuts
            .iter()
            .map(|c| self.sequence(&c.material, (dl, dr), level + 1, &mut 0))
            .filter(|m| !m.is_empty())
            .collect();
        if materials.is_empty() {
            return None;
        }
        let total: usize = materials.iter().map(String::len).sum();
        if total > MAX_GAP_CHARS {
            return Some("?*".to_owned());
        }
        Some(format!(
            "[ {} ]*",
            materials.into_iter().collect::<Vec<_>>().join(" | ")
        ))
    }

    fn tokens(
        &self,
        toks: &[Tok],
        (x, y, q): (usize, usize, usize),
        k0: usize,
        level: usize,
        plain: &mut usize,
    ) -> (Vec<String>, usize) {
        let position = |k: usize| (x + k, y + q.saturating_sub(k));
        let mut k = k0;
        let mut hits = Vec::with_capacity(toks.len());
        for tok in toks {
            if tok.segment {
                let (dl, dr) = (x + k, y + q.saturating_sub(k + 1));
                hits.push(self.zone_hit(dl, dr, tok.ch));
                k += 1;
            } else {
                hits.push(false);
            }
        }
        let zone_near = |i: usize| {
            let left = (0..i).rev().find(|&j| toks[j].segment).map(|j| hits[j]);
            let right = (i + 1..toks.len())
                .find(|&j| toks[j].segment)
                .map(|j| hits[j]);
            left.unwrap_or(false) || right.unwrap_or(false)
        };
        let mut out = Vec::new();
        let mut seen = k0;
        for (i, tok) in toks.iter().enumerate() {
            if tok.segment {
                seen += 1;
                if hits[i] {
                    out.push("( ? )".to_owned());
                } else {
                    if level == 0 {
                        *plain += 1;
                    }
                    out.push(tok.ch.to_string());
                }
            } else {
                let (dl, dr) = position(seen);
                if self.edge_hit(dl, dr) || zone_near(i) {
                    out.push(format!("( {} )", tok.ch));
                } else {
                    out.push(tok.ch.to_string());
                }
            }
            if i + 1 < toks.len() {
                let (dl, dr) = position(seen);
                out.extend(self.gap(dl, dr, level));
            }
        }
        (out, seen)
    }

    fn sequence(
        &self,
        seq: &[Mat],
        (x, y): (usize, usize),
        level: usize,
        plain: &mut usize,
    ) -> String {
        let q: usize = seq
            .iter()
            .map(|m| match m {
                Mat::Lit(toks) => toks.iter().filter(|t| t.segment).count(),
                Mat::Wild { min, .. } => *min,
            })
            .sum();
        let mut parts = Vec::new();
        let mut k = 0;
        for item in seq {
            match item {
                Mat::Wild { min, max } => {
                    parts.push(self.wild(*min, *max));
                    k += min;
                }
                Mat::Lit(toks) => {
                    let (rendered, next) = self.tokens(toks, (x, y, q), k, level, plain);
                    parts.extend(rendered);
                    k = next;
                }
            }
        }
        parts.retain(|p| !p.is_empty());
        parts.join(" ")
    }
}

/// The piece for one literal and its count of plain (unchanged, mandatory) segments; `None` when unusable.
pub(super) fn render_literal(ctx: &Ctx, lit: &Literal) -> Option<(String, usize)> {
    let toks = lit.toks.clone()?;
    let mut plain = 0;
    let text = ctx.sequence(&[Mat::Lit(toks)], (lit.before, lit.after), 0, &mut plain);
    (plain > 0).then_some((text, plain))
}
