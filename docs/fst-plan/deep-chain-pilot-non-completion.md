# Why the deep-truncation-chain grammar's `recipe-optimize` pilot never completes

Status: investigation, 2026-07-29. Answers why `samples/data/aweti.json` (private corpus; the
grammar with the ~41-rule zero-width-truncation cascade over a 24-level derivation chain — see
`p6-deep-truncation-chain-report.md`, `synthetic-stress-grammar-plan.md`) exhausts every
`recipe-optimize` deadline tried (208 words/1800s, 12 words/150s, 3 words/900s — all
`budget-exhausted`, `certifying: false`, `counts`/`pilot` both `null`) regardless of corpus size.

## 1. Root cause: an unbounded full-HC oracle call, not the FST/`apply_up` path

`pg_foma::recipe_runtime::evaluate_plans_marked` (`rust/crates/pg-foma/src/recipe_runtime.rs:275-`)
computes the ground-truth `expected` analyses for every evaluation — pilot or main search — with:

```rust
// recipe_runtime.rs:296-306
let compose = ComposeBudget::from_env().with_step_timeout(/* build budget only */);
let morpher = pg_parse::Morpher::new(grammar, usize::MAX);           // <-- cap = UNBOUNDED
let expected: Vec<_> = words
    .iter()
    .map(|w| (w.clone(), morpher.parse_word(w).structured))          // <-- no word_timeout either
    .collect();
let report = crate::emit::emit(grammar).report;
```

`Morpher::new`'s `cap` parameter is a step budget; `usize::MAX` means "never trips"
(`pg-parse/src/morpher.rs:58`: *"`usize::MAX` = uncapped (the C# behavior)"*). No
`.with_word_timeout(..)` is ever chained onto this `Morpher`, so the independent wall-clock bound
(`pg-parse/src/morpher.rs:66-71`) is also off. **Both axes that could stop this call are disabled.**
This happens **once per call** to `evaluate_plans_marked`/`evaluate_plans`, and the pilot loop in
`pg-cli/src/recipe_optimize.rs:329-368` calls it once per pilot candidate — so it re-executes,
unbounded, before that candidate's `build_candidate`/`build_controllable`/`evaluate_via_tuned_emit`
work (the FST/foma side) ever runs.

### This is a known, previously-documented hazard — for this exact grammar and this exact word

`rust/crates/pg-foma/examples/p6_templated_q3_oracle_bounds.rs` (already in the repo, predating this
investigation) documents, in its own source comments:

> "`Morpher::new(&g, usize::MAX)`... is NOT actually safe/bounded for Aweti: the corpus's 2nd word,
> `"tomoʼatu"`, ran for >10 minutes with neither bound ever tripping before this diagnostic killed it
> externally (verified: two independent processes calling this exact API, both stuck on the identical
> word)."

That example works around the hazard by using `Morpher::new(&g, 20_000)` instead — a deliberate
deviation from the "task brief" cap of `usize::MAX` it was originally asked to use, precisely because
the unbounded call does not return. `recipe_runtime.rs`'s call is the exact API shape the example
found broken, un-worked-around.

`samples/data/aweti-words.txt` line 2 (1-indexed) is `tomoʼatu`. `AdaptivePolicy::default()`
(`recipe_optimizer.rs:446-456`) sets `pilot_word_cap = 8` and `pilot_candidate_cap = 8`, so the
pilot's word slice is `words.iter().take(8)` — the first 8 lines of whatever word list the CLI was
given. Every one of the three measured word lists (the full 208-line corpus, a 12-line prefix, and a
literal 3-line file confirmed still on disk at `C:/Users/johnm/AppData/Local/Temp/aweti_3.txt`
containing exactly `parua` / `tomoʼatu` / `muʼazan`) includes `tomoʼatu` inside its first 8 (or
fewer) words. That is why the failure is **word-driven, not corpus-size-driven**: the pilot's very
first candidate evaluation reaches `tomoʼatu` in its `expected` computation and never returns,
consuming the entire external deadline before a first checkpoint, regardless of whether the corpus
has 3 or 208 words.

### Fresh reproduction (this build, this checkout, HEAD `eb4eed4`)

A throwaway diagnostic (`rust/crates/pg-foma/examples/deep_chain_pilot_bisect.rs`, written for this
investigation and deleted afterward — see below) called the exact `Morpher::new(&g, usize::MAX)`
shape on `"tomoʼatu"` from a detached, large-stack thread with a 20-second `recv_timeout`, then
repeated the call with each of the two available bounds:

```
grammar loaded: 135 mrules, 855 entries
UNCAPPED cap=usize::MAX word_timeout=None word="tomoʼatu" DID NOT COMPLETE within 20.0027505s
CAPPED   cap=20000       word_timeout=None word="tomoʼatu" completed in 91.6161ms  analyses=2 capped=true  timed_out=false
UNCAPPED cap=usize::MAX  word_timeout=2s   word="tomoʼatu" completed in 2.8272794s analyses=0 capped=false timed_out=true
```

This directly confirms, against the current source, that:
- The exact unbounded shape `recipe_runtime.rs` uses does not complete within 20s (consistent with
  the pre-existing >10-minute report — this run did not wait that long, it only needed to disprove
  "completes quickly").
- A finite step cap (`20_000`, the same value the existing oracle-bounds example already uses)
  returns in well under 100ms, with `capped: true` (a legitimate partial-but-terminating result).
- The independent wall-clock bound (`with_word_timeout`) also works standalone, on the unbounded step
  cap, returning at ~2s with `timed_out: true`.

Either existing, already-implemented `Morpher` knob — `cap` or `word_timeout` — independently turns
this from a hang into a millisecond-to-low-second bounded call. Neither is threaded into
`recipe_runtime.rs` today.

## 2. Does `ComposeBudget`/`EnumerationBudget` trip? No — they are never reached

`ComposeBudget` (`compose_budget.rs`) guards the **foma composition path** inside
`build_candidate`/`build_controllable`/`finish_controllable_net` (state/arc caps, default
`2_000_000` states / `20_000_000` arcs — calibrated in `phase-b-compose-budget-design.md` §8 with
generous headroom over Aweti's real compiled network, ~10,609 states / ~298,830 arcs per
`aweti-performance-follow-on.md`). `EnumerationBudget` (`morphotactics.rs`) guards the eager
enumeration/`preexpand`/`emit` path.

Neither type has anything to do with `pg_parse::Morpher` — that is a different crate (`pg-parse`,
the full-HC oracle port), reached through a code path (`recipe_runtime.rs:302-306`) that runs
**before** `build_candidate` is ever called for any candidate in a given `evaluate_plans_marked`
invocation. Since the oracle call for `tomoʼatu` does not return, execution never reaches:
- `build_candidate`/`build_controllable` (where `ComposeBudget` checks live),
- `crate::emit::emit(grammar)` (recipe_runtime.rs:307 — where `EnumerationBudget` would apply), or
- any FST proposal/confirmation work at all.

So the honest answer is not "a budget trips" — no budget is even consulted. The one call that could
stop this (`Morpher`'s own `cap`/`word_timeout`) is deliberately left at its "off" values by
`recipe_runtime.rs`, for both the pilot loop and the main search loop.

This also means the earlier attribution in the (uncommitted, in-progress) four-grammar evidence
write-up — *"this is the pre-existing enumeration / `apply_up` explosion for Aweti-shaped
grammars"* — is not the operative cause for `recipe-optimize` specifically. The `apply_up`/FST-side
explosion was real (see `p6-deep-truncation-chain-report.md` §1) but was already fixed by the
chain-restriction change (`dfb5025`): post-fix, `apply_up` on the previously-pathological probe word
completes 2,000,000 raw results in ~2.1s, and the full P6 compile is sub-second
(`aweti-performance-follow-on.md`). `recipe-optimize` never gets far enough to exercise that
(fixed) path at all — it is stuck earlier, in the unrelated, unbounded oracle call this document
identifies.

## 3. Is there a bounded configuration that works today? No — the CLI has no knob that reaches this call

`recipe-optimize`'s `Budget` (`--elapsed-ns`, `--build-ns`, `--memory-bytes`, `--confirmation-work`,
`--reserve-ns`, `--candidates`, `--evaluations`) and `RuntimeBudget`
(`build`/`confirmation`/`apply`/`proposals`/`states`/`arcs`, all optional) have **no field that maps
to a `Morpher` step cap or word timeout**. Grepping `recipe_runtime.rs` and `recipe_optimize.rs`
confirms `Morpher::new` is called exactly once, hardcoded to `usize::MAX`, with no
`.with_word_timeout(..)` anywhere in either file. Every `--*-ns`/`--*-work` flag only bounds the
**external supervisor's** process-level deadline/memory watch
(`run_recipe_optimize_supervised`, `recipe_optimize.rs:672-745`) or the `ComposeBudget`'s step
timeout for the (unreached) FST build. None of them can make the oracle call itself return sooner —
they only change *when the whole child process gets killed*, which is exactly the
`budget-exhausted`/`certifying: false`/`counts: null`/`pilot: null` shape observed at every deadline
tried.

**No invocation of `recipe-optimize` against `aweti.json` with the shipped word list produces a real
report today**, for any combination of `--candidates`/`--evaluations`/`--elapsed-ns`/`--build-ns`.
The capability to bound this exists and is already proven correct and shipped elsewhere in this same
codebase: `pangloss batch <grammar> <words.txt> <out.tsv> --word-timeout-ms N` (or `--step-cap N`)
threads straight into `Morpher::with_word_timeout`/the same `cap` parameter
(`pg-cli/src/main.rs:991-1047,1150-1168`). `recipe-optimize` simply never wires an equivalent flag
into its own `Morpher::new` call.

A word list that happened to omit every oracle-pathological word (not just `tomoʼatu` — the corpus
was not swept for others under this investigation's scope) could avoid tripping this specific hang,
but that is curation around a defect, not a working bounded configuration of the tool, and it would
be silently fragile: the next word list drawn from the same corpus could reintroduce the identical
non-termination with no diagnostic pointing at why.

## 4. What would have to change

1. **Thread a finite bound into `recipe_runtime.rs`'s oracle `Morpher`.** Either a step cap (mirror
   `pangloss batch --step-cap`) or a `.with_word_timeout(..)` (mirror `--word-timeout-ms`) — both
   already exist as public, tested `Morpher` methods; this fresh probe confirms either one
   independently turns `tomoʼatu` from a >20s hang into a 91.6ms/2.8s bounded call. This requires
   editing `recipe_runtime.rs` (out of scope for this investigation — see its constraints) and
   probably extending `RuntimeBudget`/`Budget`/`RecipeOptimizeArgs` with the new knob so the CLI can
   set it, the same way `batch` already exposes `--word-timeout-ms`/`--step-cap`.
2. **Decide the right default and the right failure semantics for a capped/timed-out oracle side.**
   `certify_corpus`/`certify_word` compare `expected` against `actual` as multisets; a `capped: true`
   or `timed_out: true` `expected` is a **partial** ground truth, not a complete one — using it
   as-is could silently under-certify (a candidate that actually recalls everything might be scored
   against a truncated oracle result and look wrong, or a genuinely-incomplete candidate could look
   right against an equally-truncated oracle). This needs an explicit typed outcome (something like
   today's `ResourceBreach`/`Truncated` certifications) rather than quietly feeding a partial
   `expected` into `certify_corpus`.
3. **Separately, sweep the Aweti corpus for other oracle-pathological words** (this investigation
   verified `tomoʼatu` only, per the pre-existing example's own scope) so the eventual bound is
   calibrated against the real worst case, not just the one word already known.
4. Until (1)-(2) ship, `recipe-optimize` cannot produce a real report for this grammar at any corpus
   size or deadline — this is not a tuning problem (no `--elapsed-ns` is large enough in practice,
   and no combination of the existing budget flags reaches the stuck call at all).

## Reproduction notes

- Diagnostic example used: `rust/crates/pg-foma/examples/deep_chain_pilot_bisect.rs` (written for
  this investigation, **deleted after use** per this task's constraints — not part of any gate).
  Build: `cargo build --release -p pg-foma --example deep_chain_pilot_bisect` (managed entry point
  `rust/tools/pg.ps1` unavailable in this session — PowerShell's `Microsoft.PowerShell.Management`
  module fails to load here — so plain `cargo` was used directly, per this task's explicit
  constraint).
- The full `recipe-optimize` CLI was deliberately **not** re-run live during this investigation:
  other `pangloss.exe` processes were already running in this checkout at the time (sibling
  agent-driven measurement work, confirmed via `Get-Process pangloss`), and this repo's own
  measurement-hygiene convention (`four-grammar-recipe-evidence-2026-07-28.md`,
  the preserved 2026-07-29 follow-on evidence) requires a serial, contention-free process count for
  any timing to be trustworthy. The preserved artifacts from an earlier serial run
  (`status.json`/`partial-report.json` recording `elapsed_ns: 900857254400` against
  `words: "C:/Users/johnm/AppData/Local/Temp/aweti_3.txt"`, confirmed still on disk as exactly
  `parua`/`tomoʼatu`/`muʼazan`) plus the isolated, freshly-built oracle probe together fully account
  for the observed behavior without needing a fresh multi-hundred-second run.
