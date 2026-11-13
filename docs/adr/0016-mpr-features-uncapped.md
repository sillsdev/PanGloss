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

## Consequences

The 64th feature changes representation, because bit 63 is the tag; ids 0..=62 are inline.
Parser `work_steps` are identical and wall time and memory are within noise on the sample
grammars. The parts-of-speech and feature-value caps (63 each) are separate bitsets and stay
as follow-ups.
