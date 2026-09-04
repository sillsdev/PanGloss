# STAGING: final-template-partial-discriminators

## Why this fixture exists

Pins HermitCrab's two final-template gates in `SynthesisAffixProcessRule.cs`
(`MorphologicalRules/SynthesisAffixProcessRule.cs:63-104`), both of which fire only for an
**ordinary** rule (`AffixProcessRuleDef.is_template_rule == false`, i.e. never a member of any
`AffixTemplate` `Slot`) tried immediately after a template has just applied to the same word:

- **`NonPartialRuleProhibitedAfterFinalTemplate`** — a non-partial ordinary rule is refused right
  after an `AffixTemplate final="true"` template applied, unless the rule itself is partial.
- **`NonPartialRuleRequiredAfterNonFinalTemplate`** — a **partial** ordinary rule is refused right
  after an `AffixTemplate final="false"` template applied; only a non-partial rule may complete that
  derivation.

Per `EngineGateInventoryLedgerTests.UnreachedGatesAreTheKnownCorpusGaps` (in the `machine` checkout,
`machine/tests/SIL.Machine.Morphology.HermitCrab.Tests/SemanticCoverage/
EngineGateInventoryLedgerTests.cs`), **`NonPartialRuleRequiredAfterNonFinalTemplate` was, as of this
writing, one of only 6 gates the real corpus never fires at all** — the comment there names the exact
gap this fixture closes: "needs a `partial="true"` rule tried right after an `AffixTemplate
final="false"` template (no fixture combines the two)." This fixture is that combination.

The "rule tried right after a template" ordering that both gates need is itself only reachable via
the `Unordered` `ApplyTemplates -> ApplyMorphologicalRules` recursion in `SynthesisStratumRule.cs`
(the same finding `machine/conformance/edge-cases/morphotactic-attribute-breadth`'s own header
comment documents for its own `mrPO`/`Second` stratum) — a `Linear` stratum never gives a
stratum-listed rule that chance, so `Main` below is `Unordered`.

## What it pins

Four independently observable cases, split into two branches isolated by `PartsOfSpeech` (`posX`
vs. `posY`) so a mutation to one branch's construct can never change the other branch's words:

**Branch A (root `dak`, posX) — `NonPartialRuleProhibitedAfterFinalTemplate`:**

| Word | Verdict | Case |
|---|---|---|
| `dakfaga` | `expect_fail` | 1 (Blocked): `mrGate1` (ordinary, non-partial) tried right after `finalTemplateA` (`final="true"`) — refused. |
| `dakfa` | parses | Control: `finalTemplateA` alone, no `mrGate1` — proves the template path itself works. |
| `daknagafa` | parses | 2 (Control/Permitted): the SAME `mrGate1` tried right after `nonFinalTemplateA` (`final="false"`) instead — permitted, then `finalTemplateA` closes the derivation. |
| `dakna` | `expect_fail` | Negative control: `nonFinalTemplateA`'s own output alone, with no closing final template, is never a complete word (same finding as `morphotactic-attribute-breadth`'s `kulpu`). |

**Branch B (root `pil`, posY) — `NonPartialRuleRequiredAfterNonFinalTemplate`:**

| Word | Verdict | Case |
|---|---|---|
| `pilnbvbfb` | `expect_fail` | 3 (Blocked): `mrGate2Partial` (ordinary, `partial="true"`) tried right after `nonFinalTemplateB` — refused. |
| `pilnbtbfb` | parses | 4 (Control/Permitted): `mrGate2Complete` (ordinary, NOT partial) tried in the identical position — permitted, then `finalTemplateB` closes the derivation. |
| `pilnb` | `expect_fail` | Negative control, same reasoning as `dakna`. |
| `pilfb` | parses | Control: `finalTemplateB` alone, no Branch-B ordinary rule. |

Each gate's Blocked/Control pair (case 1/2 for gate one, case 3/4 for gate two) differs by exactly
one attribute, named in `words.yaml`'s own header note and re-verified by the red-on-revert runs
below.

## Oracle discipline

**Oracle: the C# founding oracle (`hc.dll`, via `hc-conformance.exe`'s in-process self-check).**
Signatures were first transcribed from a throwaway pg-parse test driving
`pg_parse::Morpher::parse_word` directly (HC-Rust; the throwaway file has been deleted), then
verified against the C# founding oracle by copying `grammar.xml` + that draft `words.yaml` into a
scratch fixtures tree and running:

```
& 'C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe' `
    --fixtures '<scratch>' --propose --capabilities ""
```

Verdict: **`[PASS] edge-cases/final-template-partial-discriminators` — 0 failed, 0 skipped, no
`--propose` patch printed.** HC-Rust and the C# founding oracle agree on every one of the 8 words;
no divergence was found. `words.yaml` carries `# oracle-provenance: founding-oracle
machine-commit=4823a05a65aaecf5fde23d713b47b398458a0140 verified-date=2026-09-04`.

## Verification

- `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs` discovers this fixture under
  `conformance-staging/edge-cases/` (via `pg_conformance_fixtures::discover`) and replays every word
  through `pg_parse::Morpher`, asserting the declared `expect_fail`/`parses[].signature` values —
  this is the gate that actually runs in the default `-Mode test` suite going forward.
- Red-on-revert, one mutation per gate, each reverted afterward:
  - **Gate 1**: `nonFinalTemplateA`'s `final="false"` flipped to `final="true"` makes
    `daknagafa` (previously a passing `parses` word) newly have zero analyses, because `mrGate1`
    (non-partial) is now tried right after what is, under the mutation, ALSO a final template —
    `NonPartialRuleProhibitedAfterFinalTemplate` fires. (`dakna` also flips, from `expect_fail` to
    parsing, as a direct side effect of the same mutation making `nonFinalTemplateA`'s own output
    final; both are reported together in the task transcript.)
  - **Gate 2**: `mrGate2Partial`'s `partial="true"` flipped to `partial="false"` makes `pilnbvbfb`
    (previously `expect_fail`) newly parse, because the rule is no longer partial and
    `NonPartialRuleRequiredAfterNonFinalTemplate` no longer has anything to refuse.

## Graduation

Not yet proposed upstream (no `sillsdev/machine` PR opened). Candidate destination:
`machine/conformance/edge-cases/final-template-partial-discriminators/` — same two files
(`grammar.xml`, `words.yaml`), already verified against the C# founding oracle above. On acceptance,
delete this staged copy in the same change (the graduation guard enforces this mechanically).
