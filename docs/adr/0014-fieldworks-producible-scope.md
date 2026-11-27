---
status: accepted
---

# FieldWorks-producible scope: PanGloss cares only about grammar constructs a FieldWorks project can create

Owner decision, 2026-11-10: "we only care about things that FieldWorks projects can ACTUALLY create - not just what the HC xml grammar can hold."

## Scope

Parity bugs, divergences, conformance cases, performance work and reviews (including reviews of upstream `sillsdev/machine` HermitCrab PRs) count only when a real FieldWorks project can create the grammar construct involved. The construct must be emitted by FieldWorks' `Src/LexText/ParserCore/HCLoader.cs` from data a user can enter in FLEx, or be a parser setting FieldWorks exposes (the Parser Parameters dialog, read by `Src/LexText/ParserCore/HCParser.cs`; for example `DeletionReapplications`, 0-10).

## Out of scope

A construct that only hand-written HermitCrab XML can hold is out of scope. Confirmed in `HCLoader.cs` today:

- Realizational affix process rules (`HCLoader.cs` builds only `AffixProcessRule`; it carries a "TODO: use realizational affix process rules").

A Morpher option that neither `HCLoader.cs` nor `HCParser.cs` sets is also out of scope until FieldWorks exposes it.

## Bugs that need such a construct

- They are not parity bugs.
- They get no conformance case, no divergence ledger entry and no fix work.
- Review findings that need such a construct are not blockers.

## What still holds

- HermitCrab XML input still loads.
- ADR 0001's fail-closed rule still applies: no silent recall loss on anything PanGloss claims to support.
- ADR 0009 is unchanged; partial-feature grammars are FieldWorks-producible.

## How to decide producibility

Check `HCLoader.cs` and `HCParser.cs`, plus `PROTOCOL.md` section 9 and `fieldworks-producibility.tsv` in `sillsdev/fieldworks-parser-conformance`. A real FieldWorks project, the fwdata witness, is the strongest evidence. If producibility is unknown, find out before acting; do not assume either way.

## Consequences

- Existing issues and divergences that depend only on out-of-scope constructs can be closed citing this ADR.
- The oracle hierarchy in `CLAUDE.md` is unchanged for in-scope constructs; this ADR only narrows which constructs it governs.

Related: `docs/adr/0001-honest-capability-boundary.md`, `docs/adr/0009-partial-feature-grammars.md`.
