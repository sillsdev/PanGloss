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

## 4. The size bands warn for a real reason; the exact edges are provisional

**Thresholds stay.** A gigabyte-scale compiled grammar is not shippable and its author has to be
told, so these are warnings rather than a bare reported number. Only the exact edges are open.
Raised 10x to 100MB / 200MB / 1GB / 5GB, which puts anything past a gigabyte into Error — an
explicit, recorded override before it can publish.

- [ ] 4.1 Recalibrate `*_MAX_BYTES` from the spread across several backends and several grammars.
      The reasoning behind the current edges is a judgment, not a measurement: a grammar is on the
      order of a thousand parameters, so the difficulty is combining them compactly, and that is
      what the backends differ at. `calibrate-fst-resource-envelopes`, whose job was to derive such
      thresholds from evidence, was retired without producing one
- [ ] 4.2 Keep the provenance loud at every point of use meanwhile, so a crossed band reads as
      "this backend did not combine this grammar well" and never as a proven resource limit

## 5. Nothing acts on a Critical package

- [ ] 5.1 Decide whether a Critical admission should refuse. Today `HealthReport::admission` is
      printed by `fst_health.rs` and stamped into the pack manifest by `pack.rs`, and no site
      rejects on it — so `Severity`'s own doc ("requires an explicit, recorded `OverrideRecord`
      before the artifact may publish") describes an intent nothing enforces. Under the two-axis
      rule cost alone is never a rejection, so the honest resolution may be to soften the doc rather
      than add a gate

## 6. Verification

- [ ] 6.1 Run the change's own verification tasks (5.1–5.3 of the archived audit, never run)
- [ ] 6.2 ~~Strict OpenSpec validation~~ **not achievable, and deliberately so.** `openspec validate
      --all --strict` fails for all ten active changes with the same error — "Change must have at
      least one delta" — because the delta spec files were deleted on the standing decision that the
      code, agents, README, context files and docs define this system and nothing gets promoted to
      an official spec. So strict validation is permanently red for every change and cannot gate
      anything. Do not "fix" it by re-adding delta specs. Any other change still carrying a
      "run strict OpenSpec validation" task has the same dead task
