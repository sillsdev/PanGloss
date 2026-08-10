# Staging evidence

This synthetic fixture is a generic, language-neutral optimizer witness for: Ordered rules, metathesis, copying, and multiple strata.

It is derived from an existing checked-in conformance shape, with language-identifying names removed. The `words.yaml` oracle is replayed by the shared PanGloss HC conformance harness. Promotion requires oracle parity, intended grammar-fact assertions, and either a content-distinct buildable backend or an explicit elimination report.

## KNOWN VIOLATION (recorded 2026-07-29): "names removed" above is false

Only identifiers (POS/rule/entry IDs like `posInfix`, `mrSimpleMeta`) were scrubbed. The morph
DATA itself — the actual root/affix strings in `grammar.xml` and the words/glosses in
`words.yaml` — is still real-language, not synthetic:

- `sulat` glossed "write" (`grammar.xml` allomorph `aSulat`), deriving `sumulat` via an
  actor-voice `-um-` infix (`words.yaml`: "SULAT ... + AV -- actor-voice -um- infixed after the
  first consonant, `s`+`um`+`ulat` = `sumulat`") — this is real Austronesian (Indonesian/Tagalog)
  morphology, root and infix both.
- A literal `ke-...-an` circumfix over root `adil` glossed "just" (`grammar.xml` allomorph
  `aAdil`), yielding `keadilan` (`words.yaml`: "ke"+"adil"+"an" = "keadilan") — the real
  Indonesian/Malay nominalizing circumfix over a real Arabic-loan root.
- The root `ktb` (`grammar.xml` allomorph `aKtb`) — the real Semitic (Arabic) triliteral root
  "write" (`k-t-b`), bare.
- The metathesis root `niu` (`grammar.xml` allomorph `aNiu`, `words.yaml` word list `niu`/`nui`)
  — real Austronesian "coconut" (cf. Malay/Indonesian `niur`/`nyiur`), used for its real i/u
  adjacency, not a synthetic string chosen for shape alone.

This fixture's character inventory is byte-identical to the graduated
`machine/conformance/languages/metathesis-phase-isolation` grammar, which is exactly what you'd
expect if the morph strings were copied over unchanged while only the wrapper IDs were renamed.

**Do not re-author this in a routine pass.** Re-deriving a 236-line hand-authored oracle
(`words.yaml`) with genuinely synthetic morphs, while preserving every intended grammar-fact
assertion (ordered rules, metathesis, copying, multiple strata) and oracle parity, is its own task
— use the `conformance-grammars` skill for it, not an ad hoc edit.

Verification:

- `cargo test -p pg-foma --test backend_promoted_fixtures`
- `cargo test -p pg-parse --test conformance_fixtures_gate`

(Both must now go through the managed entry point — `rust/tools/pg.ps1 -Mode test` — per the repo's
own `CLAUDE.md`; bare cargo is prohibited in agent workflows.)

## Also depended on by task 7.7 (added 2026-08-03)

This is **cleanup exercise 2** of the first `Morphotactics -> BoundaryCleanup` vertical slice,
`rust/crates/pg-foma/tests/morphotactics_boundary_cleanup_slice.rs` (task 7.7 of
`openspec/changes/cleanup-and-recipe-parity`): the boundary-CONSUMER half. `mrComplexMeta`'s
`<BoundaryMarker boundary="cBnd" />` makes the boundary that rule's TRIGGER, so cleaning up before it
runs erases the trigger — the cleanup dossier's first rejected architecture. This grammar has no
compounding, which is what makes it independent of cleanup exercise 1 (`backend-strata-generic`, which
produces a boundary and has no boundary consumer).

Load-bearing rows there: `mu+i` (one identity, multiplicity one, seam retained in the surface) with
`mi` as the no-site control — without which "the rule fired" would be indistinguishable from "the
rule always fires". The gate additionally mutates this fixture's OWN DERIVED mechanism graph, moving
cleanup ahead of its consumer, and requires `MechanismGraph::validate` to refuse with
`CleanupNotTerminal`.

That gate reads every expected count OUT OF the `parses:` rows in this directory's `words.yaml` — it
hand-derives nothing — so editing a word entry here changes what it asserts. If you add, remove, or
re-count a `parses:` row, or change the `BoundaryDefinitions` block or `mrComplexMeta`'s structural
description, re-run that gate too.
