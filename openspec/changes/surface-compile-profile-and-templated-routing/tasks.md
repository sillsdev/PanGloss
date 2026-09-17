# Tasks — surface-compile-profile-and-templated-routing

Sequenced by certainty: measurement infrastructure first, routing only if measurement supports
it, the risky optimization last and independently droppable. Merges after
`cover-circumfix-cross-product-and-infix-drop` (emit.rs serialization; see its tasks 5.2).

## 1. Surface the CompileProfile  [owner: pg-cli/src/fst_health.rs]
- [x] 1.1 `fst-health --profile-json=<path>` (path required — a bare flag is refused with usage,
      keeping the report/profile stdout streams unmixed): emits the raw `CompileProfile`
      (per-stage durations, lexc lines, state/arc counts) alongside the existing findings;
      existing output unchanged when the flag is absent.
- [x] 1.2 Unit tests: profile file round-trips into `CompileProfile` with >=1 stage and lexc
      lines present on a successful compile; bare flag refused.
      Verified 2026-08-10: `pg.ps1 -Mode test -Package pg-cli -Filter fst_health` — 7/7 passed.

## 2. Measure the templated backend on the cascade-family shape  [owner: measurement docs; no production code]
- [x] 2.1 N/A — CLOSED BY THE 2.3 DO NOT ROUTE DECISION: the local real-grammar measurement already crossed the stop threshold by demonstrating material templated recall loss. A synthetic scale workload would not change that backend decision and is deferred to a future change that reopens templated routing.
- [x] 2.2 Real-grammar pair measured 2026-08-10 (local private data; aggregate evidence recorded here): tuned 84.8s compile / 1062 of 1638 word types with analyses; templated 3.2s
      compile / 796 types. Same-binary signature diff: 1151 exact matches, 341 tuned-only,
      75 templated-only, 53 partial-missing on templated.
- [x] 2.3 DECISION (2026-08-10): DO NOT route wholesale — templated loses recall on 341+53
      real word types (its known morphotactic gaps are real at this grammar's scale), so per
      this task's own stop rule the tuned path stays this grammar's backend. Task 4's
      narrowing investigation also closed negative: the conservative structural sweep remains
      because no narrower dependency predicate has a recall proof. NEW FINDING owed a follow-up: 75 templated-only words (incl. the just-ported
      circumfix-template cells, te-…-iyɛ shapes) are reachable via slot chains but NOT via
      the tuned path's structural-composite probing — a tuned-path undergeneration to
      investigate alongside `cover-circumfix-cross-product-and-infix-drop` task 4.2.

## 3. Routing (closed by the 2.3 DO NOT ROUTE decision)  [owner: backend selection / optimizer call sites]
- [x] 3.1 N/A — CLOSED BY DO NOT ROUTE (2026-08-10): no backend selector, hard-coded override, or agreement-locality machinery is added here; the tuned backend remains selected for the motivating grammar.
- [x] 3.2 N/A — CLOSED BY DO NOT ROUTE: the factorization containment fixture is deferred to a future decider/templated-routing change because this change does not route to that backend.
- [x] 3.3 N/A — CLOSED BY DO NOT ROUTE: no nested-circumfix slot-chain routing witness is required when wholesale templated routing is explicitly rejected.
- [x] 3.4 N/A — CLOSED BY DO NOT ROUTE: this change adds no routing path, so no routing-specific conformance/corpus gate is claimed.

## 4. Narrow probe_would_refuse (independent; droppable)  [owner: emit.rs probe_would_refuse region]
- [x] 4.1 Investigated and recorded in design D3: no sound local narrowing predicate was found. The measured tuned-only recall and C1-C5 containment cases make the conservative sweep load-bearing.
- [x] 4.2 N/A — CLOSED BY THE NEGATIVE RESULT: no production narrowing was implemented, so there is no altered path to gate.
- [x] 4.3 Negative result recorded 2026-08-11; retain `probe_would_refuse` unchanged until a future dependency analysis can prove a narrower candidate set recall-safe.
