# deletion-reduplication-exception-composite

Generic synthetic HC fixture combining deletion, reduplication repair, and lexical exceptions.
`prDelete` deletes a stop after a nasal introduced by `mrPrefix`; `mrRedup` copies one lexical span twice; and `mrSuffix` is blocked for the marked lexical entry by `excludedMPRFeatures` while the plain control remains productive.

All names, roots, and surfaces are invented and carry no actual-language data. `words.yaml` is oracle ground truth for `pg_parse::Morpher`. The pg-foma integration test builds the baseline and every applicable content-distinct seeded recipe Plan, then compares each result to full-HC identity and multiplicity. Build failures and mismatches are explicit non-certifying evidence.

Graduation target: `machine/conformance/edge-cases/deletion-reduplication-exception-composite/`.
