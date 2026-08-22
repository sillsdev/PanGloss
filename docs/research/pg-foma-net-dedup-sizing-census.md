# The net-level candidate dedup sizing census (`tests/net_dedup_sizing_census.rs`)

The sizing measurement for net-level candidate dedup, committed BEFORE the optimization it
justifies.

## The question

`evaluate_plans_with_cache_mode` builds, finishes and runs the whole corpus for every plan.
Nothing notices that two plans produced the same network. The optimization that follows this file
collapses those duplicates, and its entire value is the ratio measured here: across a fixture's
materialized candidate set, how many DISTINCT finished networks are there, against how many plans?

If that ratio is 1:1 everywhere, the optimization is worth nothing on our corpora and the honest
finding is worth more than the code. So the number is taken first, with the instrument
(`finished_net_digests`) and this census committed as their own change, and it stays as a
permanent gate: a future change that made every plan produce a distinct network would silently
remove the whole basis for the dedup, and this test is what says so out loud.

## Why the digest is taken after `finish_controllable_net`

That is the last point at which a plan-composed candidate is still an `Fsm`, and it is the net
that is actually queried. Digesting the pre-finish net would key on a network that returns nothing
for every surface query (`crate::build::finish_controllable_net`'s own doc), and digesting the
plan instead would measure the wrong thing entirely — plan-shape differences are ERASED by
minimization, which is exactly why duplicates exist.

## What this census deliberately does not do

No oracle, no propose, no confirm. It is the build half only, which is what makes a sweep over the
whole discoverable fixture corpus affordable. A distinct-net count needs nothing else: the
measurement is a property of the compilation.

## Why no fixture is excluded

`deep-optional-affix-nesting`/`recipe-template-generic` are not excluded: this census runs no
propose/confirm, and measurement shows it survives them — `finished_net_digests` for that
grammar's registry plans completes in a fraction of a second, since the death seen elsewhere is
entirely in `apply_up` traversal (`12^k` paths for a k-`x` word), never in construction. See
`tests/apply_path_refusal_gate.rs` and
`pg_foma::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET`.

# The net-level candidate dedup mechanism gate (`tests/net_dedup_gate.rs`)

Pins that net-level candidate dedup fires, that it changes nothing it reports, and that a cached
measurement cannot cross a grammar, a corpus, or an evidence mode.

## What is being optimized, and why it is sound

Plan-shape recipes are ERASED by minimization — measured spread 0 across 8 fixtures, and all five
Indonesian plan-composed permutations landed on identical states/arcs with identical proposals. So
`evaluate_plans_with_cache` was paying a full propose + confirm + whole-corpus traversal for
candidates whose finished networks are bit-identical. Net-level dedup collapses those.

Score attribution is TRIVIALLY sound here, and that is the whole reason this shape was chosen over
confirmation-memoization: identical networks legitimately have identical deterministic scores, so
nothing becomes order-dependent. Contrast a set-difference confirmation scheme, which is sound as
a RESULT but unsound as a MEASUREMENT, because each candidate's measured cost would become a
function of its position in the evaluation order — exactly what `Score::key`'s "why work and not
time" section exists to prevent.
`dedup_moves_no_certification_and_no_deterministic_score_field` is the assertion that keeps it
that way.

## Every test here is a NEGATIVE control by construction

`RunEvaluationCache::without_net_dedup` is not a convenience; it is the falsifier. Every claim in
this gate is stated as "dedup ON versus dedup OFF", so each test genuinely fails if the mechanism
is reverted or neutered — a same-path-twice comparison would pass whatever the mechanism did.

## Why `guesser-pattern-root-fallback` is the named firing fixture

The original `backend-ordered-generic` census fixture later acquired a
`CompositeEmissionMarker`. The PlanComposed compiler now correctly refuses those marker-bearing
plans instead of producing a potentially incomplete network, so that fixture no longer reaches
the dedup boundary. `guesser-pattern-root-fallback` has no structural marker or phonological
rewrite, while its distinct partition plans minimize to the same finished network. It therefore
exercises completed-network reuse without weakening the honest marker refusal.

It was `recipe-gated-generic`, which the same census reports as plans=5 digested=3 DISTINCT=3
**duplicates=0** — so all four fire-count-guarded gates failed on `nets_deduped() > 0`, exactly as
their own assertion message predicted. The guard earned its place: without it these four would
have passed VACUOUSLY, since dedup-on and dedup-off are trivially identical on a fixture where
dedup can never fire.

If this fixture ever stops producing a duplicate, re-run the census and pick another completed-net
witness rather than relaxing the guard or bypassing a capability refusal.

## `grammar_identity` hashes a non-canonical projection (known RED test)

`the_grammar_identity_is_stable_and_moves_for_a_single_allomorph_field` is `#[ignore]`d as a known,
real, already-diagnosed defect, not a TODO: `grammar_identity` hashes the grammar's derived
`Debug` projection precisely so that no field can be forgotten, but that projection is NOT
canonical, because the grammar tree holds hash-ordered collections as struct fields
(`pg_grammar::chardef::CharDefTable::lookup`'s `HashMap<String, CharDefId>`, `featsys`'s
`symbol_index`/`id_to_flat`, among others). Rust's `RandomState` is seeded per `HashMap` instance,
so two independent loads of the SAME grammar hold identical contents in different iteration order,
print different `Debug` output, and hash to different digests.

**Which direction it fails in, because that decides how urgent it is: it fails SAFE.** An unstable
identity means a key never matches across loads, so a cached measurement is never reused where it
should not be — the failure costs reuse, never correctness. The net-level dedup in this crate is
RUN-SCOPED and holds one `&Grammar` for the whole run, so it is unaffected and its own gates pass.

**What it does break:** any PERSISTENT, CROSS-RUN cache keyed on this identity — exactly the
design of a queued persistent oracle cache keyed on (grammar identity, word, step cap, memory
ceiling). That cache would silently never hit until this is fixed.

**The fix is NOT** "sort the HashMaps in `Debug`" — a `Debug` impl written to be canonical is a
`Debug` impl someone will later edit for readability, and the whole point of hashing the derived
projection was that no field can be forgotten. Prefer an explicit canonical serialization of the
semantic content, following the `ModelRevision` precedent that already split semantic from
presentation-only fields for this same class of reason.

The test's second assertion (flipping one `is_bound` moves the identity) is the property worth
keeping either way, and it has never been reached. Re-enable the whole test with the fix.

## Why a dedup hit re-runs the budget breach ladder on its own score

This is the one place a naive dedup would smuggle evaluation order into a CERTIFICATION, and the
production optimizer makes it live rather than hypothetical: `pg_cli`'s evaluator calls in once
per candidate with `build: Some(remaining.build)` — a budget that DECLINES as the run proceeds. So
the same network, measured at call 1 under a generous allowance and hit at call 20 under a nearly
exhausted one, must produce call 20's verdict, never call 1's inherited one.
