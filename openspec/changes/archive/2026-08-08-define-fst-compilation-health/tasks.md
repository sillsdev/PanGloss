## 1. Schema and registry

- [x] 1.1 Define Rust finding, remedy, evidence, severity, phase, override, and admission types
      (`pg-foma/src/health.rs`: `Severity`/`Phase`/`Metric`/`MetricValue`/`FindingCode`/`Remedy`/
      `OverrideRecord`/`HealthFinding`/`HealthReport`)
- [x] 1.2 Define the immutable `PGFdddd` registry and reject duplicate or undocumented codes
      (`health.rs` `FindingCode` exhaustive enum; tests
      `fst_health_schema_codes_are_unique_and_well_formed`, `from_code_rejects_unknown_code`)
- [ ] 1.3 Emit schema-versioned canonical JSON and deterministic Markdown from the same findings
      (JSON round-trip is done — `fst_health_schema_golden_json`/`golden_round_trip` — but no
      Markdown renderer was found in `health.rs`; Markdown-side of this task is not done)

## 2. Policy

- [x] 2.1 Implement exact decimal FST-payload bands at 10/20/100/500 MB
      (`health.rs`: `fst_health_size_bands_*` tests at each exact edge)
- [x] 2.2 Aggregate independent size, construction, application, and unsupported-work severities
      (`HealthReport::admission`; `fst_health_override_policy_worst_non_overridden_wins_among_several`)
- [x] 2.3 Implement explicit Error AND Critical override records via the ADR 0005 capability override (binary trust axis; no predicted verdict is non-overridable)
      (`OverrideRecord` + `fst_health_override_policy_error_and_critical_are_overridable`; the live
      ADR 0005 mechanism (`--allow-unproven`, `pg-cli/src/main.rs::capability_gate`) is the same
      pattern reused)
- [x] 2.4 Keep Warning-and-below packages deployable
      (`fst_health_override_policy_warning_and_below_never_need_override`)

## 3. Diagnostic quality

- [ ] 3.1 Require affected identifiers, values, thresholds, explanation, and applicable remedies
      (schema has the fields (`HealthFinding`, `Remedy`) but no evaluator populates real
      affected-identifier data yet — schema-only, see `add-fst-compilation-health-audit`)
- [ ] 3.2 Add goldens for ordering/constraining suggestions with linguistic-equivalence caveats
      (one golden test references a linguistic-equivalence-caveated remedy, but it is schema-level,
      not tied to a real compiler-produced ordering suggestion)
- [ ] 3.3 Prove reports contain no general linguistic-quality score or Python-owned calculation
      (not done — no test found asserting this absence)

## 4. Verification

- [ ] 4.1 Run every focused command from `design.md` (not re-verified this pass)
- [ ] 4.2 Run strict OpenSpec validation (see this bookkeeping pass's own final `openspec validate --all --strict` run)
