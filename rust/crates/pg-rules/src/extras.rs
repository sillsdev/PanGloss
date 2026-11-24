//! The parts of a `Word` that are empty on almost every word, held behind one pointer.
//!
//! An absent box and a box of empty parts mean the same thing: every read goes through the accessors, so no comparison, key, print or byte estimate can tell them apart.

use pg_grammar_model::model::{AllomorphId, MorphemeId, PRuleId, StratumId};

use crate::word::Word;

#[derive(Clone, Default)]
pub struct Extras(Option<Box<Parts>>);

#[derive(Clone, Default)]
struct Parts {
    non_heads: Vec<Word>,
    applied_realizational_rules: Vec<MorphemeId>,
    applied_phonological_rules: Vec<(StratumId, PRuleId)>,
    root_runtime_id: Option<String>,
    pinned_starts: Vec<(AllomorphId, u16, u16)>,
    deleted_gaps: Vec<u32>,
    bare_nodes: Vec<u32>,
    span_marks: SpanMarks,
}

/// What phonological rules did to morph identity in a word that asked for span tracking; empty on every other word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanMarks {
    /// Interior node indices a rewrite inserted, with the rule that did.
    pub inserted: Vec<(u32, PRuleId)>,
    /// Morph runs (allomorph, application, run) whose segments a rule made ambiguous, with the rule that did.
    pub lost: Vec<((AllomorphId, u16, u16), PRuleId)>,
    /// Morphs (allomorph, application) an affix that inserts nothing left as a positional placeholder on a real segment.
    pub positional: Vec<(AllomorphId, u16)>,
    /// Morphs (allomorph, application) whose reduplication left the engine free to call either copy the base.
    pub ambiguous_copy: Vec<(AllomorphId, u16)>,
    /// A later rule rebuilt the shape where the marks could not follow, so no attribution holds; the rule is the first one that left a mark.
    pub poisoned: Option<PRuleId>,
}

impl SpanMarks {
    pub fn is_empty(&self) -> bool {
        self.inserted.is_empty()
            && self.lost.is_empty()
            && self.positional.is_empty()
            && self.poisoned.is_none()
    }

    pub(crate) fn lose(&mut self, run: (AllomorphId, u16, u16), rule: PRuleId) {
        if !self.lost.iter().any(|(existing, _)| *existing == run) {
            self.lost.push((run, rule));
        }
    }

    /// Follows the marks onto a rebuilt shape: `origins[new]` is the old interior index of the node now at `new`, `None` for new material; a marked node copied more than once poisons the word.
    pub(crate) fn follow(&mut self, origins: &[Option<u32>]) {
        let mut inserted = Vec::with_capacity(self.inserted.len());
        for &(old, rule) in &self.inserted {
            let mut targets = origins
                .iter()
                .enumerate()
                .filter(|(_, origin)| **origin == Some(old))
                .map(|(new, _)| new as u32);
            match (targets.next(), targets.next()) {
                (Some(new), None) => inserted.push((new, rule)),
                (None, _) => {}
                (Some(_), Some(_)) => self.poisoned = self.poisoned.or(Some(rule)),
            }
        }
        inserted.sort_unstable();
        self.inserted = inserted;
    }
}

impl Extras {
    fn parts_mut(&mut self) -> &mut Parts {
        self.0.get_or_insert_with(Box::default)
    }

    pub fn non_heads(&self) -> &[Word] {
        self.0.as_ref().map_or(&[][..], |p| &p.non_heads)
    }

    pub fn non_heads_mut(&mut self) -> &mut Vec<Word> {
        &mut self.parts_mut().non_heads
    }

    pub fn applied_realizational_rules(&self) -> &[MorphemeId] {
        self.0
            .as_ref()
            .map_or(&[][..], |p| &p.applied_realizational_rules)
    }

    pub fn applied_realizational_rules_mut(&mut self) -> &mut Vec<MorphemeId> {
        &mut self.parts_mut().applied_realizational_rules
    }

    pub fn applied_phonological_rules(&self) -> &[(StratumId, PRuleId)] {
        self.0
            .as_ref()
            .map_or(&[][..], |p| &p.applied_phonological_rules)
    }

    pub fn applied_phonological_rules_mut(&mut self) -> &mut Vec<(StratumId, PRuleId)> {
        &mut self.parts_mut().applied_phonological_rules
    }

    pub fn root_runtime_id(&self) -> Option<&str> {
        self.0.as_ref()?.root_runtime_id.as_deref()
    }

    /// Morph runs, by allomorph, application and run, whose first node a synthesis rewrite deleted; C# keeps that node in the shape, so a later swap moves the morph's start only when it moves one of the morph's segments in front of that node.
    pub(crate) fn pinned_starts(&self) -> &[(AllomorphId, u16, u16)] {
        self.0.as_ref().map_or(&[][..], |p| &p.pinned_starts)
    }

    pub(crate) fn pin_start(&mut self, morph: (AllomorphId, u16, u16)) {
        if !self.pinned_starts().contains(&morph) {
            self.parts_mut().pinned_starts.push(morph);
        }
    }

    /// Interior node indices that a synthesis rewrite deleted a non-optional node just before.
    pub(crate) fn deleted_gaps(&self) -> &[u32] {
        self.0.as_ref().map_or(&[][..], |p| &p.deleted_gaps)
    }

    /// Interior node indices a synthesis rewrite inserted at a morph's range edge, which belong to no morph.
    pub(crate) fn bare_nodes(&self) -> &[u32] {
        self.0.as_ref().map_or(&[][..], |p| &p.bare_nodes)
    }

    pub(crate) fn set_shape_marks(&mut self, deleted_gaps: Vec<u32>, bare_nodes: Vec<u32>) {
        if deleted_gaps.is_empty() && bare_nodes.is_empty() && self.0.is_none() {
            return;
        }
        let parts = self.parts_mut();
        parts.deleted_gaps = deleted_gaps;
        parts.bare_nodes = bare_nodes;
    }

    pub(crate) fn span_marks(&self) -> Option<&SpanMarks> {
        self.0.as_ref().map(|p| &p.span_marks)
    }

    pub(crate) fn span_marks_mut(&mut self) -> &mut SpanMarks {
        &mut self.parts_mut().span_marks
    }

    /// Forgets everything that describes the old shape, for a rule that rebuilds the shape from its copied nodes.
    pub(crate) fn clear_shape_marks(&mut self) {
        if let Some(parts) = self.0.as_mut() {
            parts.pinned_starts.clear();
            parts.deleted_gaps.clear();
            parts.bare_nodes.clear();
        }
    }

    pub fn set_root_runtime_id(&mut self, id: Option<String>) {
        if id.is_none() && self.0.is_none() {
            return;
        }
        self.parts_mut().root_runtime_id = id;
    }
}

impl std::fmt::Debug for Extras {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extras")
            .field("non_heads", &self.non_heads())
            .field(
                "applied_realizational_rules",
                &self.applied_realizational_rules(),
            )
            .field(
                "applied_phonological_rules",
                &self.applied_phonological_rules(),
            )
            .field("root_runtime_id", &self.root_runtime_id())
            .field("pinned_starts", &self.pinned_starts())
            .field("deleted_gaps", &self.deleted_gaps())
            .field("bare_nodes", &self.bare_nodes())
            .field("span_marks", &self.span_marks().filter(|m| !m.is_empty()))
            .finish()
    }
}

#[cfg(test)]
mod tests;
