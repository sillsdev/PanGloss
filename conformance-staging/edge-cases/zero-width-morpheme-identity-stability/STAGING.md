# STAGING: zero-width-morpheme-identity-stability

## Why this fixture exists

Machine issue #506: root `sag` (N) with two toggle rules (`mrA2B`: N->V, +u; `mrB2A`: V->N, +i) plus
a zero-width rule (`mrThird`: N->V, `CopyFromInput`-only, no inserted segments) produces an unstable
parse of `sagui` under `hc.dll` -- fresh-process runs disagree with each other, and most of them drop
a morpheme a correct engine cannot drop (forward synthesis proves `sag/N --A2B(+u)--> sagu/V
--B2A(+i)--> sagui/N` is a valid 2-morph analysis, and stacking `mrThird` on that same `sagui/N` is a
valid 3-morph analysis containing all of `ROOT`, `A2B`, `B2A`, `THIRD`). This directory is a
byte-identical copy of `grammar.xml`/`words.yaml` from the (read-only, not committed here) Machine
oracle-506 worktree investigation, staged so PanGloss's own conformance replay
(`conformance_fixtures_gate`) covers this reproduction immediately rather than waiting on upstream
acceptance.

## What it pins

Four words: `sag`/`sagu`/`sagi` are stable, oracle-recorded controls (each rule composes correctly in
isolation or with a following rule); `sagui` is the bug pin itself, and is the one entry that
disagrees with `hc.dll` on purpose -- see `words.yaml`'s header for the full derivation, the exact
measured hc.dll splits (three different outputs across 50 fresh processes, in two independent
samples), and Machine PR #500's real-but-partial effect (fixes the THIRD-vs-B2A ordering in 20/50
runs on its head, but does not fix the content-dropping and does not make the result deterministic).

## PanGloss-side finding (2026-09-15): no code change needed

`words.yaml`'s own header already states the forward-synthesized expectation. What PanGloss's repo
adds here is the direct measurement against that exact grammar: `pg_parse::Morpher::parse_word` was
run 50 times in-process over every word in this fixture, plus 20 separate fresh-process invocations of
the compiled probe binary for `sagui` specifically. Every run, in every process, produced:

```
sag:   ROOT|sag ; ROOT+THIRD|sag
sagu:  ROOT+A2B|sagu
sagi:  ROOT+THIRD+B2A|sagi
sagui: ROOT+A2B+B2A|sagui ; ROOT+A2B+B2A+THIRD|sagui
```

-- an exact match to every `parses:` entry below, including `sagui`'s oracle-disagreeing answer, with
THIRD correctly rendered as the OUTERMOST (rightmost) morph. No PanGloss production code was changed
to get this result; it was already the engine's behavior before this investigation started. See
`docs/divergences/036-zero-width-morpheme-identity.md`'s 2026-09-15 section for the full measurement
and the mechanism: `pg-rules/src/morph.rs::attribute_morphs` records a zero-width affix (one that
contributes no output-side material of its own) as a `MorphStatus::Floating` marker at the
`FLOATING_ORDER` sentinel (`u32::MAX`), which sorts after every real, positioned morph in
`pg-parse/src/morpher.rs::allomorphs_in_morph_order`'s `sort_by_key(|m| m.order)` -- so a zero-width
rule anchored at the shape's last node already renders after the affix occupying that node, with no
insertion-order special case required. This is the general rule the task that produced this fixture
asked for ("a zero-width morph anchored at the last node renders after the affix occupying that
node"), and PanGloss already implements it.

Also confirmed: PanGloss is deterministic here, in-process and cross-process, unlike `hc.dll` (see
`words.yaml`'s header for the C# instability measurement). `rustc_hash::FxHashMap`'s fixed (not
per-process-randomized) hasher is a plausible structural reason PanGloss does not reproduce C#'s
`string.GetHashCode()`-driven bucket-order instability, though this fixture does not attempt to prove
that causally -- only the absence of the instability itself, empirically, is claimed.

## Oracle discipline

`oracle-provenance: mixed` in `words.yaml`, per the source fixture: `sag`/`sagu`/`sagi` are
hc.dll-recorded; `sagui` is forward-synthesis-derived and deliberately RED against `hc.dll`, per the
"when you can prove the oracle is wrong" escape hatch in `.claude/skills/conformance-grammars/
SKILL.md`. The addendum this repo added to `words.yaml`'s header records that PanGloss's own replay
is GREEN against every word here, including `sagui` -- that RED-against-C#/GREEN-against-PanGloss
asymmetry is the fixture's whole point.

## Verification

Measured via a throwaway test (`rust/crates/pg-parse/tests/zw_probe_throwaway.rs`, deleted after
transcription) driving `pg_parse::Morpher::parse_word` directly, plus 20 separate fresh-process runs
of the compiled test binary. The permanent pin is
`rust/crates/pg-parse/tests/zero_width_morph_identity.rs`, which loads this staged grammar via
`pg_conformance_fixtures::require_fixture` and asserts the exact signature multiset (not just a set
or a count) for all four words.

## Graduation

Not yet proposed upstream as its own PR (issue #506 is open; Machine PR #500 is the existing partial
fix). Candidate destination: `machine/conformance/edge-cases/
zero-width-morpheme-identity-stability/`. On acceptance, delete this staged copy in the same change
(graduation guard enforces this mechanically).
