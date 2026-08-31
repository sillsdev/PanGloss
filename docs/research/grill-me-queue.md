# Grill-me queue

**Result so far:** Aweti and Mbugwe went from **zero admitted backends to one each**
(`TunedSurfaceProbed`), matching Amharic and Indonesian. The refuse-both landing is reversed on
measurement, not by loosening a check -- the envelope still refuses everything it refused before
except the spelling cap, which was simply set too low. See G7.

| grammar | PlanComposed | TunedSurfaceProbed | TemplatedUnderlyingTokens |
|---|---|---|---|
| Aweti | refused | **ADMITTED** | refused |
| Mbugwe | refused | **ADMITTED** | refused |
| Sena | ADMITTED | ADMITTED | refused |
| Amharic | refused | ADMITTED | refused |
| Indonesian | refused | ADMITTED | refused |

Measured by `examples/backend_envelope_report.rs`. Envelope only: admission is not a successful
build, and the corpus runs that would prove recall have not been done.

Decisions taken without the user in the room, and questions raised that only the user can settle.
Each entry says what was decided, what else was considered, and what would falsify it. Newest first.

Working rule for this file: an entry goes in when a decision was **taken**, not when it was merely
contemplated. A queue of things I thought about is noise; a queue of things now true in the
repository is reviewable.

---

## G7. `REP_VARIANT_CAP` raised 64 -> 8192, after the measurement refuted my first diagnosis

**Decided, and the reasoning was wrong the first time round.** Read the correction before the entry:
I claimed enumeration was a category error and that raising the cap would NOT help Aweti or Mbugwe.
`root_variant_census` over all five reference grammars says otherwise:

| grammar | roots | overflow 64 | unbounded | worst finite |
|---|---|---|---|---|
| Aweti | 923 | 105 | **0** | **4096** |
| Mbugwe | 165 | **3** | **0** | **256** |
| Sena | 1450 | 0 | 0 | 8 |
| Amharic | 77 | 0 | 0 | 8 |
| Indonesian | 66 | 0 | 0 | 4 |

**Zero unbounded shapes in any real grammar.** `[Any]*` -- the witness the envelope's own doc cites --
comes from a conformance FIXTURE, not from the languages. Every overflowing root is `pattern=false`,
and the products are clean powers of two (`tạtupewỵpẹpo`, twelve nodes, 2^12 = 4096): ordinary roots
whose segments each carry two spellings. Mbugwe needs 256, and it needs it for **three roots**.

So the cap is 8192 now -- the measured worst with one doubling of headroom. The cost is bounded and
small: roughly 60k extra variant strings across Aweti, which become root lexicon entries.

**What I got right and am keeping:** one number still covers two unrelated populations. A finite
product is a size question that a bigger number answers; a Kleene star has no finite count and
overflows *any* cap, so raising this number is not a fix for `[Any]*` and must not be read as one.
The census reports the two populations separately for that reason.

**Then the suite forced the second half of the fix, and it is the important one.** Raising the cap
alone broke `the_published_root_spelling_fact_never_over_claims_a_drop` with:

```
no fixture exercised the root-spelling fact, so this gate proves nothing
```

A gate that refuses to pass vacuously, doing exactly its job. What it caught: at 64, the variant
count overflowed on Kleene-star shapes **by accident**, and that overflow was the only thing
reporting the star's truncation to `PATTERN_ITER_CAP`. Raise the cap and the count no longer
overflows -- so the star's under-generation goes **silent**, which is ADR-0001's forbidden direction.
I had written down that one number cannot cover two populations and then walked straight into it.

`pattern_variants` now reports `overflowed` for **either** cause: the finite product exceeding the
cap, OR any node carrying the iterative flag, whose language is infinite and which therefore drops
spellings at any cap value. The overflow message names both causes rather than claiming a count was
exceeded when a star was truncated. Result: all 9 envelope gates pass, the fact fires again on the
star fixtures, and Aweti and Mbugwe stay admitted -- they have zero star roots.

**To grill:** (a) a star is now refused rather than approximated. That is honest, but it means a
guesser-pattern root can never be represented by this route at all; is per-position union emission
(one arc, not a string list) worth building to make it representable? (b) I picked 2x headroom over
the measured max -- on five grammars. A sixth language with a fourteen-segment two-spelling root
needs 16384. Should this be a budget derived from the grammar rather than a constant? (c) is
`PATTERN_ITER_CAP` now dead weight, given a star is refused before its expansion can matter?

**Older, now-corrected framing follows for the record.**

`pattern_variants` (`emit.rs`) takes the cartesian product of every shape node's alternative
spellings and truncates at 64, reporting `rep-variant-overflow`. Two very different things hit that
ceiling:

- **Legitimate breadth.** The count is a product over segment positions. Optional tone diacritics
  double it per vowel; Mbugwe is Bantu, and a six-vowel stem with optionally-marked tone is exactly
  `2^6 = 64`. Nothing has gone wrong, and 64 is only "six binary choices" -- a *small* root.
- **Unbounded by construction.** The witness the envelope actually cites is `root shape "[Any]*"
  exceeds 64 representation variants`. A Kleene star over the union of the whole alphabet is not 65
  spellings, it is infinite. `PATTERN_ITER_CAP` truncates the star and `REP_VARIANT_CAP` then
  truncates the truncation -- two stacked approximations, neither visible in the verdict.

**The claim I want grilled:** enumerating a regular expression into a list of concrete strings is a
category error, and the cap is the symptom rather than the disease. `[Any]*` is one state with a
self-loop; a per-position union is what an FST stores in linear space. If that is right, then
**raising the cap does not make Aweti or Mbugwe work** -- it produces a much larger network that is
still wrong, spending the size budget for nothing.

**What would falsify it:** the census showing both languages' overflows are finite and modest (say
under 10^4). Then a bigger cap genuinely is the fix and the union rewrite is over-engineering.
That measurement is `examples/rep_variant_census.rs`, running now.

---

## G6. I extracted `node_alternatives` rather than letting the census re-derive it

**Decided.** The census needs the per-node alternative count that `pattern_variants` computes.
Considered: (a) re-derive the walk inside the census, (b) make `pattern_variants` return the counts,
(c) extract the per-node computation and share it.

Took (c). (a) is the exact shape this repo has burned four reverted attempts on -- a measurement that
re-derives the thing it measures can disagree with it, and then neither number is trustworthy. (b)
would force the emitter to materialize strings just to be measured, which defeats the census's whole
point: it must be able to measure a grammar too large to enumerate.

**Risk accepted:** the extraction is behaviour-preserving by inspection, not by a test that would
catch a subtle reordering. `pattern_variants` sorts and dedups inside the extracted function exactly
as before, so a divergence would have to be a compiler-visible change; `-Mode check` is green.

---

## G5. A finished background task leaks its build slot -- and the STALE warning was RIGHT

**Corrected.** I first filed this as a false positive: preflight kept reporting

```
slot 0: pid 12284 ... -- STALE: no compiler activity for 20+ min; 'pg.ps1 -Mode gc -Apply' will reap it
```

and I assumed it was libelling a healthy `-Mode test` in its execution phase. It was not. pid 12284
is the **`pwsh` that hosted a background task which had already completed**; it stays alive holding
the slot mutex long after its build work is done. The heuristic identified exactly that. My reflex --
"a warning about no compiler activity must be the compiler-activity heuristic being naive" -- was the
error, and it is the mirror image of the mistake this repo keeps cataloguing: I read a control that
WAS acting as one that could not.

**The real defect is upstream of the warning:** a managed build run as a harness background task can
leave its hosting shell alive holding the slot mutex after the build finishes. It is not every task
-- most released normally this session, and only pid 12284 stuck -- so the trigger is unidentified,
which is precisely what makes it worth recording rather than shrugging at. With `MaxConcurrent = 2`,
two stuck shells strand the machine.

**To grill:** should `Enter-BuildSlot` release on build completion rather than on process exit? A
mutex tied to process lifetime is exactly what makes abandonment safe (CLAUDE.md's own argument
against the counted semaphore), so this is a real tension, not an obvious fix.

## G5b. I orphaned a 32-line doc onto the wrong function, for the second time

**Caught and fixed before commit, by hygiene rather than by me.** Extracting `node_alternatives` out
of `pattern_variants`, I inserted it immediately after `pattern_variants`'s doc block -- so the
32-line doc welded onto the new private helper and `pattern_variants` was left undocumented. Exactly
the `replace.rs` mistake from earlier in this work.

What surfaced it is worth keeping: the doc had been counted as `api-docstrings-long` (informational,
1155) while attached to a `pub(crate)` item, and became a gated `impl-comment-too-long` the moment it
sat on a private `fn`. The count moving 14 -> 15 was the only signal; nothing about the code looked
wrong. **Inserting a function directly above another function's definition puts it inside that
function's doc.** Anchor such an edit on the doc block's first line, never on the `fn` line.

---

## G4-G1. The five questions from the grilling round, still open

Asked, not yet answered; the overnight goal partly overtook them but they remain live.

- **Q1 -- silent MISS.** Nine matrix cells compile and miss oracle-required identities without
  saying so; ADR-0001 forbids silent miss specifically. Close the silence (cheap, discharges the
  contract) or teach representability (expensive)? *The overnight goal answers this in the
  representability direction -- recorded here so the reversal is explicit rather than assumed.*
- **Q2 -- re-measure the matrix first.** Its numbers are 20+ commits stale, including a change that
  gated over-generation per backend. One run.
- **Q3 -- `pg-cli` nondeterminism, now larger than `stats_cmd`.** Three
  `recipe_optimize_continuation` tests failed in a full run, passed 4/4 in isolation immediately
  after, and had passed a full run an hour earlier with no relevant change between. That is six
  intermittent tests, all in `pg-cli`. Failing together under parallel execution and passing alone
  points at **cross-test interference** -- shared scratch state, an env var, a process-global --
  rather than the `HashMap`-iteration-order theory I offered for the `stats_cmd` pair. Chase the
  interference hypothesis first; it would explain both groups at once.
- **Q4 -- behavioral provenance.** `EvidenceProvenance` has one variant; ADR-0001 names two, and the
  missing one describes the production mainline. Amend the ADR rather than build an oracle-witness
  predicate nothing is blocked on?
- **Q5 -- realizational advice.** `PlanComposed x RealizationalMorphology` gets process-morphology
  advice via the catch-all, and no correct catalog entry exists. Emit no advice rather than invent a
  linguistic recommendation?
