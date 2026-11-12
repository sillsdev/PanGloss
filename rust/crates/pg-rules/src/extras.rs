//! The parts of a `Word` that are empty on almost every word, held behind one pointer.
//!
//! An absent box and a box of empty parts mean the same thing: every read goes through the accessors, so no comparison, key, print or byte estimate can tell them apart.

use pg_grammar_model::model::{MorphemeId, PRuleId, StratumId};

use crate::word::Word;

#[derive(Clone, Default)]
pub struct Extras(Option<Box<Parts>>);

#[derive(Clone, Default)]
struct Parts {
    non_heads: Vec<Word>,
    applied_realizational_rules: Vec<MorphemeId>,
    applied_phonological_rules: Vec<(StratumId, PRuleId)>,
    root_runtime_id: Option<String>,
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
            .finish()
    }
}

#[cfg(test)]
mod tests;
