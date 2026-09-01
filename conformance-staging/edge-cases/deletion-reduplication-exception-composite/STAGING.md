# deletion-reduplication-exception-composite

Generic synthetic HC fixture combining deletion, reduplication repair, and lexical exceptions.
`prDelete` deletes a stop after a nasal introduced by `mrPrefix`; `mrRedup` copies one lexical span twice; and `mrSuffix` is blocked for the marked lexical entry by `excludedMPRFeatures` while the plain control remains productive.

All names, roots, and surfaces are invented and carry no actual-language data. `words.yaml` is oracle ground truth for `pg_parse::Morpher`. The pg-foma integration test builds the baseline and every applicable content-distinct seeded recipe Plan, then compares each result to full-HC identity and multiplicity. Build failures and mismatches are explicit non-certifying evidence.

Graduation target: `machine/conformance/edge-cases/deletion-reduplication-exception-composite/`.

## Oracle provenance (reconciled 2026-08-31)

ust/tools/oracle-conformance.ps1 ran hc-conformance.exe self-check (C# founding oracle,
machine commit caa4ddde8782557c6fb58cac57e4761ffcafc2a6) directly against this fixture's
grammar.xml + words.yaml: PASS -- every word's signature and traced ules: list matched. The
fixture's words.yaml now carries # oracle-provenance: founding-oracle. Any "Oracle discipline"
section below describes how this fixture was originally authored, not its current verification
status.
