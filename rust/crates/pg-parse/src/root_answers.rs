//! One word's answers to "does any root match this shape?" for its last-analysed stratum, held until the word ends.

use pg_grammar_model::model::StratumId;
use pg_shape::Shape;
use rustc_hash::FxHashMap;

/// The table stops taking answers at this many bytes, counting each shape as if the table alone held it; a shape it does not hold is checked afresh.
const LIMIT: usize = 4 * 1024 * 1024;

/// The answers one parse has had for one stratum, by full shape.
pub(crate) struct RootAnswers {
    stratum: StratumId,
    table: FxHashMap<Shape, bool>,
    bytes: usize,
}

impl RootAnswers {
    pub(crate) fn new(stratum: StratumId) -> Self {
        Self {
            stratum,
            table: FxHashMap::default(),
            bytes: 0,
        }
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
        if self.table.insert(shape.clone(), has_root).is_none() {
            self.bytes += entry_bytes;
        }
    }

    /// The bytes the held answers are counted as; never above the limit.
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
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
        let mut answers = RootAnswers::new(StratumId(0));
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
        let mut answers = RootAnswers::new(StratumId(0));
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
}
