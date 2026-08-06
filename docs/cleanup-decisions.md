# Cleanup decisions

Each item was raised by an agent that hit the edge of what it could safely decide alone, then verified
centrally. Grouped so one answer releases a batch rather than a single file.

Plain English on purpose. If an item cannot be explained without jargon, it is not ready to be asked.

---

## DECIDED

### D1 — Yes, the public API is ours to change *(decided; being done now)*

There are no external users of this code, so anything exported but uncalled can simply be deleted, and
function signatures can change without a deprecation path. Five things were stuck behind this:

- **A function nobody calls.** `synthesize_guessed_stem` is exported from the parser and has zero
  callers anywhere. A near-identical sibling is the one actually used, so the risk of keeping it is
  that someone later picks the wrong one.
- **A value nothing can produce.** One variant of the evidence type can never be created — everything
  that makes evidence produces the other kind.
- **A parameter that blocks tidying.** One function insists on being handed a cache even when there
  isn't one, which is the only reason four near-identical entry points can't share a single path.
- **Four separate settings knobs** on the parser, where the synthesis side already groups its
  equivalents into one object.
- **A result type with eight public fields**, three of them admittedly just diagnostics, and three
  booleans that are really one outcome wearing three hats.

### D2 — Collapse the duplicated rule-application code *(tracked, deliberately not now)*

The rules engine writes the same logic out once per combination of "cached or not" and "traced or not".
That's about **700 near-identical lines**. Collapsing it is the biggest single structural win available.

**Why we're waiting:** doing it means deciding exactly which things vary — and the subrecipe work is
about to change those very things. Do it now and we choose twice.

**Trigger to revisit:** once the subrecipe build-out has settled which parts vary per strategy.

**Known cost of waiting, so it isn't a surprise:** the two halves can drift apart silently. The code
itself already warns about this — one of the cached functions documents a gate-ordering difference from
its uncached twin. If that drift ever causes a real bug, that's the signal to stop waiting.

---

## QUEUED — needs your call

### Q1 — A real bug: two functions ignore which table they're working on

Two functions assume "table zero" instead of looking up the right one. Seven similar functions right
beside them do the lookup properly, and the module's own header states the rule plainly: table zero is
never an implicit default.

On a grammar with only one table this is invisible. On a multi-table grammar those two functions are
looking at the wrong data. It's probably hidden today because multi-table grammars get refused earlier
on — which is exactly why it would surface later, once that refusal is lifted.

**Ask:** fix it now with a multi-table test, or file it?

### Q2 — Two questions only you can answer about intended behaviour

Neither is a tidying question; both are about what the system is *supposed* to do.

1. **When the caller supplies part of the word themselves, should we still try all the different
   orderings of the pieces?** One path does, the near-identical other path doesn't. Nobody has written
   down whether that difference is intentional.
2. **Three functions have a bug that a fourth, nearly identical one already had fixed.** The fix is
   known and understood. Applying it changes behaviour, so it needs your say-so rather than being
   slipped in as cleanup.

### Q3 — Make the "cite your evidence" check stricter

Comments here can cite the test that proves them. A deleted test used to verify that a cited test
genuinely lives in the file it claims and really is a test. What replaced it only checks the name
exists *somewhere*.

**This is the check that would have caught, automatically, all three cases this session where a comment
cited a test that does not exist** — each of which claimed the opposite of what the real test asserts.
Cheap to build; highest value per hour of anything on this list.

### Q4 — Two loose ends from removing dead code

- A JSON output dropped two fields but its version number stayed the same, so a consumer can't tell
  old from new.
- Five comments described one test fixture while the code loaded a different one. The comments are
  fixed; nobody has checked whether swapping the fixture was itself correct.

### Q5 — A build hang that will keep happening

Builds are limited to two at a time. Twice today both slots were held by builds that had already
finished: the wrapper we use to cap memory had latched onto a shared background service that never
shuts down, so the wrapper never exited and never released the slot. Nothing was running; nothing else
could start. Cleared by hand.

The automatic cleanup can't fix this, because from the outside those processes look alive and owned.

**Ask:** worth fixing properly — stop the wrapper adopting that service, and teach the cleanup to spot
a wrapper with no compiler under it?

### Q6 — Two small placement questions

- A small type just got its own file during the deletion work. It pairs 1:1 with something in a
  neighbouring file. Keep it separate for clarity, or merge them? Cheap either way, cheapest now.
- A rule about how data is framed for hashing existed in one place, was deleted with the dead code, and
  is now re-implemented in one other place. Should it have a single named home again?
- The change ledger still lists the deleted subsystem as shipped.

---

## Queued: run the formatter once comments are clean

`rustfmt` now applies automatically before every mode that compiles, so this needs no new mechanism —
only a checkpoint. **When `impl-comment-too-long` reaches 0, run one managed build and confirm
`rustfmt: already formatted`.** The formatting pass is deliberately held until then: rustfmt rewriting a
file an agent is mid-edit in can invalidate that agent's view of it and cost its work.

28 hunks are outstanding as of writing; they will be absorbed by the first build after the sweeps land.

## Not a decision — just the remaining work

**Implementation comments over one line: ~4,330.** This is the backlog under the new one-line rule, and
it is deliberately untouched: the cleanup agents were briefed on the older three-line rule before it was
tightened, so a six-line comment cut to three still counts. Separately, **~1,205 long API docstrings are
exempt** and always were — about a quarter of what once looked like debt was never debt.

The next pass should attack this now that the checker can tell an interface docstring from an
implementation comment.
