## 1. Harness

- [ ] 1.0 Centralize explicit provisional defaults and high ceilings used by earlier implementation;
      label them non-release and prohibit `usize::MAX` or other effectively unlimited production values
- [ ] 1.1 Add watchdog recipe execution with sampled worker RSS/interval, wall/CPU time, artifact
      bytes, net sizes, traversal paths, and candidate counts
- [ ] 1.2 Record platform, toolchain, build profile, grammar recipe, seed, and effective envelope
- [ ] 1.2a Run current calibration on Windows, record hardware details, and emit explicit Linux
      `not_run` evidence without blocking the remaining calibration or policy work
- [ ] 1.3 Reject concurrent heavy runs and incomplete correctness evidence

## 2. Scale sweeps

- [ ] 2.0 Gate final sweeps on all Stage-2 constructs, production cascade wiring/profile, correctness
      gates, and pairwise infrastructure being merged into the measured commit
- [ ] 2.1 Sweep lexicon size, template depth/slots, alpha variables/class size, group count, tables, strata, rule count, compounds, and segment inventory
- [ ] 2.2 Sweep long/ambiguous worst words for apply-time cliffs
- [ ] 2.3 Binary-search first typed breach and first hard-limit termination for each vector
- [ ] 2.4 Run representative real-language compile/apply workloads and generated one-factor and
      pairwise worst cases, retaining correctness gates and separating invalid evidence

## 3. Policy

- [ ] 3.1 Publish raw results and per-vector go-bars
- [ ] 3.2 Set conservative versioned defaults with documented headroom
- [ ] 3.3 Prove an over-budget sibling fails honestly rather than OOMing or hanging the parent
- [ ] 3.4 Replace provisional values with the reviewed portable runtime defaults and hard ceilings;
      block release while any provisional policy marker remains
- [ ] 3.5 Document how later credible Linux measurements are compared against and may revise the one
      portable policy without creating platform-specific runtime limits
- [ ] 3.6 Generate an advisory proposed-policy diff with raw evidence and headroom calculations;
      provide no automatic write or runtime-adaptation path for production constants
- [ ] 3.7 Require an explicit reviewed commit and policy-version increment to activate any default or
      absolute-ceiling change
