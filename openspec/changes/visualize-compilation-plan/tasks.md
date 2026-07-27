## 1. Plan JSON

- [ ] 1.1 Versioned serializable projection of `Plan` (schema version constant, same discipline as
      `COVERAGE_LEDGER_SCHEMA_VERSION`), with node identity carried as the plan's own content address
- [ ] 1.2 Round-trip test, plus a test that an unchanged grammar serializes byte-identically twice
- [ ] 1.3 Test that a single rule's content change moves that node's and its ancestors' identities and
      leaves unrelated subtrees' identities untouched — this is the property that makes revision
      diffing meaningful, so pin it rather than assuming it

## 2. Linguistic labelling

- [ ] 2.1 Derive each node's linguistic description from the plan's own payload (partition keys,
      cascade rule ids, leaf detail) — no second source of truth to drift
- [ ] 2.2 Attach the real capability verdict per node from `compose_envelope`/the predicate registry,
      never inferred from node presence (a node exists whether or not it was admitted)

## 3. Mermaid rendering

- [ ] 3.1 Pure function from plan JSON to mermaid text
- [ ] 3.2 Collapse sibling leaf groups above a readability threshold; emit the threshold, whether
      summarization occurred, and the emitted node count
- [ ] 3.3 Opt-in full rendering
- [ ] 3.4 Render test on a multi-stratum templated fixture asserting the strata are distinguishable
      and a refused construct is marked refused

## 4. Surface

- [ ] 4.1 CLI subcommand emitting JSON and/or mermaid for a grammar
- [ ] 4.2 Golden-rendered diagram for one small synthetic fixture, regenerated from the renderer's own
      output (never hand-edited), so a rendering regression is caught
