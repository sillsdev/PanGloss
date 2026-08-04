//! Deterministic per-prefix XML id minting: builders return XML fragments plus minted ids.
//! A plain monotonic counter per prefix -- no randomness involved -- so id
//! assignment depends only on CALL ORDER, which [`crate::render::render`] fixes deterministically
//! for a given recipe (never on [`crate::rng::Rng`] draws), keeping ids stable, human-legible, and
//! reproducible across re-renders of the same recipe.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct IdMinter {
    counters: HashMap<&'static str, u32>,
}

impl IdMinter {
    pub fn new() -> Self {
        IdMinter::default()
    }

    /// Mint the next id for `prefix` (e.g. `next("tbl")` -> `"tbl0"`, then `"tbl1"`, ...). Two
    /// different prefixes never collide (each has its own counter); the same prefix never repeats
    /// a number within one [`IdMinter`].
    pub fn next(&mut self, prefix: &'static str) -> String {
        let n = self.counters.entry(prefix).or_insert(0);
        let id = format!("{prefix}{n}");
        *n += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_are_independent_per_prefix() {
        let mut ids = IdMinter::new();
        assert_eq!(ids.next("tbl"), "tbl0");
        assert_eq!(ids.next("seg"), "seg0");
        assert_eq!(ids.next("tbl"), "tbl1");
        assert_eq!(ids.next("seg"), "seg1");
        assert_eq!(ids.next("tbl"), "tbl2");
    }
}
