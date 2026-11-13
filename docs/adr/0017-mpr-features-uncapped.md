---
status: accepted
---

# MPR features have no cap

Owner decision on #150: PanGloss must not refuse a grammar for its number of MPR features
(inflection classes, exception features, entry inflection types). FieldWorks allows any
number and `hc.dll` has no cap; a Cebuano grammar reached the old limit of 64.

## Decision

`MprSet` is one tagged `u64` that stays `Copy` and 8 bytes, so `Word` keeps its size and the
hot path is unchanged.

- Bit 63 clear: the other 63 bits are MPR ids 0..=62 inline, exactly as before. Every set
  operation takes a branch on the tag and does the old single-word operation.
- Bit 63 set: the other bits are `generation << 38 | index`. The generation names a table; the
  index names a canonical word list in it (`mpr_wide.rs`). Only grammars with 63 or more MPR
  features ever make one, and the slow paths are cold functions.

Each `Grammar` owns an `MprTable` (`Grammar::mpr_table`). Dropping the grammar removes its
generation from the registry and frees every list in it, so a host that loads and drops
grammars retains nothing. A wide operation resolves a handle by its own generation, so a set
needs no pointer to its table; a new set takes the generation of a wide operand. The only
sets with no wide operand are built while the compiler or loader runs, which enter the
grammar's table as the thread's current one (`MprTable::enter`).

Equality and hashing of a wide set are by content, with a handle comparison as the fast path,
so sets from two tables (two grammars, or a stored set read back) still compare equal.
Predicates (`overlaps`, `is_subset_of`, `mpr_required_ok`, `mpr_excluded_ok`) read operand
words in place and never intern.

`MprId` is `u16` (65,536 features; a grammar past that gets `GrammarError::Unsupported`, not
a silent wrap). The compile cap, the load cap and the `grammar.limit.mpr-features`
diagnostic are removed.

Serde is compatible: an inline set serialises as the bare number it always did; a wide set
serialises as an array of words. Old stored analyses read back unchanged.

## Why not the alternatives

- A plain `u128` raises the cap to 128 and grows `Word` past its size guard
  (`MAX_WORD_BYTES`); it still refuses grammars.
- `Arc<[u64]>` or a `Vec` loses `Copy` and costs a clone on the hot path.
- Widening `Word` to carry a `SmallVec` was measured to fail the size guard.
- One process-global interner (the first version of this change) never frees and is shared by every
  grammar in a host that loads several; replaced after #291.

## What is still process-wide

- Generation 0 is an ambient table, never freed, for a wide set built with no wide operand and
  no grammar in scope: a stored wide set read back outside any grammar, or a legacy stored
  number with bit 63 set (member 63). Inline sets never touch it. Deserialisation is capped
  there (1,024 distinct sets, 256 KiB of lists): a repeat is found, not added, and a new set
  past the cap is a deserialisation error, so untrusted input cannot grow it without bound.
  A deserialiser running inside a grammar's scope builds into that grammar's table instead.
- Deserialising rejects a word list longer than 1024 words, so stored data cannot name an id
  beyond `u16`.
- A wide set that outlives its grammar cannot be read: the operation panics with a message
  naming the cause, and `Debug` prints `MprSet(<table dropped>)`.
- One registry lock covers every table. Reads (`contains`, set operations, predicates) take
  it shared; a write happens when a new distinct wide set is added or a table is created or
  dropped, and a grammar's table stops growing once its words have been seen.

## Consequences

The 64th feature changes representation, because bit 63 is the tag; ids 0..=62 are inline.
Parser `work_steps` are identical and wall time and memory are within noise on the sample
grammars. A table holds 0.10 MB at 70 features and 0.41 MB at 140 after parsing 3,000 words, and is freed with its grammar. The table has no eviction and grows with the distinct wide sets a grammar's derivations reach; `MprTable::retained_bytes` reports it. The parts-of-speech and feature-value caps (63 each) are separate bitsets and stay
as follow-ups.
