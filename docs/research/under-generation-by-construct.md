# What the four under-generating fixtures are actually missing

`faithfulness_coverage_gate` reports 19 (construct, backend) containment failures — every one
`proposal set offered 0` against an oracle identity. They reduce to four (fixture, word) causes, and
this file says what each one is **linguistically**, because "19 failures" is a number and "this
backend does not apply feature-polarity rules" is a decision someone can act on.

Each is a statement about a CAPABILITY, not a bug in a fixture. The fixtures are correct; they were
written to discriminate exactly these constructs, and each carries its own oracle transcript.

## 1. Feature-polarity (alpha) rules — templated proposes nothing

`machine:edge-cases/feature-system-breadth`, word `isk`, oracle identity `morphemes=[6]`.

The lexical entry `eAsk` has the shape **`ask`**. Rule `prAlpha` fires with `polarity="minus"` and
flips the vowel's height before `s`, so /ask/ surfaces as **[isk]**. The fixture's own words.yaml
states the unflipped `ask` is *not* a surface word.

So the analysis exists only through the alpha rule. If a backend does not apply feature-polarity
phonology, there is nothing for it to propose — not a partial answer, none. Note the identity is a
BARE ROOT: this is not an affixation gap, it is the phonology.

Likely the same family as the divergence row `alpha-variable-name-collision x
templated-underlying-tokens`, which fails with "no phonological rule compiled".

## 2. Unordered-stratum MPR overwrite order — templated proposes nothing

`machine:edge-cases/mpr-overwrite-order-dependence`, word `daboyuxa`, oracle identity
`morphemes=[6, 1, 0]`.

The fixture pins one consequence of `outputType="overwrite"`: on an **unordered** stratum, two rules
that both write the same overwrite group may apply in either relative order, and the group's final
content depends on which wrote LAST. A third rule gated on `requiredMPRFeatures` naming one of the
two members therefore succeeds under one derivation order and fails under the reverse — the same
morpheme set, two different outcomes, attributable only to firing order.

A proposer that explores one order proposes one of the two analyses. Getting this right means
exploring both, which is a real construction question rather than a missing check.

## 3. An explicit boundary character in the queried surface — the surface probe proposes nothing

`machine:edge-cases/loader-isactive-breadth`, word `mo+kul`, oracle identity `morphemes=[3, 4]`,
`root_index=1`.

`mrBoundaryPfx`'s own output **inserts the live boundary character `+`**, so the surface form
legitimately contains it. The surface probe offers nothing for that word.

This is the same fact as a coverage gap recorded separately: no FST backend has a test that its query
encoder accepts a surface containing an explicit boundary character
(`conformance-containment-inventory.md`, the note under the deleted
`templated_query_accepts_a_surface_with_an_explicit_boundary`). That deletion looked like a lost
test; it is the same defect this row measures, and this fixture is the fixture the coverage note
identified as the only representable host for it.

## 4. `morphotactic-attribute-breadth` / `kuldede`

Under separate investigation with the plan-composed backend work — the same fixture also carries two
of the plan-composed divergence rows. Its oracle identity is `morphemes=[13, 1]` and both
tuned-surface and templated offer 0.

## Why this framing matters

Three of the four name a construct the backend does not implement, not a check it forgot to run. That
distinguishes them sharply from the divergence inventory in
`conformance-containment-inventory.md`, where the compiler DOES refuse and the only question is
whether the envelope said so first. Here the compiler builds a network and the network is short of
analyses, which is the failure ADR-0001 calls "silent overapproximation-that-loses" and the reason
the propose-then-confirm pipeline keeps a full-HC oracle at all.

The honest options per row are the same two ADR-0001 gives: implement the construct, or refuse the
grammar at the capability envelope naming it. What is not an option is the current state, where the
backend is admitted and quietly returns fewer analyses than the grammar licenses.
