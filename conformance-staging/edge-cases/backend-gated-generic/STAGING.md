# Staging evidence

This synthetic fixture is a generic, language-neutral optimizer witness for: Gate/class partitions, ordered phonology, and lexical MPR exceptions.

It is derived from an existing checked-in conformance shape, with language-identifying names removed. The `words.yaml` oracle is replayed by the shared PanGloss HC conformance harness. Promotion requires oracle parity, intended grammar-fact assertions, and either a content-distinct buildable backend or an explicit elimination report.

Verification:

- `cargo test -p pg-foma --test backend_promoted_fixtures`
- `cargo test -p pg-parse --test conformance_fixtures_gate`

## Oracle provenance (reconciled 2026-08-31)

ust/tools/oracle-conformance.ps1 ran hc-conformance.exe self-check (C# founding oracle,
machine commit caa4ddde8782557c6fb58cac57e4761ffcafc2a6) directly against this fixture's
grammar.xml + words.yaml: PASS -- every word's signature and traced ules: list matched. The
fixture's words.yaml now carries # oracle-provenance: founding-oracle. Any "Oracle discipline"
section below describes how this fixture was originally authored, not its current verification
status.
