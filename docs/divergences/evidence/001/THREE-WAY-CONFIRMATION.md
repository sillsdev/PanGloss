# 001 — three-way confirmation: C# `Add` vs Rust `main` vs Rust `Exact`

Independent re-run of the `evidence/001/VERIFICATION.md` evidence grammars, adding the third implementation
(the unmerged `fix/exact-analysis-fs` branch, entry 002).

Binaries: C# `Add` and C# `PriorityUnion` columns from `evidence/001/*/csharp-add.tsv` and
`csharp-pu.tsv` (hc.dll at `d3b7643d` = `origin/master` semantics, and at `2acb3c52` = PR 494's
branch). Rust `main` = `G:\cargo-build-cache\baseline-459e8cb1\release\pangloss.exe` (PanGloss `main`
at `459e8cb1`), re-run here rather than transcribed. Rust `Exact` =
`G:\cargo-build-cache\exact-analysis-fs\release\pangloss.exe` (`fix/exact-analysis-fs` at
`da564946`). All invoked as `batch <grammar.xml> <words.txt> <out.tsv> --threads 1`.

## The grammar

`evidence/001/g2-stale-branch-num/grammar.xml`. One root and four suffix rules on a `num` feature
with three symbols:

| Rule | Required | Output | Suffix |
|---|---|---|---|
| `mrA` | `num=du` | `num=pl` | `i` |
| `mrB` | *(none)* | `num=sg` | `u` |
| `mrC` | `num=sg` | `num=pl` | `e` |
| `mrD` | `num=pl` | *(none)* | `o` |

`mrB` is the load-bearing shape: a rule with an output feature and no required feature.

## Results

| Word | Morphemes | C# `Add` | C# `PriorityUnion` | Rust `main` | Rust `Exact` |
|---|---|---|---|---|---|
| `zud` | ZUD | found | found | found | found |
| `zudi` | +A | found | found | found | found |
| `zudiu` | +A+B | found | found | found | found |
| `zudiue` | +A+B+C | **lost** | **lost** | **lost** | found |
| `zudueo` | +B+C+D | found | found | found | found |
| `zudiueo` | +A+B+C+D | found | **lost** | **lost** | found |

`g3-nested-complex-agr` reproduces this exactly with the feature nested inside a `ComplexFeature`.

## Mechanism

Analysis walks the derivation outside-in, carrying an accumulated syntactic FS that is supposed to
describe the stem at the current point. Un-applying `mrB` is where both C# folds go wrong: `mrB`
declares an output feature (`num=sg`) and no required feature, and neither `Add` nor `PriorityUnion`
removes that output feature from the accumulated FS. So `sg` — which `mrB` *wrote* — is carried
backwards as though it *constrained* `mrB`'s input. The stem entering `mrB` was actually `pl`, from
`mrA`'s output. `mrA`'s own gate then checks its output `pl` against a stem the fold claims is `sg`,
and refuses.

This single mechanism predicts all six words under both folds:

- Under `Add`, `zudiueo` survives by luck: `mrD` puts `pl` on the accumulated FS first, and `mrC`'s
  `Add` unions rather than replaces, leaving `{sg, pl}`, which still overlaps `mrA`'s `pl`.
- `zudiue` has no `mrD`, so the accumulated FS is `sg` alone and `Add` loses it too.
- Under `PriorityUnion`, `mrC`'s required `sg` overwrites `mrD`'s `pl`, so even `zudiueo` is lost.

## What this establishes

**Rust `main` loses a parse the founding oracle finds** (`zudiueo`). That is a live violation of the
never-diverge rule, in the worse direction, on `main` rather than on an experimental branch.

**The regression is PR 494's, not the port's.** The C# `PriorityUnion` build agrees with Rust `main`
on all six words. HC-Rust faithfully ported the proposal; the proposal loses the parse.

**`Exact` is correct on all six, including where the oracle is wrong.** `zudiue` is forward-
synthesizable (`zud` → `mrA` → `pl` → `mrB` → `sg` → `mrC` requires `sg` → `pl`) and every other
implementation loses it. Removing the rule's own output-feature paths before re-unifying is what
makes the accumulated FS mean what analysis needs it to mean.

An earlier revision of this file described `Exact`'s extra analyses as over-generation. That was
wrong: `zudiue` is a real word, and `Exact` is the only implementation that finds it. The claim in
commit `da564946` and in `docs/research/2026-09-11-exact-analysis-fs-measurements.md` that `Exact`
"cannot lose a parse hc.dll finds" survives; what those documents did not anticipate is that `Exact`
also *recovers* parses hc.dll loses beyond the single `OverrideLoss_TenseFlipFlop` shape they cite.

**`g1-maxwell-chain`'s `sagui` diverges in both directions** on `main` and `Exact` alike: hc.dll
finds `A2B+THIRD`, which neither Rust build finds, and both Rust builds find `A2B+B2A+THIRD`, which
hc.dll does not. `evidence/001/VERIFICATION.md` records g1 as showing no parse-set divergence; that reading
compared Rust against the `PriorityUnion` C# build, which agrees with Rust. Against the founding
oracle it is a divergence, and it is the only one found so far where analyses are both gained and
lost on the same word. Its mechanism is not yet traced and it is NOT explained by the `mrB` shape
above — treat it as open.

## Consequences

- PR 494 as proposed introduces the `zudiueo` regression. The `g2`/`g3` grammars are directly usable
  as evidence in that thread.
- The complete fix is the exact inverse of synthesis, which is what
  `.../pull/494#discussion_r3974223557` predicted ("Wiping the rule's output features off the stem
  here would find it. It needs one other change first").
- The five reference grammars and the whole committed fixture corpus miss all of this. Every
  divergence here needed a purpose-built adversarial grammar, which is the argument for folding
  these shapes into the fixture suite.
- `g1`'s divergence is unexplained and needs its own investigation before any claim that Rust and
  the oracle agree once `mrB`-shaped rules are handled.
