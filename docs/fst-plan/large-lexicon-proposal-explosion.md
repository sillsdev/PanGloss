# Large-lexicon proposal explosion: diagnosis (Sena, 2026-07-29)

> **SUPERSEDED (2026-07-30).** The bug this document diagnoses is fixed by
> `build::reroute_null_shaped_affix_chains` (commit `9cb569f`, pinned by
> `tests/boundary_marker_epsilon_collapse_gate.rs`) — a different mechanism than either fix
> proposed below (context-restricted deletion was tried and rejected for a real recall
> regression). Measured post-fix: **575** proposals on the 5-word slice, down from 53,992
> (~94×). Do not cite this document's proposal counts as current; the living evidence record is
> `docs/fst-plan/recipe-parity-plan-2026-07-30.md`. The diagnosis sections remain accurate as
> history of the mechanism.

## Question

Sena (`samples/data/sena-hc.xml`) now proposes ~1080 candidates/word through
`pangloss recipe-optimize` / `pg_foma::recipe_runtime::evaluate_plans`, after the mandatory
boundary-token cleanup step (`pg_foma::build::finish_controllable_net`, added in `76cf841`) fixed a
correctness bug that previously made the same evaluator query an unqueryable net (pre-fix: 49 total
proposals across 5 words, `multiplicity-mismatch` on `ca`). 50 words exceed a 300s deadline; the full
7121-word corpus exceeds 1800s.

Is the ~1080/word figure (a) genuine ambiguity the grammar licenses, (b) dead-ends the proposer
should statically exclude, or (c) a precision regression introduced by the boundary cleanup itself?

## Verdict: (c), decisively, with a code-level mechanism identified

This is **not** a case the standard dead-end-census d1-d6 taxonomy or its E1-E5 proposer-precision
encodings apply to. It is a correctness/precision bug in one specific, non-production build path
(`pg_foma::recipe_runtime::evaluate_plans`'s `build_controllable` + `finish_controllable_net`), and
it does not reproduce on the production analyzer path (`pg_foma::analyzer::FomaProposer::new`,
which every real `pangloss --engine=foma` invocation uses). No new proposer-precision encoding is
licensed by this finding — the FST *construction* itself needs a targeted fix, not a new dead-end
filter downstream of it.

## Why the standard census workflow didn't already show this

`.claude/skills/dead-end-census` was followed as instructed, but its harnesses
(`examples/worst_words.rs`, `examples/deadend_census.rs`) both build their network via
`pg_foma::emit::emit` (the "tuned emit" production path) — never through
`pg_foma::build::build_controllable` / `finish_controllable_net`, the path
`recipe_runtime::evaluate_plans` actually uses. The bug this mission was chasing lives entirely in
the path the standard census tooling never exercises. Running `deadend_census` therefore reproduces
old, already-known numbers and reports nothing anomalous — which is exactly what happened (see
"Census run" below) and is itself informative: it proves the explosion is confined to the
recipe-optimizer's own alternate network construction, not something wrong with Sena's grammar or
the production pipeline.

## The mechanism

`finish_controllable_net` (`rust/crates/pg-foma/src/build.rs:173-185`) composes a mandatory
"boundary cleanup" net before re-minimizing:

```rust
fn boundary_cleanup_net(...) -> Option<Fsm> {
    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    ...
    let cleanup_regex = boundary_tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    foma::regex::fsm_parse_regex(opts, &cleanup_regex, None, None)
}
```

This deletes **every** `Boundary`-kind char-def **identically and unconditionally**, in one
context-free parallel-replace regex, then composes it onto the whole network. It has to exist:
`uflexc`'s emitted lexc leaves these tokens as required literal characters in the network (the
commit message for `76cf841` confirms the pre-fix net was "unqueryable" — a bare surface query with
no literal boundary characters in it matched nothing), so *some* cleanup is mandatory for recall.

Sena's own char-def table (`samples/data/sena-hc.xml:369-388`) declares three semantically
**different** `Boundary` kinds:

| id | representations | apparent role |
|---|---|---|
| char41 | `+` | ordinary morph-boundary separator |
| char42 | `^0`, `*0`, `&0`, `∅` | **null/zero-morph marker family** — signals "a zero-realized morph occurred here", not a separator |
| char43 | `.` | another separator |

`boundary_cleanup_net` deletes char41, char42, and char43 the same way: unconditionally, with no
reference to which specific rule instance licensed that occurrence. char42 is not cosmetic — it is
exactly the kind of adjacency-carrying information the skill's own hard-won-lessons section already
names for a different erasure mechanism ("Flags cannot encode adjacency... Anything touching
environments uses composition over boundary-marked strings, where adjacency is native"). Composing
away every occurrence of a null-morph marker with one blanket rule converts what used to be
*required*, uniquely-identifying transitions into free/epsilon-like branches: for the same zero
surface characters, the automaton can now non-deterministically choose any of the (formerly
boundary-char-distinguished) continuation classes at that state, most of which are structurally
invalid combinations that used to be excluded by needing the correct literal boundary character.
Confirm then dead-ends on nearly all of them — that is the explosion.

## Direct A/B experiment (not assumed — measured)

Built a throwaway probe, `rust/crates/pg-foma/examples/boundary_cleanup_precision_probe.rs`
(deleted after this investigation, per task constraints), that runs the identical 5 words through
both paths on the identical grammar:

- **Path A** — production: `pg_foma::analyzer::FomaProposer::new(&g)` (`emit::emit`, never puts
  boundary tokens on the queryable tape — its own module doc: "boundary characters dropped,
  representation variants enumerated").
- **Path B** — the recipe-optimizer's own path: `enumerate::enumerate_default` →
  `build::build_controllable` → `build::finish_controllable_net`, driven only through the fully
  public `recipe_runtime::evaluate_plans` (one call per word, since the aggregate function only
  returns a summed `Score.proposals`), so `recipe_runtime.rs`/`build.rs` were never edited.

Commands:

```
cargo build -p pg-foma --release --example boundary_cleanup_precision_probe
./target/release/examples/boundary_cleanup_precision_probe.exe samples/data/sena-hc.xml <5-word file>
```

Words (first 5 lines of `sena-words.txt`, CRLF-stripped): `pibubu`, `piratu`, `mbali`, `n'nyumba`,
`ya`.

| word | Path A (production, states=106365/arcs=702364) | Path B (build_controllable+cleanup, states=2028/arcs=11620) | ratio |
|---|---|---|---|
| pibubu | 18 | 0 (`Truncated: no-analyzable-words`) | — |
| piratu | 2 | 16 | 8x |
| mbali | 104 | **53720** | **516x** |
| n'nyumba | 0 | 0 | — |
| ya | 3 | 256 | 85x |
| **total** | **127** | **53992** | **425x** |

This exactly reproduces the mission's reported figures (states=2028, arcs=11620, proposals=53992
for these 5 words — confirmed independently via `pangloss.exe recipe-optimize` directly, see
below). `mbali` alone is 53720/53992 = 99.5% of the total explosion; it is not a uniform ~1080/word
phenomenon, it is concentrated combinatorial blow-up on specific words, consistent with the
mechanism above (words whose morphology crosses a null-morph-marker boundary at a productive
slot — Bantu nasal-class prefixation, matching `mbali`'s and `ya`'s shapes — trigger it; words with
no such juncture, like `pibubu`/`n'nyumba` here, do not).

Broadened to the 8 pre-existing "worst word" pinned outliers
(`samples/data/sena-worst-words.txt`, pinned 2026-07-17 against the *production* path at
122-1327 candidates each, confirming in 1.2-2.1s each): **every single one** of the 8 now exceeds a
15-second per-word cutoff under Path B without finishing (`timeout -s KILL 15`, all 8 killed). Under
Path A (production, same grammar, same words) these are all sub-2.2s. This is not a marginal
regression on one word — it is systemic across every morphologically complex word tested.

## Confirming Path A is unaffected and the ambiguity there is old news

Ran the standard census (which is Path A, i.e. `emit::emit`) for cross-check:

```
CENSUS_SENA_CAP=3 timeout -s KILL 180 ./target/release/examples/deadend_census.exe sena 3
```

Result (11 words = 3 corpus words + the 8 pinned outliers, union'd automatically):
`pibubu`=18, `piratu`=2, `mbali`=104 — an **exact** match to the probe's Path A column, confirming
both measurements are sound and consistent. The pinned outliers now show 794-2250 candidates each
(up from their 2026-07-17 pin values of 122-1327) — real growth, but this is `emit::emit`'s own
long-known genuine-ambiguity story for Sena's long/complex words (candidate_precision=0.0133;
d4 shape-mismatch 47.1%, d3 feature-clash 31.3%, d5 ordering 21.6%, d1 environment negligible — the
same shape the 2026-07-17 census already documented for Sena, "72 env constraints and zero rewrite
rules... d5 dominated"). It is unrelated to `finish_controllable_net` (which `deadend_census.rs`
never calls) and explains none of the 425-516x blow-up seen on ordinary, short, simple words like
`mbali`/`ya` under Path B — those words are cheap and unambiguous under Path A.

## Ruling out (a) and (b)

- **(a) genuine ambiguity**: ruled out directly. The *same grammar*, queried against the *same
  words* through the analyzer that actually ships (`FomaProposer::new`), produces 127 total
  candidates, not 53992, for the identical 5-word set. Whatever ambiguity Sena's morphology
  genuinely licenses is already fully present in Path A's net (which is a proven-equivalent
  construction per `build_controllable`'s own module doc, modulo this cleanup defect) — it does not
  license the extra ~53865 candidates that only appear once the boundary tokens are blanket-deleted.
- **(b) staticaly-excludable dead-ends the proposer should have caught**: this framing assumes the
  network is a reasonably faithful, already-tightened representation of the grammar and the
  question is which further precision encoding (E1-E5) would tighten it more. That is not what is
  happening here: the exploded candidates are not "dead-ends the proposer failed to prune", they are
  candidates that are only *reachable at all* because a compose step erased the specific piece of
  information (which literal boundary/null-morph token occurred where) that used to make them
  unreachable. Fixing this is a correctness fix to the emission/cleanup construction, not a proposer
  optimization pass over an otherwise-sound net.

## Recommendation

No dead-end-census encoding (E1-E5) is licensed or relevant here — this finding does not enter that
decision framework at all, because the defect is upstream of confirm-side cascade attribution
entirely. Concrete direction for whoever fixes `finish_controllable_net` (out of scope for this
mission; not implemented here per the task's file-write constraints):

1. **Do not blanket-delete every `Boundary`-kind char-def with one context-free regex.** Sena's
   char-def table shows these are not interchangeable: a genuine null/zero-morph marker family
   (char42: `^0`/`*0`/`&0`/`∅`) is grammatically load-bearing, unlike plain separators (char41 `+`,
   char43 `.`). Treating all three identically is the root cause.
2. **Preferred fix**: mirror what `emit.rs`'s own tuned path already does correctly — never place
   these tokens on the queryable surface tape to begin with (enumerate representation variants at
   emit/`uflexc` time, the way `emit::emit`'s module doc describes: "boundary characters dropped,
   representation variants enumerated"), rather than emitting them into the network and hoping a
   post-hoc compose-time deletion is semantically safe. This is exactly why Path A doesn't have this
   problem; there is a working, proven reference implementation already in this codebase.
3. **Fallback fix** (if (2) is a larger change than warranted right now): make the cleanup
   composition context-restricted per boundary occurrence (an E1-style "adjacency is native via
   composition over boundary-marked strings" construction) rather than a global, unconditional
   deletion — i.e., only strip a boundary token within the specific environment the rule that
   inserted it actually licenses, not everywhere it appears in the alphabet.
4. Regardless of which fix lands, add a regression check that runs `recipe_runtime::evaluate_plans`
   (or equivalent) against a synthetic fixture carrying a null-morph-marker-shaped `Boundary`
   char-def and asserts propose/confirm cost stays within a small multiple of the production
   (`emit::emit`) path's own cost on the same words — the 2026-07-29 fix's own commit message
   already flagged that no checked-in fixture reproduces this pathology
   ("A synthetic fixture reproducing the boundary-token pathology is owed").
5. Separately, note as a tooling gap: `.claude/skills/dead-end-census`'s harnesses hardcode
   `emit::emit` and cannot see bugs confined to `build_controllable`/`recipe_runtime`. Anyone
   diagnosing a slow `recipe-optimize` run in the future should confirm which build path produced
   the numbers being censused before trusting the standard d1-d6 workflow's silence as a clean bill
   of health — exactly the miss this mission's own framing (test (c) directly) was designed to
   catch.

## Every command run

```
# Word-list hygiene (CRLF stripped, verified no gloss header lines present)
tr -d '\r' < samples/data/sena-words.txt | grep -v '^\s*$' > <scratch>/sena-words-clean.txt
head -5 <scratch>/sena-words-clean.txt > <scratch>/sena-words-5.txt

# Reproduce the mission's reported figures directly via the CLI
cargo build -p pg-foma --release   # (implicit via first cargo run below)
./rust/target/release/pangloss.exe recipe-optimize samples/data/sena-hc.xml <scratch>/sena-words-5.txt \
  <scratch>/sena-out5 --seed 17 --candidates 8 --evaluations 8 --elapsed-ns 60000000000
# -> report.md: states=2028 arcs=11620 build=60223300 apply=2587858200 proposals=53992 confirmation=77
#    (exact match on states/arcs/proposals to the mission's measured facts)

# Grammar/char-def inspection
grep -n "kind=\"boundary\"\|Boundary" samples/data/sena-hc.xml
grep -n "enum CharDefKind\|Boundary" rust/crates/pg-grammar/src/chardef.rs

# Code reading
#   rust/crates/pg-foma/src/build.rs (finish_controllable_net, boundary_cleanup_net)
#   rust/crates/pg-foma/src/recipe_runtime.rs (evaluate_plans_marked — the one production caller)
#   rust/crates/pg-foma/src/emit.rs (module doc: "boundary characters dropped, representation
#     variants enumerated" — the production path's own, working, alternate strategy)
#   rust/crates/pg-foma/src/analyzer.rs (FomaProposer::new / propose_budgeted's existing
#     (morphemes, root_index) dedup — ruled out raw-path duplication as an alternate explanation)
#   git show 76cf841   # the fix's own commit message, confirms "unqueryable" pre-fix mechanism
#   git show eb4eed4 --stat   # most recent commit, routes marker-requiring baselines to tuned emit
#     (does not touch Sena's path, which has no composite/structural markers)

# Direct A/B probe (throwaway, deleted after this investigation)
cargo build -p pg-foma --release --example boundary_cleanup_precision_probe
./rust/target/release/examples/boundary_cleanup_precision_probe.exe samples/data/sena-hc.xml \
  <scratch>/sena-words-5.txt
# -> Path A (tuned) total=127, Path B (controllable+cleanup) total=53992, ratio=425.1x

# Broadened to the 8 pre-existing pinned worst-word outliers, one word at a time, 15s cutoff each
grep -v '^#' samples/data/sena-worst-words.txt | grep -v '^\s*$' > <scratch>/sena-pinned-8.txt
# looped boundary_cleanup_precision_probe.exe over each word with `timeout -s KILL 15`
# -> every one of the 8 killed (no result within 15s) under Path B; all sub-2.2s historically
#    (and reconfirmed here) under Path A

# Standard dead-end-census cross-check (Path A only, per its own hardcoded emit::emit construction)
cargo build -p pg-foma --release --example deadend_census
CENSUS_SENA_CAP=3 timeout -s KILL 180 ./rust/target/release/examples/deadend_census.exe sena 3
# -> pibubu/piratu/mbali candidate counts match the probe's Path A column exactly; pinned outliers'
#    d1/d3/d4/d5 shape matches the grammar's pre-existing, already-censused ambiguity story
```

`rust/tools/pg.ps1` was not used, per this task's own explicit constraint that PowerShell is broken
in this environment (`Microsoft.PowerShell.Management` fails to load) and the managed entry point
cannot run here. Bare `cargo build`/`cargo run` were used instead, as directed by that constraint.
