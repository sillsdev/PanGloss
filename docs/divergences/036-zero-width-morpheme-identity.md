# 036 — Zero-width morpheme identity preservation

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open upstream fix. Machine #500 proposes preserving a wrapped morpheme before adding the fallback zero-width morph. PanGloss was reported correct on the reproduction; no new Rust patch is needed on that evidence alone.

## C# site
`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs / MarkMorph`.

## Rust site
`pg-rules/src/morph.rs::attribute_morphs`.

## Evidence
#500's `sagui` reproduction must retain A2B and B2A in both analyses, with THIRD added only to the longer derivation. The PR contains unit tests and reported results. A stable shared conformance fixture is still missing; those reports were not rerun during this documentation cleanup.

## Remaining work
Do not equate correct surface or parse count with correct morpheme identity. Separate the dropped-ID fix from unresolved process-to-process annotation ordering (entry 037).

## Upstream
[PR #500](https://github.com/sillsdev/machine/pull/500); related remaining instability [issue #506](https://github.com/sillsdev/machine/issues/506).

## 2026-09-15 update — reproduction confirmed, measured splits, PanGloss confirmed correct

This entry's earlier "no new Rust patch is needed" line was second-hand (PR #500's own reported
results). This update is a direct, independent measurement against this repo's own build.

- **Reproduced the grammar directly.** Root `sag`/N; `mrA2B` (N->V, +u); `mrB2A` (V->N, +i);
  `mrThird` (N->V, `CopyFromInput`-only, zero-width) — byte-identical to the Machine oracle-506
  investigation's `grammar.xml`, now staged at
  `conformance-staging/edge-cases/zero-width-morpheme-identity-stability/`.
- **Measured hc.dll's instability independently.** A fresh 50-fresh-process sample of this exact
  grammar split three ways for `sagui`: 22/50, 19/50, 9/50 across the same three outputs an earlier
  independent sample found (25/50, 22/50, 3/50) — the SET of outputs is stable, the proportions are
  not, which is itself the evidence of genuine per-process nondeterminism. The same 50-run probe
  against Machine PR #500's head (`b15073bb`) still splits three ways: 20/50 correct
  (`ROOT+A2B+B2A+THIRD`), 24/50 drop THIRD entirely, 6/50 drop B2A. PR #500 fixes the
  THIRD-vs-B2A ORDERING some of the time; it does not fix the content-dropping, and hc.dll remains
  nondeterministic on its own head.
- **Measured PanGloss directly (this repo, this branch, before any code change).** Built the grammar
  via `pg_grammar::load` and drove `pg_parse::Morpher::parse_word` directly (no `pg-foma`/`pg-cli`
  build needed). 50 in-process parses of every word (`sag`, `sagu`, `sagi`, `sagui`), plus 20
  separate fresh-process runs of a compiled probe binary for `sagui` specifically, ALL produced the
  identical multiset, every time:
  - `sag` -> `ROOT|sag` ; `ROOT+THIRD|sag`
  - `sagu` -> `ROOT+A2B|sagu`
  - `sagi` -> `ROOT+THIRD+B2A|sagi`
  - `sagui` -> `ROOT+A2B+B2A|sagui` ; `ROOT+A2B+B2A+THIRD|sagui`

  This is exactly the forward-synthesized answer (THIRD outermost/rightmost, both A2B and B2A
  retained in both analyses). **No PanGloss production code was changed** to get this result — it
  was already the engine's behavior. Confirmed by reading `pg-rules/src/morph.rs::attribute_morphs`:
  a zero-width affix (no `Origin::Affix` output material) is recorded as a `MorphStatus::Floating`
  marker at the `FLOATING_ORDER` sentinel (`u32::MAX`), and
  `pg-parse/src/morpher.rs::allomorphs_in_morph_order`'s `sort_by_key(|m| m.order)` (a stable sort)
  places that marker after every real, positioned morph unconditionally — so a zero-width rule
  anchored at the shape's last node already renders after the affix occupying that node, which is the
  general rule (not a special case for this rule/grammar/word) this investigation set out to verify.
- **Pinned.** `rust/crates/pg-parse/tests/zero_width_morph_identity.rs` (exact-multiset assertions
  for all four words, plus a 50-fresh-`Morpher` in-process determinism check) and the staged fixture
  above (`words.yaml` records both the deliberate RED-against-hc.dll provenance for `sagui` and this
  measurement's GREEN-against-PanGloss result).
- **Status stays `open`** from the C#/upstream side — PR #500 is real but partial (see the measured
  split above) and hc.dll itself has not converged on a deterministic answer. Nothing further is
  identified on the Rust side from this evidence; this is the "PanGloss already correct" outcome, not
  a claim that the underlying ledger entry has closed.
