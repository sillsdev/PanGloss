# 002 — Exact validation: the g1 signature question, and an 8-grammar adversarial sweep

## Answers, up front

**Q1 (g1 `sagui`).** The two engines do **not** disagree about the set of analyses. They agree
exactly (2 analyses each, structurally identical derivations). The apparent disagreement is a
**rendering bug in C#'s own morph-order/signature construction**: when a zero-width identity rule
(`mrThird`, `CopyFromInput` only, no `InsertSegments`) wraps immediately outside another rule whose
own affix ends at the same shape position, C#'s `Word.AllomorphsInMorphOrder` silently drops the
inner rule's morpheme ID from both the interactive trace's `Gloss` line and `BatchCommand`'s
signature string. Rust's signature builder has no such collision and reports the true, longer
morpheme chain. This is a reporting bug, not a recall divergence. Evidence and mechanism below.

**Q2 (8-grammar adversarial sweep).** Built and ran 8 new grammars beyond g1/g2/g3, covering
2-symbol/3-symbol "covers-all" hit/miss, a deeper chain (5 rules), a nested `ComplexFeature` with
an inert sibling subfeature, compounding (`ana_compound`'s own `head_required_syn_fs`/`out_syn_fs`
fold), an `AffixTemplate` slot battery, a rule applied twice (`multipleApplication="2"`), a second
independent 3-symbol permutation, and a control where no divergence is structurally possible.

**Result: in all 7 non-control grammars, `Exact` differs from the founding oracle (C# `Add`) in
exactly one direction and on exactly one class of word: `Exact` FINDS a shorter, forward-
synthesizable prefix-chain word that BOTH `Add` and `PriorityUnion` LOSE.** On every other word,
including the full "headline" chain, `Exact` matches `Add` exactly, and `Add` and `Exact` together
disagree with `PriorityUnion`/Rust `main` on that one headline word (`PriorityUnion` loses it,
`Add`/`Exact` find it) — the same `zudiueo` pattern from divergence-001, now reproduced in a fresh
2-symbol grammar, a 5-rule grammar, a nested-feature grammar, a compounding grammar, a template
grammar, and a `multipleApplication=2` grammar. `Exact` never lost a single word any other engine
found, in any of the 8 new grammars or the control. Every word where `Exact` differs from `Add` is
forward-synthesizable (traced below); `Add`'s loss on those words is itself a recall bug in the
founding oracle, matching the `zudiue` finding divergence-001 already established for g2.

## Binaries used, and how each one's semantics was confirmed (fresh this session)

- **C# founding oracle, `Add` semantics.** `hc.exe` built `Release` from
  `C:\Users\johnm\Documents\repos\machine\.worktrees\pr494-base`, detached at
  **`d3b7643d7eee8e0471fb586874337a3d04d6296e`**. `git status --short` in that worktree is empty
  (clean). Re-ran `batch` on `evidence/001/g2-stale-branch-num` this session: reproduced the known
  table exactly (`zudiueo` found, `zudiue` lost) — see "Confirming the binaries" below for the raw
  output.
- **C# `PriorityUnion` (PR #494 research branch).** `hc.exe` built `Release` from
  `C:\Users\johnm\Documents\repos\machine\.worktrees\pr494`, branch `perf/pr494-priority-union`, at
  **`2acb3c52534cf6a8d879c22e5aaa5dfceba8d110`**. **Confirmed the prior agent's restored-but-
  uncommitted state exactly as divergence-001 recorded it**: `git status --short` in this worktree
  shows
  ```
   M src/SIL.Machine.Morphology.HermitCrab.Tool/Program.cs
  ?? src/SIL.Machine.Morphology.HermitCrab.Tool/BatchCommand.cs
  ?? src/SIL.Machine.Morphology.HermitCrab.Tool/SignatureFormat.cs
  ```
  i.e. `BatchCommand.cs`/`SignatureFormat.cs` are present as untracked files (copied in from
  `pr494-base` so `batch` can run at all on this branch, which predates that CLI plumbing) and
  `Program.cs` carries the one line registering `BatchCommand`. This matches 001-verification.md's
  account precisely; nothing was re-applied or changed this session, and `pr494-base` itself
  remains untouched. Re-ran `batch` on g2 this session: reproduced the known table exactly
  (`zudiueo` lost, matching `PriorityUnion`'s own documented narrowing).
- **Rust `main`.** `G:\cargo-build-cache\baseline-459e8cb1\release\pangloss.exe`, PanGloss `main` at
  **`459e8cb1`**. Invoked exclusively through `& .\rust\tools\pg.ps1 -Mode run -Exe <exe>
  -RunMemoryGB 4 -- batch <grammar> <words> <out.tsv> --threads 1 --word-timeout-ms 30000`, one run
  slot at a time, per the task's mandated launcher. Re-ran on g2 this session: `zudiueo` lost,
  `zudiue` lost, byte-identical to the `PriorityUnion` C# build on every word — confirms this build
  really runs `PriorityUnion` semantics.
- **Rust `Exact`.** `G:\cargo-build-cache\exact-analysis-fs\release\pangloss.exe`, branch
  `fix/exact-analysis-fs` at **`da564946`**. Same launcher. Re-ran on g2 this session: `zudiueo`
  found, `zudiue` **also** found (the extra recovery divergence-001's three-way-confirmation
  already reported) — confirms this build really runs `Exact` semantics, not a stale artifact of
  either other mode.

### Confirming the binaries (fresh output, this session)

```
C# Add          (d3b7643d): zud=ZUD  zudi=ZUD+A  zudiu=ZUD+A+B  zudiue=-              zudueo=ZUD+B+C+D  zudiueo=ZUD+A+B+C+D
C# PriorityUnion(2acb3c52): zud=ZUD  zudi=ZUD+A  zudiu=ZUD+A+B  zudiue=-              zudueo=ZUD+B+C+D  zudiueo=-
Rust main       (459e8cb1): zud=ZUD  zudi=ZUD+A  zudiu=ZUD+A+B  zudiue=-              zudueo=ZUD+B+C+D  zudiueo=-
Rust Exact      (da564946): zud=ZUD  zudi=ZUD+A  zudiu=ZUD+A+B  zudiue=ZUD+A+B+C      zudueo=ZUD+B+C+D  zudiueo=ZUD+A+B+C+D
```

Every row matches the table already published in `001-three-way-confirmation.md`. All four
binaries are confirmed to run the semantics claimed for them, from fresh invocations in this
session, before any of the new-grammar evidence below is trusted.

## Q1 — g1 `sagui`: rendering difference, not a set difference

### The direct evidence that settles it

`evidence/001/g1-maxwell-chain/csharp-add-trace-sagui.txt` (already in the repo from divergence-001,
read directly this session) contains the full `--trace`/interactive analysis tree for parsing
`sagui` under C# `Add`. The tree has exactly **two** branches that reach `Successful Parse`:

1. **Branch 1**: `third(0)` tried as the OUTERMOST rule on the raw input `sagui` (`Input: sagui,
   Output: sagui` — zero-width, contributes nothing), wrapping `b2a(0)` (`Input: sagui, Output:
   sagu`) wrapping `a2b(0)` (`Input: sagu, Output: sag`) wrapping a lexical lookup on `sag`. The
   re-synthesis check inside this branch is `sag -a2b-> sagu -b2a-> sagui`, `Successful Parse
   [Output: sagui]`.
2. **Branch 2**: `b2a(0)` tried directly on `sagui` (no `third` wrapper at all) wrapping `a2b(0)`
   wrapping the same lexical lookup on `sag`, with the identical re-synthesis check and identical
   `Successful Parse [Output: sagui]`.

Both branches apply **the same two real rules, `a2b` then `b2a`**, to reach `sagui` from `sag`.
The only structural difference is whether `third` is *additionally* wrapped, zero-width, around the
outside of that same two-rule derivation. This is a real second analysis of `sagui` (third
genuinely can attach there, contributing an invisible extra morph), not a different derivation.

The top of the same trace file prints the two parses as:

```
Parse 1   Morphs: sag u i         Gloss: sag A2B B2A
Parse 2   Morphs: sag u           Gloss: sag A2B THIRD
```

Parse 2's own **Gloss line** — a completely different code path from `BatchCommand.BuildSignature`,
generated by the interactive `parse`/`tracing` command, not `batch` — already drops `B2A` from the
description of a derivation the tree two lines below proves applied `a2b` **and** `b2a` before
`third` wrapped it. Two independent C# renderers (the interactive Gloss line and the batch
signature builder) both under-report this exact derivation the same way, which is strong evidence
the defect is in how C# computes "which morphemes are in this parse," not in what the parse
searcher itself found.

### The mechanism, read directly from the C# source

`SignatureFormat.BuildSignature` (`SIL.Machine.Morphology.HermitCrab.Tool/SignatureFormat.cs`,
read this session on `pr494`) builds each entry as
`string.Join("+", w.AllomorphsInMorphOrder.Select(a => a.Morpheme.Id))`. `Word.AllomorphsInMorphOrder`
(`SIL.Machine.Morphology.HermitCrab/Word.cs`, read this session on `pr494-base`) is:

```csharp
// there can be multiple morphs for a single allomorph, but we only want to return an allomorph on
// its first occurrence, so we use distinct
public IEnumerable<Allomorph> AllomorphsInMorphOrder => Morphs.Select(GetAllomorph).Distinct();
```

`Morphs` is a **postorder traversal of the word's `Annotations` tree**, collecting every annotation
tagged `HCFeatureSystem.Morph` (`Word.cs`). Each such annotation is created by `Word.MarkMorph`,
which spans `Range<ShapeNode>.Create(nodeArray[0], nodeArray[last])` — the first and last shape
node the rule's output action touched — tagged with the owning allomorph's ID.

`mrThird`'s output action is `CopyFromInput` only (`MorphologicalRules/CopyFromInput.cs`, read this
session): it clones every node in the captured span 1:1 into the output with **no**
`InsertSegments`, so `third`'s own morph annotation, when it wraps the entire existing `a2b(b2a(...))`
derivation with zero added characters, spans the *identical* trailing shape-node range that `b2a`'s
own morph annotation already occupies (neither rule adds anything after `b2a`'s own affix, so
`third`'s copied span and `b2a`'s own morph both end at the same rightmost node, and — because
`third` copies the *entire* current shape, not just a sub-range — `third`'s span also reaches back
to cover `b2a`'s own start node). Rust's signature builder walks the actual applied-rule sequence
directly and does not go through an annotation-range/postorder reconstruction with this coincidence
built in, so it reports the full, correct chain, `SAG+A2B+B2A+THIRD`.

This explains every word in the g1 evidence, checked directly against the raw TSVs that shipped
with divergence-001 (`evidence/001/g1-maxwell-chain/{csharp-add,rust}.tsv`, re-read this session,
unchanged):

| Word | C# Add | Rust main/Exact | Reading |
|---|---|---|---|
| `sag` | `SAG+THIRD\|sag;SAG\|sag` | `SAG+THIRD\|sag;SAG\|sag` | **Byte-identical.** `third` wraps the bare root directly — no intervening rule, no shared-boundary collision, so C#'s signature is correct here and matches Rust exactly. This is the control that isolates the bug to the *nesting*, not to `third` itself. |
| `sagu` | `SAG+A2B\|sagu` | `SAG+A2B\|sagu` | **Byte-identical.** `third` cannot spuriously wrap here at all (its own gate, `is_unifiable(out=V, fs)`, fails once `a2b`'s own `req=N` has already pinned the accumulated value to `N` — a definite, non-widened value in *either* mode, since only one rule has written anything yet) — so there is no second analysis to mislabel in the first place, on either engine. |
| `sagui` | `SAG+A2B+B2A\|sagui;SAG+A2B+THIRD\|sagui` (2 analyses) | `SAG+A2B+B2A\|sagui;SAG+A2B+B2A+THIRD\|sagui` (2 analyses) | **Same 2 analyses, one mislabeled by C#.** `SAG+A2B+THIRD` and `SAG+A2B+B2A+THIRD` denote the identical derivation (a2b, b2a, then third wrapping outermost); C# drops `B2A`'s ID because of the shared-boundary collision described above. |

`001-verification.md` already reached this same conclusion independently (see its G1 section) and
called it "a labeling artifact in C#'s own signature construction, not an Add/PriorityUnion
difference." **`001-three-way-confirmation.md` contradicts this**, stating "g1's divergence is
unexplained... treat it as open" and comparing the raw strings `SAG+A2B+THIRD` (Add) against
`SAG+A2B+B2A+THIRD` (Rust) as if they were two different claimed analysis sets. Having now
independently reconstructed the mechanism from the C# source (`AllomorphsInMorphOrder`,
`MarkMorph`, `CopyFromInput`) and cross-checked it against three separate rendering surfaces
(`BatchCommand`'s signature, the interactive `Gloss` line, and the full derivation tree — all three
agreeing that `a2b`+`b2a` both really applied in the `third`-wrapped branch), **the verdict in
`001-verification.md` is the one confirmed by this investigation; `001-three-way-confirmation.md`'s
"open" classification of g1 is superseded.** Per this file's instructions, this contradiction is
recorded here and in the report rather than edited into either existing file.

**On `sagui`, the two engines do not disagree about the SET of analyses.** Both C# `Add` and both
Rust builds find exactly 2 analyses, corresponding to the same 2 underlying derivations. The
disagreement is entirely in how C# renders one of those two derivations into a string — a reporting
bug, not a recall divergence, and (per this repo's own classification framework)
correctness/representability-adjacent only in the sense that C#'s *signature format* mis-describes
a correctly-found parse; it says nothing about either engine's actual analysis search.

## Q2 — the 8-grammar adversarial sweep

### Grammars built

All under `docs/divergences/evidence/002/<grammar>/` (`grammar.xml`, `words.txt`, and this
session's four TSVs: `csharp-add.tsv`, `csharp-pu.tsv`, `rust-main.tsv`, `rust-exact.tsv`).

| Grammar | Dimension exercised | Root/feature | Depth |
|---|---|---|---|
| `g04-two-symbol-cover` | 2-symbol feature, the union **covers every declared symbol** (`FeatureStruct.AddImpl`'s delete-on-cover branch), at chain depth 4 | `dof`, `sz`={sm,lg} | 4 rules |
| `g05-deep-chain` | Deeper chain: 5 rules (two consecutive `req=none` passthrough rules, `mrB`/`mrB2`, instead of g2's one) | `fen`, `num`={sg,du,pl} | 5 rules |
| `g06-nested-sibling` | `ComplexFeature` with **two** sibling subfeatures - one diverging (`pers`), one held constant (`num`=sg everywhere) - tests whether an inert co-present subfeature masks or alters the divergence | `cot`, `agr`{pers,num} | 4 rules |
| `g07-compounding` | `ana_compound`'s own `head_required_syn_fs`/`out_syn_fs` fold, via a `CompoundingRule` (`crJoin`) rather than a fourth affix rule | `bim`+`tok`, `cls`={alpha,beta,gamma} | compound + 3 suffixes |
| `g08-affix-template` | The identical g2 recipe declared as 4 ordered `AffixTemplate` slots instead of a bare stratum rule list | `wug`, `num`={sg,du,pl} | 4 rules, templated |
| `g09-max-application-twice` | `multipleApplication="2"` - the SAME rule id (`mrB`) genuinely fires twice in the headline word | `wig`, `num`={sg,du,pl} | 5 applications (4 rules, one used twice) |
| `g10-control-agreement` | **Control**: no rule's own gate is ever checked against a value written more than one layer further out - required to show zero divergence everywhere | `nep`, `grade`={one,two,three} | 3 rules |
| `g11-symbol-swap` | A second, independent 3-symbol permutation (fresh symbol identities/declaration order, an unused third symbol) to check the mechanism generalizes | `hox`, `deg`={x,y,z} | 4 rules |

Each grammar's own header comment (in `grammar.xml`) carries the full forward-synthesis derivation
and the predicted Add/PriorityUnion fold at each step; the predictions were confirmed empirically
against all four engines before being trusted (see the g04 dead-end below, kept in this grammar's
own comment as "REVISION NOTE" since it is instructive about a real HC architectural constraint).

**One design dead-end, corrected and recorded rather than hidden**: the first draft of
`g07-compounding` put the compounding rule OUTERMOST, requiring the head to already be an
affixed word. C# tracing (`tracing on` / `parse bimatok`) showed this never reaches the
feature-fold question at all: a `CompoundingRule`'s head/nonhead spans are resolved via a
**direct lexical lookup only** - HC does not recursively re-enter the stratum's own morphological-
rule un-application loop for a compound's constituent span within one stratum. Every word
depending on that construction returned `-` on all three engines uniformly (a dead construction,
not a divergence). The grammar was rewritten with `crJoin` **innermost** (compounding two bare
roots first, each independently just a lexical entry) and given a declared `OutputHeadFeatures` so
its own gate plays the "consumer" role instead - this is a legitimate, DTD-supported shape
(`CompoundingRule`'s content model is `(Name, CompoundingSubrules, OutputSubcategorizationOverrides?,
OutputHeadFeatures?, OutputFootFeatures?, HeadRequiredHeadFeatures?, ...)`, confirmed by reading
`HermitCrabInput.dtd` directly) and it reproduces the mechanism cleanly.

### Full result table

`ok` = found; `-` = zero analyses (all "found" cells shown here match byte-for-byte across the
engines named; only the divergence column is spelled out). Full raw TSVs are the source of truth
under `evidence/002/<grammar>/`.

| Grammar | Word | C# Add | C# PriorityUnion | Rust main | Rust Exact |
|---|---|---|---|---|---|
| g04 | `dof`/`dofp`/`dofpk` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g04 | `dofpkt` (A+B+C, D skipped) | **-** | **-** | **-** | **ok** `DOF+A+B+C` |
| g04 | `dofktn` (B+C+D, A skipped) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g04 | `dofpktn` (full chain) | **ok** `DOF+A+B+C+D` | **-** | **-** | **ok** `DOF+A+B+C+D` |
| g05 | `fen`..`feniux` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g05 | `feniuxe` (A+B+B2+C, D skipped) | **-** | **-** | **-** | **ok** `FEN+A+B+B2+C` |
| g05 | `fenuxeo` (B+B2+C+D, A skipped) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g05 | `feniuxeo` (full chain) | **ok** | **-** | **-** | **ok** |
| g06 | `cot`..`cotao` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g06 | `cotaou` (A+B+C, D skipped) | **-** | **-** | **-** | **ok** `COT+A+B+C` |
| g06 | `cotoue` (B+C+D, A skipped) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g06 | `cotaoue` (full chain) | **ok** | **-** | **-** | **ok** |
| g07 | `bimtok`/`bimtoku` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g07 | `bimtokue` (B+C, D skipped) | **-** | **-** | **-** | **ok** `BIM+TOK+B+C` |
| g07 | `bimtoko` (D alone) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g07 | `bimtokueo` (full chain) | **ok** | **-** | **-** | **ok** |
| g08 | `wug`..`wugau` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g08 | `wugaue` (A+B+C, D skipped) | **-** | **-** | **-** | **ok** `WUG+A+B+C` |
| g08 | `wugueo` (B+C+D, A skipped) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g08 | `wugaueo` (full chain) | **ok** | **-** | **-** | **ok** |
| g09 | `wig`..`wigauu` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g09 | `wigauue` (A+B+B+C, D skipped) | **-** | **-** | **-** | **ok** `WIG+A+B+C` |
| g09 | `wiguueo` (B+B+C+D, A skipped) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g09 | `wigauueo` (full chain) | **ok** | **-** | **-** | **ok** |
| g10 | `nep`/`nepa`/`nepab`/`nepabc` (every word) | ok | ok | ok | ok |
| g11 | `hox`..`hoxpq` | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g11 | `hoxpqr` (A+B+C, D skipped) | **-** | **-** | **-** | **ok** `HOX+A+B+C` |
| g11 | `hoxqrs` (B+C+D, A skipped) | ok (agree) | ok (agree) | ok (agree) | ok (agree) |
| g11 | `hoxpqrs` (full chain) | **ok** | **-** | **-** | **ok** |

`g10-control-agreement`: **all four engines agree on every one of its 4 words**, confirming the
predicted "no rule's own gate is checked against a value from more than one layer out" shape
produces zero divergence, as required by the task brief's item (g).

### Forward-synthesis trace and verdict for every Exact-vs-oracle difference

Two distinct words differ from the founding oracle (`Add`) in each of the 7 non-control grammars,
and both differences run the **same direction** (`Exact` finds something `Add` does not) in every
case except the shared full-chain word, where `Exact` agrees with `Add` against `PriorityUnion`.
Tracing each by hand (root → applied rule → surface, checking each rule's own `RequiredHeadFeatures`
unifies with the *immediately preceding* rule's `OutputHeadFeatures`, which is exactly what a
correct forward synthesis pass checks):

- **g04 `dofpkt`** (= `dof`+`p`+`k`+`t`): `dof` (unconstrained) → `mrA` (no req; out=sm) → `dofp`
  (sz=sm) → `mrB` (no req; out=lg) → `dofpk` (sz=lg) → `mrC` (req=lg, matches; out=sm) → `dofpkt`
  (sz=sm). **Every rule's own requirement is satisfied by the immediately preceding rule's output.
  Forward-synthesizable.** `Add` and `PriorityUnion` both lose it (traced in the grammar's own
  header: analysis un-applies `mrC` first from an empty accumulated FS, since `mrD` is absent here,
  so `mrC`'s fold produces the single value `lg` under *either* mode - no widened set exists yet -
  and `mrA`'s own gate, checking `out=sm`, fails identically against `lg` on both sides). **Verdict:
  `Exact` is right, `Add` (the founding oracle) itself under-generates here.**
- **g04 `dofpktn`** (full chain, `+n`): adds `mrD` (req=sm, matches `mrC`'s own out=sm) → `dofpktn`
  (sz unconstrained, `mrD` has no output). Forward-synthesizable (same check, one more link).
  `Add` finds it (traced: `mrD` contributes `sm` from empty; `mrC`'s fold unions `sm+lg` = **both**
  declared symbols of a 2-symbol feature, hitting `AddImpl`'s delete-on-cover branch, so `Add`'s
  accumulated value becomes fully unconstrained - even more permissive than g2's partial-subset
  case - and `mrA`'s gate trivially passes against an unconstrained value). `PriorityUnion` narrows
  to `lg` alone and refuses `mrA`'s `out=sm` check. **Verdict: this is g2's `zudiueo` pattern,
  reproduced through the delete-on-cover branch specifically; `Exact` and `Add` are both right,
  `PriorityUnion`/Rust `main` under-generate.**
- **g05 `feniuxe`**/**`feniuxeo`**: identical reasoning one layer deeper (`mrA`→`mrB`→`mrB2`→`mrC`
  [→`mrD`]); both are forward-synthesizable by the same requirement-chases-preceding-output check,
  confirming the mechanism is depth-independent up to at least 5 rules. Same verdict.
- **g06 `cotaou`**/**`cotaoue`**: identical reasoning with `agr.pers`/`agr.num` nested one level
  down; the inert sibling `num=sg` is written identically by every rule that has an
  `OutputHeadFeatures` element and never gates anything, confirmed by its presence changing nothing
  relative to g3's single-subfeature result. Same verdict.
- **g07 `bimtokue`**/**`bimtokueo`**: `bim`+`tok` (bare lexical entries, `crJoin`'s own
  `OutputHeadFeatures`=alpha) → `mrB` (no req; out=beta) → `bimtoku` → `mrC` (req=beta, matches;
  out=alpha) → `bimtokue` [→ `mrD` (req=alpha, matches) → `bimtokueo`]. Forward-synthesizable by
  the same chain check; `bimtokue`'s loss under `Add`/`PriorityUnion` and recovery under `Exact`
  is the identical mechanism, now demonstrated to run through `CompoundingRule`'s own
  `head_required_syn_fs`/`out_syn_fs` fold (`crJoin`'s own gate, not an ordinary affix rule's) -
  confirming `ana_compound` really does share the exact bug and the exact fix. **Verdict: `Exact`
  is right; the founding oracle under-generates through compounding exactly as it does through
  affixation.**
- **g08 `wugaue`**/**`wugaueo`**: identical chain, declared as `AffixTemplate` slots instead of a
  bare rule list. Same forward-synthesis check, same result. **Verdict: templating the rules
  changes nothing about the fold** - the "template battery's feature-blind dedup hazard" named in
  the task brief as a separate suspect mechanism does not manifest here; this fixture does not
  demonstrate it (see "What this does not show," below).
- **g09 `wigauue`**/**`wigauueo`**: `mrB` genuinely fires twice (`multipleApplication="2"`);
  forward chain `wig`→`mrA`→`wiga`→`mrB`(1st)→`wigau`→`mrB`(2nd)→`wigauu`→`mrC`→`wigauue`
  [→`mrD`→`wigauueo`], each application's own req (empty) trivially satisfied. Forward-
  synthesizable. Note the **signature itself under-reports the repeat**: all four engines print
  `WIG+A+B` (not `WIG+A+B+B`) for `wigau`/`wigauu`, because `AllomorphsInMorphOrder`'s `.Distinct()`
  collapses the SAME allomorph object used twice into one entry - the identical mechanism class as
  Q1's g1 finding (a signature construction that under-counts real rule applications), but here it
  is **not** a divergence: all four engines do it identically, so it never produces a false
  found/lost mismatch. **Verdict: `Exact` is right on `wigauue`/`wigauueo`; the repeat-application
  mechanism itself introduces no NEW divergence beyond the already-established stale-branch one.**
- **g11 `hoxpqr`**/**`hoxpqrs`**: a second, independently-chosen 3-symbol permutation
  (`x`/`y`/`z`, `y` unused) of the identical recipe. Same forward-synthesis check, same result,
  confirming the mechanism is not an artifact of g2's specific symbol identities, declaration
  order, or which symbol the innermost rule happens to write.

**No word in any of the 8 new grammars, or the control, showed the reverse direction** —
`PriorityUnion` or Rust `main` finding something `Add` or `Exact` does not. This is consistent with
`001-verification.md`'s structural monotonicity proof (Add's accumulated set is always a superset
of PriorityUnion's accumulated singleton) and extends it empirically to: 2-symbol delete-on-cover,
5-rule depth, nested-with-sibling features, compounding, templating, and repeat-application.

### What this does not show

- **The "template battery's feature-blind dedup hazard"** named in the task brief as a separately-
  suspect mechanism was not triggered by `g08-affix-template` - the fold behaved identically to the
  un-templated g2/g5/g11 recipes. This grammar rules out one simple hypothesis (that mere slot
  declaration changes the fold) but does not investigate the dedup hazard itself, which per its
  name likely concerns two *different* rules or allomorphs converging on the same slot rather than
  a single rule chain - a genuinely different construction this sweep did not attempt.
- **`ana_compound`'s `HeadRequiredHeadFeatures`/nonhead side, and a head that is itself an affixed
  word within one stratum**, were not reachable at all in this sweep (see the g07 dead-end above).
  Whether a multi-stratum grammar (affixation on a lower stratum, compounding on a higher one)
  changes anything was not tested - flagged under "what could not be verified."
- **No grammar combined two or more of these dimensions at once** (e.g. a templated compounding
  rule with a nested complex feature applied twice). Each dimension was isolated deliberately so a
  result could be attributed to one mechanism; a combined-dimension sweep is future work if a
  reason to suspect interaction ever arises.

## Is Exact safe to adopt, and what would upstream need to change?

**Empirically, over 3 independently-authored grammars (g2/g3, divergence-001) plus 8 more spanning
every adversarial dimension named in this task, `Exact` has never lost a parse any other
implementation (C# `Add`, C# `PriorityUnion`, Rust `main`) found, and it has recovered a genuinely
forward-synthesizable word on every single non-control grammar that the founding oracle itself
loses.** Per this repo's own oracle-hierarchy rule, "if HC-Rust diverges from C#, an HC-Rust-only
fixture enshrines the divergence AS the expected answer" is exactly the hazard to guard against
here - which is why every divergent word in this document was checked by hand against forward
synthesis rather than taken on `Exact`'s own say-so, and why `Exact`'s extra finds are not "over-
generation" but a real recall gap in `Add` itself, independently reproducible by tracing the rule
chain from the root.

**What this means for adoption**: `Exact` is not simply "closer to `PriorityUnion` than `Add`" or
vice versa - it is a **strict superset of `Add`** in every grammar tested (never a subset, never
incomparable), plus additional genuine recoveries `Add` itself misses. If `Add`/`origin/master` is
the repo's binding oracle today, adopting `Exact` verbatim would make Rust **diverge from the
oracle** on the `dofpkt`/`feniuxe`/`cotaou`/`bimtokue`/`wugaue`/`wigauue`/`hoxpqr`-class words -
technically a violation of "HC-Rust must never produce a different parse than C#," even though the
evidence here says C# is the one that is wrong on those specific words. That is precisely the
situation this repo's own rules anticipate needing human/upstream resolution for, not something a
port should silently correct on its own authority.

**What would have to change upstream for `Exact` to become "the oracle"**: `AnalysisAffixProcessRule.Apply`
and `AnalysisCompoundingRule.Apply` on `origin/master` would need to adopt the same fold `Exact`
already implements - gate on `PriorityUnion(required, out)`, strip `out`'s own feature paths off the
accumulated stem via something like `pg_featstruct::remove_paths`, unify with `required`, and never
clear the feature entirely - in place of the current plain `Add`. This is a strictly larger change
than PR #494 as currently written: PR #494's `PriorityUnion` mode does **not** fix the `zudiueo`-
class regression (it reproduces it identically to Rust `main`, confirmed in divergence-001 and
reconfirmed here on g2), and the C# side has no `Exact` mode implemented at all yet (per
`docs/pr494-review.md` section 4, cited in divergence-001, `Exact` there is "not shippable yet").
Concretely: someone with authority over `sillsdev/machine` would need to (a) accept that `Add`'s
current behavior on these `mrB`-shaped constructions is a genuine recall bug, not merely a
divergent-but-acceptable choice, (b) port the `remove_paths`-based fold into
`AnalysisAffixProcessRule`/`AnalysisCompoundingRule`, and (c) re-derive every fixture whose
committed `words.yaml` ground truth was authored against `Add`'s current (buggy) behavior on a
`mrB`-shaped construction, since those fixtures would newly fail under a corrected oracle. Until
that happens, `Exact`'s status per this repo's own rule is unambiguous: it is a divergence from the
oracle of record, evidenced here to be a *correct* divergence, but a divergence nonetheless -
adopting it on PanGloss `main` today would require either (i) getting the equivalent fix accepted
upstream first (this document's evidence is exactly the kind of grammar+trace package that
argument would need), or (ii) an explicit, documented policy decision by this repo's maintainers to
prefer a demonstrably-more-correct `Exact` result over `Add`'s current output on this specific,
narrow, well-characterized bug class, distinct from "porting a bug PanGloss doesn't have yet."

## What could not be verified

- The deeper CLR-level cause of C#'s `AllomorphsInMorphOrder`/`Morphs` postorder-traversal ID
  collision (Q1) was reconstructed from source (`MarkMorph`'s per-rule `Range<ShapeNode>`
  annotation, `CopyFromInput`'s 1:1 node cloning, `.Distinct()`'s allomorph-identity dedup) and
  corroborated by three independent renderings all agreeing on the same 2-analysis structure, but
  was not confirmed by attaching a debugger to `hc.exe` and inspecting the actual `Annotations` tree
  node-by-node. The SET-vs-RENDERING verdict does not depend on this deeper cause; the precise
  reason the collision happens (rather than merely that it demonstrably does, and only in the
  shared-boundary shape) is offered as the best available explanation, not a debugger-verified one.
- `g09-max-application-twice`'s all-four-engines-agree "the signature under-reports the repeat"
  finding was read from the TSV output only; the underlying derivation tree was not traced via
  `--trace`/`tracing on` to directly confirm two distinct rule applications (as opposed to some
  other explanation for the missing digit). Given all four engines behave identically and the
  forward-synthesis chain independently requires two applications to reach the observed surface
  string, this is treated as settled, but it was not cross-checked against a trace the way Q1's g1
  finding was.
- The two items under "What this does not show" above (the template dedup hazard proper, and a
  multi-stratum affixed-head compounding construction) were identified as gaps but not built or
  run - they remain open per the task's explicit request to say what could not be verified.
- No fixture here was run against `hc-conformance.exe`'s own self-check mode; every C# result comes
  from `hc.exe`'s `batch`-via-script-file path (`PROTOCOL.md` section 7's documented C#-specific
  invocation), consistent with how divergence-001's own evidence was gathered, but not independently
  cross-checked against the conformance harness's in-process path for these specific ad hoc
  grammars.
