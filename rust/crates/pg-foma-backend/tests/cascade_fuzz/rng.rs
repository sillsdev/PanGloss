//! Seeded draws on top of the workspace's SplitMix64.

use pg_grammar_gen::rng::Rng as Mix;

pub struct Rng(Mix);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(Mix::seeded("cascade-fuzz", seed))
    }

    pub fn below(&mut self, bound: usize) -> usize {
        self.0.gen_below(bound.max(1))
    }

    pub fn range(&mut self, lo: usize, hi_inclusive: usize) -> usize {
        lo + self.below(hi_inclusive - lo + 1)
    }

    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}
