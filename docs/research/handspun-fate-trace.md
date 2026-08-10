# What happened to the hand-spun compiler with switches?

Read-only history trace, worked in worktree `cleanup-and-recipe-parity`
(`C:\Users\johnm\Documents\repos\PanGloss\.claude\worktrees\cleanup-and-recipe-parity`). No code
was edited; no builds were run. Every claim below is cited to a commit SHA and/or a `file:line`,
read directly in this worktree unless marked otherwise. Section 6 states plainly what could not be
determined.

## 0. The answer, up front

**The hand-spun compiler was never deleted, migrated away from, or superseded. It is alive today
under the name `TunedSurfaceProbed`, and it is the ONLY backend `pangloss --engine=foma` actually
compiles with in production right now.**

- The hand-spun compiler is `crate::emit` (`rust/crates/pg-foma/src/emit.rs`,
  `junctions.rs`, `preexpand.rs`, `peel.rs`) — confirmed by its own module doc header
  (`emit.rs:1-47`, "a faithful, upward-approximating port of the retired C# `hc-hybrid`'s trie") and
  by `docs/fst-plan/recipe-parity-plan-2026-07-30.md:1-3`, which names it explicitly: "the hand-spun
  (`emit::emit`, SurfaceProbed) compiler."
- That compiler is the SAME code as one of today's three `EmissionStrategy` variants:
  `EmissionStrategy::TunedSurfaceProbed`, whose own doc comment reads "`emit::emit`
  (surface-probed) + rules: whole-grammar, every construct covered"
  (`rust/crates/pg-foma/src/enumerate.rs:359-361`).
- `FomaProposer::EMISSION_STRATEGY` — the constant that decides what `--engine=foma` actually
  compiles — is hard-set to `TunedSurfaceProbed` (`rust/crates/pg-foma/src/analyzer.rs:185-186`).
  `pg-cli/src/main.rs:455-456` re-derives this as `GATED_BACKEND` and documents it as "The backend
  a `--engine=foma` run actually compiles with, and therefore the only one whose compatibility
  report licenses that run."

So the premise that the hand-spun/switches compiler "was migrated into one of the current
`EmissionStrategy` backends" is correct, but the direction of surprise should flip: it was never
migrated *out of* production. It is not a fourth, retired thing sitting outside the three-backend
menu — it IS the default member of that menu, and the default that ships today.

What DID happen, confirmed below: (1) the vocabulary around it was renamed twice in the last two
weeks (`recipe`→`compiler`/`backend`, informal "hand-spun"/"switches" → the formal glossary in
`CONTEXT.md`), which is almost certainly why it now reads as unfamiliar; and (2) a *different*,
still-experimental compilation strategy (`PlanComposed`, formerly "recipe-optimize") is being
measured against hand-spun with the explicit, stated goal of one day *replacing* it as the sole
path — but as of the most recent scoreboard in this repo, that has not happened.

## 1. Was "hand-spun with switches" ever a fourth thing outside the three `EmissionStrategy` variants?

No — and the record documents exactly this correction being made in real time. Commit
`e8185817` ("docs: three FST audits and the synthesis — menu on top, switches underneath", Wed Aug 5
2026) is the point where this shape was worked out:

> "The shape: a small fixed menu on top (a real block-diagonal seam — which whole-grammar compiler
> runs is an either/or), freely composable switches underneath (MPR/POS gating, alpha-variable
> resolution and morphotactic pruning already reuse unmodified across the hand-tuned grammars)."

"Switches" was never a name for a fourth compiler. It named the composable variations *within* one
menu item (one backend) — MPR/POS subrule gating, α-variable resolution, morphotactic pruning, and
similar. The "menu" (three items) is what later became "backend"; "switches" is what later became,
verbatim, "Switch" in the settled glossary. This was confirmed two days later in `CONTEXT.md`
(added by commit `bc5daf9e`, "context: settle the vocabulary — compilers, switches, compatibility
report", Fri Aug 7 2026), `CONTEXT.md:5-29`:

> "**Compiler.** PanGloss itself — the whole process that turns a HermitCrab grammar into an
> FST-backed analyzer. Singular. There is one compiler.
>
> **Backend.** One of three ways the compiler can emit. …
>
> **Switch.** An optional variation within one backend. Composable, and each must earn its place;
> added complexity is earned rather than assumed."

So the settled hierarchy is: one compiler → three backends (`PlanComposed`, `TunedSurfaceProbed`,
`TemplatedUnderlyingTokens`) → switches inside a backend. There is no room in this hierarchy for a
fourth, separately-named "hand-spun with switches" compiler, because "hand-spun" IS one of the
three backends (`TunedSurfaceProbed`) and "switches" always meant sub-techniques inside it, never a
sibling of it. The prior investigation's assumption that the three variants were "the whole space
of compilers" was correct in the sense that there is no fourth backend — it just didn't recognize
that `TunedSurfaceProbed` *is* the hand-spun compiler under its new name.

## 2. The rename chain, in order

1. Historically, informal docs called the mainline pipeline "hand-spun" (contrasted against a
   "recipe"/optimizer pipeline) and its internal knobs "switches." This is attested across
   `docs/fst-plan/recipe-parity-plan-2026-07-30.md`, `docs/research/handspun-technique-audit.md`,
   `docs/research/technique-index.md`, `docs/research/per-language-fst-synthesis.md`, and code
   comments in `rust/crates/pg-foma/src/{lib.rs,net_shape.rs,backend_optimizer.rs,
   backend_runtime.rs}` (all found via `git log --all -S"hand-spun"` and a live `grep -rl`).
2. Commit `e8185817` (Aug 5) works out the menu/switches shape described in §1.
3. Commit `bc5daf9e` (Aug 7, `CONTEXT.md` added) settles vocabulary as Compiler/Backend/Switch/
   Compatibility report/Selector, explicitly rejecting "recipe" as a name ("a recipe implies the
   same dish each time and is overloaded in build tooling") and rejecting "switch" as the name for
   the top-level menu items ("a switch can change what is REPRESENTABLE, not just how fast").
4. Commit `f49d12ae` ("rename: retire recipe vocabulary for the pg-foma backend/optimizer
   machinery", the most recent commit on `main` per this worktree's `git log`) is the mechanical
   follow-through: renaming `recipe_*` identifiers to `backend_*` across the `pg-foma` crate
   (`recipe_registry.rs`→`backend_registry.rs`, `recipe_optimizer_design`→`backend_optimizer`
   module, etc. — confirmed by `git ls-files rust/crates/pg-foma/src | grep backend_`).

At no point in this chain is `emit::emit`/`crate::emit` itself touched, moved, or renamed — only
the words used to TALK ABOUT it and its siblings changed. This matches what `git log -S"HandSpun"`/
`-S"hand_spun"` (identifier-form, case-sensitive) finds across all branches: nothing — "hand-spun"
was always prose/doc-comment vocabulary, never a Rust type or module name, so there was no
identifier to rename or delete in the first place.

## 3. What "the optimizer becomes the only path" actually says

The task's paraphrase — "the optimizer becomes the only path once conformance plus four corpora
reach hand-spun parity" — is not a verbatim quote anywhere in the repo, but its content is real and
is stated plainly in `docs/backend-choice-plan.md:387` (a 2026-08-10 planning document, the most
recent dated doc found on this topic):

> "...the optimizer-endgame decision (**optimizer becomes the ONLY path once it beats hand-spun on
> the gates**) needs to know which of these is the shipping shape."

And the explicit target/goal statement is `docs/fst-plan/recipe-parity-plan-2026-07-30.md:1-3`:

> "Goal: the recipe optimizer reaches or beats the hand-spun (`emit::emit`, SurfaceProbed) compiler
> on the four language corpora."

This means: hand-spun (`TunedSurfaceProbed`) is the **baseline to be matched or beaten**, not a
superseded draft. "Optimizer" here refers to the `PlanComposed` backend plus its candidate-search
machinery (`backend_registry.rs`, `backend_optimizer.rs`, `backend_runtime.rs`,
`enumerate.rs::enumerate_default`) — what used to be called `recipe-optimize`. The plan is
conditional and has NOT been executed: as of the same document's own scoreboard
(`recipe-parity-plan-2026-07-30.md:9-14`, measured 2026-07-30, release binary):

| Corpus | Status |
|---|---|
| Indonesian | Optimizer ahead on every metric |
| Sena | Split — optimizer's net is 50x smaller but ~1300x slower to apply (`net_shape.rs:7-19`) |
| Amharic | No result — 600s search budget exhausted at 7 candidates |
| Aweti | First certified candidate exists on a 6-word pilot slice only; full corpus not attempted |

`docs/research/README.md:164` independently tags this same document's own verdict as **MIXED**:
"Can the optimizer match the hand-spun emitter on the real corpora? The current living scoreboard;
it corrects two other documents by name." No later, more conclusive document supersedes this
scoreboard in this worktree's history. So: the trigger condition for "optimizer becomes the only
path" has not fired. Hand-spun (`TunedSurfaceProbed`) remains what production ships, and remains
the reference every parity claim is measured against.

## 4. What the four grammars' corpus gates actually exercise, per grammar

Declared in `rust/tools/corpus-manifest.json` (current HEAD, `f49d12ae`). Each row below traces the
declared test to its actual imports/calls, read directly in the test source, to determine which
compiler pipeline it drives: **Path A** = hand-spun/mainline (`crate::emit`,
`crate::analyzer::FomaProposer`, `TunedSurfaceProbed`) vs. **Path B** = the prototype/optimizer
pipeline (`crate::uflexc`, `crate::gate`, `crate::replace`, `crate::templated_compile` —
`PlanComposed`/`TemplatedUnderlyingTokens` machinery). This Path A/B split is
`docs/research/handspun-technique-audit.md`'s own framing (§1, `handspun-technique-audit.md:30-64`),
verified independently here against the actual test files rather than taken on the audit's word.

### Sena — `corpus-manifest.json:63-65`

- **`pg-foma --test f1_large_lexicon_gate`** (all functions, no sub-test named).
  `rust/crates/pg-foma/tests/f1_large_lexicon_gate.rs:1-9`: imports
  `pg_foma::analyzer::FomaProposer` and `pg_foma::emit` directly, loads real `sena-hc.xml`.
  **This is Path A — the actual hand-spun/`TunedSurfaceProbed` compiler.** `#[ignore]`d
  unconditionally (needs the gitignored real corpus; run with `--include-ignored`).

### Amharic — `corpus-manifest.json:85-88`

- **`pg-foma --test f3_interdigitation_gate`** (all functions).
  `rust/crates/pg-foma/tests/f3_interdigitation_gate.rs:1-11`: imports
  `pg_foma::analyzer::FomaProposer`, `pg_foma::composite::FomaAnalyzer`, `pg_foma::emit`,
  `pg_foma::peel::ReduplicationPeeler`, loads real `amharic-hc.xml`. **This is also Path A —
  hand-spun.** `#[ignore]`d unconditionally.
- **`pg-foma --test p6_gate_parity amharic_gated_subrules_and_tuple_counts_unregressed`**.
  `rust/crates/pg-foma/tests/p6_gate_parity.rs:472-548`: loads real `amharic-hc.xml`, but only
  calls `gate::find_gated_subrules`, `gate::partition_entries`, and
  `replace::compile_and_compose_rules` — **Path B**, the prototype rewrite-rule-cascade compiler,
  not `crate::emit`. It checks the un-gated `compile_and_compose_rules` entry point reproduces
  fixed numbers (82 states / 1,110,358 arcs) and that 3 gated subrules (`prule1`/`prule2`/`prule3`)
  are found — nothing here runs `FomaProposer`/`emit::emit`.

  **So Amharic is the one grammar with a corpus-manifest-declared gate on BOTH paths**: a real
  hand-spun gate (`f3_interdigitation_gate`, ignored) plus a real Path-B prototype gate
  (`p6_gate_parity::amharic_gated_subrules_and_tuple_counts_unregressed`).

### Indonesian — `corpus-manifest.json:40-44`

- **`pg-foma --test p6_gate_parity indonesian_full_corpus_parity_unregressed`** and
  **`indonesian_mpr_exclusion_matches_oracle`**.
  `rust/crates/pg-foma/tests/p6_gate_parity.rs:11-14` imports
  `pg_foma::gate::{compile_gated_grammar, find_gated_subrules, partition_entries}`,
  `pg_foma::replace::{compile_and_compose_rules, SegAlphabet}`, and
  `pg_foma::uflexc::emit_underlying` directly. `indonesian_full_corpus_parity_unregressed`
  (`p6_gate_parity.rs:389-466`) compiles via `compile_gated_grammar` (`gate.rs:279-338`), whose own
  body calls `emit_underlying_filtered_with_budget` — an `rust/crates/pg-foma/src/uflexc.rs`
  function (`gate.rs:152,338`). **This is Path B — the `uflexc`/plan-composed emitter, NOT
  `crate::emit`.** It runs the real `indonesian-words.txt` corpus and asserts 100% recall
  (97/97 analyses), so it is a genuine, real-corpus recall gate — just not one that exercises the
  compiler that actually ships in `--engine=foma`.
- **`pg-foma --test backend_runtime_net_is_queryable_gate corpus_indonesian_confirms_after_the_finish_step`**.
  `rust/crates/pg-foma/tests/backend_runtime_net_is_queryable_gate.rs:1-97`: calls
  `enumerate::enumerate_default`, `backend_registry::Registry::seeded().materialize_distinct`, and
  `backend_runtime::evaluate_plans` — this materializes and scores candidates across MULTIPLE
  `EmissionStrategy` values (whichever the registry seeds), asserting only that at least one
  candidate reaches `FullHcConfirmed`. It is optimizer/candidate-registry machinery, not a
  dedicated test of the hand-spun compiler specifically, though `TunedSurfaceProbed` is one of the
  candidates it can materialize.

  **So none of Indonesian's three declared corpus-manifest tests specifically and only exercises
  `crate::emit`/hand-spun; all three go through Path B (`uflexc`/`gate`/`replace`) or the
  multi-candidate optimizer registry.** This is a genuine, verified gap: Indonesian has no
  ignored-but-real analogue of Sena's `f1_large_lexicon_gate` or Amharic's
  `f3_interdigitation_gate` in the manifest today.

### Aweti — `corpus-manifest.json:107-110`

- **`pg-foma --test p6_templated_morphotactics_gate`** (all functions).
  `rust/crates/pg-foma/tests/p6_templated_morphotactics_gate.rs:1-27`: module doc states "needs
  the gitignored real corpus," and imports `pg_foma::emit::{emit_underlying_templated, FomaTier}`,
  `pg_foma::replace::compile_and_compose_rules_recall_safe`, and
  `pg_foma::templated_compile::compile_templated_morphotactics`. **This is Path B — specifically
  the `TemplatedUnderlyingTokens` backend, not `TunedSurfaceProbed`/hand-spun.** This file exists
  and is real; it is the ONE gate Aweti's corpus manifest declares that actually exists in the repo.
- **`pg-foma --test compose_recall_aweti_gate`** — **confirmed not to exist anywhere in this
  repository's history, on any branch.** Checked three independent ways:
  1. `git log --all --diff-filter=A --name-only -- "*compose_recall_aweti_gate*"` — no output (no
     file matching that name was ever added on any reachable commit).
  2. `git log --all --name-only --format="" -- "*compose_recall_aweti_gate*"` — no output.
  3. `grep -rn "fn compose_recall\|mod compose_recall"` over the current worktree — no output; no
     `[[test]]` entry in `rust/crates/pg-foma/Cargo.toml` names it either.

  The string `compose_recall_aweti_gate` exists ONLY as a literal in
  `rust/tools/corpus-manifest.json`'s `requiring_tests` array, first introduced by commit
  `40b5b47a` ("implement fail-closed corpus mode (build hardening, part 1 of 4)", Wed Jul 29 2026)
  and still present, unchanged, at current HEAD `f49d12ae`. It is a **declared-but-never-built
  gate**: the manifest promises a corpus-backed recall test for Aweti that was never written. This
  matches the task's stated expectation exactly.

### Summary table

| Grammar | Declared gate(s) | Exists? | Compiler exercised |
|---|---|---|---|
| Sena | `f1_large_lexicon_gate` | Yes | **Path A — hand-spun (`crate::emit`, `FomaProposer`)** |
| Amharic | `f3_interdigitation_gate` | Yes | **Path A — hand-spun** |
| Amharic | `p6_gate_parity::amharic_gated_subrules_and_tuple_counts_unregressed` | Yes | Path B (`gate.rs`/`replace.rs` prototype) |
| Indonesian | `p6_gate_parity::indonesian_full_corpus_parity_unregressed` | Yes | Path B (`uflexc::emit_underlying` via `gate::compile_gated_grammar`) |
| Indonesian | `p6_gate_parity::indonesian_mpr_exclusion_matches_oracle` | Yes | Path B (same) |
| Indonesian | `backend_runtime_net_is_queryable_gate::corpus_indonesian_confirms_after_the_finish_step` | Yes | Multi-candidate optimizer registry (not hand-spun-specific) |
| Aweti | `p6_templated_morphotactics_gate` | Yes | Path B (`TemplatedUnderlyingTokens`: `emit_underlying_templated` + `replace` + `templated_compile`) |
| Aweti | `compose_recall_aweti_gate` | **No — never existed on any branch** | N/A |

## 5. Reconciling: where did the four grammars' "worked really well" behavior actually come from?

`docs/research/handspun-technique-audit.md` — a from-source audit, not a plan — is unambiguous
about this for THREE of the four grammars, and its own §0 table (lines 9-28) names Sena,
Indonesian, Amharic, Aweti as "the four hand-tuned reference grammars." Its central finding (§1,
`handspun-technique-audit.md:36-41`):

> "What `pangloss batch|parse|fst-health|pack --engine=foma` actually compiles: `FomaProposer::new`
> … call `emit::emit_with_budget_profiled` … This is `emit.rs`/`junctions.rs`/`preexpand.rs`/
> `peel.rs` — call this **Path A (mainline)**."

The entire §2 technique catalogue (24 of 37 techniques tagged `[A]`) was discovered by running
hand-spun (`crate::emit`) against real corpus recall gates and fixing what it missed — e.g. §2.4
("Sena's corpus word `kubulukira` stacks THREE derivational suffixes … which depth 2 silently
loses," `emit.rs:24-32`), §2.8 (Sena's `"tun"`/`"tum"` char-def variant, "found as 13 of the first
recall gate's 19 misses," `emit.rs:91-95`), §2.10 (Indonesian's `meN+tulis → menulis`, `emit.rs:
109-127`), §2.12 (Amharic's interdigitation/boundary-fusion, `preexpand.rs:53-56`). Sena's own
mainline-only gate (`f1_large_lexicon_gate`) and Amharic's (`f3_interdigitation_gate`) are directly
the harnesses that surfaced these fixes, and both still run the same `FomaProposer`/`crate::emit`
path today (§4 above).

**For Aweti, this is less clean.** The audit itself flags Aweti as the outlier: §2.13 documents
that the hand-spun mainline enumeration path (`preexpand::extend`/`emit::struct_extend`) OOMs and
then crashes on the very first query for Aweti without the morphotactic-pruning and
`EnumerationBudget` fixes (§2.13-2.15), and even after those fixes "did not fix the end-to-end
usability problem" — the emitted network is still "unusably large" (§2.13,
`handspun-technique-audit.md:476-479`). And per §4 above, Aweti's one *existing* corpus gate
(`p6_templated_morphotactics_gate`) does not exercise hand-spun/`TunedSurfaceProbed` at all — it
exercises the `TemplatedUnderlyingTokens` backend (`emit_underlying_templated` + a compiled rewrite
cascade), and its own header cites `docs/research/pg-foma-p6-aweti-gate.md` for the "full
investigation history." **The record does not establish that Aweti's good behavior, to the extent
it has been measured at all, came from the hand-spun compiler** — the evidence in this repo points
the other way, toward `TemplatedUnderlyingTokens` being the strategy that was actually built and
gated for Aweti, precisely because hand-spun's enumeration blew up on it.

For Indonesian specifically, there is a further wrinkle: the audit's own historical framing (§1,
§2.10-2.11, §2.21) describes hand-spun's `PhonologyProbe`/junction machinery as what makes
`meN-` assimilation work, and that IS `crate::emit`/Path A — but per §4 above, none of the
corpus-manifest-declared tests for Indonesian today actually run that code path; they all run
`uflexc`/`gate.rs` (Path B) or the multi-candidate optimizer registry instead. So while the
narrative record says Indonesian's good behavior traces to hand-spun, the currently-wired
regression coverage for Indonesian does not independently confirm that on every run — it is
covered by `--engine=foma` being hand-spun by construction (§0/§3 above), not by a corpus gate that
names `crate::emit` directly.

## 6. What is established vs. inferred vs. unknown

**Established directly from code/config, not inferred:**
- Hand-spun = `crate::emit` = `EmissionStrategy::TunedSurfaceProbed` = what `--engine=foma`
  compiles with today, unconditionally (`analyzer.rs:185-186`, `pg-cli/src/main.rs:455-456`).
- "Switch" was never a name for a whole compiler; the settled glossary (`CONTEXT.md:28-29`) defines
  it as a variation *within* one backend, matching the informal usage that preceded it
  (`e8185817`'s commit message).
- The "optimizer becomes the only path" condition is stated as a still-unfired FUTURE trigger
  (`docs/backend-choice-plan.md:387`), not something that has happened; the most recent scoreboard
  this repo has (`recipe-parity-plan-2026-07-30.md`, tagged MIXED by `docs/research/README.md:164`)
  shows the optimizer beating hand-spun on Indonesian only, split on Sena, and incomplete on Amharic
  and Aweti.
- `compose_recall_aweti_gate` does not exist in this repository's history on any reachable branch;
  it has been a dangling name in `corpus-manifest.json` since it was introduced by `40b5b47a`.
- Per-grammar, which compiler pipeline each corpus-manifest-declared test actually drives (table in
  §4), read directly from each test file's imports and call chain.

**Inferred (a reasonable reading of verified facts, not itself a quoted statement):**
- The user's memory of "hand-spun compiler with switches" performing well on all four grammars
  maps most cleanly onto the informal Path-A/"switches" vocabulary attested in
  `handspun-technique-audit.md` and `technique-index.md`, which predates the `bc5daf9e`/`f49d12ae`
  renames by roughly two-to-five days at most (this repo's commits are all from a narrow window
  ending 2026-08-10) — i.e., the user very plausibly saw this system under its OLD name shortly
  before the rename landed, which is consistent with it now reading as unfamiliar/renamed even
  though the underlying code never moved.

**Resolved on a later read of `docs/research/pg-foma-p6-aweti-gate.md` (which §6 below had left
open): Aweti cannot ever have worked well on hand-spun, because hand-spun cannot compile it.** That
doc's opening states it directly — Aweti is "a grammar whose enumeration-based emitter
(`pg_foma::emit::emit`) OOMs before ever reaching a compilable lexc source (855 entries, 135 mrules
trip the composite pre-expansion stage's enumeration budget)", and `emit_underlying_templated` plus
a replace-rule cascade "is the first construction that gets Aweti's templated
(`<AffixTemplate>`-based) morphotactics past that wall at all"
(`docs/research/pg-foma-p6-aweti-gate.md:3-10`). So Aweti's gate runs
`TemplatedUnderlyingTokens` not by preference but by necessity: hand-spun has no result on this
grammar to be measured against. Any recollection that all four grammars worked well on hand-spun
holds for at most three of them.

**Not established / could not be determined from this repo's history:**
- Whether any branch OTHER than `main`/the ones surfaced by `git log --all` (this repo has roughly
  fifty worktrees, each potentially its own branch) contains a `compose_recall_aweti_gate`
  implementation that was simply never merged. `git log --all` does cover every locally-fetched
  branch tip, which should include worktree branches created in this same repository, but a branch
  that exists only in another worktree's uncommitted state (never committed at all) would not be
  visible to `git log --all` and was not separately checked.
