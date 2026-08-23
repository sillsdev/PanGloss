# Health signals scoped per recipe, with actionable grammar guidance

Successor to `add-fst-compilation-health-audit` (archived 2026-08-06). That change built the
machinery — preflight walker, evaluator, `pangloss fst-health`, `health.json`, the pack manifest's
admission record — and left three loose ends. This change carries those, and changes what health is
*for*.

## Why

Today a health finding says a net is large or a compile was slow. That is a measurement, and a
measurement is not advice. It tells a grammar author that something is wrong without telling them
what to do, and it says nothing about the choice that actually determined the outcome — which
compiler ran.

Once a recipe is selected per grammar, health stops being a property of "the compilation" and becomes
a property of **this grammar under this recipe**. The same grammar can be healthy under one recipe and
pathological under another, and that difference is the single most useful thing we could tell someone.

## What Changes

**Health is scoped to a recipe, and to a sub-recipe where one applies.** A finding names the recipe it
was measured under. An unscoped finding is not meaningful once more than one compiler can run.

**The report says which recipe compiled this grammar, which did not, and why not.** A refusal or a
non-selection is a fact the author needs. This is the same underlying data
`visualize-subrecipe-selection` will render as a diagram — the diagram shows the decision, this shows
what to do about it. **Both must read it from whatever the selector returns, never re-derive it.**
Two consumers re-deriving one fact is how they come to disagree.

**Findings become actionable text, not just a severity.** A finding carries free-form guidance,
labelled with a warning level, addressed to an AI or a human who can change the grammar: what in this
grammar drove the cost, and what change would make the FST smaller or faster. Naming the responsible
rule or construct is the point; a finding that cannot be acted on is noise with a severity attached.

**Findings may propose a different recipe, conditionally.** "This grammar could compile under recipe
X, which would be faster/smaller, if these things were changed" — even when X is not the primary
recipe. That is the most valuable form the advice can take, because it converts a refusal into a
route.

**Report the resulting size, per recipe, as readiness evidence.** States, arcs and on-disk bytes
for what each recipe actually produced remain measurements, and the health bands describe
production readiness under the managed envelope. `Error` means not production-ready; it does not
change correctness or forbid an explicit developer stress attempt. A complete exact,
parity-verified stress result may be accurate evidence while retaining Error. Hidden
`--remove-size-limits` disables only internal deterministic size/work caps and retains worker
isolation, bounded I/O, external watchdog/RSS/absolute ceilings, capability checks, completion,
finalized payload, and parity. Hidden `--allow-unproven` is a separate developer-only correctness
override that may omit valid parses, is rejected in production/publication/certification, and does
not remove limits. `--no-enforce-capability` is legacy developer-only/non-production.

**Carried from the archived change**, each verified as genuinely outstanding rather than trusted from
its notes:

- Remedies are never populated on the CLI's own findings. `Remedy` has a `rank` field and `health.rs`
  fills it in two places, but every finding `fst_health.rs` constructs passes `remedies: Vec::new()`.
  The ranking machinery exists and the command ranks nothing.
- Correctness/capability Critical remains a refusal for trusted production output. Health Error is
  a readiness finding, not a correctness override; implementation must preserve both dispositions.
- The change's own verification tasks (5.1–5.3) were never run.

## Impact

`pg-foma/src/health.rs`, `health_evaluator.rs`, `preflight.rs`; `pg-cli/src/fst_health.rs`;
`pg-pack`'s manifest admission record. Shares the selector's output with
`visualize-subrecipe-selection`.

## Non-goals

Re-measuring anything. The evaluator consumes existing compiler measurements and must keep doing so;
the point is to scope, explain and act on them, not to add a second measurement path.

## Dependencies

The recipe/sub-recipe scheme, and a selector that reports both its choice and its rejections with
reasons. The three carried gaps above are actionable before that lands; everything else is not.
