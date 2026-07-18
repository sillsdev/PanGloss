# Plan: deterministic candidate pre-filter between FST propose and HC confirm

Status: PLANNED 2026-07-16 (John's direction: "pre-prune deterministically after the FST
finds candidates, but not embedded in the FST itself" — or constrain candidates so the HC
check gets faster). Execute AFTER round-2 perf commits land AND after the knob teardown
(`2026-07-16-knob-teardown-plan.md`) — the teardown preserves `ConstraintCatalog`, which
this plan consumes, and the parallel-confirm work touches the same seam.

## Why this exists (the money)

- Confirm cost = mrule/template unapply cascade, 88–99% of confirm time.
- **Sena spends 97% of confirm time on FAILING candidates** (candidate precision 5%:
  51.5 candidates/word, 2.6 real). Amharic 31% precision, Indonesian 65%.
- Three attempts to push precision INTO the network (flags/eliminate/compose) all failed
  structurally. A Rust-side predicate has none of those costs: exact (not the 20/72
  over-approximate flag subset), zero network growth, zero propose slowdown, and it can
  decline per-constraint for free.
- Caution flag: AllFlags' env encoding only cut candidates ~0.25% (though the ones it cut
  were expensive: 15% confirm saving). An EXACT check over ALL constraints should do better,
  but this is unproven — which is why Phase 0 measures before anything is built.

## The soundness contract (never negotiable)

A pre-filter verdict `Reject` is a claim that HC confirm would provably return ZERO analyses
for this candidate. Recall is the one invariant this pipeline may never break
(FST proposes at 100% recall; HC prunes). Therefore:

- Every filter must be an UPWARD over-approximation of HC acceptance: uncertain ⇒ `Keep`.
- Every filter ships in SHADOW mode first (verdicts computed and logged, nothing dropped),
  with a hard assertion that no `Reject`ed candidate is HC-confirmed. Only after
  shadow-clean runs over all three grammar corpora AND the conformance suites does the
  filter flip to ENFORCE.
- Semantics must be REUSED from `pg_rules::validity` (compiled matchers via `RuleCache`),
  never re-implemented ad hoc. Divergent reimplementation is how recall bugs happen
  (see the miseru under-generation trap in the knob design doc).

## What a candidate is (the filter's input)

`pg_foma::confirm::Candidate` = morpheme sequence (`Vec<MorphemeId>`) + designated
`root_index`; `build_morpheme_owners` maps each morpheme to its `LexEntry` or `MRule`
(`resolve_pins`, confirm.rs). The candidate has LINEAR ORDER and (from the FST match)
implicitly a surface segmentation. Note: environments/co-occurrence attach to ALLOMORPHS;
candidates carry morphemes. Where the allomorph is ambiguous, the filter must test
"does ANY allomorph of this morpheme survive" — upward-safe by construction.

## Phase 0 — rejection census (measure BEFORE building; go/no-go gate)

Instrument confirm to attribute FAILING-candidate TIME (not counts — counts already exist)
to categories, per grammar, over the standard corpora (knob_probe word sets):

- (a) **derives but fails the final validity gate** — `allomorphs_valid_cached_traced`
  already emits which of the 11 `FailureReason`s fired (env, stem-name, co-occurrence, …).
  These are the deterministically pre-checkable rejections.
- (b) **cascade dead-end** — the unapply cascade never produces a synthesis-matching
  derivation at all. NOT pre-checkable by a per-candidate predicate; only addressable by
  constraining the search (Phase 3 stretch) or accepting the cost.
- (c) anything else (positional-routing mismatch, pin-resolution `None`, timeouts).

Deliverable: a table (grammar × category × % of failing-candidate wall time + counts),
plus the FailureReason breakdown within (a).

**Go/no-go:** if category (a) is under ~10% of failing time on every grammar, STOP —
report and recommend against building the filter (the cascade itself is then the only
target, and that is a different plan). Do not build the framework speculatively.

## Phase 1 — filter framework (shadow mode)

Insertion point: in `pg_foma::analyzer` between candidate dedup and `confirm_batch` — one
place, before chunking, so chunk fusion sees the thinned set.

- `CandidateVerdict { Keep, Reject { reason } }`; a filter is
  `fn(&Grammar, &[Option<MorphemeOwner>], &Candidate, word: &str) -> CandidateVerdict`.
- Modes: `Off | Shadow | Enforce` (builder flag on the analyzer; default Shadow in probes,
  Off in production until Phase 3 flips it).
- Shadow bookkeeping: per-filter counters (would-reject, kept), and the soundness assert:
  a would-rejected candidate that confirms non-empty is a hard failure in tests/probes
  (panic) and a logged counter in production.
- Reuse `ConstraintCatalog` (kept by the knob teardown) for env-instance enumeration and
  per-constraint decline bookkeeping.

## Phase 2 — concrete filters (build only what Phase 0 justifies, in this order)

1. **Exact adjacency environment check** (flagship; Sena has 144 REQUIRED envs, 0 excludes,
   and — decisive — ZERO phonological rules).
   - Test each candidate morpheme's allomorph `RequiredEnvironments`/`ExcludedEnvironments`
     against the candidate's own surface segmentation, using `pg_rules::validity`'s
     compiled environment matchers (OR over the env list, anchors, natural classes — exact
     `environments_ok` semantics).
   - **Static soundness precondition, per grammar (or per constraint):** no phonological
     rule / stratum processing can rewrite the tested window between the candidate's
     lexical concatenation and the shape `environments_ok` ultimately sees. Sena: no phon
     rules ⇒ globally sound. Amharic/Indonesian: decline (filter disabled) unless a
     provable subset emerges; declining costs nothing here, unlike in the FST.
   - Surface spans per morpheme: recover deterministically from the FST match / allomorph
     shapes. If span recovery is ambiguous for a morpheme, treat as unknowable ⇒ `Keep`.
2. **Morpheme co-occurrence rules** (`MorphemeCoOccurrenceRuleDef`, validity.rs W6): a pure
   predicate over the candidate's `MorphemeId` sequence (order available for the Adjacency
   variants) — mirror `co_occurrence_rule_ok`. No phonology involved ⇒ sound everywhere.
   Reference grammars have zero instances (build-for-full-scale rule: implement anyway ONLY
   if trivial after filter 1's framework exists; it exercises no corpus, so its shadow
   validation is vacuous — gate it behind a unit fixture instead).
3. **Root-level gates** (bound-root, stem-name required/excluded, W5): cheap `root_index`
   checks; same treatment as 2.
4. Anything else Phase 0's FailureReason table surfaces with real time behind it.

## Phase 3 (stretch, separate go decision) — constrain the confirm search itself

If Phase 0 shows failing time is dominated by category (b) cascade dead-ends, the
per-candidate predicate can't help; the lever is passing MORE of the candidate into the
cascade — e.g. the candidate's linear morpheme ORDER (confirm currently admits an unordered
rule SET), pruning unapply branches inconsistent with the proposed order. This is genuinely
riskier: unapplication order ≠ surface order under templates and unordered strata, and the
round-2 lesson stands (cross-chunk memo sharing looked obvious and was UNSOUND). Requires
its own design pass; do not attempt as a rider on Phases 1–2.

## Phase 4 — enforce + measure

- Flip Shadow → Enforce only after: shadow-clean on all three corpora (zero soundness-assert
  hits) + `cargo test -p pg-foma -p pg-rules -p pg-parse --release` + conformance/parity
  suites green.
- Identity bar: confirmed-analysis multiset byte-identical per word, all grammars, Enforce
  vs Off.
- Report: `confirm_total_ms` and candidates-killed-by-filter per grammar (knob_probe),
  plus filter cost itself (must be ≪ the cascade time it saves; if filter cost is
  noticeable, it runs per candidate before chunking — consider batching per rule-set).

## Process (repo conventions)

Worktree agents (Sonnet): `git merge --ff-only main` first; copy fixtures
(`samples/data/*.xml`, `*-words.txt`) and `knob_probe.rs` from the main checkout, never
commit them; BEFORE/AFTER knob_probe numbers; commit to worktree branch with the
`Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer; no commit if any identity
bar fails — report the failure verbatim instead. Main loop reviews diffs and cherry-picks.
Phase 0 is a separate agent/report from Phases 1–2 (census first, build second).
