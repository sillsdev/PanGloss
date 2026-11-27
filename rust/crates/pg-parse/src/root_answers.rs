//! One word's answers to "does any root match this shape?" for its last-analysed stratum, held until the word ends.

use pg_fst::work::WorkBudget;
use pg_grammar_model::model::StratumId;
use pg_shape::Shape;
use rustc_hash::FxHashMap;

/// The table stops taking answers at this many bytes, counting each shape as if the table alone held it; a shape it does not hold is checked afresh.
const LIMIT: usize = 4 * 1024 * 1024;

/// The search steps all of one word's checks may spend between them; past it a check says yes without searching, the safe answer.
pub(crate) const STEP_LIMIT: usize = 16 * 1024 * 1024;

/// The answers one parse has had for one stratum, by full shape.
pub(crate) struct RootAnswers {
    stratum: StratumId,
    table: FxHashMap<Shape, bool>,
    bytes: usize,
    /// The word's memory meter, which holds `bytes` as optional state.
    memory: Option<WorkBudget>,
    /// Search steps the checks of this word may still spend.
    steps_left: usize,
}

impl RootAnswers {
    pub(crate) fn new(stratum: StratumId, memory: Option<WorkBudget>) -> Self {
        Self {
            stratum,
            table: FxHashMap::default(),
            bytes: 0,
            memory,
            steps_left: STEP_LIMIT,
        }
    }

    pub(crate) fn steps_left(&self) -> usize {
        self.steps_left
    }

    pub(crate) fn spend_steps(&mut self, steps: usize) {
        self.steps_left = self.steps_left.saturating_sub(steps);
    }

    pub(crate) fn get(&self, stratum: StratumId, shape: &Shape) -> Option<bool> {
        if stratum != self.stratum {
            return None;
        }
        self.table.get(shape).copied()
    }

    pub(crate) fn insert(&mut self, stratum: StratumId, shape: &Shape, has_root: bool) {
        // Twice the entry allows for the table's spare buckets.
        let entry_bytes = shape
            .heap_bytes_upper_bound()
            .saturating_add(2 * (std::mem::size_of::<(Shape, bool)>() + 1));
        if stratum != self.stratum || self.bytes.saturating_add(entry_bytes) > LIMIT {
            return;
        }
        if self.table.contains_key(shape) {
            return;
        }
        // A refused charge leaves the table as it is and the shape to be searched again.
        if let Some(memory) = &self.memory {
            if !memory.try_charge_optional(entry_bytes) {
                return;
            }
        }
        self.table.insert(shape.clone(), has_root);
        self.bytes += entry_bytes;
    }

    /// The bytes the held answers are counted as; never above the limit.
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for RootAnswers {
    fn drop(&mut self) {
        if let Some(memory) = &self.memory {
            memory.release_optional(self.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(cd: u32) -> Shape {
        let mut builder = pg_shape::ShapeBuilder::new();
        builder.push_segment(cd);
        builder.finish()
    }

    #[test]
    fn an_answer_is_held_for_its_shape_and_stratum_only() {
        let mut answers = RootAnswers::new(StratumId(0), None);
        answers.insert(StratumId(0), &shape(1), true);
        answers.insert(StratumId(0), &shape(2), false);
        answers.insert(StratumId(1), &shape(3), true);
        assert_eq!(answers.get(StratumId(0), &shape(1)), Some(true));
        assert_eq!(answers.get(StratumId(0), &shape(2)), Some(false));
        assert_eq!(answers.get(StratumId(0), &shape(3)), None);
        assert_eq!(answers.get(StratumId(1), &shape(1)), None);
    }

    #[test]
    fn the_table_stops_taking_answers_at_its_limit_and_keeps_those_it_has() {
        let mut answers = RootAnswers::new(StratumId(0), None);
        let mut held = 0;
        for cd in 0..1_000_000 {
            let before = answers.bytes();
            answers.insert(StratumId(0), &shape(cd), cd % 2 == 0);
            if answers.bytes() == before {
                break;
            }
            held += 1;
        }
        assert!(held > 200, "only {held} answers fitted");
        assert!(answers.bytes() <= LIMIT);
        assert_eq!(answers.get(StratumId(0), &shape(held)), None);
        assert_eq!(answers.get(StratumId(0), &shape(0)), Some(true));
        assert_eq!(answers.get(StratumId(0), &shape(1)), Some(false));
    }

    #[test]
    fn a_refused_memory_charge_stops_the_table_taking_answers() {
        let budget = WorkBudget::new(usize::MAX).with_word_cap(Some(1));
        let mut answers = RootAnswers::new(StratumId(0), Some(budget.clone()));
        answers.insert(StratumId(0), &shape(1), true);
        assert_eq!(answers.get(StratumId(0), &shape(1)), None);
        assert!(!budget.word_capped());
        let roomy = WorkBudget::new(usize::MAX).with_word_cap(Some(1 << 20));
        let mut answers = RootAnswers::new(StratumId(0), Some(roomy.clone()));
        answers.insert(StratumId(0), &shape(1), true);
        assert_eq!(answers.get(StratumId(0), &shape(1)), Some(true));
        assert!(roomy.word_headroom().is_some());
        drop(answers);
        // The charge is returned: a fresh table can take the same bytes again.
        let mut again = RootAnswers::new(StratumId(0), Some(roomy));
        again.insert(StratumId(0), &shape(1), true);
        assert_eq!(again.get(StratumId(0), &shape(1)), Some(true));
    }
}
