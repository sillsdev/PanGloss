# 078: XAMPLE minimum: authored phonology outranks XAMPLE

Kind: behavioural (XAMPLE minimum and unreadable literal environment).
Status: deliberate divergence under [ADR 0008](../adr/0008-provisional-definitions.md).

## Contract and independent justification

Rules the author wrote apply even though XAMPLE ignores phonological rules. PanGloss does
not retry a word with its rules disabled. ADR 0008 also supplies a provisional literal for
an undefined environment letter: the authored restriction continues to bind. FieldWorks'
XAMPLE export and C# HCLoader discard that unreadable literal environment instead, which
widens the suffix's distribution. This differs from a nonexistent natural class, whose
whole-environment drop is retained and reported in [077](077-missing-natural-class-environment-info.md).

The XAMPLE stored-analysis minimum remains unconditional except for seven explicitly
reviewed rows. Each per-case `xample-minimum-exceptions.json` names this entry, the case,
word, exact lost ordered allomorph/MSA/inflection-type key, multiplicities and reason.
An unlisted loss fails; a listed loss that disappears also fails. No native expectation
is rewritten to match PanGloss.

| Case | Word | Why the XAMPLE key is absent in PanGloss |
|---|---|---|
| `08-rule-context` | `muma` | Deleting the structured context reference leaves an unconditional authored m→p rule; forward synthesis produces `pupa`. |
| `08-rule-context` | `xuma` | The same rule changes the root's m; forward synthesis produces `xupa`. |
| `08-rule-context` | `xmuma` | The same rule changes both occurrences of m; forward synthesis produces `xpupa`. |
| `12-featureless-rule-class` | `muma` | Authored m→p after V=[voc +] applies after u; forward synthesis produces `mupa`. |
| `12-featureless-rule-class` | `xuma` | This root contains m after u; forward synthesis produces `xupa`, regardless of featureless x's exclusion from V. |
| `12-featureless-rule-class` | `xmuma` | The m after u changes while initial x is excluded from V; forward synthesis produces `xmupa`. |
| `10-undefined-environment-letter` | `mumas` | Provisional literal x retains suffix s's `/ x _` restriction. The preceding a fails it; both native loaders discard the unreadable environment and admit this key. |

The seventh row also belongs in the C# difference table: its captured HC root-plus-suffix
key is absent in PanGloss because the literal environment is preserved. This is an
owner-authorized definition policy, not a newly reported defect in the founding oracle.
The provisional-definition discussion is already open at
[Machine #537](https://github.com/sillsdev/machine/issues/537); no new upstream post or
claim about its current disposition is made from this network-closed lane.

## Evidence, gate and comparison limits

The twelve saved cases in `conformance-staging/underdefined/` retain native XAMPLE and
C# HC outputs, installed binary hashes/versions, source-project hashes and control/off/on
projects. PanGloss replays the OFF projects through the production fwdata importer,
compiler and parse owner's source-identity projection. Native engines were not rerun in
the Linux integration lane. The authored grammar and saved-project mutation, not PanGloss
output, justify the seven exceptions above.

`pg-cli/tests/underdefined_stored_keys_gate.rs` checks all 61 words. The XAMPLE minimum remains
stored-key based; its keys do not contain root position or category. The C# comparison uses
`underdefined-structured-identity-and-status/v2`: ordered stable morpheme identities, root
position, category/POS, captured HC errors and PanGloss invalid-shape status.

The 36 saved HC XML files were replayed through the pinned Machine 3.8.2.0 assemblies. Their
stored-key multisets and statuses agree with the original C# captures for all 183 case/state/word
rows. Sidecars preserve root position and category, with DLL, harness, input and saved-project
hashes plus line evidence for source-ID crosswalks. All 34 C# differences were re-derived under v2
and remain covered by their existing ledger entries. Unlisted and disappeared differences fail;
empty tables assert none.

Category/POS in each C# sidecar is the stable FieldWorks PartOfSpeech GUID. The capture records
the HC symbol and saved-project Name and Abbreviation lines used for its unique crosswalk, and the
gate verifies that evidence against the pinned input hashes.

The lane report records actual managed commands, exits and table-removal regression checks
separately from the native captures' provenance.
