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

Measured by `examples/backend_envelope_report.rs`. Admission is only an envelope verdict, so it was
checked against the compiler: `pangloss fst-health` on both languages reports

```
admission=LargeMultiplier (representability=WithinLimits, readiness=LargeMultiplier,
                           containment=WithinLimits, process=WithinLimits)
```

**`representability=WithinLimits` for both.** The single finding is readiness-tier, and this repo's
own classification says `LargeMultiplier` is class `Readiness` and blocks nothing. So Aweti and
Mbugwe are representable and buildable, not merely un-refused.

**A trap worth keeping.** I first "verified" this with `pangloss batch`, which reports
`engine=default` and builds a `Morpher` -- that is the **HC oracle**, not `TunedSurfaceProbed`.
Those numbers (Mbugwe 83 ok / 71 timeout over 155 words at 0.1GB; Aweti 15 ok / 5 timeout then an
abort at the 19GB job cap) say nothing about the backend this work expanded. `fst-health` is the
instrument that does. I nearly reported oracle behaviour as backend behaviour.

Two live facts fall out of that mistake rather than being wasted:
- **Aweti exhausts 19GB on a single word through the HC path.** `procgov` failed its allocation and
  killed it rather than taking the machine to the 118GB of the historical incidents -- the kernel
  ceiling doing exactly its job.
- **Mbugwe's HC path is latency-bound, not memory-bound**: 0.1GB RSS, ~54% of words analysed, the
  rest timing out at 10s.

## G9. I re-pointed the unpushed `v0.2.0` tag after rebasing the release branch onto a moved `main`

**Decided.** `main` had advanced two commits (`db3d9dd5`, `2d93dc52`: docs plus the run-slot pool in
`pg.ps1`/`_common.ps1`) since the release branch forked, so the linear-history rule required a rebase
before the fast-forward. The rebase rewrote every branch commit, including `release: v0.2.0`
(`23778a0e` -> `7ea75b66`), and the annotated tag still pointed at the orphaned original. The tag had
never been pushed, so I moved it (`git tag -f -a v0.2.0 7ea75b66`) and pushed `main` and the tag
together.

Considered: (a) push the branch un-rebased and let `main` take a merge commit -- rejected by the
repo's own no-merge-commits rule; (b) leave the tag on the orphaned commit -- a tag whose commit is
not in `main`'s history is exactly the "release not reproducible from its tag" state `release.ps1`
exists to prevent; (c) re-run the whole release gate sequence on the rebased tree before tagging --
what I did NOT do.

**Why (c) was skipped, and what would make that wrong:** the two commits `main` gained touch no Rust
crate source (`git diff --stat ea604151..2d93dc52` is CLAUDE.md, docs, reports, and `rust/tools/*`),
so the Rust tree under the moved tag is byte-identical to the one all six release gates passed on.
The tools tree is not identical, and for that part I re-ran what the release gates would have:
`rust/tools/tests/run-all.ps1` (16 files, 0 failed) and `comment-hygiene.ps1` (0 violations). The
oracle and rustdoc gates were not re-run; both read only Rust source and fixtures, which did not
change. **Falsified if** `git diff --stat ea604151..2d93dc52 -- rust/crates` is non-empty.

**To grill:** should `release.ps1` refuse to tag unless the branch is already a descendant of
`origin/main`, so this situation cannot arise? It would have forced the rebase BEFORE the gates ran
rather than after, at the cost of a network round-trip in a script that otherwise never touches the
remote.

## G8. Mbugwe drops two circumfix entries at grammar-compile time

**Found while proving the above, not yet fixed.** Loading Mbugwe warns twice:

```
unsupported: circumfix cross-product allomorphs (entry "577b6780-...") not implemented; entry skipped
```

`compile/affixes.rs:60` returns `None` for any entry whose `lexeme_morph_type` is
`MorphType::Circumfix`. Note this is a **warning, not a refusal**: the grammar loads with two
morphological rules missing, and nothing downstream knows the word list can never be fully analysed.
That is the file's own signature defect -- a control that cannot act, staying quiet -- sitting in the
loader rather than the envelope.

The capability itself is not missing: `Role::CircumfixPrefix` exists and Aweti exercises it
(mrule 40), derived by `classify_affix` from an allomorph whose RHS inserts on both sides. What is
missing is the FieldWorks path: a circumfix-typed entry whose left and right forms need a
cross-product to become such allomorphs.

### The data, read out of `mbugwe.fwdata` rather than assumed

Entry `577b6780` holds three `MoAffixAllomorph` records:

| record | form | `MorphType` guid | meaning |
|---|---|---|---|
| LexemeForm `f2774ad4` | `kaa- -iyɛ` | `d7f713df` | circumfix (display form) |
| AlternateForm `f252f2b2` | `kaa` | `d7f713db` | **prefix half** |
| AlternateForm `7da95189` | `iyɛ` | `d7f713dd` | **suffix half** |

The halves are already present as ordinary prefix- and suffix-typed allomorphs; the lexeme form is
only a combined display string. "Cross-product" means prefix-typed x suffix-typed -- here 1x1.

### The design this implies

`build_concatenative` (`compile/affixes.rs:441`) builds `Shape::Prefix` as
`insert("kaa+") · Copy(Input(0))` and `Shape::Suffix` as `Copy(Input(0)) · insert("+iyɛ")`. A
circumfix is exactly the composition:

```
lhs = [Pattern { nodes: any_plus }]
rhs = [ insert("kaa+"), Copy(PartRef::Input(0)), insert("+iyɛ") ]
```

which is what `crate::emit::classify_affix` already reads as `Role::CircumfixPrefix` -- the role
Aweti's mrule 40 exercises today. **So the emitter needs nothing new; only the loader does.** Replace
the early `return None` at `affixes.rs:60` with a partition of `rule_form_allos` by morph type and a
cross-product over the two buckets.

### Why I did NOT implement it tonight

Two open semantic questions, and this file's own history says guessing at them is how attempts get
reverted:

1. **Environments.** Each half may carry its own `PhoneEnvRC`. Does a circumfix allomorph require
   the prefix half's left environment AND the suffix half's right environment simultaneously, or
   does HC drop them? `build_concatenative` returns an `EnvironmentDef` per half today; combining
   them is a linguistic decision, not a mechanical one.
2. **Inflection classes and MPR.** The halves can disagree. Union or intersection changes which
   stems the rule applies to.

Both are answerable from `HCLoader.cs:1048-1332`, which `pg-snapshot/src/lexicon.rs:5` already cites
as the reference for exactly this cross-product. That reading is the next step, not more inference
from the Rust side.

**To grill, and it is a genuine tension.** Until it is implemented, should the warning become a typed
refusal? It would make three currently-invisible missing rules visible across Mbugwe and Aweti --
this file's own "a control that cannot act must say so" -- but it would refuse the two grammars
tonight's work just got admitted, and `fst-health` would flip from `representability=WithinLimits` to
`CannotRepresent`. I have left it as a warning rather than take that trade unilaterally.

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
