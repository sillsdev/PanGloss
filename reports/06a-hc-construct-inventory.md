# HermitCrab Grammar Construct Inventory — Evidence for FST Compilation Complexity Analysis

Compiled 2026-07-14. Sources: `rust/crates/hc-grammar`, `rust/crates/hc-lexicon`, `rust/crates/hc-rules`,
`rust/crates/hc-parse` (Rust port); `machine/src/SIL.Machine.Morphology.HermitCrab` and
`machine/src/SIL.Machine` (C# reference, git submodule `sillsdev/machine` @ `conformance-framework`,
checked out fresh for this task); `samples/data/{indonesian,sena,amharic}-hc.xml` (the three reference
grammars); `docs/fst-plan/*`, `docs/hermitcrab-rust-port-audit.md`, `rust/docs/*` (design/quirk docs);
`reports/oracle/*` (frozen oracle parse output, used to empirically check the "max morphemes/word" claim
rather than guess it).

All file:line citations below were gathered by three parallel research passes (Rust model reading, C#
semantics reading, XML census) plus a fourth doc-mining pass, then merged and spot-verified directly
(see "Verification" note at the end of each family where a claim was independently re-checked against
the source rather than trusted from the sub-report).

**Census methodology** (all three XML files): tag counts via `grep -c '<TagName '` / `<TagName>` boundary
anchoring (so `<LexicalEntry ` never matches `<LexicalEntries>`, etc. — verified no element nests inside
same-named parent per the DTD), or a small Python regex script for nested extraction (subrule counts,
context lengths, feature-value counts). Exact commands are given per table. Indonesian = 2,563 lines /
66 entries; Sena = 33,091 lines / 1,369 entries; Amharic = 17,603 lines / 54 entries (+22 clitics).

---

## 1. Strata

**Semantics.** A stratum bundles a rule-application table: an ordered list of phonological rules, an
ordered list of morphological rules (order-sensitivity controlled by `MorphRuleOrder::{Linear,Unordered}`),
a list of affix templates, and the lexical entries visible at that level. Strata form a **flat, linearly
ordered pipeline** (a `Vec`, not a graph, not a repeatable cycle): on synthesis the pipeline runs
deepest→surface exactly once per stratum per parse (`PipelineRuleCascade`, C# `Language.cs:145-151`; Rust
`hc-parse/src/morpher.rs:611-638`, `for s in 0..n`); on analysis it runs surface→deepest exactly once
(Rust `morpher.rs:367`, `for s in (0..n).rev()`), unioning every stratum's un-application output (not just
the deepest one) into the result set while also threading the narrower output forward as the next
stratum's input. **Within** one stratum-visit, phonological rules apply as a single linear sequence in
declared-rule-list order — rule *i* runs to completion before rule *i+1* starts (C# `LinearRuleCascade`,
`SIL.Machine/Rules/LinearRuleCascade.cs:25-56`; Rust `hc-rules/src/stratum.rs:1053-1068` for analysis,
`:1522-1543` for synthesis) — and only *after* the stratum's entire morphology (rules + templates) has
finished (C# `SynthesisStratumRule.cs:49-92`). Morphology and phonology thus interleave **at stratum
granularity** (the classic Lexical-Phonology "cyclic" architecture: morph(1)→phon(1)→morph(2)→phon(2)→…)
but **not** at single-rule granularity within a stratum.

Rust ports this with one added mutual-recursion detail not present as a simple linear fact in C#: within
a `Linear` stratum, `apply_mrules` runs the full rule cascade, then calls `apply_templates` on each
result, which — for `Linear` mode — can itself recurse back into `apply_mrules` on its output
(`hc-rules/src/stratum.rs:853-897`), i.e. templates can trigger another morphological-rule pass. Rust also
adds a shared `StepBudget` safety valve across this recursion (`stratum.rs:141-257`) that has no C#
analog (C# instead relies on `Morpher.MaxUnapplications` to cap the analysis output count, not the search
itself).

**Citations:** Rust model `hc-grammar/src/model.rs:1046-1058` (`StratumDef`), `:37` (`StratumId(u8)`
newtype — implies a soft ceiling of 256 strata, never enforced); C# `Stratum.cs`, `Language.cs:49-58`
(`Stratum.Depth` ordering), `Language.cs:145-151` (`PipelineRuleCascade` wiring).

| Grammar | # Strata | Names/order | Roots (stratum 1) | Rules per stratum |
|---|---|---|---|---|
| Indonesian | 3 | Morphology → Clitics → Surface | 66 | Morphology: 13 mrules, 2 compounding, 5 prules; Clitics/Surface: empty |
| Sena | 3 | Morphology → Clitics → Surface | 1,369 | Morphology: 130 mrules, 8 compounding, 0 prules; Clitics: 2 clitic entries, 2 mrules; Surface: empty |
| Amharic | 3 | Morphology → Clitics → Surface | 54 | Morphology: 65 mrules, 1 compounding, 0 prules; Clitics: 22 clitic entries, 22 mrules, 7 prules; Surface: empty |

```bash
grep -c '<Stratum ' samples/data/<lang>-hc.xml   # = 3 for all three
```

**Wild-max.** No cap anywhere in code (`StratumId(u8)` is a soft 256 ceiling never checked at load time).
All three reference grammars use exactly 3 strata (Morphology/Clitics/Surface), which appears to be a
FieldWorks convention, not a schema requirement — **flag as unbounded** for complexity purposes; a
maximally elaborate FLEx grammar (e.g. one separating derivational vs. inflectional strata further) could
plausibly reach 5–10, but no in-repo evidence bounds this.

---

## 2. Morphological rules — general affixation

**Semantics.** HermitCrab has **no dedicated prefix/suffix/infix/circumfix/simulfix enum** — every
affixation rule is one `AffixProcessRuleDef`, containing an ordered list of allomorph subrules, each of
which is a **pattern-match LHS + ordered output-action RHS** pair. The traditional affix-type distinction
is emergent from the RHS action sequence:

```rust
pub enum OutputAction {
    Copy(PartRef),                                       // <CopyFromInput index=..>
    InsertSegments { table: TableId, shape: SegmentedText }, // <InsertSegments>
    Modify(PartRef, SimpleContext),                      // <ModifyFromInput> (feature-changing copy)
    InsertContext(SimpleContext),                        // <InsertSimpleContext>
}
```
(`hc-grammar/src/model.rs:645-686`, mirrors C# `AffixProcessAllomorph`/`MorphologicalOutputAction`
subclasses in `MorphologicalRules/*.cs`.) `[Copy, Insert]` = suffix; `[Insert, Copy]` = prefix; LHS split
into ≥2 named parts with interleaved `Copy`s and `Insert`s = infix/circumfix; RHS with only
`Modify`/`Copy` (no `InsertSegments`) = simulfix/stem-change (no new phonological material, only feature
changes to copied material). Reduplication is its own emergent pattern, detailed in §3.

`AffixProcessRuleDef` fields (`model.rs:606-641`): `blockable: bool` (default true — a more specific
lexical entry can block the rule), `partial: bool` (default false), `max_apps: u16` (`multipleApplication`,
default 1; **DTD-capped to the literal enumeration `0|1|…|9`**, i.e. a **schema-level cap of 9**
reapplications per rule, `HermitCrabInput.dtd:352,389`), `required_syn_fs`/`out_syn_fs` (POS/head/foot
gate and output percolation, §11), `obligatory_features`, `required_stem_name` (W5), and an ordered
`allomorphs: Vec<AffixAllomorphDef>` — **first-declared-match wins** on synthesis, subject to a
free-fluctuation exception (§4).

Multiple-application (`max_apps > 1`) lets one rule fire several times on its own output within a single
stratum-visit (e.g. repeated pluralization-like affixes) — this is the second, rule-level "self-feeding"
mechanism in HC (distinct from phonological-rule self-feeding, §5).

**Citations:** Rust `hc-grammar/src/model.rs:606-686`; C# `MorphologicalRules/AffixProcessRule.cs`,
`MorphologicalRules/SynthesisAffixProcessRule.cs:41-236` (per-application unify-once-then-share-across-
allomorphs semantics, verified — see §11), `HermitCrabInput.dtd:352,389` (`multipleApplication` enum cap).

| Grammar | Total affixal `MorphologicalRule` | Prefix | Suffix | Circumfix | Infix/templatic (multi-copy) | Reduplication (see §3) | Subrules/rule max | Subrules/rule mean |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Indonesian | 13 | 2 | 5 | 3 | 0 | 3 | 1 | 1.000 |
| Sena | 132 | 112 | 20 | 0 | 0 | 0 | 6 | 1.811 |
| Amharic | 87 | 25 | 54 | 0 | 3 | 5 | 3 | 1.069 |

Type classification is structural (inferred from `MorphologicalOutput` child-element ordering), not a
literal XML attribute — see census agent methodology note; Amharic's 3 "infix/templatic" rules are
genuine Semitic root-and-pattern morphology (multiple discontinuous copies interleaved with inserted
vocalism), not simple point-infixes.

```bash
grep -c '<MorphologicalRule ' samples/data/<lang>-hc.xml
# subrule/type classification via a small Python script walking each <MorphologicalRule> block's
# first <MorphologicalSubrule>'s CopyFromInput/InsertSegments/ModifyFromInput child sequence
```

**Wild-max.** `max_apps` (multiple-application count) is schema-capped at 9 — a real, citable bound.
Number of affixal rules per grammar and subrules per rule: **no cap found** in code or schema (plain
`Vec`s). Sena already has 132 rules with up to 6 subrules; a FLEx grammar for a highly synthetic language
(e.g. a polysynthetic Amerind or Caucasian language) could plausibly have several hundred rules — flag as
**unbounded**, with Sena's 132 as the largest observed data point in this repo.

---

## 2a. Realizational rules (the third `MorphRuleDef` variant)

**Semantics.** `MorphRuleDef` has exactly three variants (`AffixProcess`, `Compounding`, `Realizational`)
— `RealizationalRuleDef` is a distinct, first-class rule kind, not a special case of the other two:

```rust
pub struct RealizationalRuleDef {          // model.rs:581-603
    pub morpheme: MorphemeId,
    pub name: Option<String>,
    pub blockable: bool,
    pub required_syn_fs: FsId,      // {head, foot}-only — no POS attribute in the DTD
    pub real_fs: FsId,              // <RealizationalFeatures>, wrapped in the head feature
    pub allomorphs: Vec<AffixAllomorphDef>,   // shares the affix-allomorph shape/loader with AffixProcess
}
```
Deliberately **lacks** `max_apps`/`partial`/`out_syn_fs`/`obligatory_features`/`is_template_rule` — none of
those C# fields exist on `RealizationalAffixProcessRule`. It gates on `real_fs.subsumes(word.real_fs)`
then, if `real_fs` is non-empty, a recursive `IsBlocked` feature-presence check
(`realizational_is_blocked`, `hc-rules/src/morph.rs:1497-1522`), then the ordinary `required_syn_fs` ↔
`word.syn_fs` unify+priority-union, with `real_fs` standing in for the ordinary affixation path's
`out_syn_fs`. Realizational rules exist to model **realizational** (word-and-paradigm-style) morphology,
where a single feature bundle (e.g. a whole inflectional cell) is spelled out by one rule rather than by
compositional affix stacking — distinct from ordinary concatenative affixation in how its output feature
structure is determined (`real_fs` rather than `out_syn_fs`).

A genuine, documented authoring quirk: the DTD comment suggests `RealizationalRule` is not a direct member
of a stratum's `morphologicalRules` list, but **the C# oracle accepts it there directly** — "the DTD
comment is misleading" (`rust/docs/phase2-completed/workstreams-landed.md:75-86`). The Rust port was built
to match the oracle's actual accepted behavior, not the DTD's stated one.

**Citations:** Rust `hc-grammar/src/model.rs:581-603`, `hc-rules/src/morph.rs:1497-1522`; C#
`MorphologicalRules/RealizationalAffixProcessRule.cs`,
`MorphologicalRules/SynthesisRealizationalAffixProcessRule.cs:61-122`;
`rust/docs/phase2-completed/workstreams-landed.md:75-86`.

**Census.** Zero occurrences in all three reference grammars (Indonesian, Sena, Amharic). Ported anyway
under this project's "no reference-grammar gate may move [a construct out of scope]" floor (`model.rs:
20-25`) — i.e., it is a real, spec-mandated construct with **no empirical data point** in this repo, not a
speculative one; flag it to the downstream analysis as untested-in-practice rather than nonexistent.

**Wild-max.** No cap in code (`max_apps` doesn't even exist on this variant — the model reports
`u16::MAX` for it since no C# gate exists, `model.rs:538-551`). No empirical basis to estimate a realistic
count; realizational rules model whole-paradigm-cell spellouts, so a plausible upper bound tracks the
number of distinct inflectional-feature combinations a language's paradigm defines (potentially large for
a richly inflecting language, but genuinely unbounded from the model's perspective).

---

## 2b. Clitics

**Semantics.** There is **no dedicated clitic construct anywhere in the grammar model** — confirmed absent
from `model.rs`, `morph.rs`, `load.rs`, and the rest of `hc-rules`/`hc-grammar` (nothing named "clitic"
appears in any of them). A clitic is represented, in both the C# reference and this port, as an **ordinary
`LexEntryDef`** (for the clitic's own lexical content) combined with **ordinary `AffixProcessRuleDef`s**
(for how it attaches), conventionally placed in a separate stratum named "Clitics" that sits between the
"Morphology" (root/derivation/inflection) stratum and a final "Surface" stratum — a FieldWorks/HermitCrab
authoring **convention**, not a schema-enforced distinction. Because it is not a first-class construct, a
clitic's attachment rule is subject to exactly the same complexity profile as any other affixation rule
(§2) — no special-cased simplification or restriction applies.

**Complexity-relevant finding from the FST-compilation branch of this repo**: `MorphOp.Clitic` — the
FST-branch's own internal tag for clitic-attachment operations — currently has **no generator/proposer at
all**; clitic attachment "falls to the engine entirely," alongside `MorphOp.Process`/`ModifyFromInput`
(`docs/fst-plan/FST_FAST_PATH_PLAN.md:499-503`). This is not a claim about clitics being non-regular (they
are ordinary affixation, hence regular per Kaplan & Kay like the rest of §2) — it is a documented
**implementation-completeness gap**: as of the cited doc, nothing in this codebase's FST-compilation
pipeline yet handles clitics as a fast path, so any clitic-bearing word must currently be verified/parsed
by the full non-FST engine. This is exactly the kind of "compiler must handle this, and today doesn't"
fact relevant to a big-O analysis of what a *complete* HC→FST compiler would need to cover.

**Citations:** Rust — absence confirmed by full-text search of `hc-grammar/src`, `hc-rules/src` for
"clitic" (no hits); C# — likewise no `Clitic*.cs` file exists in
`SIL.Machine.Morphology.HermitCrab/`; `docs/fst-plan/FST_FAST_PATH_PLAN.md:499-503`.

| Grammar | Clitic-stratum `LexicalEntry` | Clitic-stratum `MorphologicalRule` | Clitic-stratum `PhonologicalRule` |
|---|---:|---:|---:|
| Indonesian | 0 | 0 | 0 |
| Sena | 2 | 2 | 0 |
| Amharic | 22 | 22 | 7 |

(Same underlying data as the §1 strata table, re-surfaced here as its own construct family per the task's
explicit request to treat clitics separately.)

```bash
# stratum-scoped LexicalEntry/MorphologicalRule/PhonologicalRule counts via the same per-<Stratum>
# block extraction used in §1's Python script, isolating the block whose <Name> is "Clitics"
```

**Wild-max.** No cap in code — a clitic-heavy language (e.g. one with a rich second-position clitic
cluster system) could define an arbitrarily large clitic inventory; Amharic's 22 is the largest observed
data point here. The more load-bearing complexity fact is not cardinality but **coverage**: clitics are
architecturally ordinary affixation (§2's bounds apply), but the FST-compilation branch of this repo does
not yet generate fast-path arcs for them at all, so today they are a 100%-engine-fallback category
regardless of grammar size.

---

## 3. Reduplication

**Semantics.** Not a distinct rule kind — it is *detected* from an ordinary `AffixAllomorphDef` whose RHS
repeats one LHS `Input` part via ≥2 `Copy`/`Modify` actions. Full parameter inventory (Rust
`hc-rules/src/morph.rs:1690-1804`, a line-for-line port of C# `SynthesisAffixProcessAllomorphRuleSpec.cs:
23-120`):

1. **`ReduplicationHint`** (`Prefix | Suffix | Implicit`, `model.rs:666-671`, from the `redupMorphType`
   XML attribute) — disambiguates which repeated occurrence is "existing" (stem) vs. "new" (reduplicant);
   it does **not** on its own determine which side of the LHS gets copied.
2. **Existing-vs-new classification** (`classify_redup`, `morph.rs:1714-1804`): groups RHS indices by
   referenced LHS part (only groups with ≥2 references are true reduplication); searches for a contiguous
   run matching the whole LHS, scanning right-to-left for `Prefix` / left-to-right for `Suffix`/`Implicit`;
   falls back to "last occurrence = existing" (`Prefix`) or "first occurrence = existing"
   (`Suffix`/`Implicit`) if no such run is found.
3. **No explicit size-limit field** — copy size is whatever the LHS `Pattern` (a quantified segment
   sequence, `Quantifier{min, max}`, `max=None` ⇒ unbounded) matches. **This is the one place in the whole
   grammar model where an author can request literally unbounded copying** (`Quantifier{max: None}` over
   an entire stem part) — see §a below and the "no classical-FST encoding" section.
4. **Fixed (non-copied) segments**: ordinary `InsertSegments` actions interleaved with the repeated
   `Copy`/`Modify` actions — no separate "fixed segment" field.
5. Partial vs. full reduplication is implicit in whether the LHS pattern captures the whole allomorph
   input as one part (whole-stem reduplication) or a sub-part (partial reduplication of that sub-part) —
   there is no dedicated `partial: bool` flag for this (distinct from `AffixProcessRuleDef.partial`, an
   unrelated flag).

Kaplan & Kay–grounded classification (`docs/fst-plan/HERMITCRAB_FST_ADVISOR.md:73-124`): reduplication of
a **length-bounded** part (fixed CV/CVC template) is `Regular = true` (finite copy, expressible as an FST);
reduplication of an **unbounded** part (`Annotation(any).OneOrMore`, i.e. whole-stem copy) is
`Regular = false` — "the one genuinely non-regular operation (`{ww}` is not regular)". This is confirmed
independently in `docs/fst-plan/FST_FAST_PATH_PLAN.md:92-108` and `HYBRID_FST_FEASIBILITY.md:259-269`
(pumping-lemma argument, Hopcroft & Ullman 1979), and handled in the FST-compilation branch of this repo
by a **runtime peel** (detect copy on the surface string, strip it, analyze the residual, verify the whole
against the engine) — an `O(n²)` scan capped at **≤2 applications** (`FST_FAST_PATH_PLAN.md:92-108,478`).

**Citations:** Rust `hc-rules/src/morph.rs:33-45,1690-1804`; C#
`MorphologicalRules/SynthesisAffixProcessAllomorphRuleSpec.cs:23-120`; docs as above.

| Grammar | Redup-tagged (`redupMorphType`) subrules | Subrules that actually trigger group-classification (≥2 refs to one part) |
|---|---:|---:|
| Indonesian | 3 (msubrule5/11/13) | 3 (real reduplication) |
| Sena | 0 | 0 |
| Amharic | 5 | 0 (each references its Input part exactly once — attribute present but inert) |

**Wild-max.** No size cap in the model itself for the copied span — **flag as unbounded / non-regular**
when the copied part is the whole stem (see final "no classical-FST" section). Engine-side, the FST branch
caps applications at 2 and uses `O(n²)` peel per application — an implementation choice, not a grammar-
model bound.

---

## 4. Lexical entries and allomorphs

**Semantics.** `LexEntryDef` (`hc-grammar/src/model.rs:744-753`): syntactic FS, MPR-feature set, optional
`Family` (W5), and an ordered list of `RootAllomorphDef`s. Each `RootAllomorphDef`
(`model.rs:756-778`): a pre-segmented phonetic shape, `is_bound: bool` (default false — a bound allomorph
cannot be the word's only distinct allomorph), an ordered `environments: Vec<EnvironmentDef>` (required
XOR excluded contexts), `co_occurrence` rules (§9), an optional `stem_name` (W5), and a computed
`is_pattern: bool` flag (diverts pattern-bearing "allomorphs" — really lexical guesser patterns — out of
the surface root trie).

**Environment predicate expressiveness** (`EnvironmentDef`, `model.rs:362-368`): left/right are full
phonetic-template `Pattern`s built from the *same* AST as phonological-rule LHS/environments —
natural-class + alpha-variable constraints, literal segment/boundary constraints, `Quantifier{min,max}`
(optional/repeated groups), and `Anchor(Left|Right)` word-edge anchors (`model.rs:270-317). So an
environment can express arbitrary regular left/right context, including alpha-variable agreement and word
edges — but **not** reference to non-adjacent morphemes (that's the job of co-occurrence rules, §9 — see
question (d) below for the sharp adjacent/non-adjacent line).

**Allomorph selection at final-validity time** (C# `Allomorph.IsWordValid`, ported at `hc-rules/src/
validity.rs`): every morph must satisfy at least one declared environment; bound-root exclusivity;
stem-name subsumption (required + excluded, W5); the affix's `required_syn_fs` re-checked against the
word's *final accumulated* FS (not just at apply time); morpheme/allomorph co-occurrence (§9); and a
**W3.2 disjunctive/free-fluctuation re-check**: every earlier-listed "passed-over" allomorph of the same
morpheme is re-tested, and if it would also have matched and does *not* free-fluctuate with the one
actually used, the word is **rejected** (first-listed-match-wins is enforced retroactively,
`validity.rs:274-291,620-648,684-710`). At synthesis-application time, allomorphs are tried in declaration
order and normally `break` after the first LHS-pattern match — *unless* the matched allomorph is
environment/FS-unconstrained and the next one free-fluctuates with it, in which case **both fire**,
producing two distinct analyses (morph.rs:1357-1370). This free-fluctuation branching is empirically
confirmed to matter: Sena `mbali` is retained **9×** by both the C# gold reference and the Rust port,
because 9 combinatorially-distinct free-fluctuating allomorph choices render to the identical surface
string (`rust/docs/phase2-completed/tearouts-and-lessons.md:21-31`, cross-checked directly against
`parity-out/golden/{master,parse-opt}/sena-*.tsv`). This is decision-critical for question (c) below.

**Citations:** Rust `hc-grammar/src/model.rs:744-778`, `hc-rules/src/validity.rs:98-291,500-715`; C#
`LexEntry.cs`, `RootAllomorph.cs`, `AllomorphEnvironment.cs:12-145`, `Allomorph.cs:105-204`.

| Grammar | Root entries | Allomorphs (roots only) | Max allomorphs/entry | Mean | Homograph surface shapes* | Entries in homograph shapes |
|---|---:|---:|---:|---:|---:|---:|
| Indonesian | 66 | 66 | 1 | 1.000 | 1 | 2 |
| Sena | 1,369 | 1,461 | 5 | 1.067 | 79 | 168 |
| Amharic | 54 | 55 | 2 | 1.019 | 0 | 0 |

\*Homograph = same first-allomorph surface `PhoneticShape` string, different gloss/POS between entries.
Detection uses only each entry's first allomorph (a documented coverage limitation — see census
methodology notes).

```bash
grep -c '<LexicalEntry ' <stratum-1-slice-of-file>
# allomorph max/mean and homograph grouping via Python re-extraction of PhoneticShape/Gloss/POS per entry
```

**Wild-max.** #Entries: unbounded in code — Sena's 1,369 is well within FLEx-scale lexica (FLEx databases
routinely hold 10,000–50,000+ lexical entries for a documented language) — **flag as unbounded, no
in-repo cap; realistic ceiling driven by target-language dictionary size, not engine design (order
10^4–10^5 for a mature FLEx project)**. #Allomorphs/entry: no cap in code; Sena's max of 5 is the largest
observed; free-fluctuation and suppletion can in principle produce many more (a suppletive paradigm with
many conditioning environments) — flag as unbounded.

---

## 5. Phonological rewrite rules

**Semantics.** `RewriteRuleDef` (`hc-grammar/src/model.rs:370-441`):
```rust
pub enum RewriteMode { Iterative, Simultaneous }
pub enum Dir { LeftToRight, RightToLeft }
pub struct RewriteRuleDef {
    pub mode: RewriteMode,   // default Iterative
    pub dir: Dir,            // default LeftToRight
    pub vars: VarTable,      // rule-scoped alpha-variable declarations
    pub lhs: Pattern,        // empty ⇒ epenthesis
    pub subrules: Vec<RewriteSubruleDef>,
}
pub struct RewriteSubruleDef {
    pub required_pos: Option<SymbolBits>,
    pub required_mpr: MprSet, pub excluded_mpr: MprSet,
    pub rhs: Pattern,          // empty ⇒ deletion
    pub left_env: Option<Pattern>, pub right_env: Option<Pattern>,
    pub self_opaquing: bool,   // computed once at LOAD time, static per rule (model.rs:420-441)
}
```
Both `mode` and `dir` come from **one packed XML attribute**, `multipleApplicationOrder
(leftToRightIterative | rightToLeftIterative | simultaneous)` (`HermitCrabInput.dtd:176-180`), split into
two orthogonal fields by the loader (C# `XmlLanguageLoader.cs:67-93`; Rust `load.rs:1156-1171`). Rules are
classified at runtime by LHS-vs-RHS length into three kinds (`rewrite.rs:865-882`, exact C# port): **Feature**
(equal length — pure feature change), **Narrow** (RHS shorter — deletion — or longer — expansion),
**Epenthesis** (empty LHS). There is **no rule-level obligatory/optional flag** distinct from the pattern's
own `Quantifier`/optional nodes — confirmed absent from the DTD (`grep -c obligatory` = 0 across all three
sample grammars; the only `optional="..."` attribute in the schema is on `<Slot>`, unrelated).

**(a) Application mode/order/direction — see question (a) below for the full analysis.** Summary:
Iterative = one continuous scan, re-matching against live (already-rewritten) state after every
application — can feed/bleed itself within one pass. Simultaneous = collect all matches against the
pristine input in one pass, then apply all at once — cannot feed/bleed within one pass. A confirmed,
counter-intuitive asymmetry (`rust/docs/p13-simultaneous-design.md:116-169`): on the **analysis** side, the
rule's declared `ApplicationMode` has almost no effect — Feature and Epenthesis subrules are **always**
analyzed Iterative-style regardless of the declared mode (the mode only gates whether the pass repeats to
a fixpoint, via `self_opaquing`); Narrow/deletion subrules are **always** analyzed Simultaneous-style
(`ReapplyType.Deletion`, repeated up to `1 + Morpher.DeletionReapplications` times) regardless of declared
mode. Synthesis, by contrast, honors the declared mode directly and uniformly. This asymmetry is
explicitly flagged in the docs as "real, in the live source."

**Citations:** Rust `hc-grammar/src/model.rs:370-441`, `hc-rules/src/rewrite.rs:865-882,1009-1039,1458-
1613`; C# `PhonologicalRules/RewriteRule.cs:15-45`, `IterativePhonologicalPatternRule.cs:17-48`,
`SimultaneousPhonologicalPatternRule.cs:22-37`, `AnalysisRewriteRule.cs:13-195`,
`HermitCrabInput.dtd:176-180`; `rust/docs/p13-simultaneous-design.md:32-183`.

| Grammar | Total prules | Assigned stratum | Subrules/rule | Left-ctx len (segments/classes) | Right-ctx len | Mode (all default) |
|---|---:|---|---:|---|---|---|
| Indonesian prule1 (Unspecified nasal) | — | Morphology | 1 | 0 | 2 | leftToRightIterative |
| Indonesian prule2 (Nasal deletion) | — | Morphology | 1 | 0 | 2 | leftToRightIterative |
| Indonesian prule3 (Nasalization in redup) | — | Morphology | 1 | 5 | 0 | leftToRightIterative |
| Indonesian prule4 (Nasal assimilation) | — | Morphology | 1 | 1 | 2 | leftToRightIterative |
| Indonesian prule5 (Voiceless obstruent deletion) | — | Morphology | 1 | 3 | 1 | leftToRightIterative |
| **Indonesian total** | **5** | | | | | |
| **Sena total** | **0** | (no `<PhonologicalFeatureSystem>` at all) | | | | |
| Amharic prule1–3 (e/o-creation) | — | Clitics | 1 | 0 | 0 | leftToRightIterative |
| Amharic prule4 (remove Cˑ length) | — | Clitics | 1 | 1 | 0 | leftToRightIterative |
| Amharic prule5 (a-deletion before a) | — | Clitics | 1 | 0 | 2 | leftToRightIterative |
| Amharic prule6–7 (CV merger) | — | Clitics | 1 | 0 | 0 | leftToRightIterative |
| **Amharic total** | **7** | | | | | |

```bash
grep -c '<PhonologicalRule ' samples/data/<lang>-hc.xml
grep -o 'multipleApplicationOrder="[^"]*"' samples/data/<lang>-hc.xml | sort | uniq -c   # empty everywhere = all default
```

**Wild-max.** No cap on rule count or subrule count per rule, or context length, anywhere in code — flag
**unbounded**. All three reference grammars are phonologically shallow (0–7 rules, 1 subrule each,
context ≤5 segments); FieldWorks/FLEx grammars for languages with rich morphophonemics (vowel harmony,
extensive assimilation cascades) can plausibly reach dozens of rules with multi-subrule disjunctions and
longer contexts — no in-repo evidence bounds this beyond the general `u32` quantifier field (`max: Option
<u32>`, unbounded when `None`).

---

## 6. Alpha-variables

**Semantics — see question (b) below for the full write-up.** Summary: alpha variables range **only over
the phonological feature system** (never syntactic/head/foot features — `XmlLanguageLoader.cs:1371-1374`
resolves a `<VariableFeature>`'s `phonologicalFeature` IDREF specifically into
`PhonologicalFeatureSystem`). A variable's value cardinality is bounded by however many symbols its bound
feature has (2 for a binary feature, up to the system's general per-feature cap of 63 symbols — §7), not
fixed at binary. **Schema-level cap of 24 distinct simultaneous variable names per rule** (the DTD
enumerates exactly the 24 lowercase Greek letters α–ω as the only legal `name` values for
`<VariableFeature>`, `HermitCrabInput.dtd:463` — independently re-verified by direct read, not just
reported by the sub-agent). The same named variable can be bound once (typically in the target match) and
then referenced identically in the left environment, right environment, and RHS structural change of the
same rule — confirmed by tracing the single shared `variables` dictionary threaded through all four
positions in the loader (`XmlLanguageLoader.cs:759-819`) and the shared `VariableBindings` object threaded
through match→environment→apply at runtime (`RewriteRuleSpec.MatchSubrule`, `PhonologicalRules/
RewriteRuleSpec.cs:37-115`; Rust `hc-rules/src/rewrite.rs:650-745`, `bind_or_check`/`resolve_bindings`).

**Wild-max.** 24 is a hard, citable schema cap. None of the three reference grammars come close to
exercising many simultaneous variables per rule (typical usage is 1 variable per rule, e.g. Amharic's
CV-merger rules).

---

## 7. Natural classes and phonological features

**Semantics.** Two disjoint feature systems: (1) `PhonFeatureSystem` — flat `SymbolicFeature`s over the
segment domain, no `ComplexFeature`s, plus one synthetic always-appended `Type` feature (`Segment` vs.
`Boundary`, 2 symbols) so the system always has ≥1 feature even when a grammar authors none (Sena has
none, `featsys.rs:10-15`); (2) `SynFeatureSystem` — POS (always feature 0) plus **head** and **foot**
complex features sharing one namespace (a `<FeatureValue>` under `AssignedFootFeatures` can reference a
feature declared under `<HeadFeatures>` — confirmed C# behavior, `model.rs:164-169`). Natural classes are
either `NaturalClassKind::Feature` (sparse feature-struct constraint) or `NaturalClassKind::Segments`
(explicit segment list) (`model.rs:341-355`).

**Hard cardinality caps in code (not just data-driven):**
- Phonological symbolic feature: **≥64 symbols → `GrammarError::Unsupported`** (max 63 supported),
  `featsys.rs:75-79,113-122`, test-verified up to 63 (`featsys.rs:394-404`).
- Syntactic POS symbol count: `>= 64` lints, `load.rs:633`.
- Syntactic non-POS symbolic feature: `>= 64` lints, `load.rs:707`.
- MPR features: **>64 → `Unsupported`** — `MprSet` is a `u64` bitset, `MprId(u8)` (`load.rs:333-335`;
  `model.rs:105-113`).

**Citations:** Rust `hc-grammar/src/featsys.rs:75-268`, `model.rs:155-238,341-355`; C#
`HCFeatureSystem.cs`, `SyntacticFeatureSystem.cs`, `NaturalClass.cs`, `SegmentNaturalClass.cs`.

| Grammar | Alphabet size (`SegmentDefinition`) | Phon. `SymbolicFeature` count | Phon. feat. values max/mean | Head/syntactic `SymbolicFeature` count | Head feat. values max/mean | `FeatureNaturalClass` | `SegmentNaturalClass` |
|---|---:|---:|---|---:|---|---:|---:|
| Indonesian | 29 | 14 | 7/2.357 | 0 | — | 10 | 4 |
| Sena | 40 | 0 (no phon. feature system authored) | — | 2 (`genro`=20 vals, `Num`=2) | 20/11.0 | 1 | 12 |
| Amharic | 417 | 22 | 6/2.182 | 11 | 5/2.091 | 14 | 3 |

Amharic's 417-segment alphabet is real (Ge'ez/Ethiopic script encodes whole CV syllables as single
characters, each its own `SegmentDefinition`), not a counting artifact — verified by inspecting sample
`<Representation>` values.

```bash
grep -c '<SegmentDefinition ' samples/data/<lang>-hc.xml
# feature-value counts via Python extraction of <SymbolicFeature>...<Symbol> counts per PhonologicalFeatureSystem/HeadFeatures/FootFeatures section
```

**Wild-max.** 64 symbols/feature and 64 MPR features are hard, citable code caps. Alphabet size and
#features/#natural-classes are **unbounded in code**; Amharic's 417-segment syllabary is already a
significant real-world outlier flagged repeatedly in the design docs as a **performance** (not
correctness) problem for the FST-compilation branch (alphabet² probing costs, §"complexity concerns" doc
notes below) — any large-alphabet script-based orthography (e.g. other Ethiopic-family or CJK-adjacent
transliteration schemes) could exceed even this.

---

## 8. MPR features and MPR feature groups

**Semantics.** Not morpheme-adjacency co-occurrence (that's §9) — MPR (Morphological/Phonological-Rule)
features are **productivity/gating classes**: an `MprId(u8)` bit position in a `u64` `MprSet` bitset
(`model.rs:105-151`). Rules can require/exclude specific MPR features on a word before applying
(`required_mpr`/`excluded_mpr`) and can add MPR features to a word's accumulated set on output
(`out_mpr`). Groups (`MprGroup`) add cross-cutting semantics via `MprGroupMatchType::{All, Any}` (default
`Any`) and `MprGroupOutput::{Overwrite, Append}` (default `Overwrite`) — e.g. an `Any`-group's "required"
check needs only one member present; an `Overwrite`-group's output clears sibling members not in the
output before unioning in the new ones (`model.rs:844-918`). Distinct from this, **compound productivity
restrictions** (`MprSet::compound_match`) are deliberately group-**unaware** even in C# — a flat
"intersect non-empty" check (`model.rs:143-150`).

The MPR-feature accumulator (`word.MprFeatures`/`word.mpr`) is carried on the whole word across the entire
derivation and is a **long-distance, position-independent** conditioning channel — a rule far from where
an MPR feature was set can still gate on it (see question (d)).

**Citations:** Rust `hc-grammar/src/model.rs:105-151,806-934`; C# `MprFeature.cs`, `MprFeatureGroup.cs`,
`MprFeatureSet.cs`.

| Grammar | `MorphologicalPhonologicalRuleFeature` | `MorphologicalPhonologicalRuleFeatureGroup` |
|---|---:|---:|
| Indonesian | 4 | 2 |
| Sena | 3 | 1 |
| Amharic | 6 | 2 |

```bash
grep -c '<MorphologicalPhonologicalRuleFeature ' samples/data/<lang>-hc.xml
```

**Wild-max.** Hard code cap of **64** (bitset width) — a real, citable bound, unlikely to be approached by
any real grammar (largest observed here is 6).

---

## 9. Morpheme/allomorph co-occurrence rules ("adhoc" prohibitions)

**Semantics.** Two parallel constructs (`MorphemeCoOccurrenceRuleDef` over `MorphemeId`,
`AllomorphCoOccurrenceRuleDef` over `AllomorphId`), each with a `require: bool` (co-occur vs. exclude), a
list of `others`, and an adjacency mode:
```rust
pub enum CoOccurrenceAdjacency { Anywhere, SomewhereToLeft, SomewhereToRight, AdjacentToLeft, AdjacentToRight }
```
`Anywhere` (the schema default) scans the **entire** morph sequence of the finished word — a genuine
**arbitrary-distance** constraint, not limited to neighbors. `SomewhereToLeft`/`Right` require the listed
others to appear, in order, anywhere to one side (not necessarily adjacent). Only `AdjacentToLeft`/`Right`
require strict positional adjacency (to the key itself, or — with multiple `others` — to the next listed
other). **Every attached rule must pass (AND across rules, not OR)** — deliberately not reproducing a
known, already-fixed C# bug (LT-22156) where OR-semantics were used (`hc-rules/src/validity.rs:300-
304,398-452`, citing the fix history). Checked as a **final acceptance gate over the whole assembled
word** (`Allomorph.CheckAllomorphConstraints` → `Allomorph.IsWordValid`, C# `Allomorph.cs:105-204`; Rust
`validity.rs:314-379,500-715`), not during incremental rule application — so this is a genuinely global,
whole-word predicate, not a local one.

**Citations:** Rust `hc-grammar/src/model.rs:494-527`, `hc-rules/src/validity.rs:300-452`; C#
`MorphCoOccurrenceRule.cs:11-196`, `AllomorphCoOccurrenceRule.cs`, `MorphemeCoOccurrenceRule.cs`.

| Grammar | `MorphemeCoOccurrenceRule` | `AllomorphCoOccurrenceRule` |
|---|---:|---:|
| Indonesian | 0 | 0 |
| Sena | 0 | 0 |
| Amharic | 0 | 0 |

```bash
grep -c '<MorphemeCoOccurrenceRule ' samples/data/<lang>-hc.xml
grep -c '<AllomorphCoOccurrenceRule ' samples/data/<lang>-hc.xml
```

**Wild-max.** No numeric distance/count limit beyond the binary adjacent-vs-anywhere/somewhere
distinction — genuinely **no principled bound** on the number of `others` per rule or the number of
co-occurrence rules per grammar; none of the three reference grammars use this construct at all (0
instances everywhere), so there is no empirical data point either — this is the weakest-evidenced
construct in the census and should be flagged as such to the complexity analysis rather than assigned a
number.

---

## 10. Category/POS gating

**Semantics.** There is **no POS hierarchy or inheritance tree anywhere in the model.** POS is one flat
symbolic feature (`SynFeatureSystem::pos`, always feature index 0, symbols = declared `<PartOfSpeech>`
elements in document order, `model.rs:174-176`). All "gating" (phonological-rule `required_pos`,
morphological-rule `required_syn_fs`, template `required_syn_fs`) is uniform feature-struct
unification/bit-overlap against this one feature — no separate category-gating construct exists. This
simplifies the complexity analysis: POS contributes at most `⌈log2(#POS symbols)⌉` bits of state, capped
at 64 symbols (§7), with no combinatorial hierarchy to reason about.

**Citations:** Rust `hc-grammar/src/model.rs:174-176,192-201`; C# `SyntacticFeatureSystem.cs`.

**Wild-max.** Bounded by the general 64-symbol cap on syntactic symbolic features (§7).

---

## 11. Feature percolation (head/foot features, stem↔affix propagation)

**Semantics — see question (c) below for the load-bearing analysis.** There is **no dedicated
"percolation" algorithm or function** distinct from ordinary feature-struct unification. Every
affixation/compounding/realizational rule computes its output syntactic FS via one shared idiom:
```rust
// synthesis: unify(required_fs, word.syn_fs) then priority_union(out_fs)  — morph.rs:1160-1172
// analysis:  is_unifiable check, then widening `add`                      — morph.rs:1182-1200
```
For affixation this is computed **once per rule application, before the per-allomorph loop**, so all
allomorphs of one rule application share the same resulting syntactic FS (verified: C#
`SynthesisAffixProcessRule.cs:41-236`, unify at line 122, shared across the allomorph loop at
`:139-233`). For compounding, only the **head**'s (not the non-head's) syntactic FS feeds the output —
`Unify(rule.HeadRequiredSyntacticFeatureStruct, input.SyntacticFeatureStruct)` then
`PriorityUnion(rule.OutSyntacticFeatureStruct)` (C# `SynthesisCompoundingRule.cs:100-101,181-182`); the
non-head's FS is checked only as a compatibility gate and is **discarded** from the output FS. Since head
and foot features share one namespace (§7), this single mechanism *is* what "head/foot feature
percolation" means operationally in HC — there is no separate mechanism to model.

**A confirmed, code-level gap in the compounding path**: C#'s `SynthesisCompoundingRule.ApplySubrule`
contains an explicit `// TODO: unify the variable bindings from the head and non-head matches`
(`SynthesisCompoundingRule.cs:235`) — alpha-variable bindings from the two sides of a compound are **not**
merged when building the compound's shape, an acknowledged incompleteness in the reference implementation
itself.

**Citations:** Rust `hc-rules/src/morph.rs:1160-1200,1284-1372,2553-2607,2871-2905`,
`hc-rules/src/stratum.rs:1639-1699` (W5 `choose_inflectional_stem`, family-relative stem reseeding); C#
`SynthesisAffixProcessRule.cs:41-236`, `SynthesisCompoundingRule.cs:80-235`.

---

## 12. Default/variable feature values, `ModifyFromInput`, truncation

**Semantics.** Default symbol values (`defaultSymbol` XML attribute → `PhonFeatureSystem::default_bits`,
`featsys.rs:49-55,219-225`) feed a "use-defaults" confirm step during matching (mirrors C#'s
`FeatureStruct.IsUnifiable/Subsumes/DestructiveUnify(..., useDefaults: true, ...)`); none of the three
reference grammars declare a default for any feature. `ModifyFromInput` (`OutputAction::Modify`) on
synthesis copies a part with feature constraints applied (the stem-changing/simulfix mechanism, §2); on
analysis it inverts by resetting the changed feature lane(s) to fully underspecified
(`full_mask`) — the dense-lane-representation reduction of C#'s `AntiFeatureStruct` negate-and-union
(`morph.rs:24-32`). **Truncation is not a distinct modeled operation** — there is no `Truncate`/`DeleteN`
output-action variant; the closest analog is the "copy-only affix" fallback, where a rule whose RHS
produces no positioned output node at all falls back to marking the word's *last* shape node as that
morph's anchor (`MorphStatus::Floating`, `word.rs:73-91`), not a genuine delete-N-segments primitive.

**Citations:** Rust `hc-grammar/src/featsys.rs:49-55,219-225`, `hc-rules/src/morph.rs:24-32,68-75`,
`hc-rules/src/word.rs:73-91`.

**Census note.** Amharic has 1 `ModifyFromInput` occurrence; Indonesian and Sena have zero — this
construct is essentially unexercised in the reference grammars.

**Wild-max.** No cap in code; not exercised enough in the reference grammars to have an empirical
data point beyond "rare" (1 of ~230 total rules across the three grammars).

---

## 13. Metathesis rules

**Semantics.** `MetathesisRuleDef` (`model.rs:453-472`): **one** match pattern (no LHS/RHS split, no
environments) plus two integer indices, `left_switch`/`right_switch`, into that pattern's node list. **No
MPR/POS gating at all** — `IsApplicable` is hardcoded `true` in both C# specs, and the DTD element has no
`requiredMPRFeatures`/`excludedMPRFeatures`/`requiredPartsOfSpeech` attribute. Synthesis physically swaps
the two switch ranges in the shape (segments between them keep their positions); non-`Segment` nodes
(mid-span boundaries) do **not** move during the swap — "segments jump over a boundary that stays put"
(`rust/docs/phase2-completed/metathesis-w4.md:33-36`). Analysis does **not** physically reorder nodes — it
is a feature-content exchange at fixed positions (union each matched node's `FeatureStruct` into the
paired node), searching the surface for the arrangement synthesis would have produced.

**An empirically-discovered engine limit, not a design choice**: switch groups are always exactly **1
node wide** in every real grammar seen — wider switch groups **crash the real C# reference engine**, so
the Rust port deliberately pins this as a hard constraint rather than "fixing" it into a divergence
(`rust/docs/phase2-completed/metathesis-w4.md:1-12`). This is a genuine undocumented cardinality limit in
the C# reference implementation itself, worth flagging to the complexity analysis as a de facto bound.

**Citations:** Rust `hc-grammar/src/model.rs:447-472`, `hc-rules/src/metathesis.rs:20-206`; C#
`PhonologicalRules/MetathesisRule.cs`, `SynthesisMetathesisRule.cs`, `AnalysisMetathesisRule.cs`;
`rust/docs/phase2-completed/metathesis-w4.md`.

| Grammar | `MetathesisRule` count |
|---|---:|
| Indonesian | 0 |
| Sena | 0 |
| Amharic | 0 |

None of the three reference grammars use metathesis. **Wild-max**: switch-group width is empirically
capped at 1 node by the reference engine's own crash behavior — a real, citable bound even though it is
not written down anywhere as a spec.

---

## 14. Compounding rules

**Semantics.** `CompoundingRuleDef` (`model.rs:688-714`): head/non-head required syntactic FS, output
syntactic FS, three separate MPR-restriction sets (head/non-head/output productivity restrictions,
group-unaware — §8), and an ordered list of `CompoundingSubruleDef`s (each with its own `head_lhs`/
`non_head_lhs` pattern parts and an RHS of `Copy(Head(i))`/`Copy(NonHead(i))`/`InsertSegments`/
`InsertContext` actions). **A single rule application combines exactly one head word with exactly one
non-head word** (binary by construction — `Word.CurrentNonHead` returns a single `Word`, C# `Word.cs:
380-388`); N-ary compounds arise only from **repeated** application (each application appends one more
`_nonHeadApps` entry). The **analysis-direction recursion depth is explicitly capped**:
`Morpher.MaxStemCount` defaults to **2** (verified directly: `Morpher.cs:56,72`; enforced at
`AnalysisCompoundingRule.cs:45`, `input.NonHeadCount + 1 >= _morpher.MaxStemCount ⇒ return
Enumerable.Empty<Word>()`) — by default the analyzer will not attempt to de-compound a word into more than
2 roots, with the C# source code comment explicitly framing this as a complexity control: "for
computational complexity reasons, we ensure that the non-head is a root, otherwise we assume it is not a
valid analysis and throw it away" (`AnalysisCompoundingRule.cs:59-60`). Head/feature combination: only the
**head**'s syntactic FS feeds the compound's output FS (§11); the non-head's FS is a compatibility gate
only. `AnalysisCompoundingRule` also performs a non-exhaustive dedup pruning "to reduce the search space"
(keeps only the longer of two same-shape/same-non-head-root candidates) — a deliberate, documented
completeness trade-off, not a correctness-neutral optimization.

**Citations:** Rust `hc-grammar/src/model.rs:688-714`, `hc-rules/src/morph.rs:2553-2836`,
`hc-rules/src/cascade.rs:417-438`, `hc-rules/src/stratum.rs:669-673`; C# `MorphologicalRules/
CompoundingRule.cs:19`, `SynthesisCompoundingRule.cs:45-235`, `AnalysisCompoundingRule.cs:44-115`,
`Morpher.cs:56,72` (both directly re-verified for this report).

| Grammar | `CompoundingRule` count | Subrules/rule |
|---|---:|---:|
| Indonesian | 2 | 1 |
| Sena | 8 | 1 |
| Amharic | 1 | 1 |

Sena's `ndikhali` (a real corpus word) is ground-truthed in the design docs to 8 valid analyses, all
shape `{root-class-agreement, é, ser/khal, NZR}`, confirming a genuine 2-root compound via `mrule7`/
`mrule8` plus a zero-surface noun-class-agreement prefix (`docs/fst-plan/FST_FULL_GRAMMAR_PLAN.md:268-
303`).

```bash
grep -c '<CompoundingRule ' samples/data/<lang>-hc.xml
```

**Wild-max.** `MaxStemCount = 2` is the **default**, but it is a `Morpher`-instance-configurable setting,
not a hard code ceiling — a grammar author/consumer can raise it, at real cost (the C# code comment
frames the default explicitly as a chosen complexity trade-off, and raising it changes the recursion
search space, not just a count). For a big-O analysis: **treat 2 as the realistic default bound, but note
it is a configuration knob, not an inherent grammar-model limit** — a FLEx grammar for a
compounding-heavy language (e.g. Germanic-style noun compounding) could in principle need 3+ and would
require raising this setting, with a corresponding cost increase the docs already characterize as a
"complexity reasons" trade-off.

---

## 15. Affix templates and slots

**Semantics.** `AffixTemplateDef` (`model.rs:718-726`): `is_final: bool` (default **true**), a POS-only
`required_syn_fs`, and an ordered `slots: Vec<SlotDef>`. `SlotDef` (`model.rs:728-737`): `optional: bool`
(default **false** — obligatory unless authored otherwise; a slot with zero rules is *always* treated as
optional regardless of the flag, `stratum.rs:1116-1120`), and `rules: Vec<MRuleId>` — a **non-disjunctive
rule-batch union**: every rule in the slot is tried and every distinct-keyed result from every alternative
rule survives (deduped by structural key, not "first match wins") — so a slot with *k* candidate rules can
fan out to *k* branches into the next slot. Slot traversal order: analysis descends top-down (last slot
first); synthesis ascends bottom-up (slot 0 first); a non-optional slot that produces nothing kills that
derivation branch. No cap on template count or slots-per-template anywhere in code (plain `Vec`s,
document order).

**Citations:** Rust `hc-grammar/src/model.rs:718-737`, `hc-rules/src/stratum.rs:964-1269`; C#
`AffixTemplate.cs`, `AffixTemplateSlot.cs`, `SynthesisAffixTemplateRule.cs`.

| Grammar | `AffixTemplate` count | Slots/template max | Slots/template mean | Rules/slot max | Rules/slot mean |
|---|---:|---:|---:|---:|---:|
| Indonesian | 0 (no templates authored; rules chain via POS input/output matching instead) | — | — | — | — |
| Sena | 24 | 7 | 3.125 | 19 | 7.160 |
| Amharic | 15 | 3 | 1.600 | 10 | 2.750 |

```bash
grep -c '<AffixTemplate ' samples/data/<lang>-hc.xml
grep -c '<Slot ' samples/data/<lang>-hc.xml
```

**Wild-max.** No cap found. Sena's 7-slot, 19-rules-in-one-slot template is the largest observed data
point; a maximally elaborate polysynthetic-language FLEx grammar (e.g. Athabaskan-style verb template with
10+ prefix positions) could plausibly exceed both — **flag as unbounded**.

---

## 16. Maximum morphemes chained per word (empirical, cross-checked against oracle output)

Rather than trust the raw (unannotated) word lists, the actual frozen oracle-gloss output in
`reports/oracle/{indonesian,sena}-oracle-gloss.tsv` was scanned directly for the longest morpheme chain in
any single analysis (counting `-`-separated gloss segments per analysis string):

```bash
python - <<'PY'
maxlen=0
with open("reports/oracle/<file>.tsv", encoding="utf-8") as f:
    for line in f:
        parts=line.rstrip("\n").split("\t")
        for a in parts[2].split(";"):
            maxlen=max(maxlen, a.count("-")+1)
print(maxlen)
PY
```

| Grammar | Max morphemes in one attested analysis | Example |
|---|---:|---|
| Indonesian (full 121-word oracle) | 4 | `AV-observe-Cont-LOC` (`mengamat-amati`) |
| Sena (300-word sample oracle) | 8 | `4+5+9-IMP-PLPRF-3S+1-encontrar-NEU-REC-IND` |
| Amharic | not measured (no oracle-gloss file in this repo for Amharic) | structural template estimate only: max 4 (1 prefix slot + 2 suffix slots + root, template #1/#4) |

Sena's empirical max of 8 matches the template-structural estimate (5 prefix slots + 2 suffix slots + 1
root = 8) computed independently from the `<AffixTemplate>` XML, corroborating both methods. Indonesian's
naive POS-graph theoretical upper bound (10 stacked rules via category cycling) is a substantial
overestimate not reflected in any attested form — the empirical/template-based numbers (4 and 8) are the
trustworthy figures for this census; the POS-graph bound should not be used.

**Wild-max.** No cap on chain length in the model itself (bounded only by however many template
slots/mrule-cascade steps a grammar author defines, and by the shared `StepBudget`/`MaxUnapplications`
engineering safety valves at parse time, which are performance guards, not semantic limits) — Sena's
attested 8 is already close to the template-structural ceiling for that specific grammar; other grammars
could define deeper templates. **Flag as unbounded in principle, empirically single digits (4–8) in the
three reference grammars.**

---

# Answers to the critical semantic questions

## (a) Phonological rules: obligatory/optional, self-feeding, ordering, direction

**Obligatory or optional?** Always obligatory when the pattern matches — there is **no** per-rule
obligatory/optional flag in the schema or object model (confirmed: DTD's only `PhonologicalRule`
attributes are `id`, `isActive` (compiled-in or not), and `multipleApplicationOrder`;
`HermitCrabInput.dtd:176-180`). All optionality is expressed through the pattern's own `Quantifier`/
optional nodes, not a rule-level toggle. (Contrast: `AffixProcessRule.Blockable` is about a more specific
*lexical entry* blocking a *morphological* rule, an unrelated concept.)

**Can a rule re-apply to its own output (self-feed)?** Yes, in two distinct ways depending on mode and
direction:
- **Iterative mode**: a *single continuous left-to-right (or right-to-left) scan* — after each
  application, matching resumes just past the rewritten span, reading *live, already-mutated* shape state
  (C# `IterativePhonologicalPatternRule.Apply`, `PhonologicalRules/IterativePhonologicalPatternRule.cs:
  17-48`; verified empirically: `gigugi` parses only under Iterative because the cursor's re-match after
  each application reads current node state, `rust/docs/p13-simultaneous-design.md:32-69`). This is one
  pass, not a fixpoint loop — it does not restart the whole rule from the beginning.
- **Simultaneous mode**: collects **all** matches against the **pristine, unmodified** input in one pass
  (`Matcher.AllMatches`), and only *afterward* applies all rewrites at once — a rewrite found in this pass
  **cannot** feed or bleed another match in the same pass (`SimultaneousPhonologicalPatternRule.Apply`,
  `PhonologicalRules/SimultaneousPhonologicalPatternRule.cs:22-37`; verified: `gigugu` parses only under
  Simultaneous, on the *same rule* as the Iterative example above — confirming this is a real semantic
  fork, not just an implementation detail).
- **On analysis (un-application)**, a genuine fixpoint loop exists for `self_opaquing` subrules — computed
  once at grammar-load time (`self_opaquing: bool`, `model.rs:420-441`; static per-rule fact, not
  per-word), the un-application repeats until no more matches are found (`ReapplyType.SelfOpaquing`, C#
  `AnalysisRewriteRule.cs:166-175`). A subrule is self-opaquing iff (Feature kind) `mode==Simultaneous` AND
  some RHS constraint fails to feature-unify with the rule's own environment, or (Epenthesis kind)
  `mode==Simultaneous` unconditionally; Narrow/deletion subrules use a *count-bounded* repeat instead
  (`ReapplyType.Deletion`, up to `1 + Morpher.DeletionReapplications` times), not an unbounded fixpoint.
  **On synthesis, no fixpoint wrapper exists** — self-feeding there is purely the Iterative single-scan
  mechanism above.
- A confirmed asymmetry (`rust/docs/p13-simultaneous-design.md:116-169`): on analysis, the rule's
  *declared* `ApplicationMode` has almost no direct effect — Feature/Epenthesis subrules are **always**
  dispatched Iterative-style (mode only toggles the self-opaquing fixpoint wrapper); Narrow/deletion
  subrules are **always** dispatched Simultaneous-style regardless of declared mode. This means a naive
  reading of the rule's XML attribute is insufficient to predict analysis-side behavior — the dispatch key
  is actually LHS-vs-RHS length (rule *kind*), not the declared mode.
- A related known hazard: an epenthesis rule whose own RHS re-satisfies its own trigger environment can, in
  the real C# reference engine under Iterative mode, **crash with an uncaught `InfiniteLoopException`** —
  a hard-coded 256-shape-node guard (`EpenthesisSynthesisRewriteSubruleSpec.cs:32-33`) is the only place
  this exception is thrown anywhere in the C# codebase. This is confirmed to be a real, exercisable engine
  bug/limit, not merely theoretical (`rust/docs/p13-simultaneous-design.md:220-238`).

**Ordering within a stratum**: strict linear sequence in declared-rule-list order — rule *i* runs to
completion (across the whole word) before rule *i+1* starts (`LinearRuleCascade`, `SIL.Machine/Rules/
LinearRuleCascade.cs:25-56`). This happens only *after* all of that stratum's morphology (rules + templates)
has finished. **Across strata**, morphology and phonology interleave in the classic cyclic sense (each
stratum's full morphology-then-phonology output feeds the next stratum), but never within one stratum at
single-rule granularity, and never with re-entry into an already-passed stratum.

**Direction**: a per-rule setting (`Dir::LeftToRight`/`RightToLeft`), packed into the same
`multipleApplicationOrder` attribute as mode (`rightToLeftIterative` ⇒ RightToLeft; anything else ⇒
LeftToRight, including `simultaneous`, for which direction is largely moot since all matches are found
before any rewrite). Analysis direction is always the **reverse** of the rule's declared synthesis
direction (`AnalysisRewriteRule.cs:33`).

## (b) Alpha-variables

- **Feature system scope**: phonological features only — never syntactic/head/foot features (a variable's
  `phonologicalFeature` IDREF resolves specifically into `PhonologicalFeatureSystem`,
  `XmlLanguageLoader.cs:1371-1374`). Attempting to attach a variable to an allomorph *environment* pattern
  (as opposed to a phonological-rule pattern) is unsupported and lints (Rust `model.rs:16-18`,
  `load.rs:1123-1125`) — C#'s per-environment variable scope is always empty.
- **Value cardinality**: not fixed at binary — a variable ranges over however many symbols its bound
  feature has (2 for the common binary case, up to the general 63-symbol cap on any phonological feature,
  §7). The backing representation (`UlongSymbolicFeatureValueFlags` vs. `BitArraySymbolicFeatureValueFlags`
  by symbol count, `SIL.Machine/FeatureModel/SymbolicFeatureValue.cs:22-39`) is a performance detail, not a
  semantic ceiling.
- **Multiple variables per rule**: yes — confirmed at both the schema level (`<VariableFeatures>` is
  `(VariableFeature+)`, one-or-more) and the runtime level (a `Dictionary<string, ...>` keyed by variable
  ID). **Hard schema cap: 24 distinct simultaneous variable names per rule** — the DTD's `name` attribute
  enumeration for `<VariableFeature>` lists exactly the 24 Greek letters α through ω and nothing else
  (independently re-verified: `HermitCrabInput.dtd:463`).
- **Cross-position linking**: confirmed — the same named variable, declared once, is threaded through the
  target pattern, left environment, right environment, and RHS structural change of the same rule (the
  loader shares one `variables` dictionary across all four load calls, `XmlLanguageLoader.cs:759-819`; at
  match time, a single `VariableBindings` object accumulates across target→left-env→right-env matching and
  is then consumed by the RHS apply step, `PhonologicalRules/RewriteRuleSpec.cs:37-115`). Binding
  semantics: first occurrence binds (to the matched node's actual value, or its negation if the occurrence
  has `polarity="minus"`), every subsequent occurrence checks for overlap with the bound value — a
  real, functioning multi-position agreement mechanism, not merely declarative.

## (c) Final WordAnalysis assembly — is it a pure function of the morpheme-ID sequence? (DECISION-CRITICAL)

**Short answer: mostly yes for gloss-relevant content, but with two independently-confirmed caveats that
mean "morpheme-ID sequence alone" is not a sufficient key for either (i) enumerating the correct analysis
*set*, or (ii) guaranteeing the exposed `Category` field never diverges.**

**What the public analysis object actually carries.** `WordAnalysis` (C# `SIL.Machine/Morphology/
WordAnalysis.cs:12-70`, directly re-read for this report) stores exactly three things: an ordered list of
morpheme IDs, a `RootMorphemeIndex`, and a single `Category` string (the word's POS symbol). Its
`Equals`/`GetHashCode` are defined **purely** over these three fields (`WordAnalysis.cs:46-61`) — the full
unified `SyntacticFeatureStruct` (features, agreement values) computed internally during parsing is **not**
part of the exposed identity at all. The Rust FST-branch signature format matches this exactly:
`join("+", morpheme.Id)` + `:` + `RootMorphemeIndex` (`rust/docs/HYBRID_FST_RUST_PLAN.md:288-297`).

**Caveat 1 — the morpheme-ID sequence is not, by itself, sufficient to enumerate the correct *set* of
analyses; allomorph identity is also needed, and the multiplicity this introduces is combinatorial, not a
small fixed additive bump.** This is directly confirmed, not inferred: Sena's real word `mbali` is
retained **9 times** by both the frozen C# gold reference and the Rust port, because 9
**combinatorially-distinct** free-fluctuating allomorph choices (§4) render to the identical surface
string while remaining, in HermitCrab's own semantics, separate analyses — the source itself uses the word
"combinatorially" (`rust/docs/phase2-completed/tearouts-and-lessons.md:21-31`; independently cross-checked
against `parity-out/golden/{master,parse-opt}/sena-*.tsv`, which reproduce the same 9×/6× duplicate
pattern). A deliberate attempt to deduplicate these broke a regression test that treats the 9 as
legitimately distinct recovered analyses. Free-fluctuating allomorphs of the same morpheme do **not**
change the gloss or POS — so this caveat does not mean two identical-morpheme-sequence analyses can have
*different glosses*; it means the **count/multiplicity** of analyses for a given morpheme-ID sequence is
not 1. The right mental model for a big-O analysis is a **product across positions/slots with free
variation** (each independently-fluctuating slot contributes a multiplicative factor equal to its own
number of interchangeable allomorphs), not an additive constant — this repo's evidence does not pin down
`mbali`'s exact factorization, but 9×/6× magnitudes on ordinary words are consistent with a small product
of per-slot factors, and this can grow combinatorially with the number of free-fluctuating slots a word
happens to exercise, not stay bounded by a fixed small constant per word. A compiler that collapses "same
morpheme-sequence" analyses into one output would silently under-count relative to what the reference
engine (and its own gold test suite) considers correct, by a factor that itself scales combinatorially.
**For an FST output design: the output must be able to distinguish allomorph choice, not just morpheme
choice, if exact analysis-count/enumeration parity with the reference engine is a goal** — a bare
morpheme-ID tag sequence is insufficient for that; a (morpheme ID, allomorph ID) sequence is, and the
output-path cardinality this adds is the combinatorial product described above, not a rounding error.

**Caveat 2 — the actionable case first: under the default (`Linear`, single-threaded) configuration that
all three reference grammars use, the analysis (including `Category`) IS a pure function of the (morpheme
ID, allomorph ID, root index) sequence — a tag-sequence output keyed on those three is sufficient for the
realistic case.** The purity gap described below is real but is strictly confined to a non-default
configuration (`Unordered` morphological-rule order combined with multi-threaded execution) that none of
the three reference grammars exercise. The exposed `Category` (POS) field is computed via feature
unification, and the dedup machinery that decides "is this the same analysis" does not check the full
feature structure, only a structural key. `Word.ValueEquals`/`FreezeImpl` (C# `Word.cs:508-546`) — the
equality comparer used
throughout the pipeline to dedup in-flight `Word` objects — is computed from shape, `_mruleApps` sequence,
root allomorph, stratum, etc., but **explicitly excludes `SyntacticFeatureStruct`**. Within one rule
application, the design correctly computes the syntactic FS **once**, shared across all allomorphs of that
application (`SynthesisAffixProcessRule.cs:41-236`, unify at line 122, shared at lines 139-233 — directly
re-verified), so under the default single-threaded configuration no live code path was found that
produces two different FS values for a literally identical `(mrule-sequence, root-allomorph, shape)`
tuple. However, a concrete non-default configuration **does** create this risk: when a stratum's
`MorphologicalRuleOrder` is `Unordered` and execution is multi-threaded (not the `SINGLE_THREADED` build),
`ParallelCombinationRuleCascade` (`SIL.Machine/Rules/ParallelCombinationRuleCascade.cs:32-77`) runs rule
applications in parallel and deduplicates the results via `.Distinct()` keyed on the same
feature-struct-blind comparer — since `FeatureStruct.PriorityUnion` is explicitly not commutative/
associative (`SIL.Machine/FeatureModel/FeatureStruct.cs:286-360`, priority goes to whichever operand is
applied second), **two different orderings of the same unordered rule set can produce the same shape/
mrule-ID sequence but different final `SyntacticFeatureStruct` values, and which one survives the dedup
is determined by non-deterministic thread-scheduling order, not by grammar semantics.** Since
`WordAnalysis.Category` is derived from this same feature structure, this is a real (if
non-default-configuration-gated) mechanism by which the same nominal morpheme-ID sequence could expose a
*different* `Category` across runs. **Net assessment for the FST design**: under the default (Linear,
single-threaded) configuration used by all three reference grammars, `Category`/features are a pure
function of the (morpheme ID, allomorph ID) sequence; under `Unordered`+parallel (a real, schema-legal
configuration this repo does not currently exercise in any reference grammar), purity is not guaranteed by
the reference implementation's own correctness invariants — the equality comparer's blindness to feature
structure means such a divergence, if it occurred, would go completely undetected rather than being caught
or reported.

## (d) Allomorph selection: adjacency vs. long-distance

**Both mechanisms coexist, cleanly separated.** Ordinary phonological **environments**
(`AllomorphEnvironment.cs:12-145`) are strictly adjacent — `IsMatch` only inspects the immediate left/right
shape-node neighbors of the morph. But **co-occurrence rules** (§9) are explicitly long-distance-capable:
the `Anywhere` adjacency mode (the schema default) scans the **entire** word's morph sequence regardless
of position (`MorphCoOccurrenceRule.cs:99-102`), and `SomewhereToLeft`/`SomewhereToRight` scan one whole
direction without requiring adjacency — only `AdjacentToLeft`/`AdjacentToRight` are truly local. A third,
even coarser long-distance channel is the **MPR-feature accumulator** (§8): `word.MprFeatures` unions in
contributions from every rule applied so far across the *entire* derivation, and later rules gate on this
whole-word accumulated set with no positional restriction at all — a rule can react to an MPR feature set
by a rule far earlier in the derivation. **Conclusion: allomorph selection is not purely local** — while
phonological *environments* are adjacency-only, the co-occurrence and MPR mechanisms make the *overall*
allomorph/rule-applicability decision a genuinely global (whole-word) predicate, which matters for FST
compilation because it means allomorph choice cannot always be resolved by a bounded-window automaton
without some form of accumulated state.

## (e) Compounding: max roots, head/feature combination

A single `CompoundingRule` application is **always binary** (exactly one head word + one non-head word,
`Word.CurrentNonHead` returns a single `Word`) — N-ary compounds arise only through repeated application,
each adding one more non-head to an accumulating list. The **analysis direction is capped by default at 2
roots total** (`Morpher.MaxStemCount = 2`, directly verified at `Morpher.cs:56,72`, enforced in
`AnalysisCompoundingRule.cs:45`), explicitly framed in the C# source comment as a computational-complexity
trade-off, not a linguistic universal — it is a configurable `Morpher` setting, so a real grammar could set
it higher at a corresponding cost. Feature combination is asymmetric: only the **head**'s syntactic
feature structure (unified with the rule's `HeadRequiredSyntacticFeatureStruct`, then priority-unioned with
`OutSyntacticFeatureStruct`) becomes the compound's output FS; the non-head's FS is checked only as a
compatibility gate (`NonHeadRequiredSyntacticFeatureStruct.IsUnifiable(...)`) and does not itself
contribute to the output. MPR features, by contrast, combine from **both** sides (`AddOutput` unions in
both the rule's own output-restriction MPR set and whatever the head-word already accumulated). A known,
acknowledged incompleteness: alpha-variable bindings from the head-side and non-head-side pattern matches
are explicitly **not** unified when constructing the compound's shape (`SynthesisCompoundingRule.cs:235`,
a live `TODO` comment in the reference implementation).

## (f) Non-pure-function / external-state concerns

Several genuine mechanisms make "surface string → analysis set" not a perfectly clean pure function under
some configurations, though none of them are exercised by the three reference grammars at their default
settings:

1. **Trace mode changes the returned analysis-set *structure*, not just logging.** `mergeEquivalentAnalyses
   = Morpher.MergeEquivalentAnalyses && !TraceManager.IsTracing` (`AnalysisStratumRule.cs:114-115`) — with
   tracing off (the default), analyses producing the same shape are folded into one canonical `Word` with
   others demoted to an `Alternatives` list and excluded from the returned set; with tracing on, this
   folding is skipped entirely, so the same input word returns a *differently shaped* (flatter, larger)
   collection purely as a function of whether tracing is enabled. This is a documented, intentional
   trade-off (the code comment explains merging "messes up the tracing"), not an accidental bug — but it
   is real and means the parse-result API is not invocation-mode-independent.
2. **`Morpher.MaxUnapplications`** (default 0 = unlimited) truncates analysis generation early once the
   output count reaches the cap, inside a loop whose enumeration order depends on internal traversal order
   — when set to a nonzero value (opt-in, intentionally, for debugging pathologically slow words), *which*
   analyses survive is order-dependent, a genuine parse-order-dependent cutoff.
3. **`Unordered` + multi-threaded execution** (§c above) can make the exposed feature structure/`Category`
   depend on thread-scheduling order for a fixed input — not exercised by any of the three reference
   grammars (all use `Linear` order in their reference configuration) but schema-legal and a real gap in
   the reference implementation's own guarantees.
4. **`ITraceManager`/`TraceManager` are otherwise pure logging side effects** — no case was found where
   trace-manager methods themselves mutate shape, features, or rule selection (only item 1's *caller-side*
   gate does).
5. Two independently-confirmed **live oracle bugs** further complicate "pure function" framing (both
   found during this porting project, not hypothesized): **LT-22613** — `GrammarAnalyzer.
   ComputeMaxAnalysisLength` under-budgets analysis-length growth for any phonological rewrite subrule
   whose LHS/RHS segment counts differ, causing the default (non-tracing) parse path to silently
   over-prune valid analyses that the traced path finds correctly (`docs/hermitcrab-rust-port-audit.md:
   29-36`) — i.e., **on the live C# reference oracle, tracing on vs. off can change which analyses are
   returned for the identical grammar and word, independent of the Rust port entirely.** A second,
   independently bisected case (Simultaneous-mode self-opaquing epenthesis reapplication,
   `rust/docs/p13-simultaneous-design.md:241-289`) reproduces the same phenomenon via a different
   mechanism (the nogood-memoization cascade is only installed when not tracing) — verified three
   independent ways (a passing NUnit test, a from-scratch in-memory reconstruction, and the same loaded
   `Language` object giving 0 vs. 1 result purely based on `TraceManager.IsTracing`). **These mean the
   reference oracle itself is not always a pure function of (grammar, word) — it also depends on whether
   tracing is enabled**, which is important context for any complexity analysis that treats "the C#
   engine's output" as ground truth.

---

# Constructs with no obvious classical-FST encoding

Kept deliberately short and precise, per the design docs' own repeated framing (Kaplan & Kay 1994: an SPE-
style rewrite-rule cascade over regular components denotes a regular relation regardless of context
length; the only mathematically forced exception is unbounded copying):

1. **Whole-stem (unbounded-span) reduplication.** A reduplication allomorph whose repeated LHS part is an
   unbounded quantified span (`Quantifier{max: None}` over an entire stem, i.e. `w → ww` for stems of
   arbitrary length) is **provably not a regular language** (pumping lemma; Hopcroft & Ullman 1979) — no
   finite-state transducer of any size can represent it exactly. This is the **only** construct in the
   entire inventory above that is mathematically, not just practically, inexpressible as a classical FST.
   Confirmed independently by the model (no size-limit field exists on the reduplication mechanism, §3)
   and by the design docs (`docs/fst-plan/HYBRID_FST_FEASIBILITY.md:259-269`,
   `docs/fst-plan/HERMITCRAB_FST_ADVISOR.md:73-124`, `docs/fst-plan/FST_FAST_PATH_PLAN.md:92-108`), all of
   which converge on the same textbook justification and the same practical workaround (a bounded runtime
   peel/strip-reanalyze-verify loop outside the FST, not a larger or cleverer FST).

Everything else surveyed in this document — including feeding/bleeding rule cascades, long-distance vowel
harmony, deletion/epenthesis (with their reapplication-count semantics), metathesis, arbitrary-distance
morpheme co-occurrence constraints, and MPR-feature-accumulator conditioning — is **regular** per Kaplan &
Kay and is handled in this codebase's FST-compilation branch via lazy per-rule inverse-transducer
composition (never an eagerly materialized product, which the docs record as having been tried and having
exploded combinatorially exactly as theory predicts, `docs/fst-plan/HYBRID_FST_FEASIBILITY.md:223-243`).
Two of these are flagged in the docs as merely **"slow today"** (an engineering/implementation-completeness
gap, not a regularity gap) and should not be listed here as non-regular:
- **Length-bounded reduplication/infixation at a fixed template slot** — regular, reclaimable, per
  `docs/fst-plan/HERMITCRAB_FST_ADVISOR.md:73-124`.
- **Unbounded-environment harmony/spreading rules** (a feature that must match arbitrarily far away) —
  still regular (state-encode the spreading feature); merely not yet fast in the current implementation.

Two further items are genuinely open questions about the *reference engine's own* behavior (not about
regularity of the underlying language) and are called out separately rather than folded into the
regularity claim above, per question (f):
- Whether the C# reference oracle's own tracing-on/off divergence (two independently confirmed cases) is
  itself something a classical-FST compilation should reproduce, or should instead treat as an oracle bug
  to be bypassed — this is a scoping decision for the analysis, not a mathematical fact about FSTs.
- Whether exact analysis-*count* parity (not just analysis-*set* parity) is a goal — if so, the free-
  fluctuation multiplicity finding in question (c) means the FST's output alphabet must carry allomorph
  IDs, not just morpheme IDs, which is a real (if modest) increase in output-label cardinality, not a
  regularity problem.
