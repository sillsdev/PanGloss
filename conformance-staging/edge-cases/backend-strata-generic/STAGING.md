# Staging evidence

This synthetic fixture is a generic, language-neutral optimizer witness for: Layered morphology, specialized branches, copying, and multiple strata.

It is derived from an existing checked-in conformance shape, with language-identifying names removed. The `words.yaml` oracle is replayed by the shared PanGloss HC conformance harness. Promotion requires oracle parity, intended grammar-fact assertions, and either a content-distinct buildable backend or an explicit elimination report.

Verification:

- `cargo test -p pg-foma --test backend_promoted_fixtures`
- `cargo test -p pg-parse --test conformance_fixtures_gate`

(Both must now go through the managed entry point — `rust/tools/pg.ps1 -Mode test` — per the repo's
own `CLAUDE.md`; bare cargo is prohibited in agent workflows.)

## Also depended on by task 7.7 (added 2026-08-03)

This is **cleanup exercise 1** of the first `Morphotactics -> BoundaryCleanup` vertical slice,
`rust/crates/pg-foma/tests/morphotactics_boundary_cleanup_slice.rs` (task 7.7 of
`openspec/changes/cleanup-and-recipe-parity`): the boundary-PRODUCER half. Its compounding join seam
is a `BoundaryDefinition`, so the boundary the cleanup end of that slice removes is one the
morphotactics end created. This grammar declares no boundary-consuming phonological rule at all,
which is exactly what makes it independent of cleanup exercise 2 (`backend-ordered-generic`, which has
such a consumer and no compounding).

Its load-bearing row there is `akutat`: two distinct identities at multiplicity one each, across the
`+` seam. The gate also requires this grammar's derived cleanup inventory to be NON-empty — a cleanup
exercise with an empty boundary inventory certifies nothing about cleanup.

That gate reads every expected count OUT OF the `parses:` rows in this directory's `words.yaml` — it
hand-derives nothing — so editing a word entry here changes what it asserts. If you add, remove, or
re-count a `parses:` row, or change the `BoundaryDefinitions` block, re-run that gate too.

## Oracle provenance (reconciled 2026-08-31)

ust/tools/oracle-conformance.ps1 ran hc-conformance.exe self-check (C# founding oracle,
machine commit caa4ddde8782557c6fb58cac57e4761ffcafc2a6) directly against this fixture's
grammar.xml + words.yaml: PASS -- every word's signature and traced ules: list matched. The
fixture's words.yaml now carries # oracle-provenance: founding-oracle. Any "Oracle discipline"
section below describes how this fixture was originally authored, not its current verification
status.
