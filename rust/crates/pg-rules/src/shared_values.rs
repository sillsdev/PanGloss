//! One parse's shapes and syntactic feature structures, held once each.
//!
//! Every rule unapplication freezes a new shape, and a search over null or near-null affixes reaches the same few shapes under many rule histories: Mbugwe `mindeshi` at a 200M work cap holds 379,394 words over 160 distinct shapes. Sharing makes each stored word cost a pointer for its shape instead of a private copy of every column.
//!
//! This is not a result cache: it answers no question and skips no work. It hands back the first value equal to the one given, compared in full, so a hash collision can never substitute a different value.

use std::cell::RefCell;
use std::hash::BuildHasher;

use hashbrown::HashTable;
use pg_featstruct::FeatureStruct;
use pg_shape::Shape;
use rustc_hash::FxBuildHasher;

use crate::word::Word;

/// The distinct shapes and syntactic feature structures one parse has produced. A table entry is one shared pointer.
#[derive(Default)]
pub struct SharedValues {
    shapes: RefCell<HashTable<Shape>>,
    feature_structs: RefCell<HashTable<FeatureStruct>>,
}

impl SharedValues {
    /// Points `w`'s shape and syntactic feature structure at the first equal value this parse produced.
    pub fn share(&self, w: &mut Word) {
        share_one(&mut self.shapes.borrow_mut(), &mut w.shape);
        if w.syn_fs != FeatureStruct::EMPTY {
            share_one(&mut self.feature_structs.borrow_mut(), &mut w.syn_fs);
        }
    }
}

fn share_one<T: Clone + Eq + std::hash::Hash>(table: &mut HashTable<T>, value: &mut T) {
    let hash = FxBuildHasher.hash_one(&*value);
    match table.find(hash, |held| held == value) {
        Some(held) => *value = held.clone(),
        None => {
            table.insert_unique(hash, value.clone(), |held| FxBuildHasher.hash_one(held));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_featstruct::{FeatId, FeatureStructBuilder, FeatureValue, SymbolBits};
    use pg_grammar_model::model::StratumId;
    use pg_shape::ShapeBuilder;

    fn shape(char_def: u32) -> Shape {
        let mut b = ShapeBuilder::new();
        b.push_segment(char_def);
        b.finish()
    }

    fn syn_fs(bits: u64) -> FeatureStruct {
        let mut b = FeatureStructBuilder::new();
        b.add(FeatId(0), FeatureValue::Symbolic(SymbolBits(bits)));
        b.build()
    }

    #[test]
    fn equal_values_come_back_as_the_first_allocation() {
        let values = SharedValues::default();
        let mut first = Word::new(shape(1), StratumId(0));
        first.syn_fs = syn_fs(1);
        let mut second = Word::new(shape(1), StratumId(0));
        second.syn_fs = syn_fs(1);
        assert!(!first.shape.same_allocation(&second.shape));
        values.share(&mut first);
        values.share(&mut second);
        assert!(first.shape.same_allocation(&second.shape));
        assert!(first.syn_fs.same_allocation(&second.syn_fs));
    }

    #[test]
    fn different_values_stay_apart() {
        let values = SharedValues::default();
        let mut first = Word::new(shape(1), StratumId(0));
        first.syn_fs = syn_fs(1);
        let mut second = Word::new(shape(2), StratumId(0));
        second.syn_fs = syn_fs(2);
        let (shape_before, fs_before) = (second.shape.clone(), second.syn_fs.clone());
        values.share(&mut first);
        values.share(&mut second);
        assert_eq!(second.shape, shape_before);
        assert_eq!(second.syn_fs, fs_before);
        assert!(!first.shape.same_allocation(&second.shape));
        assert!(!first.syn_fs.same_allocation(&second.syn_fs));
    }
}
