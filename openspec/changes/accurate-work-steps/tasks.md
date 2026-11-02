Each numbered group is one reviewable merge unit. A group needs the groups before it unless stated.
Heavy measurement, policy and code are separate tasks. Real-language data is never committed.

## 1. Parse-neutral cleanup (owns `pg-fst/src/work.rs`)

- [x] 1.1 Rename `SynthesisBudget::steps()` to `work_steps()` in `pg-parse/src/morpher.rs`, and its callers
- [x] 1.2 Poll the deadline every 64 units in `WorkBudget::consume`, always in `stopped()`
- [x] 1.3 Pin that an expired deadline still stops the first reservation, and that `stopped()` alone observes it: `pg-fst` unit tests
- [x] 1.4 Measure instructions with and without a deadline by PMC, confirm the per-unit overhead fell

## 2. Measurement tooling

- [ ] 2.1 Land the Valgrind per-word instruction script under `rust/tools/`, with its validation against PMC data
- [ ] 2.2 Land the Windows PMC trace script under `rust/tools/`, refusing a stale trace
- [ ] 2.3 Publish `docs/design/work-measurement.md`: tools, noise floors, what each misses
- [x] 2.4 Measurement: first A/B pair set with a no-change control, recorded in D5
- [ ] 2.5 Add the remaining hard cases to the set: other memo pairs and the `distinct` fix; root cache on/off is done
- [x] 2.6 Re-measure the two D5 disagreements with counts: one fixed, one did not reproduce in one binary
- [ ] 2.7 Score weighted cost on all seven pairs, which needs counters on the older builds

## 3. Report-only size charges

- [ ] 3.0 Speedup, separate from this signal work: about 27-54% of instructions are SipHash in one `HashMap` in `pg_fst::traverse`; prove no iteration-order dependence, then file or fix it
- [x] 3.1 Add `ParseOutcome.cost` for allocated bytes and registers copied, read by no cap, deadline or decision
- [ ] 3.2 Gate: parses, `work_steps` and `CAP` byte-identical to the base build across conformance
- [x] 3.3 Measurement: fit against stored instructions, per grammar and held out by grammar
- [x] 3.4 Decide: which categories earn a charge, recorded in `design.md`

- [x] 3.5 Report weighted cost from built-in weights beside `work_steps`, read by no decision
- [x] 3.6 Measure a grammar outside the five fitted: built-in weights against its own instructions

## 3a. Project calibration (needs 3.6 to show built-in weights drift)

- [x] 3a.1 Add a calibration command: about 40 words, one thread, instructions or wall time
- [x] 3a.2 Refuse an update when thread CPU / wall is under 0.95, and bound weights per D10
- [x] 3a.3 Dropped (D10): weights are code constants and calibration is the off-by-default `cost-calibration` feature

## 3b. Deployed bypass

- [x] 3b.1 Keep the WASM default cap; `withoutWorkLimits` is opt-in (D11)
- [ ] 3b.2 Gate: parses identical to a run with an unreachable cap, across conformance
- [ ] 3b.3 Measure the speed gain and the longest word on the deployed path

## 4. Acceptance test

- [ ] 4.1 Record final D5 thresholds from 2.4 and 3.3
- [ ] 4.2 Add the no-change control gate to CI, using `work_steps` only
- [ ] 4.3 Add a local instruction drift report per release, comparing predicted with measured

## 5. Accurate `work_steps` (output-affecting)

- [ ] 5.1 Charge accepted categories in `WorkBudget`; bump `SEARCH_BUDGET_SEMANTICS`
- [ ] 5.2 Recalibrate the default work cap and its multiplier
- [ ] 5.3 Gate: no word that completes today newly caps, on conformance and all local real grammars
- [ ] 5.4 Update divergence 052 and `docs/research/per-word-search-work-cap.md`
- [ ] 5.5 Add a measured raw-path weight, then move `Score::key` to work

## 6. Retire `steps` (breaking)

- [ ] 6.1 Warning release: `--step-cap` and the `steps` column report deprecation
- [ ] 6.2 Confirm the Motif transition with its maintainers
- [ ] 6.3 Gate: the Maasai recall sample does not fall below its current count of words with analyses
- [ ] 6.4 Remove the attempt cap, `--step-cap`, `ParseOutcome.steps` and oracle step ticks
- [ ] 6.5 Move `interfaces.batchTsv` to 5 (4 added `--cost-column`), update serve, FFI, WASM and the stats schema
- [ ] 6.6 Remove `confirmation_steps` from `Score` and the reports

## 7. Host signal

- [ ] 7.1 Measure how far k moves under small grammar edits
- [ ] 7.2 Publish D9's rules and the display band in `docs/formats/`
- [ ] 7.3 Hand the contract to Motif
