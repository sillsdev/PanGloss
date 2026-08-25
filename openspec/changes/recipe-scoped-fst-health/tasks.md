## 1. Scope a finding to the backend that produced it

- [ ] 1.1 Add the backend (and sub-recipe, where one applies) to `HealthFinding`, and make an
      unscoped finding unconstructable rather than merely discouraged — a finding that cannot say
      which compiler it measured is not meaningful once more than one can run
- [ ] 1.2 Populate it from the selector's actual choice at every construction site in
      `health_evaluator.rs` and `pg-cli/src/fst_health.rs`
- [ ] 1.3 Report which backend compiled the grammar, which did not, and why not — a refusal and a
      non-selection are both facts the author needs

## 2. Make a finding actionable

- [ ] 2.1 Populate `remedies` on the CLI's own findings. `Remedy` has a `rank` and `health.rs` fills
      it in two places, but every finding `fst_health.rs` constructs passes `remedies: Vec::new()`,
      so the ranking machinery ranks nothing
- [ ] 2.2 Include the cross-backend remedy: "this would compile under backend X if these things
      changed" — the option the author cannot discover any other way
- [ ] 2.3 Goldens for ordering/constraining suggestions, each carrying its linguistic-equivalence
      caveat (carried from `define-fst-compilation-health` 3.2, which reached schema level only)

## 3. Carried from `define-fst-compilation-health` (archived 2026-08-08)

That change built the schema and it is sound — severity is already documented as living on the
cost axis and never the capability-trust axis, and a finding already carries free-form
`explanation` plus ranked `remedies`. These are its unfinished parts, re-verified as genuinely
outstanding rather than trusted from its notes.

- [ ] 3.1 Render deterministic Markdown from the same findings as the JSON. The JSON side is done
      and golden-tested; no Markdown renderer exists (its 1.3)
- [ ] 3.2 Populate real affected-identifier data. The schema has the fields; no evaluator fills
      them with anything a reader could act on (its 3.1)
- [ ] 3.3 Prove a report carries no general linguistic-quality score and no Python-owned
      calculation. No test asserts this absence today (its 3.3) — worth keeping whoever owns it,
      because it is the guard that stops health drifting into judging the analysis
- [ ] 3.4 Run the focused commands from the archived change's `design.md` (its 4.1, never run)

## 4. The size bands report production readiness; the exact edges are provisional

**Thresholds stay as readiness findings.** A large compiled grammar is reported, but health does
not decide representability or select a backend. Explicit stress attempts may configure higher but
still-finite execution limits. Hidden `--allow-unproven` is for local correctness testing only and
never removes limits. Publication rejects unproven artifacts. Delete `--remove-size-limits` and
`--no-enforce-capability` rather than preserving compatibility.

- [ ] 4.1 Recalibrate `*_MAX_BYTES` from the spread across several backends and several grammars.
      The reasoning behind the current edges is a judgment, not a measurement: a grammar is on the
      order of a thousand parameters, so the difficulty is combining them compactly, and that is
      what the backends differ at. `calibrate-fst-resource-envelopes`, whose job was to derive such
      thresholds from evidence, was retired without producing one
- [ ] 4.2 Keep the provenance loud at every point of use meanwhile, so a crossed band reads as
      "this backend did not combine this grammar well" and never as a proven resource limit

## 5. Correctness Critical and health Error remain distinct

- [ ] 5.1 Wire production publication/selection to refuse a correctness/capability Critical or
      any incomplete, truncated, skipped, or parity-unverified result. A health Error remains a
      production-readiness refusal under configured execution limits, but may be attempted by explicit
      developer stress control and never becomes a trusted production result merely because it
      completed.
- [ ] 5.2 Test that local `--allow-unproven` may omit valid parses, never publishes, and never
      disables execution limits. Test that the retired `--remove-size-limits` and
      `--no-enforce-capability` spellings are rejected.

## 6. Verification

- [ ] 6.1 Run the change's own verification tasks (5.1–5.3 of the archived audit, never run)
- [ ] 6.2 ~~Strict OpenSpec validation~~ **not achievable, and deliberately so.** `openspec validate
      --all --strict` fails for all ten active changes with the same error — "Change must have at
      least one delta" — because the delta spec files were deleted on the standing decision that the
      code, agents, README, context files and docs define this system and nothing gets promoted to
      an official spec. So strict validation is permanently red for every change and cannot gate
      anything. Do not "fix" it by re-adding delta specs. Any other change still carrying a
      "run strict OpenSpec validation" task has the same dead task
