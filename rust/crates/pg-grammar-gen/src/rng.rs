//! In-house SplitMix64 (design doc §2: "no `rand` dependency for a dev tool"). Not cryptographic,
//! not published as its own crate -- just a small, fast, well-mixed, deterministic bit source
//! seeded from `hash(name, seed)` so [`crate::render::render`] can stay a pure function of the
//! recipe alone (that module's own determinism contract).

/// A SplitMix64 generator (Vigna & Steele's public-domain 64-bit mixing recurrence -- the same
/// one, e.g., `java.util.SplittableRandom`'s seeding step uses -- chosen here purely for "fast,
/// well-mixed, zero external crates" properties, not for any cryptographic guarantee stage 1's
/// tiny fixed recipes don't need).
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Seed from a recipe's own `(name, seed)` pair (module doc): FNV-1a-mix `name`'s bytes into
    /// `seed` first, so two recipes sharing a `seed` but differing in `name` still start from
    /// different states, then treat the result as the SplitMix64 initial state.
    pub fn seeded(name: &str, seed: u64) -> Self {
        let mut state = seed ^ 0xcbf29ce484222325;
        for &b in name.as_bytes() {
            state ^= b as u64;
            state = state.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Rng { state }
    }

    /// One SplitMix64 step (state advance + output mix) -- the next 64 pseudo-random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound` (`bound` must be `> 0`) via `next_u64() % bound`. Stage-1 recipes
    /// only ever draw from tiny bounds (a handful of segment letters), so the small modulo bias
    /// this introduces is not worth a rejection-sampling loop; revisit if a stage-2 builder needs
    /// a larger, bias-sensitive draw.
    pub fn gen_below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "Rng::gen_below: bound must be > 0");
        (self.next_u64() % bound as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_name_and_seed_reproduce_the_same_stream() {
        let mut a = Rng::seeded("recipe-a", 42);
        let mut b = Rng::seeded("recipe-a", 42);
        let seq_a: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn different_name_or_seed_diverges() {
        let mut a = Rng::seeded("recipe-a", 42);
        let mut b = Rng::seeded("recipe-b", 42);
        let mut c = Rng::seeded("recipe-a", 43);
        assert_ne!(a.next_u64(), b.next_u64());
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn gen_below_stays_in_bounds() {
        let mut r = Rng::seeded("bounds", 7);
        for _ in 0..100 {
            assert!(r.gen_below(5) < 5);
        }
    }
}
