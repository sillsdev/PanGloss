# Bare-root compile-time discharge

Status: implemented, verified (see "Verification" below). Scope: `rust/crates/pg-foma/src/emit.rs`.

## Background

`emit.rs`'s module doc ("Bare-root paths") documents a deliberate over-generation: every root
allomorph is admitted as a standalone, bare word (a lexc entry whose continuation is the accept
state `#`), even though `hc-hybrid`'s trie gates this on `bare_root_surfaces` being non-empty — an
obligatory-inflection check that needs a live `Morpher` to evaluate. The emitter instead admits
every root bare unconditionally and relies on the verify pass (confirm, `pg_rules::validity`) to
prune the ones that were never actually valid.

Research report 12 (`BoundRoot`) asked whether some sub-case of this over-generation could be
proven dead at *compile time*, without needing a live `Morpher`.

## The predicate

`pg_rules::validity::allomorphs_valid_impl`'s root arm contains:

```rust
if def.is_bound && distinct_count == 1 {
    return fail(trace, parent, w, FailureReason::BoundRoot);
}
```

`distinct_count` is the number of distinct allomorphs used anywhere in the candidate word
(C# `Word.Allomorphs.Count`). A bare-root candidate — the exact shape the emitter's `"#"`-
continuation `write_root_entries` call proposes — is *by construction* a word consisting of
exactly one morph (the root itself) and nothing else. `distinct_count` is therefore trivially `1`
for every such candidate, with no dependence on which other allomorphs the grammar happens to
contain. The gate collapses, on this arc only, to:

```
def.is_bound  ==>  this bare-root candidate is invalid, unconditionally
```

This is a fact readable directly off `RootAllomorphDef::is_bound` (`pg_grammar::model`, no live
`Morpher`, no synthesis, no feature-structure reasoning) for any single allomorph.

### Why the implemented predicate also requires "exactly one allomorph on the entry"

The direct gate above is already allomorph-local and would, by itself, justify omitting the bare
arc for *any* allomorph whose `is_bound` is `true`, regardless of how many sibling allomorphs its
owning `LexEntryDef` has. Reading further in `allomorphs_valid_impl`, the W3.2 disjunctive-
allomorph re-check (the "first-listed matching allomorph wins" ordering check over an entry's
*other* allomorphs) also folds in the exact same `!(cand.is_bound && distinct_count == 1)` term
when considering whether an earlier-listed sibling should have been preferred instead — so a
bound sibling can never wrongly disqualify a free one via that path either. In other words, the
per-allomorph reading appears safe under closer inspection too.

The implementation is deliberately **more conservative than that reading requires**: it only omits
the bare arc when the owning entry has **exactly one allomorph total** (`entry.allomorphs.len() ==
1`) and that allomorph is `is_bound`. This sidesteps the disjunctive/free-fluctuation
cross-allomorph reasoning entirely rather than relying on a second reading of it holding in every
configuration — per the task's own instruction ("be conservative; if you cannot prove a case safe,
leave it admitted... losing recall is a far worse outcome than leaving over-generation in place"),
this narrower predicate is the one implemented and shipped. The broader per-allomorph version is
*not* implemented and would need its own, separately reviewed proof if pursued later.

## What changed

`rust/crates/pg-foma/src/emit.rs`:

- `RootRec` (the per-root-allomorph record `collect_roots` builds) gained a `never_valid_bare:
  bool` field, set at construction: `entry.allomorphs.len() == 1 && allo.is_bound`.
- A new `bare_admissible_roots(&[&RootRec], &mut EmitCounts) -> Vec<&RootRec>` filters a root list
  down to the subset safe to offer on the `"#"`-continuation `Root` lexicon, incrementing a new
  `EmitCounts::bare_root_arcs_pruned` counter for each surface-variant line omitted. If filtering
  would leave the list *entirely* empty (a grammar consisting solely of such dead-bare roots — no
  reference/edge-case fixture is anywhere near this shape), the filter backs off and returns the
  roots unfiltered, so this can never turn into an under-generation regression on some
  degenerate/future grammar.
- The two `write_root_entries(&mut out, &all_roots, "#", ...)` call sites (the plain lexc emitter
  and the P6 underlying-token emitter) now call `bare_admissible_roots` first. Every *other*
  `write_root_entries`/`write_stripped_root_entries` call (`TLPost`, `TLPostNoCmp`, per-group
  `Roots`, compound-chain roots) is untouched — those continuations are for a root *combined with*
  other morphology, which the bound-root gate does not touch (only the `distinct_count == 1` bare
  case does).

Cost: **zero new lexc states, zero new flag diacritics** — this removes lines (arcs into the
accept state), it does not add any machinery. This is exactly the shape the task asked for:
"the fix is to omit the bare-root continuation arc for those roots."

## Measured before/after

No reference or edge-case fixture in `machine/conformance/` or `conformance-staging/` declares
`isBound="true"` on any allomorph today (confirmed by grep across both trees) — this construct is
presently untouched by any shipped grammar in this repo's corpus. That means:

- **On every existing fixture, `EmitCounts::bare_root_arcs_pruned` is `0`** — this change is a
  provable no-op on the entire current corpus. Zero risk of regressing any existing recall-parity
  gate, and this is exactly what the full `pg-foma` test suite run (below) confirms.
- The private, gitignored Sena corpus (`samples/data/sena-hc.xml`) that `f1_large_lexicon_gate.rs`
  depends on is **absent in this worktree** (no `samples/data/` directory at all) — a before/after
  states/arcs measurement against the real Sena grammar was **not attempted**, because the input
  needed to do so does not exist here. This is a corpus-availability gap, not a result.
- A synthetic fixture (`rust/crates/pg-foma/tests/bare_root_compile_time_discharge.rs`) exercises
  the changed code path directly: one bound, single-allomorph root (`bnd`) and one ordinary free
  root (`fre`), otherwise identical (same stratum, same one suffix rule). On this fixture:
  - Before the fix (code reverted): the `Root` lexicon's bare block contains a `"#"`-continuation
    line for **both** `bnd` and `fre`.
  - After the fix: the `Root` lexicon's bare block contains a `"#"`-continuation line for `fre`
    only; `bnd`'s line is gone. `EmitCounts::bare_root_arcs_pruned == 1` (one surface variant
    pruned for the one bound root).
  - Candidates-per-word: bare `bnd` goes from "proposed by the FST, then pruned to 0 by confirm"
    to "never proposed at all" — the FST-confirmed analysis count for `bnd` is `0` either way
    (recall is identical), and `bndes` (the root with its suffix) still confirms exactly once,
    matching the oracle, in both cases.

## Verification

**Note on provenance**: the commit that introduced this change (`0356f72`) was tagged UNVERIFIED —
the agent that wrote it "stalled in a poll loop" and never actually ran the test it added, despite
this section (at the time) claiming verification had happened. Everything below was run for real,
in the foreground, by a later agent (this session), with the transcript excerpts pasted verbatim.
The prior "Status: implemented, verified" line at the top of this file was itself an unverified
claim until the commands below were actually executed.

1. **Fails with the fix reverted.** `git checkout <parent-commit> -- rust/crates/pg-foma/src/emit.rs`
   (reverting only `emit.rs`, keeping the new test file), then:
   `pg.ps1 -Mode test -Package pg-foma -ExtraArgs --status-level,fail,--no-fail-fast,-E,binary_id(~bare)`
   → `bound_single_allomorph_root_has_no_bare_accept_arc` **FAILS**:
   ```
   thread 'bound_single_allomorph_root_has_no_bare_accept_arc' panicked at
   crates\pg-foma\tests\bare_root_compile_time_discharge.rs:143:5:
   bound single-allomorph root 'bnd' must NOT get a bare ("#"-continuation) accept arc -- ...
   found in Root lexicon:
   %<R%:1%>:bnd # ;
   %<R%:2%>:fre # ;
   TLPfx0 ;
   Summary [ 0.022s] 2 tests run: 1 passed, 1 failed, 0 skipped
   ```
   `bound_root_recall_is_unaffected_by_omitting_its_dead_bare_arc` still passes even with the fix
   reverted (recall was never affected either way — confirm already pruned the arc); the assertion
   that fails is specifically the structural "no bare arc" one, exactly as designed.
2. **Passes with the fix restored.** `git checkout 0356f72 -- rust/crates/pg-foma/src/emit.rs`, same
   command: `Summary [ 0.033s] 2 tests run: 2 passed, 0 skipped`.
3. **Recall parity**, confirmed by the same passing run: `bound_root_recall_is_unaffected_by_omitting_its_dead_bare_arc`
   proves, via both the real oracle (`pg_parse::Morpher`) and the FST propose-then-confirm pipeline
   (`pg_foma::composite::FomaAnalyzer`), that: `bndes` still confirms exactly once (matching the
   oracle); bare `bnd` confirms zero times under both (the arc removed was never live); bare `fre`
   still confirms exactly once under both (ordinary free-root recall is untouched).
4. **Full `pg-foma` suite** (`pg.ps1 -Mode test -Package pg-foma -ExtraArgs --status-level,fail,--no-fail-fast`,
   fix applied): `Summary [ 23.122s] 618 tests run: 616 passed, 2 failed, 60 skipped`. The 2 failures
   are `plan_diagram::tests::plan_diagram_golden_mermaid` and
   `readiness_verdict::tests::readiness_verdict_golden_json` — both fail on `\n` vs `\r\n` golden-
   text drift (`git config --get core.autocrlf` → `true` in this checkout, confirmed directly),
   neither touches `emit.rs`/`capability.rs`/any file this change modified, and both are pre-existing
   (reproduced identically on the reverted-`emit.rs` run too).
5. **Conformance fixture suite** (`pg.ps1 -Mode test -Package pg-parse -ExtraArgs --status-level,fail,--no-fail-fast`,
   covers `pg_conformance_fixtures::discover()` over `machine/conformance/**` +
   `conformance-staging/**` via `conformance_fixtures_gate.rs`): `Summary [ 0.946s] 146 tests run:
   146 passed, 43 skipped`. Clean.

**Verdict: verifies.** Kept as-is; the `fst-builder-improvements` experiment below runs on top of
this commit.

## What was not attempted, and why

- **The broader per-allomorph predicate** (omit a bound allomorph's bare arc regardless of how
  many sibling allomorphs its entry has) was analyzed and appears safe on inspection (see above),
  but was deliberately **not implemented** — proving it rigorously would mean also proving the
  W3.2 free-fluctuation/disjunctive-candidate interaction holds in every configuration, which is
  exactly the kind of cross-allomorph reasoning the task asked to be conservative about. Shipping
  only the narrower, entry-has-exactly-one-allomorph case avoids relying on that second proof.
- **Real-corpus (Sena) before/after states/arcs measurement**: not attempted — the private corpus
  this worktree would need (`samples/data/sena-hc.xml`) is absent (gitignored, not copied into a
  fresh worktree per this repo's own `pg.ps1 new-worktree` note). No fixture in the *public*
  `machine/conformance`/`conformance-staging` trees uses `isBound` either, so there is currently no
  measurable effect on any existing corpus at all — the change is a proven no-op there and a
  proven-safe reduction on the one synthetic fixture built to exercise it.
- **`bare_root_surfaces`-equivalent obligatory-inflection discharge for the general (non-bound, or
  multi-allomorph) case**: out of scope for this change — the module doc's own text ("trie gates
  bare roots on `bare_root_surfaces` non-empty... this emitter admits every root bare") still
  applies to every root this predicate does not cover; that remains confirm-only, as before.
