# Local unproven generation is never publication

> **Status: CURRENT.** This ADR supersedes the earlier capability-override wording. The
> ratified pipeline contract is also recorded in `docs/simplification-rip-list.md`.

## Decision

`--allow-unproven` is available only for local testing and inspection. It may permit generation of
a locally retained artifact for corpus testing, but the artifact must be marked `unproven` in build
metadata and must never be treated as publishable.

`pangloss pack` rejects `--allow-unproven` and rejects every unproven artifact. Every other
publication or distribution route follows the same rule. Corpus success never promotes an
unproven artifact to publishable status. There is no persistent capability-override record in a
pack, and no consumer can clear or launder the unproven state.

The override does not waive execution limits, compilation failures, or other containment rules.
All local attempts still use finite configured limits, and every failed or unproven result remains
local testing evidence rather than a product claim.

## Rationale

The capability analysis is a correctness proof. An unproven result may omit valid analyses by
definition, so the only honest use of the override is to inspect behavior while developing the
missing proof. A real proof requires conformance coverage and a clean recompilation without the
override; no metadata edit or corpus result can substitute for that proof.

## Superseded history

The previous revision described a hidden developer override, an indelible override record in a
pack manifest, and publication-time handling of that record. That design is retained only in Git
history for provenance. It is not a current API, storage format, or publication contract.
