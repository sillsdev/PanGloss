# Archived changes

Completed OpenSpec changes, moved here so `openspec list` shows only work that is still open. The
directories are unmodified — proposal, design, tasks and delta specs are exactly as they were when
the work finished.

## Why the delta specs were NOT synced into canonical specs

The normal archive flow merges a change's `specs/<capability>/spec.md` deltas into a canonical
`openspec/specs/` tree. That step was deliberately skipped, and the reason is worth recording so the
next person does not "fix" it by accident.

**`openspec/specs/` has never existed in this repo.** Across 33 changes, no delta was ever synced.
Syncing five of them now would create a canonical tree describing five capabilities and silently
omitting the other twenty-eight — a document that looks authoritative, is mostly absent, and which
nothing reads and nobody updates. This repo's own rule is that an unmaintained artifact is worse than
no artifact, and a half-populated spec tree is exactly that shape.

Where durable knowledge lives here instead: `docs/research/` for measurements and design rationale,
and the code plus its tests for behaviour. A capability's requirements, as written at the time, stay
readable in this directory.

If a canonical spec tree is ever wanted, the honest way to get one is to sync **all** changes at
once, not to accrete it five at a time from whichever happened to finish.
