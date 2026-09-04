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

Six independently observable cases, split into three branches isolated by `PartsOfSpeech` (`posX`,
`posY`, `posZ`) so a mutation to one branch's construct can never change another branch's words:

**Branch A (root `dak`, posX) — `NonPartialRuleProhibitedAfterFinalTemplate`:**

| Word | Verdict | Case |
|---|---|---|
| `dakfaga` | `expect_fail` | 1 (Blocked): `mrGate1` (ordinary, non-partial) tried right after `finalTemplateA` (`final="true"`) — refused. |
| `dakfa` | parses | Control: `finalTemplateA` alone, no `mrGate1` — proves the template path itself works. |
| `daknagafa` | parses | 2 (Control/Permitted): the SAME `mrGate1` tried right after `nonFinalTemplateA` (`final="false"`) instead — permitted, then `finalTemplateA` closes the derivation. |
| `dakna` | `expect_fail` | Negative control: `nonFinalTemplateA`'s own output alone, with no closing final template, is never a complete word (same finding as `morphotactic-attribute-breadth`'s `kulpu`). |

**Branch B (root `pil`, posY) — `NonPartialRuleRequiredAfterNonFinalTemplate`, plus the fixture's
first POSITIVE, partial-dependent word:**

| Word | Verdict | Case |
|---|---|---|
| `pilnbvbfb` | `expect_fail` | 3 (Blocked): `mrGate2Partial` (ordinary, `partial="true"`) tried right after `nonFinalTemplateB` — refused. |
| `pilnbtbfb` | parses | 4 (Control/Permitted): `mrGate2Complete` (ordinary, NOT partial) tried in the identical position — permitted, then `finalTemplateB` closes the derivation. |
| `pilnb` | `expect_fail` | Negative control, same reasoning as `dakna`. |
| `pilfb` | parses | Control: `finalTemplateB` alone, no Branch-B ordinary rule. |
| `pilfbvb` | parses | 5 (POSITIVE): `mrGate2Partial` tried right after `finalTemplateB` instead — the mirror of `dakfaga` with a *partial* rule in `mrGate1`'s position. `NonPartialRuleProhibitedAfterFinalTemplate` does not fire because the rule itself is partial, so — unlike `dakfaga` — this one parses. |

**Branch C (root `nib`, posZ) — the OTHER arm of `SynthesisAffixTemplatesRule.cs`'s line-59
pass-through (a partial word survives an applicable-but-empty template battery; a non-partial word
dies with `ApplicableTemplatesNotApplied`):**

| Word | Verdict | Case |
|---|---|---|
| `nibgi` | parses | 6 (POSITIVE): `mrGate3Partial` (ordinary, `partial="true"`) makes the word partial; `templateZ` is applicable to `posZ` but its one mandatory slot's rule (`mrZTagSlot`) can never match, so the template battery yields zero output. Because the word is partial, line 59's `!input.IsPartial && applicableTemplate` is false, so the word passes through unchanged instead of dying. |

Each gate's Blocked/Control pair (case 1/2 for gate one, case 3/4 for gate two) differs by exactly
one attribute, named in `words.yaml`'s own header note and re-verified by the red-on-revert runs
below. Cases 5 and 6 are the fixture's positive, partial-dependent parses: before they existed, no
word in this fixture had a parse that existed *only because* a rule was partial, so an FST backend
that silently under-proposes on partial-bearing grammars would lose exactly these two parses and no
gate here would have noticed. Both also have their own red-on-revert (below).

### Branch C's construction: keeping the template applicable while its slot yields nothing

Line 59 of `SynthesisAffixTemplatesRule.cs` is `if (!input.IsPartial && applicableTemplate)`, where
`applicableTemplate` is set once a template's required POS/syntactic-feature-struct unify with the
input and `RuleSelector` accepts it — independent of whether the template's own slot rule(s)
actually match. Reaching this line's ELSE arm (the pass-through) therefore needs a template that
stays *applicable* while its slot's rule structurally cannot fire on the word it's tried against.

`templateZ`'s one mandatory slot (`mrZTagSlot`, no `optional="true"`, so its failure empties the
whole template's output) requires its `MorphologicalInput` to end in `ncOnlyK`, a `SegmentNaturalClass`
whose only member is the segment "k". Neither the bare root `nib` (ends in "b") nor `mrGate3Partial`'s
own output `nibgi` (ends in "i") ends in "k", so `mrZTagSlot` never matches either input state the
template is tried against — while `templateZ` itself stays applicable throughout, since its
POS/feature match never depended on the slot rule succeeding.

One earlier attempt at this same construction used an UNDEFINED phoneme ("zz", not declared in the
`CharacterDefinitionTable`) as `mrZTagSlot`'s never-produced output shape. HC-Rust tolerated it
(the rule never actually applies, so the invalid literal is never reached at runtime), but the C#
founding oracle's *loader* validates every `InsertSegments` shape at grammar-load time regardless of
whether the rule can ever fire, and refused the whole file: `engine crashed: The shape, zz, contains
an undefined phoneme at 0.` This was caught before committing by running the oracle, not by
inspecting the grammar, and is exactly the class of HC-Rust-vs-hc.dll divergence this repo's
`CLAUDE.md` treats as a hard rule (see its "oracle hierarchy" section) — the fix was to change the
unreachable output shape to `"kv"` (both segments declared), which changes nothing about the
mechanism (the rule still never applies) and the oracle now accepts.

## Oracle discipline

**Oracle: the C# founding oracle (`hc.dll`, via `hc-conformance.exe`'s in-process self-check).**
The original eight words' signatures were first transcribed from a throwaway pg-parse test driving
`pg_parse::Morpher::parse_word` directly (HC-Rust; the throwaway file has been deleted), then
verified against the C# founding oracle. `pilfbvb` and `nibgi` (added later, see "Branch C's
construction" above) were derived directly from a close reading of `SynthesisAffixProcessRule.cs`/
`SynthesisAffixTemplatesRule.cs`/`SynthesisStratumRule.cs` and then confirmed against the oracle,
not the other way around. All ten words are verified by copying `grammar.xml` + `words.yaml` into a
scratch fixtures tree and running:

```
& 'C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe' `
    --fixtures '<scratch>' --propose --capabilities ""
```

Verdict: **`[PASS] edge-cases/final-template-partial-discriminators` — 0 failed, 0 skipped, no
`--propose` patch printed.** HC-Rust and the C# founding oracle agree on every one of the 10 words;
no divergence was found. `words.yaml` carries `# oracle-provenance: founding-oracle
machine-commit=4823a05a65aaecf5fde23d713b47b398458a0140 verified-date=2026-09-04`.

## Verification

- `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs` discovers this fixture under
  `conformance-staging/edge-cases/` (via `pg_conformance_fixtures::discover`) and replays every word
  through `pg_parse::Morpher`, asserting the declared `expect_fail`/`parses[].signature` values —
  this is the gate that actually runs in the default `-Mode test` suite going forward.
- Red-on-revert, one mutation per gate/case, each run against the C# founding oracle (not just
  HC-Rust) and reverted afterward, restore confirmed by re-reading the file:
  - **Gate 1**: `nonFinalTemplateA`'s `final="false"` flipped to `final="true"` makes
    `daknagafa` (previously a passing `parses` word) newly have zero analyses, because `mrGate1`
    (non-partial) is now tried right after what is, under the mutation, ALSO a final template —
    `NonPartialRuleProhibitedAfterFinalTemplate` fires. (`dakna` also flips, from `expect_fail` to
    parsing, as a direct side effect of the same mutation making `nonFinalTemplateA`'s own output
    final; both are reported together in the task transcript.)
  - **Gate 2 / Case 5**: `mrGate2Partial`'s `partial="true"` flipped to `partial="false"` was
    re-run against the oracle for this fixture's current (ten-word) shape. Oracle output:
    ```
    [FAIL] edge-cases/final-template-partial-discriminators (233ms) 2/10 word(s) mismatched:
        pilnbvbfb: expected [] got [PIL+NONFINTAGB+GATE2PARTIAL+FINTAGB|pilnbvbfb]
        pilfbvb: expected [PIL+FINTAGB+GATE2PARTIAL|pilfbvb] got []
    ```
    Both flip together from this ONE mutation, since both cases key off the same rule's `partial`
    attribute: `pilnbvbfb` (previously `expect_fail`) newly parses, because the rule is no longer
    partial and `NonPartialRuleRequiredAfterNonFinalTemplate` no longer has anything to refuse;
    `pilfbvb` (previously parsing, case 5) drops to zero analyses, because
    `NonPartialRuleProhibitedAfterFinalTemplate` now has a non-partial rule to refuse right after
    `finalTemplateB`. Reverted; re-run confirmed 10/10 PASS again (see below).
  - **Case 6**: `mrGate3Partial`'s `partial="true"` flipped to `partial="false"`. Oracle output:
    ```
    [FAIL] edge-cases/final-template-partial-discriminators (210ms) 1/10 word(s) mismatched:
        nibgi: expected [NIB+GATE3PARTIAL|nibgi] got []
    ```
    `nibgi` alone drops from one analysis to zero (Branch C's isolation holds: no other word is
    affected), because the word is no longer partial when `templateZ`'s empty battery is reached,
    so line 59's `!input.IsPartial && applicableTemplate` is now true and the word dies with
    `ApplicableTemplatesNotApplied` instead of passing through. Reverted; re-run confirmed 10/10
    PASS again.

Final re-verification after both reversions: `[PASS] edge-cases/final-template-partial-discriminators
(254ms)` — `1 passed, 0 failed, 0 skipped`, and a fresh `ET.parse` confirms `grammar.xml` is still
well-formed XML.

## Graduation

Not yet proposed upstream (no `sillsdev/machine` PR opened). Candidate destination:
`machine/conformance/edge-cases/final-template-partial-discriminators/` — same two files
(`grammar.xml`, `words.yaml`), already verified against the C# founding oracle above. On acceptance,
delete this staged copy in the same change (the graduation guard enforces this mechanically).
