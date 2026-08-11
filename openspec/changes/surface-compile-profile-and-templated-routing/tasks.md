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

## 2. Measure the templated backend on the cascade-family shape  [owner: new synthetic fixture + measurement docs; no production code]
- [ ] 2.1 Synthetic cascade-family scale fixture (templates + real phonological cascade at a
      scale that reproduces the two-pass enumeration cost; synthetic-only rule applies).
- [x] 2.2 Real-grammar pair measured 2026-08-10 (local data; synthetic fixture pair still owed
      by 2.1): tuned 84.8s compile / 1062 of 1638 word types with analyses; templated 3.2s
      compile / 796 types. Same-binary signature diff: 1151 exact matches, 341 tuned-only,
      75 templated-only, 53 partial-missing on templated.
- [x] 2.3 DECISION (2026-08-10): DO NOT route wholesale — templated loses recall on 341+53
      real word types (its known morphotactic gaps are real at this grammar's scale), so per
      this task's own stop rule the tuned path stays this grammar's backend. The compile-time
      lever is task 4 (narrow the broadened structural sweep = 92.7% of compile, measured),
      whose adversarial-fixture requirement is now confirmed load-bearing by real data: the
      341 tuned-only words are exactly what the broadened sweep buys, so narrowing must keep
      them. NEW FINDING owed a follow-up: 75 templated-only words (incl. the just-ported
      circumfix-template cells, te-…-iyɛ shapes) are reachable via slot chains but NOT via
      the tuned path's structural-composite probing — a tuned-path undergeneration to
      investigate alongside `cover-circumfix-cross-product-and-infix-drop` task 4.2.

## 3. Routing (conditional on 2.3 = route)  [owner: backend selection / optimizer call sites]
- [ ] 3.1 SCOPE NOTE (2026-08-10): the general backend DECIDER is owned by a concurrent session —
      this change must NOT build selection machinery. Interim: hard-code the templated backend
      for the motivating grammar via an explicit override (env var or CLI flag on the existing
      strategy-selection seam), clearly marked as a stopgap the decider replaces. The
      agreement-locality predicate (design D1) is handed to the decider work as input, not
      implemented here.
- [ ] 3.2 Containment fixture for the factorization boundary case (design D1): a synthetic
      grammar where a suffix's FORM covaries with the chosen prefix — the factorized proposer
      must over-propose every variant (superset witnessed against the Morpher oracle), never
      pick one.
- [ ] 3.3 Nested-circumfix scale witness: synthetic grammar with circumfixes nested to depth 5,
      k≥2 pairs per level; assert the slot-chain proposer's entry count grows ~k·d (not k^d)
      AND recall vs oracle is 100% with confirm rejecting mismatched pairings; record
      candidate-volume/confirm-time deltas vs the paired-composite compile of the same grammar.
- [ ] 3.4 Conformance + corpus gates green on both engines
      (`pg.ps1 -Mode test`, `-Mode corpus-test` where corpus present).

## 4. Narrow probe_would_refuse (independent; droppable)  [owner: emit.rs probe_would_refuse region]
- [ ] 4.1 Design note: can an empty-LHS rewrite's broadening be scoped to rules whose output can
      actually interact with the insertion site? Needs the fire-count evidence pattern
      (0 fires before on a grammar where narrowing applies, >0 on one where it must not) AND
      a deterministic counter delta; wall clock inadmissible.
- [ ] 4.2 Implement + recall-preservation gate (existing recall parity fixtures + a new
      adversarial fixture where epenthesis DOES feed a structural composite — must still be
      found after narrowing).
- [ ] 4.3 If no sound narrowing exists, record the negative result in design.md and close.
