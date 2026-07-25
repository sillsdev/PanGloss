## 1. Preflight

- [ ] 1.1 Walk every frozen model variant and emit construct/disposition/cost inputs
      (not done — no preflight walker over model variants found in `pg-foma/src`;
      `health_evaluator.rs`'s own doc says it only consumes existing measurements, never walks
      variants)
- [ ] 1.2 Calculate bounded products for alternatives, quantifiers, alpha tuples, templates, and slots
      (not done — no such preflight product calculation found)
- [ ] 1.3 Separate semantic uncertainty from cost uncertainty: reject possible analysis loss, but
      attempt recall-preserving unknown growth under the shared worker and logical budgets
      (not done — no preflight stage exists to make this distinction)

## 2. Observed findings

- [x] 2.1 Consume profile/budget events without recomputing their values
      (`pg-foma/src/health_evaluator.rs` reads `ComposeError`/`EmitReport`/`ApplyBudgetTrip` directly,
      per its own module doc)
- [ ] 2.2 Evaluate intermediate/final nets, FST bytes, compilation time, paths/candidates, and apply time
      (partial — net-size/bytes/paths covered via `ComposeError`/`ApplyBudgetTrip` mapping tests
      (`fst_health_evaluator_net_size_exceeded_*`, `apply_budget_trip_*`); compile-time/apply-time
      elapsed-millis dimension is explicitly not populated per the module doc — no per-word
      wall-clock/allocation instrumentation exists yet)
- [ ] 2.2a Evaluate proposal count, confirmation count/work, and rejection share independently from
      semantic correctness and payload size
      (not done — module doc explicitly states `ProposalVolume`/`ConfirmationWork` findings stay
      unpopulated)
- [ ] 2.2b Record pre-dedup duplicate count/ratio and available rule/proposal-path provenance; keep
      duplicates out of semantic set equality while making them actionable health evidence
      (not done — module doc explicitly: `DuplicateAnalysisOverlap` needs `crate::confirm`'s pre-dedup
      counts, which are not produced anywhere; `confirm.rs` has no duplicate-count tracking)
- [ ] 2.3 Preserve predicted and observed evidence separately when estimates differ
      (partial — `ValueProvenance` types exist but are only exercised on the "already tripped" path)

## 3. Compiler and reports

- [ ] 3.1 Add `pangloss fst-health` preflight-only and observed modes
      (not done — no `fst-health` subcommand exists in `pg-cli`; the evaluator is library-only, not
      wired into the CLI)
- [ ] 3.2 Emit standard compiler finding lines plus canonical `health.json` and derived `health.md`
      (partial — `health.json`-shaped goldens exist as unit tests
      (`health_evaluator.rs::fst_health_evaluator_golden_json`), but there is no CLI command that
      actually emits `health.json`/`health.md` files)
- [ ] 3.3 Rank only applicable remedies and include rule/construct identifiers and exact factors
      (not done — no remedy-ranking code found; no command exists to exercise it)
- [ ] 3.4 For terminal resource findings, include the reached limit, effective named envelope,
      partial measurements, grammar-first remedies, and explicit-retry instructions
      (not done — no terminal-resource-finding CLI output exists)
- [ ] 3.5 Keep potentially meaning-changing remedies advisory; record automatically applied internal
      optimizations only when their owning lowering provides semantics-preservation evidence
      (not done — no such recording mechanism found)

## 4. Admission and packages

- [ ] 4.1 Permit Warning packages normally and require explicit recorded Error override
      (not done — no admission/publish wiring found in `pg-cli`)
- [ ] 4.2 Reject Critical, incomplete, truncated, or watchdog-terminated package publication
      (not done — `pg-pack/src/format.rs::write_pack` takes raw byte payloads with no health/admission
      gate call site found)
- [ ] 4.3 Embed schema version, overall admission, findings, and override record in the one-file manifest
      (not done — no such embedding found in the pack manifest)

## 5. Verification

- [ ] 5.1 Run all focused commands from `design.md` (not run — most of sections 1/3/4 have no code to
      exercise)
- [ ] 5.2 Add generated-grammar properties: every input finishes inside policy or returns typed findings
      (not done)
- [ ] 5.3 Audit public compiler entry points so none bypass preflight/admission
      (not done — there is no preflight/admission gate yet to bypass)
