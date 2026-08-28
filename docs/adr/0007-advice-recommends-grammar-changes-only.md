# Advice recommends grammar changes only

The backend advice catalog carries remedies a language owner can apply **to their grammar**. A
condition no grammar change can address gets a typed finding and no advice entry. The catalog is not
a general troubleshooting index.

## Why

The catalog once held an entry, `backend-build-unavailable`, for "the backend is unavailable or its
compiler process failed". Its only remedy was `retry-backend-build`. That is not a grammar change —
it is what you do when PanGloss itself is broken or not installed, and the grammar under analysis is
irrelevant to it.

The distinction matters because of who reads a remedy. Advice is rendered to a language owner as
"if its prerequisites hold, this change would make this backend work for your language". Attaching
that sentence to a missing compiler tells someone to go edit a grammar that is not what went wrong.
For a linguist who cannot inspect PanGloss's internals, that is worse than silence: it converts our
defect into their homework, and any time they spend acting on it is wasted by construction.

Every remedy such an entry could plausibly offer is also already rejected elsewhere in this
repository: automatic retry, "increase the envelope", and cross-backend substitution advice are all
on the demolition ledger's rejected list. An entry whose entire remedy space has been refused is not
an entry with a gap in it; it is an entry that does not belong.

## What this does not mean

A build failure is still **reported**, and reported as a typed thing: `BuildProcessFailed` when
there was no tool to run, `BackendCompilationFailed` when a compile ran and failed, both at
`NotProductionReady` with the failing strategy named. Removing the advice removes a
grammar-directed recommendation, never the diagnosis. "We could not build this backend" remains
fully visible; it just no longer arrives dressed as something the language owner did.

Nor does this say infrastructure problems should go unexplained. It says the explanation does not
belong in a catalog whose rendering contract, safety warning, and effort ratings are all built for
grammar edits. If operational guidance is wanted later, it needs its own channel with its own
audience — not a shape key here.

## Consequences

- `validate_catalog`'s rule that every entry carries at least one remedy stays as it is. It was
  briefly tempting to relax it so the entry could remain with no remedies, which would have kept a
  non-grammar problem in a grammar-advice book and weakened a contract that usefully guarantees
  every diagnosis is actionable.
- `BackendReport::missing` and `BackendReport::failed` produce empty `shapes` and
  `advice_references`. `backend_selection_contract`'s
  `missing_and_failed_backends_are_typed_errors_carrying_no_grammar_advice` pins exactly that, so
  re-adding an entry fails a test that says why.
- The catalog's `REQUIRED_SHAPES` list is nine shapes, all of them grammar constructs.

## How this surfaced

The entry's last remedy was deleted by `12deffdb` ("remove retry backend advice") without noticing
it was the entry's only one. `validate_catalog` then rejected the catalog, `builtin_catalog()`
started returning `Err`, and two `.expect(...)` call sites in `backend_selection.rs` turned that
into a panic — which accounted for **15 of the 18** failures in `pg-foma` at the time. The bug was a
missing remedy; the fix was not to supply one.
