//! Budgets for the cascade backend's product: named outcomes, charged before allocation (plan S.8, 3.5).
//!
//! Every charge is a check-then-add: when the charge would pass the limit nothing is added and the
//! dimension is returned, so the counters never exceed a limit and the caller has not allocated
//! anything yet. No budget truncates a worklist and carries on (mutations C20, C36).

/// The dimension that stopped a per-word product.
///
/// TODO(#419): these four names are 1:1 the `ApplyDimension::{ProductStates, ProductArcs,
/// ProductSteps, ProductBytes}` variants of PR #419. Replace this enum by that one once #419 is on
/// main; the product only ever builds and matches these four.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CascadeStop {
    ProductStates,
    ProductArcs,
    ProductSteps,
    ProductBytes,
}

/// Nominal bytes, not allocator bytes: fixed figures so the same word gives the same outcome on any
/// machine (3.5). A state costs its key (twice, table plus index), a hash entry and its record.
pub const STATE_FIXED_BYTES: u64 = 64;
pub const ARC_BYTES: u64 = 32;
pub const TAGSET_FIXED_BYTES: u64 = 16;
pub const QUEUE_ENTRY_BYTES: u64 = 24;

/// Bytes charged for one product state of `key_words` u32 words.
pub const fn state_bytes(key_words: usize) -> u64 {
    STATE_FIXED_BYTES + 8 * key_words as u64
}

/// Bytes charged for one state interned by a lazy layer's scratch of `key_words` words: the key
/// twice (table and index) and a hash entry.
pub const fn interned_bytes(key_words: usize) -> u64 {
    INTERNED_FIXED_BYTES + 8 * key_words as u64
}

pub const INTERNED_FIXED_BYTES: u64 = 48;

/// Bytes charged for one interned tag set of `len` tags.
pub const fn tagset_bytes(len: usize) -> u64 {
    TAGSET_FIXED_BYTES + 4 * len as u64
}

/// Ids are `u32` and `u32::MAX` is a sentinel, so no table may hold more entries than this, whatever
/// the limits say (`WordBudget::UNLIMITED` included).
pub const ID_LIMIT: u64 = u32::MAX as u64 - 1;

const MIB: u64 = 1 << 20;

/// Per-word limits of the main pass (S.8 table, one column per edit limit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordBudget {
    pub states: u64,
    pub arcs: u64,
    pub steps: u64,
    pub bytes: u64,
}

impl WordBudget {
    pub const UNLIMITED: WordBudget = WordBudget {
        states: u64::MAX,
        arcs: u64::MAX,
        steps: u64::MAX,
        bytes: u64::MAX,
    };

    /// The S.8 defaults for an edit limit of `max_edits` (0, 1, 2 and above).
    pub fn for_edits(max_edits: u8) -> Self {
        match max_edits {
            0 => WordBudget {
                states: 1_000_000,
                arcs: 8_000_000,
                steps: 50_000_000,
                bytes: 256 * MIB,
            },
            1 => WordBudget {
                states: 2_000_000,
                arcs: 16_000_000,
                steps: 150_000_000,
                bytes: 512 * MIB,
            },
            _ => WordBudget {
                states: 4_000_000,
                arcs: 32_000_000,
                steps: 400_000_000,
                bytes: 1024 * MIB,
            },
        }
    }
}

/// Limits of one pre-pass attempt. Never charged to the word; an overrun shortens the suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrepassBudget {
    pub states: u64,
    pub steps: u64,
    pub bytes: u64,
}

impl Default for PrepassBudget {
    /// S.8: 600,000 states / 15,000,000 steps / 128 MiB per attempt.
    fn default() -> Self {
        PrepassBudget {
            states: 600_000,
            steps: 15_000_000,
            bytes: 128 * MIB,
        }
    }
}

/// Running counters against a [`WordBudget`]. Also used (with `arcs` unlimited) for a pre-pass attempt.
#[derive(Clone, Debug)]
pub struct WordMeter {
    limits: WordBudget,
    pub states: u64,
    pub arcs: u64,
    pub steps: u64,
    pub bytes: u64,
}

fn over(used: u64, add: u64, limit: u64) -> bool {
    used.saturating_add(add) > limit
}

impl WordMeter {
    pub fn new(limits: WordBudget) -> Self {
        WordMeter {
            limits,
            states: 0,
            arcs: 0,
            steps: 0,
            bytes: 0,
        }
    }

    pub fn for_prepass(b: PrepassBudget) -> Self {
        Self::new(WordBudget {
            states: b.states,
            arcs: u64::MAX,
            steps: b.steps,
            bytes: b.bytes,
        })
    }

    pub fn limits(&self) -> WordBudget {
        self.limits
    }

    pub fn charge_steps(&mut self, n: u64) -> Result<(), CascadeStop> {
        if over(self.steps, n, self.limits.steps) {
            return Err(CascadeStop::ProductSteps);
        }
        self.steps += n;
        Ok(())
    }

    /// Charge `bytes` (queue entries, tag sets, buffers) before they are allocated.
    pub fn charge_bytes(&mut self, n: u64) -> Result<(), CascadeStop> {
        if over(self.bytes, n, self.limits.bytes) {
            return Err(CascadeStop::ProductBytes);
        }
        self.bytes += n;
        Ok(())
    }

    /// One new state of `bytes` bytes: the states limit first, then the bytes limit; nothing is
    /// added unless both pass.
    pub fn charge_state(&mut self, bytes: u64) -> Result<(), CascadeStop> {
        if over(self.states, 1, self.limits.states.min(ID_LIMIT)) {
            return Err(CascadeStop::ProductStates);
        }
        if over(self.bytes, bytes, self.limits.bytes) {
            return Err(CascadeStop::ProductBytes);
        }
        self.states += 1;
        self.bytes += bytes;
        Ok(())
    }

    /// One new product arc (a merge into an existing arc is not an arc and is not charged here).
    pub fn charge_arc(&mut self) -> Result<(), CascadeStop> {
        if over(self.arcs, 1, self.limits.arcs.min(ID_LIMIT)) {
            return Err(CascadeStop::ProductArcs);
        }
        if over(self.bytes, ARC_BYTES, self.limits.bytes) {
            return Err(CascadeStop::ProductBytes);
        }
        self.arcs += 1;
        self.bytes += ARC_BYTES;
        Ok(())
    }
}

/// The build-time dimension that refused a layer or a gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildStop {
    BuildBytes,
    BuildSteps,
}

/// `Refused { ConstructionBudget }` naming the layer or step (3.1, 3.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRefused {
    pub dim: BuildStop,
    pub what: String,
}

/// `BuildBytes` and `BuildSteps` for one build (defaults 4 GiB and 2,000,000,000 unit operations).
#[derive(Clone, Debug)]
pub struct BuildMeter {
    bytes_limit: u64,
    steps_limit: u64,
    bytes: u64,
    steps: u64,
}

impl BuildMeter {
    pub fn new(bytes_limit: u64, steps_limit: u64) -> Self {
        BuildMeter {
            bytes_limit,
            steps_limit,
            bytes: 0,
            steps: 0,
        }
    }

    pub fn default_limits() -> Self {
        Self::new(4096 * MIB, 2_000_000_000)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Charged BEFORE the allocation it pays for; on refusal nothing is added.
    pub fn charge_bytes(&mut self, n: u64, what: &str) -> Result<(), BuildRefused> {
        if over(self.bytes, n, self.bytes_limit) {
            return Err(BuildRefused {
                dim: BuildStop::BuildBytes,
                what: what.to_string(),
            });
        }
        self.bytes += n;
        Ok(())
    }

    pub fn charge_steps(&mut self, n: u64, what: &str) -> Result<(), BuildRefused> {
        if over(self.steps, n, self.steps_limit) {
            return Err(BuildRefused {
                dim: BuildStop::BuildSteps,
                what: what.to_string(),
            });
        }
        self.steps += n;
        Ok(())
    }
}
