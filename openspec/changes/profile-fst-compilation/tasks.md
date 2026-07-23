## A. Production emitter profile — Stage 1

- [ ] A.1 Define compile-profile events in the shared diagnostic schema: top-line compile time, emit/probe/enumeration stages, per-template/continuation lexc lines, lexc parse time, final states/arcs, and resource outcomes
- [ ] A.2 Thread an optional sink through the active `emit_with_budget` and production `FomaProposer` constructor; do not instrument `replace.rs` as production
- [ ] A.3 Split rewrite-model category counts into plain, alpha-variable, gated, RightToLeft, Simultaneous, metathesis, quantifier, circumfix/null-role, compounding, templates, strata, and character tables without implying compiled support
- [ ] A.4 Add a top-line compile-time field and render the production profile in JSON/Markdown
- [ ] A.5 Verify sink-off structural/result equivalence and measure instrumentation overhead

## B. Replacement-cascade profile — blocked on Stage 2 production wiring

- [ ] B.1 Add a hard prerequisite test proving the network under query was built by the production replacement-cascade constructor
- [ ] B.2 Record each rule's own returned net states/arcs/time, alpha tuple survivors, partition groups, and skipped/unsupported disposition
- [ ] B.3 Record raw returned composed-net states/arcs after each existing fold without adding minimization or other observer work
- [ ] B.4 Label any pre-production capture `experimental_composition` and prevent it from satisfying production-profile requirements
- [ ] B.5 After the production switch, update the pipeline version and prove the profile fingerprint matches the network used for lookup

## Verification

- [ ] C.1 Test small, spiking, skipped-rule, alpha, template, and over-budget fixtures
- [ ] C.2 Confirm Stage 1 reports never contain a production cascade curve
- [ ] C.3 Run strict OpenSpec validation when available
