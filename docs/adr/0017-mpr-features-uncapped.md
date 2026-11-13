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
- Bit 63 set: the other bits are an index into a process-wide interner of word vectors
  (`mpr_wide.rs`). Equal sets intern to the same handle, so `==` and `Hash` stay handle
  comparisons. Only grammars with 64 or more MPR features ever reach this path, and the
  slow paths are cold functions.

`MprId` is now `u16` (65,536 features; a grammar past that gets `GrammarError::Unsupported`,
not a silent wrap). The compile cap, the load cap and the `grammar.limit.mpr-features`
diagnostic are removed.

Serde is compatible: an inline set serialises as the bare number it always did; a wide set
serialises as an array of words. Old stored analyses read back unchanged.

## Why not the alternatives

- A plain `u128` raises the cap to 128 and grows `Word` past its size guard
  (`MAX_WORD_BYTES`); it still refuses grammars.
- `Arc<[u64]>` or a `Vec` loses `Copy` and costs a clone on the hot path.
- Widening `Word` to carry a `SmallVec` was measured to fail the size guard.

## The interner is a process-global exception

PanGloss avoids parser-side caches unless they are provably small. This table is a
conscious exception, because a `Copy` set cannot reach a per-grammar table without carrying a
handle to it.

- It is bounded in practice. Only grammars with 63 or more MPR features ever touch it, and
  those grammars were refused before this change. A set is added only when a new distinct
  wide set appears, and the sets that appear are unions of a grammar's own entry, rule and
  group sets, not of its words, so a second pass over the same words adds none. A test parses
  the 140-feature fixture twice and asserts the count is unchanged
  (`MprSet::interned_wide_sets`). Measured: 9.3 MB and 12.1 MB resident for 70 and 140
  features.
- Reads copy nothing: wide `contains`, `iter` and the set operations run inside the read lock
  on the stored list.
- It is not freed, and it is shared across every grammar in the process. A host that loads
  many different wide grammars, or parses against a long-lived process, only grows it.
- Deserialising interns too: reading a stored wide set (an array of words) adds its list, so
  untrusted stored data can add entries. Inline sets never touch the table.
- Follow-up: a per-grammar interner (generation or epoch reset). Drafted in the PR thread, not
  yet filed.

## Consequences

The 64th feature changes representation, because bit 63 is the tag; ids 0..=62 are inline.
Parser `work_steps` are identical and wall time and memory are within noise on the sample
grammars. The parts-of-speech and feature-value caps (63 each) are separate bitsets and stay
as follow-ups.
