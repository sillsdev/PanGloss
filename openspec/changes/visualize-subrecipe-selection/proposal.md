# Migrate plan visualization to the sub-recipe scheme

**Status: intent only.** Design and tasks are deliberately unwritten — the sub-recipe scheme they
would describe does not exist yet. This change exists so the requirement is not lost when the
sub-recipe work lands and the thing it visualizes changes shape.

## Why

`pangloss plan-diagram` answers "how was this grammar compiled?" against the *current* model: strata
become nodes, rules join cascades, a gate splits on a partition. It is complete and tested
(`visualize-compilation-plan`, retired 2026-08-06, 11/11).

The sub-recipe work changes the question. Once a grammar is compiled by a chosen recipe with a set of
switches, the author's question stops being "what does my plan look like" and becomes:

- **Which recipe was selected for my language, and why that one?**
- **Which switches fired, and what in my grammar triggered each?**
- **What was rejected, and would a different recipe have done better?**

A diagram that shows only the resulting plan cannot answer any of those. It shows the consequence and
hides the decision — and the decision is the part a field linguist and an engineer actually argue
about.

## What Changes

Intent, not a design:

- The diagram gains the **selection** layer: recipe chosen, switches active, and the grammar evidence
  that drove each. Today's node/verdict view becomes the layer beneath it.
- Selection facts must come from whatever the selector actually returns, not be re-derived for
  display. `visualize-compilation-plan` already established this discipline — node descriptions are
  derived from the plan's own payload, and the capability verdict is read from `compose_envelope`
  rather than inferred from node presence. The same rule applies to the recipe and switch layer, for
  the same reason: a second source of truth drifts.
- Whether this extends `plan_diagram.rs` or sits beside it is a design question, deferred.

## Impact

`pg-foma/src/plan_diagram.rs` (67KB, 17 tests, golden mermaid artifact) and the `plan-diagram`
subcommand. Blocked on the sub-recipe scheme existing.

## Non-goals

Re-doing the existing diagram. It works, it is tested, and it stays until there is something better
to show.
