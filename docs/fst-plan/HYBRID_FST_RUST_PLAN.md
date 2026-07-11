# Hybrid FST → Rust — implementation plan (full parity, oracle-gated)

> **Audience:** implementing agents (Sonnet, with Opus/Fable review) working on the `rust` branch
> of the `machine` repo, after the `fst-advisor` branch has fully landed. Read this whole file
> before editing anything. Companion documents: `HYBRID_FST_FEASIBILITY.md` (what the system is and
> why it works — read it first if you don't know the architecture), `FST_FAST_PATH_PLAN.md` +
> `FST_FULL_GRAMMAR_PLAN.md` (the C# execution history — the authoritative record of every design
> decision, measured number, and known bug), and `rust-conversion.md` (the engine-port plan whose
> conventions, oracles, and process rules this plan inherits).

## 1. Mission

Port the **hybrid propose-and-verify FST analyzer** (the entire FST subsystem on the `fst-advisor`
branch — trie proposer, surface-phonology precompile, junction probing, peels, v1 lockstep
phonology, the general rule-inverse chain, composite, restricted-re-analysis verify, coverage
probe, and grammar advisor) to native Rust on the `rust` branch, at **100% absolute parity**: for
every word in every corpus, the Rust hybrid's verified analysis set is **byte-identical** to the
C# hybrid's, gated by the same oracle discipline (golden TSVs generated from the C# implementation,
compared mechanically) already used for the engine port.

Non-goals for this plan: flipping the chain-vs-v1 default (a post-parity measured decision, §12),
performance work beyond "don't be slower than C#" (tracked, not gated), and any Rust-side design
"improvements" that change observable behavior (see the bug-for-bug rule, §4.3).

## 2. Context you must have

### 2.1 What exists in C# (the source of truth)

The `fst-advisor` branch (single squashed commit on master; ~7,300 new/changed lines in
`src/SIL.Machine.Morphology.HermitCrab`, ~7,000 in tests; suite 144/144 green). Read the C# via
`git show <landed-ref>:<path>` from the rust branch, or in a dedicated worktree. Inventory, by
port unit (line counts are the `git diff --stat` sizes, a good proxy for effort):

| C# file (src/SIL.Machine.Morphology.HermitCrab/) | Lines | Role |
|---|---|---|
| `FstTemplateAnalyzer.cs` | 2,015 | THE core: trie builder (root chains + checkpoints, affix/template arcs, derivation BFS `DerivableToCategory` incl. compounding edge, compound loop, boundary arcs, junction deletion-skips) + BOTH walkers (bare NFA walk `AnalyzeShape`/`EpsilonClosure`; chain walk `AnalyzeChain`/`ChainClosure`/`CascadeSymbol` with `PConfigKey` state-vector configs, boundary insertion, `InsertionsUsed`) + `BeamBudget` + `ToWordAnalyses` |
| `RuleInverseCompiler.cs` | 1,067 | per-rule inverse-transducer compiler: substitution (I1), deletion floors + epenthesis (I3), metathesis + combo cap + identity seeding (I5), three-tier report (Exact/Permissive/IdentitySkip + reasons) |
| `GrammarFstAdvisor.cs` | 614 | static linter: per-rule advisories (Escape/Cost/Info), `Regular` axis, tier verdict |
| `PhonologyRuleCompiler.cs` | 349 | v1 compiler (merged single automaton; the measured-faster DEFAULT phonology path) |
| `SurfacePhonology.cs` | 341 | build-time probing through real synthesis: `Variants`, `DeletionJunctions`, bare-root surfaces, `RenderNodes` (skips `IsDeleted()`), memoization + capability gates |
| `FstCoverageProbe.cs` | 299 | the probe product: `ForLanguage(...).Probe(words)` → `ProbeReport`; `CompareGrammars` diff |
| `ReduplicationProposer.cs` | 249 | peel: full/partial/tail-copy scans + literal-separator scan + suffix-peel fallback |
| `ForwardSynthesisProposer.cs` | 222 | opt-in root×affix-combo synthesis precompile (`forwardSynthesis` flag) |
| `EnvNfaCompiler.cs` | 191 | environment `Pattern` → NFA fragments (Constraint/Quantifier/Group/Alternation; anchors → Permissive) |
| `CompositeProposer.cs` | 151 | union + signature dedup of all proposers; `ForLanguage` wiring (incl. `useChainPhonology` opt-in) |
| `FstVerification.cs` | 138 | set-parity diff (manual divergence-inspection tool for benchmarks) |
| `MorphTokenCodec.cs` + `MorphToken.cs` | 242 | token encoding of trie paths → `WordAnalysis` (op classification: Prefix/Suffix/Infix/Redup/Compound/Clitic/Process...) |
| `InfixProposer.cs` | 117 | peel: infix strip-and-reparse over `SurfacePhonology` variants |
| `FstReplay.cs` | 115 | verify: restricted re-analysis (pins `LexEntrySelector`/`RuleSelector`; compound extra-roots) |
| `ChainPhonologyProposer.cs` | 108 | wraps the chain walker as the opt-in phonology proposer |
| `InversePhonology.cs` | 92 | the inverse-transducer substrate (arcs: substitution / ε-input restoration / ε-output / structural-ε) |
| `ComposedPhonologyProposer.cs` | 91 | runtime cascade un-application proposer (still in the default composite) |
| `LockstepPhonologyProposer.cs` | 69 | v1 lockstep proposer (DEFAULT; carries a known, deliberately-kept gate bug — §4.3) |
| `VerifiedFstAnalyzer.cs` | 50 | propose → verify loop |
| `MorpherPool.cs` | 42 | Rent/Return pool for concurrent verify |

Tests to port (toy-grammar families; §9): `VerifiedFstAnalyzerTests` (880), `RuleInverseCompilerTests`
(567), `ChainWalkerTests` (532), `ChainDeletionEpenthesisTests` (360), `BoundaryTape*Tests`
(535 across 3 files), `PhonologyRuleCompilerTests` (317), `GrammarFstAdvisorTests` (258),
`ChainPhonologyProposerTests` (225), `LeverTwoSpikeTests` (224), `FstTemplateAnalyzerTests` (206),
`FstCoverageProbeTests` (164), `BeamCapTests` (159), `SurfacePhonologyJunctionTests` (151),
`MorphTokenCodecTests` (122), `FstVerificationTests` (74), `MorphTokenTests` (67), plus
`FstSenaBenchmark` (902, `[Explicit]` — becomes the Rust benchmark battery).

### 2.2 What exists in Rust (the substrate)

The `rust` branch engine port (see `rust-conversion.md` and its progress notes): crates
`hc-featstruct` (feature structures, unification, interner), `hc-shape` (SoA shapes),
`hc-grammar` (XML loader + full grammar model), `hc-fst` (FSA engine: Thompson/determinize/
traversal/registers), `hc-rules` (rewrite + morphological rules, analysis + synthesis, in-flight
`Word`), `hc-parse` (Morpher pipeline, root-allomorph trie, memoization, batch CLI `hc-rs`),
`hc-cli`. Engine parity scoreboard at plan-writing time: **Indonesian 121/121 byte-identical**,
Amharic 532/673, Sena tractable but cap-limited (full Sena deferred to the engine plan's M9).

The hybrid port sits ON TOP of that engine: it needs the engine as (a) its **verifier** (restricted
re-analysis), (b) its **build-time prober** (SurfacePhonology and both rule compilers run real
synthesis cascades to observe rule effects), and (c) its **fixture loader** (toy grammars arrive as
XML, §9). It does NOT need the engine's unrestricted-analysis performance: verify pins one root and
a few rules, which collapses the search that currently caps out on Sena/Amharic.

### 2.3 Sequencing constraint

Do not start until (a) `fst-advisor` has landed (its C# is the oracle — porting against a moving
branch invalidates goldens), and (b) the engine port has **Indonesian full parity** (already true).
Per-grammar gating below (§5) handles Sena/Amharic engine-parity residuals — the hybrid port does
not wait for them globally.

## 3. Parity: the definition of done

"100% absolute parity" means ALL of the following, mechanically compared, per grammar:

1. **Verified analysis-set parity (the headline):** for every corpus word, the Rust composite's
   verified analysis set equals the C# composite's **byte-identically** under the frozen signature
   format (§6.2) — both in the default configuration (chain OFF) and with `useChainPhonology` ON.
   Corpora: Indonesian 121, Sena 7,121 (engine-watchdogged; see §5), Amharic 673 (engine-parity
   gated; see §5).
2. **Candidate-set parity (pre-verify):** per word, the multiset of candidates leaving the
   composite (post-dedup), labeled by proposer, matches. This is a stronger, faster-to-debug gate
   than #1 — verify can mask proposer drift (two different candidate sets can verify to the same
   answer). Any deliberate deviation must be recorded per §4.3.
3. **Structural parity:** FST `StateCount` per grammar equal to the C# build's; tier reports equal
   (Indonesian `Exact=2, Permissive=3, IdentitySkip=0`; Amharic `Exact=2, Permissive=4,
   IdentitySkip=1`; Sena 0 rules) including per-rule reason strings; advisor reports equal
   (advisory list, severities, `Regular` flags, tier verdict); `BeamOverflowCount` equal per corpus
   run at the same budget.
4. **Soundness battery:** the negative-examples sets (near-miss non-words; defined by the C#
   `Soundness_NegativeExamples` battery and exported per grammar as `negatives.txt` goldens in F0,
   §6.1) yield empty on both sides; a Rust-side re-verification that every emitted analysis is
   confirmed (this is by-construction, but assert it in the harness once).
5. **Toy-test parity:** every ported toy-grammar test (§9) passes with the same expected sets,
   including the LIVE honesty baselines (composite-covers / v1-misses assertions) where their
   subject mechanisms exist in Rust.
6. **Knob parity:** `forwardSynthesis`, `maxAffixes`, `useChainPhonology`, `enableJunctionProbing`,
   `maxBeamWork`, `restorationCap`, `maxBoundaryInsertions` all exist with the same defaults and
   the same pinned-by-test default values.

Anything less at a gate is a blocker for that milestone, not a note.

## 4. Ground rules

### 4.1 Process (inherited from the engine port — these are standing orders)

- **One revertible milestone per commit**, measurements in the commit message. Full workspace
  green + `cargo fmt`/`clippy` clean at every commit.
- **Sonnet implements, Fable/Opus freezes contracts and reviews** every milestone before the next
  starts. Implementing agents: commit incrementally, write durable output early, terse status;
  if an agent dies mid-run, check `git status` before dispatching a fresh agent (fresh > resume
  when the tree is clean/partial).
- **Byte-identical gates use self-generated baselines**: regenerate the C# "before" side yourself
  (or from the frozen goldens) — never trust a long-running agent's captured artifact (a truncated
  capture reads as drift; this bit the C# I4 gate).
- **Corpora and goldens stay untracked local files** under `rust/parity-out/` (gitignored), same
  convention as the engine port. Grammar paths:
  `C:\Users\johnm\Documents\repos\machine\samples\data\{indonesian,sena,amharic}-hc.xml` + word
  lists.
- Kill orphaned test processes before rebuilding (DLL/exe locks — testhost on the C# side, stale
  `hc-rs`/cargo test binaries on the Rust side).
- **Standing per-commit gates** once the relevant subsystem exists: tier-report diff vs recorded
  numbers (catches silent compiler drift — this caught a real regression in C# I5), and the stats
  battery (state count, build time, walk p50/p95, coverage, overflow count) reported with every
  measured result.

### 4.2 Determinism rules (Rust-specific)

Byte-parity dies on iteration order. From day one:

- No `HashMap`/`HashSet` iteration order may reach any observable output (goldens, state IDs used
  in dumps, candidate order pre-dedup where it affects dedup outcomes, report lines). Use
  insertion-ordered structures (`Vec` + membership set, `indexmap`) or sort at the boundary.
- Golden lines are always **sorted ordinal** (byte-wise) before writing, `-` for the empty set —
  same normalization as the engine goldens.
- Candidate **dedup order matters semantically**: `CompositeProposer` yields first-proposer-wins
  under signature dedup, and proposer order is fixed (FST, [forwardSynthesis], redup, infix,
  composed, lockstep/chain). Preserve that exact order — a different winner with the same signature
  can carry a different `WordAnalysis` payload into verify.
- Threading: verified runs must be thread-count-invariant (the C# side is; assert
  `--threads=1` equals `--threads=N` once, then run parallel).

### 4.3 Bug-for-bug parity (the rule that prevents re-litigating the port)

The C# implementation contains **deliberately-kept quirks whose behavior the goldens encode**. Port
them faithfully and pin each with a test + a `// PARITY:` comment linking the C# source. Known
list (audit for more during F1 — any newly found quirk gets added here):

1. `LockstepPhonologyProposer.HasNonIdentityArcs` inspects only arcs leaving the START state, so
   any rule whose branches all begin with a left-environment identity arc silently disables the v1
   proposer for that rule (documented in `ChainDeletionEpenthesisTests`; kept as I7 retirement
   evidence). The default path's candidate sets depend on this.
2. `PhonologyRuleCompiler` (v1) builds `_alphabet` from Segment-type char defs only — any rule
   with a `BoundaryMarker` in its environment is unconditionally unsupported in v1. (The chain
   compiler fixed this; v1 deliberately didn't change.)
3. `BuildAffixArcs` dedups variants **by rendered string**, not by FeatureStruct sequence (the
   Phase-H state-count note). State counts encode this.
4. The chain's deletion-restoration cap counts EVENTS while an engine round restores multiple
   sites at once — the chain is narrower on multi-site words (pinned by test).
5. `RuleInverseCompiler` α-variables: ONE representative probed per class → Permissive tier, no
   per-binding enumeration. Amharic's `CV merger at morpheme boundaries` must come out
   IdentitySkip `[alpha-variable,no-effect]` — that exact string is in the tier-report gate.
6. Self-feeding iterative rules: no detection, documented residual (the honest criterion was
   researched and dropped in the C# I5 redo — do not "improve" this).
7. Beam accounting: the budget is debited at (a) every post-dedup frontier admission AND (b) once
   per matching arc inside `CascadeSymbol` before recursing; `Overflowed` latches. Overflow counts
   are gate #3 — the debit points must match exactly.
8. `FstReplay` keeps templates, strata, and ALL phonological rules open; `CompoundingRule` opens
   only when extra roots are present. Signature match is per-morpheme identity + root index.
9. **(Found during F4.)** `State.Arcs` (`ArcCollection.AddInternal`) stores arcs via
   `List<T>.BinarySearch` against a comparer keyed on `ArcPriorityType`; `FstTemplateAnalyzer` never
   varies that priority (always the implicit default), so every comparison ties, and .NET's
   binary search returns the first-probed midpoint on a tie — a deterministic, closed-form,
   NON-insertion-order arc storage order: the `k`-th arc added to a state (`k` = arcs already
   present) lands at index `0` if `k==0` else `(k-1)/2`. This determines per-word CANDIDATE
   EMISSION ORDER (the bare/chain walkers iterate `state.Arcs` forward), which F3's own structural
   dump gate could not catch (it canonicalizes/sorts arc lines before comparing — see `canon.rs`'s
   doc, which predicted exactly this). Ported in `trie.rs`'s `arc_insert_index`/`insert_arc`
   (replaces plain `push`); confirmed by F4's candidate-order gate going from a count-only match to
   byte-identical, line order included, the moment this was implemented.

If a quirk turns out to be *unportable* exactly (e.g. it leans on C# reference identity), stop,
document the smallest behavioral delta, and get review sign-off before proceeding — do not silently
approximate.

## 5. Per-grammar gating (how engine-parity residuals are handled)

The hybrid's verify inherits the Rust engine's semantics. Where the engine itself is not yet at
parity, hybrid parity is gated per grammar, in this order:

1. **Indonesian — the primary development grammar.** Engine at full parity; every milestone gates
   on it. All 121 words, chain-off and chain-on, candidate + verified parity.
2. **Sena — second.** Zero phonological rules (phonology proposers inert — this isolates the trie/
   peel/verify half of the port), but the largest trie (18,871 states) and the compound/`ndikhali`
   case. Unrestricted engine analysis caps out on pathological words, but hybrid verify only runs
   RESTRICTED analysis — expected tractable everywhere. Gate on the slice-60 file first (the
   "guarded slice", defined in §6.1; 57/57 in C#), then the full 7,121-word corpus with the
   watchdog harness; compare
   FST-side outputs word-for-word vs the C# hybrid goldens (NOT vs the raw engine — the C# hybrid
   already encodes the engine agreement).
3. **Amharic — last, engine-parity-gated.** The Rust engine is at 532/673; the C# hybrid also has
   no end-to-end Amharic run yet (feasibility report §9). Do the structural gates unconditionally
   (StateCount, tier report `2/4/1` with exact reason strings, census/advisor report, build-cost
   sanity), and gate verified-set parity on the intersection of words where the Rust ENGINE is at
   parity, expanding as the engine port closes its residuals. Record the exclusion list
   explicitly — never report a subset gate as a full gate.
   **Hard precondition (added post-F5, per an independent Fable review):** F5's `replay.rs` never
   wires `RealizationalAffixProcessRule`/mrule gating on the synthesis side — harmless on
   Indonesian/Sena (zero realizational rules in either), but Amharic has real realizational rules,
   so this must be closed (wiring `SynthesisRealizationalAffixProcessRule.cs`/
   `SynthesisAffixTemplatesRule.cs`'s C# equivalents into the Rust synthesis-side gate) **before**
   the first milestone runs any Amharic word through `VerifiedFstAnalyzer` — don't rely on a cold
   golden mismatch to rediscover this gap; it's a known, named precondition, not a surprise to
   debug later.

## 6. Oracles and goldens

### 6.1 F0 — C#-side golden tooling (built on the landed fst-advisor code)

Extend the C# `hc` tool (the same net10.0 tool used for engine goldens —
`SIL.Machine.Morphology.HermitCrab.Tool`) with hybrid dump commands, or add `[Explicit]` dump
tests; either way the output formats below are FROZEN in F0 and versioned in the golden directory:

- `fst-batch <grammar> <words> <out.tsv> [--bare] [--chain] [--forward-synthesis]
  [--no-junctions]` — per word: `{idx}\t{word}\t{status}\t{verified-signatures}` (signatures
  sorted ordinal, `;`-joined, `-` if empty). A `STARTED` sentinel line precedes each word
  (crash/hang resumability), same as engine goldens. `--bare` = the `FstTemplateAnalyzer` proposer
  alone, no sibling generators — this produces the bare-FST goldens that F4/F5 gate on.
- `fst-candidates <grammar> <words> <out.tsv> [--bare] [--chain]` — per word, per candidate:
  `{idx}\t{word}\t{proposer}\t{signature}` — post-composite-dedup, in composite emission order.
- `fst-restricted <grammar> <words> <out.tsv>` — for each word, for each verified candidate: the
  candidate signature plus the full analysis set `Morpher.AnalyzeWord` returns under that
  candidate's pinned selectors (sorted ordinal). This is the F1 selector-parity oracle: it
  isolates `FstReplay`'s restricted-run semantics from everything else.
- `fst-stats <grammar> <out.txt>` — `StateCount`, tier report (rule → tier + reasons, exact
  `GrammarFstReport.Format()` text), advisor report text, per-affix `Variants`/`DeletionJunctions`
  dumps (affix underlying → sorted variant surfaces / junction pairs), bare-root surface dump,
  beam default, knob defaults.
- Toy-grammar XML export (§9).

Goldens land under `rust/parity-out/golden/fst-advisor/{indonesian,sena,amharic}/...` (gitignored),
generated once per C# oracle ref, with the generating commit hash in a `MANIFEST.txt`. Sena's full
`fst-batch` run is safe (the C# hybrid answers pathological words in ms — that's the product);
generate all 7,121 words.

F0 also generates, per grammar where the C# tests define them:

- **Negative-examples goldens**: export the near-miss non-word lists from the C# soundness battery
  (`FstSenaBenchmark.Soundness_NegativeExamples` — the file is Sena-named but the battery covers
  each grammar with its own list, e.g. Indonesian's 50-word set; extract each to
  `<grammar>/negatives.txt`) and run `fst-batch` over each; the golden is all-`-` lines. §3.4
  gates against these files.
- **The Sena guarded slice**: the FIRST 60 words of `sena-words.txt` ("guarded" refers to the C#
  oracle-side 5 s/word engine timeout used when these numbers were first measured; the slice
  itself is just those 60 words). Emit it as `sena/slice-60.txt` so both sides run literally the
  same file.
- **The I4 marquee word list**: the 46 non-reduplicated meN- Indonesian words (defined as: the
  engine's own analyses contain `AV` and lack `Cont` — the definition is encoded in the C#
  `BoundaryTapeMarqueeCrossCheckTests`, which F7 ports); emit as `indonesian/men-words.txt` plus
  its `--no-junctions --chain` batch golden.
- **The F1 selector-parity dump**: `fst-restricted` over the first 20 Indonesian corpus words
  (deterministic pick — no hand selection).

**Oracle ref + commit destination**: the oracle is the master commit at which the fst-advisor
subsystem landed — identify it with `git log --oneline master --
src/SIL.Machine.Morphology.HermitCrab/FstTemplateAnalyzer.cs` and record the hash in
`MANIFEST.txt`. The F0 tooling itself is C# work in the `machine` repo: put it on a dedicated
branch `fst-oracle` cut from that ref (it does not need to merge anywhere; it exists to be checked
out and run — record ITS tip hash in the manifest too). Word lists live beside the grammars:
`C:\Users\johnm\Documents\repos\machine\samples\data\{indonesian,sena,amharic}-words.txt`.

### 6.2 Signature format (freeze in F0)

The composite emits engine `WordAnalysis` objects (morpheme list + root index). Frozen signature:
`join("+", morpheme.Id)` in morpheme order + `":"` + `RootMorphemeIndex`, where `morpheme.Id` is
the grammar XML morpheme id (the same stable id space the engine goldens use). Requirements the
format must satisfy (verify in F0 before freezing): ids non-empty and stable for every morpheme
kind that can appear (LexEntry, affix rules, compound non-heads); collision-free within a grammar.
If affix `Morpheme.Id` turns out empty anywhere (the C# `FstReplay.Signature` comment warns
shape-only matching is unsafe for exactly this reason), fall back to the XML rule key — decide
once, in F0, and record it in the manifest.

### 6.3 Cross-checking rule

Every parity gate compares Rust output to the **C# hybrid goldens**. Never compare Rust hybrid to
the Rust engine directly as a substitute (that gate exists separately — soundness, §3.4 — but it
cannot detect coverage drift). Normalization: `awk`-cut the comparison columns, byte-sort, `comm`
— same recipe as the engine port.

## 7. Crate plan

One new crate: **`hc-hybrid`** (name final unless review objects in F1), depending on
`hc-grammar`, `hc-featstruct`, `hc-shape`, `hc-rules`, `hc-parse`. The FST here is NOT
`hc-fst`'s determinized-CSR machine — the hybrid's trie/NFA is unification-arc, multi-analysis,
walked nondeterministically with ε-closure; it gets its own module (`trie.rs`) mirroring
`FstTemplateAnalyzer`'s structures. Module sketch:

```
hc-hybrid/
  src/
    token.rs        // MorphToken, MorphTokenCodec, MorphOp classification
    surface.rs      // SurfacePhonology: Variants, DeletionJunctions, bare-root surfaces
    trie.rs         // trie build: root chains/checkpoints, affixes, templates,
                    //   DerivableToCategory (+compound edge), compound loop, boundary arcs,
                    //   junction skips
    walk.rs         // bare walker + chain walker (ONE walker, length-1 delegation),
                    //   PConfig/PConfigKey, CascadeSymbol, ChainClosure, BeamBudget
    inverse.rs      // InversePhonology substrate (arc kinds incl. ε-input/ε-output/structural-ε)
    compiler_v1.rs  // PhonologyRuleCompiler (v1, bug-for-bug)
    env_nfa.rs      // EnvNfaCompiler
    compiler.rs     // RuleInverseCompiler (tiers, floors, epenthesis, metathesis, caps)
    proposers.rs    // Reduplication/Infix/Composed/Lockstep/Chain/ForwardSynthesis proposers
    composite.rs    // CompositeProposer (order + signature dedup)
    replay.rs       // FstReplay + verify pool + VerifiedFstAnalyzer
    probe.rs        // FstCoverageProbe, ProbeReport, CompareGrammars
    advisor.rs      // GrammarFstAdvisor + report Format()
  tests/            // ported toy-grammar tests (§9), golden-gate integration tests
```

`hc-cli` gains the mirror commands: `hc-rs fst-batch`, `hc-rs fst-candidates`, `hc-rs fst-stats` —
flag-compatible with §6.1 so gate scripts are symmetric.

### 7.0 rustfst evaluation (bounded investigation, during F1)

[`rustfst`](https://github.com/garvys-org/rustfst) is a Rust re-implementation of OpenFst
(weighted FSTs: composition including lazy/delayed composition, determinization, minimization,
shortest-path; `0.9.x-alpha` as of mid-2026). Before writing `trie.rs`/`walk.rs` from scratch,
spend a **time-boxed half-day** in F1 answering, in writing (a short `docs/` note or the F1 commit
message):

1. **Can it be the walker substrate? Expected answer: no, for the parity port.** rustfst arcs
   (`Tr`) are labeled with concrete `u32` symbols; the hybrid's trie is unification-arc
   (FeatureStruct labels matched by `IsUnifiable`), multi-analysis, and deliberately never
   determinized/minimized (feasibility report §5.2) — and the port's bug-for-bug/byte-parity
   discipline (§4.3) leaves no room for an off-the-shelf machine's semantics anyway. Confirm and
   record this rather than assuming it, so the question is answered once.
2. **What is worth mining regardless:** its lazy-composition design (state-pair caching, delayed
   arc expansion) is the same pattern as `ChainClosure`/`CascadeSymbol` and is tuned OpenFst
   practice — read it before optimizing the chain walk; likewise its arc/state storage layouts.
3. **Where it could genuinely plug in later (record as a §12 pointer, do not act):** if the
   post-parity chain-walk optimization adopts feature-quotienting (probe/enumerate over
   equivalence classes the rules distinguish — feasibility §8.3(b)/§10.3), the quotient alphabet
   IS concrete symbols, and per-rule inverse transducers over it become ordinary FSTs — at which
   point rustfst's composition/optimization machinery (and its alpha-status/maintenance risk)
   becomes a real build-vs-buy decision for that subsystem only.

Outcome either way: a recorded decision with reasons, not a standing question.

### 7.1 Prerequisites in existing crates (small, reviewed contract changes)

These are the only edits outside `hc-hybrid`; each is its own commit with its own tests:

- **`hc-parse`: analysis selectors.** `Morpher` (or the analysis entry point) accepts
  `lex_entry_filter` and `rule_filter` predicates threaded to where the C# selectors bite
  (lexical lookup admission; rule cascade admission). Semantics must match `Morpher.cs` exactly —
  find every C# read site of `LexEntrySelector`/`RuleSelector` first and mirror the set. Prefer
  per-call parameters over C#'s mutable instance state (Rust idiom, thread-safe by construction;
  behavioral parity is what's gated, not the mutation style — record this as an approved deviation).
- **`hc-parse`/`hc-rules`: synthesis-for-probing.** `SurfacePhonology` and both rule compilers need
  (a) "run this stratum's/language's synthesis cascade over this shape" and (b) "generate surface
  words for this lex entry" (`Morpher.GenerateWords` analog — bare-root surfaces, and the opt-in
  `ForwardSynthesisProposer`). The engine port's synthesize-confirm path has most of this; expose a
  probing-friendly API without cloning the pipeline.
- **`hc-shape`: deleted-node-aware rendering.** A `render_nodes` that skips deleted nodes
  (`IsDeleted()` analog) matching C# `SurfacePhonology.RenderNodes`; plus whatever
  shape-construction helpers the probe assembly needs (build a probe `Shape` node-by-node from
  known FeatureStructs — the C# I1 lesson: NEVER build probe strings and re-segment; port the
  fixed design, not the buggy first draft).
- **`hc-grammar`:** confirm every object-model surface the advisor/compilers read is loaded
  (quantifier bounds, MPR gates, `Direction`/`ApplicationMode` defaults — remember
  `XmlLanguageLoader` defaults every rule to `Iterative`; anchors; boundary char defs;
  `MaxStemCount`; `DeletionReapplications`). Grep-audit against the C# read sites; extend the
  loader where a field was skipped as engine-unneeded.

## 8. Milestones (commit-gated, in order)

Estimates are focused agent-days; every milestone ends with: full workspace green, gates run and
recorded in the commit message, tier-report + stats battery where applicable, review before the
next milestone.

**F0 — C# golden tooling + format freeze (0.5–1 d, C# side, branch `fst-oracle`).** Build §6.1 on
the landed fst-advisor code; generate goldens for all three grammars: batch chain-off/chain-on/
`--bare`, candidates (composite and `--bare`), stats, restricted-run dump, negatives, the Sena
slice-60 and Indonesian meN- word lists + their goldens, toy-grammar XML fixtures (§9). Gate:
formats frozen + manifest written (oracle ref + tooling-branch hash); Indonesian `fst-batch`
chain-off reproduces the recorded 121/121-fully-covered result from raw goldens alone.

**F1 — prerequisites + scaffold (1–2 d).** §7.1 contract changes; `hc-hybrid` scaffold;
`token.rs` (codec + op classification). Gates: selector-restricted Rust analysis byte-matches the
F0 `fst-restricted` golden (first 20 Indonesian corpus words, deterministic); toy XML fixtures all
load; `MorphTokenCodec` unit tests ported and green; quirk audit (§4.3) done — reviewer signs off
that every C# read site of a §4.3-listed member was visited (grep list attached to the commit
message; the audit's completeness is a review judgment, so make the evidence explicit).

**F2 — SurfacePhonology (1–2 d).** `surface.rs` complete with memoization + capability gates
(`_anyPhonologicalRules`/`_anyDeletionSubrule` analogs). Gate: `Variants`/`DeletionJunctions`/
bare-root dumps byte-identical to `fst-stats` goldens on all three grammars (this is the gate —
mechanical). Informational, not gated: record Amharic's probing build time next to the C# ~112 s
figure; do NOT "fix" the pathology here (§12).

**F3 — trie builder (2–3 d).** `trie.rs` complete: shared root trie + checkpoints, affix arcs
(with junction variants + deletion skips), templates/slots, derivation BFS with the compounding
edge, compound loop, boundary arcs. Gate: `StateCount` equal per grammar (Indonesian 547, Sena
18,871, Amharic vs regenerated golden — trust the golden, not doc numbers); a full structural dump
(canonically renumbered sorted arc list: from-state, label-repr, to-state, token) byte-identical
on ALL THREE grammars (add the dump to `fst-stats` on both sides in this milestone if F0 didn't —
a format extension recorded in the manifest, not a re-freeze; Sena's dump is large but the diff is
mechanical).

**F4 — bare walker + beam (2–3 d).** `walk.rs` bare half: NFA walk, ε-closure (boundary arcs free),
`BeamBudget` with exact debit points, `ToWordAnalyses` (multi-root head choices). Gate: bare-FST
candidate parity per word (Indonesian 121, Sena full corpus) vs the F0 `--bare` candidates golden;
overflow counts equal at default budget; `BeamCapTests` ported (pathological 12-rank chain
bounded, knob test).

**F5 — verify path (1–2 d).** `replay.rs`: pool, `FstReplay::confirm` (selector pinning, compound
extra-roots, signature match), `VerifiedFstAnalyzer`. Gate: **verified bare-FST parity** on
Indonesian + the Sena slice-60 file vs the F0 `--bare` batch goldens; thread-invariance asserted;
soundness assert (every emitted analysis re-confirms) run once on Indonesian.

**F6 — sibling proposers + composite (2–3 d) — DONE, re-scoped 2026-07-11 per an independent Fable
review.** `proposers.rs`: `ReduplicationProposer` (all four C# scan kinds) and `InfixProposer` built
for real. `composite.rs`: `CompositeProposer`'s fixed order (FST → [ForwardSynthesis] → Redup →
Infix → ComposedPhonology → Lockstep, confirmed against `CompositeProposer.cs:32-46,87-104`) and
signature dedup, wired through F5's verify path.

**`ComposedPhonologyProposer`, v1 `compiler_v1.rs`/`LockstepPhonologyProposer`, and
`ForwardSynthesisProposer` were NOT built this milestone — wired as permanently-empty stubs at
their correct order position instead, moved to F7 below.** This is not a silent gap: the review
independently confirmed via the real C# `fst-candidates` golden (labeled by proposer) that on
Indonesian, Sena, and even Amharic (673 words, checked opportunistically though outside F6's own
scope), every candidate in the real C# composite comes from `FstTemplateAnalyzer`
(+`ReduplicationProposer` on Indonesian) alone — Composed/Lockstep never contribute a
distinguishable candidate on any corpus this port currently has. The reasons differ by grammar and
matter for what F7 must still prove: **Sena** is a structural guarantee (zero phonological rules,
both proposers no-op by construction — Sena's pass proves nothing about them specifically).
**Indonesian's Lockstep v1 no-op is also a guarantee**, not a coincidence: every one of its 5
phonological rules is v1-unsupported by an already-audited quirk (quirks 1/2 in
`F1_QUIRK_AUDIT.md` — boundary markers in the environment, α-variables + quantifiers, or an
explicit excluded-MPR-feature reject), so v1's alphabet/arc-building rejects all of them and the
proposer is provably inert. **Indonesian's Composed no-op is empirical-plus-structural, not
guaranteed**: it genuinely runs (5 rules), but every candidate it proposes is deduped against one
FST/Redup already proposes via F2/F3's junction-arc baking — because all 5 rules are
junction/redup-conditioned, exactly the shape junction probing was built to subsume. **A future
grammar with a genuine mid-word (non-junction-conditioned) phonological rule would get candidates
ONLY from Composed/Lockstep/the chain — nothing else in this pipeline can produce them** — so this
is a real, not cosmetic, scope carve-out.

Gate actually met this milestone (verified independently, byte-identical): full composite candidate
AND verified parity, chain-off, Indonesian 121/121 + Sena slice-60 + negatives (Indonesian + Sena,
50 words each) all `-`. **NOT met, moved to F7**: `PhonologyRuleCompilerTests` (impossible to port
with no compiler built) and quirks 1-2's behavioral verification (they describe the unbuilt v1
compiler). 5 new toy tests (redup/infix, hand-authored XML, not C#-`XmlLanguageWriter`-exported —
acceptable for now per the review, but revisit the export/round-trip step in F9 to close that loop
per §9's stated convention) cover what F6 actually built.

**F7 — the chain (3–4 d) — SCOPE EXPANDED 2026-07-11 to also cover what F6 deferred.** In addition
to its original scope below: build `compiler_v1.rs`/`LockstepPhonologyProposer` (bug-for-bug per
quirks 1-2) and `ComposedPhonologyProposer`, port `PhonologyRuleCompilerTests`, and add a **new
required gate**: a toy grammar with a genuine word-internal (non-junction-conditioned) single-segment
phonological rule, generated so its C# `fst-candidates` dump shows real `LockstepPhonologyProposer`/
`ComposedPhonologyProposer` lines (not just FST/Redup) — byte-match this specifically in Rust. This
is the only gate anywhere in the plan that actually forces these two proposers to produce a
non-empty result and be checked; without it, F6's "empty stub happens to match" state could persist
undetected indefinitely (Amharic's own real-composite golden is ALSO FST-only, so Amharic will not
force this either — confirmed, not assumed).

`inverse.rs`, `env_nfa.rs`, `compiler.rs` (tiers + reasons, deletion
floors, epenthesis ε-output, metathesis + 256-combo cap + identity seeding), chain half of
`walk.rs` (state-vector configs, `CascadeSymbol`, `ChainClosure` branches, boundary insertion +
`InsertionsUsed`), `ChainPhonologyProposer`. Gates: tier reports byte-identical (incl. reason
strings) on all three grammars; chain-on verified parity (Indonesian 121/121 byte-identical vs the
chain-on golden); the I4 marquee cross-check reproduced (`--no-junctions --chain` over the F0
`men-words.txt` byte-matches its golden — 46/46); `RuleInverseCompilerTests`/`ChainWalkerTests`/
`ChainDeletionEpenthesisTests`/`BoundaryTape*` ported.

**F8 — probe + advisor (1 d).** `probe.rs` (`ProbeReport` incl. `BeamOverflows`, tier report,
uncovered constructs; `CompareGrammars`), `advisor.rs` (advisories + `Regular` axis + verdict +
`Format()`). Gate: `fst-stats` output byte-identical on all three grammars; the three C# probe
edit-loop tests ported with their concrete assertions (each asserts specific gained/lost coverage
after an affix/phonology/redup edit — mechanical, not "visibly moves").

**F9 — full battery + docs (1–2 d).** Full-corpus runs recorded for the battery: Sena 7,121
verified parity (watchdogged), Amharic per §5.3 with explicit exclusion list; Rust-vs-C# benchmark
table (states, build ms, walk p50/p95 chain-off/on, verify cost/word, overflow counts — the
standing stats battery); `KNOWN_GAPS` equivalent written into the crate docs; this plan's status
block updated. Gate: §3 checklist all green (with Amharic's recorded gating), README for the crate
+ CLI usage.

Total: **~14–21 agent-days** across 10 commits minimum (expect more — one per sub-slice is fine;
never less).

## 9. Test-porting strategy: toy grammars travel as XML

The C# toy tests build grammars **in code**. Rust gets them via the XML round-trip:

- F0 adds a C# generator (`[Explicit]` test or tool command) that constructs every toy grammar
  used by the test files in §2.1 and saves each with `XmlLanguageWriter.Save` (public; the
  dangling-co-occurrence writer fix #450 is already on master) to
  `tests/fixtures/fst-advisor-toys/<TestClass>.<grammar-name>.xml` — committed (small files), NOT
  gitignored, because Rust CI needs them.
- Each Rust test loads the fixture with `hc-grammar::load` and asserts the same expectations as
  the C# original (expected analysis sets, tier + reason, overflow behavior, live baselines).
- Verify the round-trip on the C# side in F0: for each exported toy, load the XML back and re-run
  the original assertions against the loaded grammar (guards against writer lossiness — if a toy
  doesn't survive round-trip, fix the export or mark that test C#-only with a recorded reason).
- Port test-by-test with the milestone that builds the subject (§8 lists which). The LIVE honesty
  baselines ("composite covers X", "v1 misses X") port as-is — they pin exactly the quirks §4.3
  demands.

## 10. Verification commands (the gate scripts)

```bash
# C# oracle (generate once per oracle ref; from the landed fst-advisor build)
dotnet hc.dll fst-batch samples/data/indonesian-hc.xml indonesian-words.txt golden/ind-batch.tsv
dotnet hc.dll fst-batch ... --chain ...           # chain-on goldens
dotnet hc.dll fst-candidates ... ; dotnet hc.dll fst-stats ...

# Rust side (mirror flags)
hc-rs fst-batch <grammar> <words> out.tsv [--chain] [--threads=N]
hc-rs fst-candidates ... ; hc-rs fst-stats ...

# Compare (same recipe as engine goldens; last cols = status+signatures)
awk -F'\t' 'NF>=4 {print $2"\t"$3"\t"$4}' out.tsv | sort > rust.sorted
comm -3 golden.sorted rust.sorted   # must be empty
```

Windows note (from the engine port): script files for the C# tool need Windows-style absolute
paths; run the oracle with `DOTNET_gcServer=0`.

## 11. Risks and their mitigations

- **Engine-parity coupling (biggest).** Verified parity on a word requires the Rust engine to
  agree with C# on that word's *restricted* analysis. Mitigation: per-grammar gating (§5),
  candidate-parity gate (#2) localizes whether a mismatch is proposer-side or engine-side in one
  diff, and restricted analysis is far inside the engine's already-verified envelope (single root,
  ≤ a few rules). Any engine divergence found here is an engine-port bug — file it against the
  engine plan's residual list, don't work around it in `hc-hybrid`.
- **FeatureStruct semantic drift.** The hybrid leans on unification in new call patterns
  (`IsUnifiable` on arc labels, permissive category gates). The engine port's tree-FS ops are
  traced to C# line numbers; F3/F4's structural + candidate dumps are designed to catch drift
  early. When a mismatch appears, go empirical immediately (dump both sides' FS at the arc) — the
  C# history's repeated lesson is that static reasoning stalls where a dump answers in minutes.
- **Signature/id instability (§6.2).** Resolved once in F0 by construction + manifest; if ids
  fail the requirements, the fallback decision is made there, not mid-port.
- **Iteration-order nondeterminism.** §4.2 rules from day one; add a CI-run double-execution check
  (two runs, byte-equal outputs) cheaply in F1.
- **Scope creep via "while we're here" optimization.** Forbidden until F9 is green (§12). The C#
  branch's history shows every speculative improvement that skipped measurement got reverted.
- **Agent-session mortality on long gates.** Full-corpus Sena runs are long; use the engine port's
  watchdog + `STARTED`-sentinel resumable TSVs so a dead agent's partial run resumes rather than
  restarts; background long runs and gate on the artifact.

## 12. After parity: the recorded follow-ups (do NOT do these during the port)

0. **Investigate (don't necessarily fix yet) the bare walker's Sena full-corpus performance.**
   F4 found the bare walker takes multiple minutes (confirmed independently: 9+ CPU-minutes and
   still running when checked) to walk the full 7,121-word Sena corpus in `--release`, well past
   the architecture's whole premise of sub-millisecond-to-millisecond per-word cost. F4 itself
   correctly deferred this per this section's own "no while-we're-here optimization before F9"
   rule and gated on a 60-word slice instead (byte-identical, including both known pathological
   words) -- but before F7/F8 stack more per-word walk logic on top of `walk.rs`, check whether
   this is isolated to a handful of pathological words (matching the feasibility report's own
   Sena pathological-tail framing) or systemic (e.g. non-O(1) arc/closure lookups that would
   compound as more machinery gets added). Flagged by an independent Fable review of F4
   (2026-07-11) as worth investigating sooner rather than purely deferring to post-parity.
1. **Re-measure chain vs v1 in Rust.** The C# 37× chain penalty is allocation/hash-heavy; Rust
   may compress it dramatically. If chain-on comes within the original ≤1.5× p50 budget on the
   battery, the default flip + the C# plan's deferred retirements (junction probing,
   `ComposedPhonologyProposer`, v1) become live decisions — measured, one commit each, exactly as
   I7 specified. This changes observable candidate sets, so it happens strictly AFTER the parity
   scoreboard is archived.
2. **Amharic follow-ups**: feature-quotient probe alphabet / static pre-gate (feasibility §8.3)
   once end-to-end Amharic parity exists to gate against.
3. **Per-grammar beam calibration** (complexity-cap plan) — both sides.
4. Surface the hybrid through `hc-ffi`/the .NET bridge alongside the engine (engine plan M8) if
   the product wants the probe callable from FieldWorks.
5. **Revisit rustfst for the quotiented chain (see §7.0 item 3).** If follow-up #1's chain
   optimization lands feature-quotienting, the rule-inverse transducers become concrete-alphabet
   FSTs and rustfst's lazy composition/optimization algorithms become a candidate substrate for
   that subsystem — evaluate build-vs-buy then, with its alpha status and our determinism rules
   (§4.2) as explicit criteria.
6. **Delivery architecture: word-level cache + frequency-list precache (the embedded-engine
   product shape).** Analyses are a pure function of (grammar, word), so a word→analysis-set cache
   is trivially sound: cache key = word, invalidation = grammar change, and negative results
   (no-parse) cache as well as positive ones — an editor re-checks the same misspelling on every
   repaint, and documents reuse their own vocabulary heavily. The product shape this enables:
   - **Precache the top-N (~10,000) most frequent word forms at package-build time** — just a
     batch `fst-batch` run over the frequency list, shipped as a static map in the language pack.
     Common words then cost a lookup (offline, zero engine invocations); the live engine runs only
     on first encounter with a novel form, so the walk/verify p95 tail becomes a rare one-time
     cost, not an interactive-latency problem. This is also what makes batch document checking
     tractable (a 10k-word document is mostly cache hits).
   - **This dissolves the static-export vs embedded-engine delivery choice**: the shipped precache
     IS the static artifact (instant, offline), and the embedded engine is the completeness
     backstop for the productive tail no finite list can hold.
   - **Zipf caveat (sizing input, not a blocker):** top-10k coverage of running text is an
     English-like fact (~90%+); for the morphologically rich languages this engine exists for
     (agglutinative, Bantu verb morphology, polysynthetic), type/token ratios push coverage down —
     the cache handles the common mass, and the engine's whole value is the tail. Size N per
     language from its corpus, don't assume 10k universally.
   - **Design requirements when built:** version the precache by grammar hash (a field grammar
     update must never serve stale analyses); cache full analysis sets, not accept/reject booleans
     (same lookup cost, and the cache then serves dictionary-lookup/glossing features, not only
     spelling); session LRU above the precache for document-local vocabulary.
   Not port work: nothing here changes parity gates — it sits entirely above the analyzer API.
7. **User lexicon: stem-only additions on deployed devices — delta FST + stem guesser (research
   track).** A deployed analyzer (handset keyboard, office extension) must let a user add a new
   word — specifically a new **stem** — without shipping a new language pack. Scope guard, fixed
   up front: **users add stems, never rules or categories.** The grammar stays linguist-authored;
   user additions are data, not grammar. This is linguistically safe by Zipf's law of
   irregularity: rare words are more regular than common ones, so novel stems overwhelmingly fit
   existing paradigms — the irregulars are already in the shipped lexicon.

   Why this architecture makes it cheap where classic FSTs can't (eager composition smears the
   lexicon across minimized states; patching = full recompile): the lexicon-dependent and
   lexicon-independent build products are already separate. Per-rule phonology inverses never see
   the lexicon; junction arcs are probed per affix × onset *class* (a new stem is classified by
   lookup, not re-probed); the trie is additive; and verify pins roots against the engine's
   lexicon object, so adding the entry engine-side extends the soundness contract to user words
   unchanged — no proposer-side bug can emit a wrong analysis for them.

   **Things to try** (competing mechanisms — spike each, keep the measured winner):
   - *(a) Delta FST (expected winner):* base FST ships immutable (mmap-shared, signed); on each
     user-lexicon change, run the SAME trie builder over user stems only + the full
     affix/template machinery (build is ~1 ms/root extrapolating Sena's 1,463 roots @ ~1.4 s
     Debug C#, so tens of stems ≈ milliseconds on-device); the result joins `CompositeProposer`
     as one more proposer — the composite's union+dedup is the extension seam it was shaped for.
   - *(b) Patch-in-place:* incremental insert into a mutable copy of the base trie (the builder
     already works root-by-root). Expected loser — it forfeits the immutable/shared base and
     complicates memoized structures — but cheap to test against (a).
   - *(c) Hook arcs:* reserved ε-arcs at each attachment site targeting a swappable user-trie
     module — the middle ground; only worth trying if (a)'s composite-level union misses
     candidates that require base-trie interaction (compounds of user stem + base stem are the
     test case; (a) should handle them via the engine-side lexicon during verify, but measure).
   - *(d) The stem guesser (the acquisition path — pairs with the planned typed-word guesser):*
     a **wildcard-stem walk** — run the analysis walk with the root-trie coordinate replaced by
     an open stem hypothesis (Σ⁺ with a min-length floor), so affix/template/junction arcs
     consume what they match and the residue is the stem candidate; each surviving walk yields
     (stem hypothesis, category, rules used). Un-apply phonology on the residue through the
     existing rule-inverse chain (a typed surface form may hide stem alternations — deletion
     restorations already model this; reuse, don't rebuild). A guess is verify-gated like
     everything else: add the hypothesized entry provisionally, run restricted verify on the
     observed word — "if this stem existed with this category, this word is a real parse."
     If the user has typed several inflected forms of the same unknown stem, intersect the
     hypothesis sets (strong disambiguation signal — try it).

   **UI support (concrete, buildable with the engine as-is):** for each candidate
   (stem, category/inflection-class) pair, forward-synthesize a small paradigm sample, and pick a
   **discriminating set** — slots where the candidate categories' outputs actually differ — so
   the user sees a minimal contrast and picks "ah — it looks and acts like *this*." Show each
   category with a high-frequency exemplar ("a verb, like *tulis*"). The word is usable
   immediately on best-guess (marked provisional); the category confirms lazily when the user
   answers. "Dumb code" per the product intent: strip the morphology, present the stem, list the
   categories — the engine does all the linguistics via generation, no new cleverness required.

   **Concrete experiments (post-parity, in order; each is a spike with a mechanical gate):**
   - *R1 — delta-FST parity spike:* inject N synthetic stems into each of the three grammars via
     (a); gate: analysis sets for words formed from those stems are **identical to a monolithic
     rebuild** containing the same stems (the rebuild is the free oracle). Record on-device-scale
     build times for N ∈ {10, 100, 1,000}.
   - *R2 — guesser leave-one-out:* delete K known stems from a grammar's lexicon, feed their
     corpus surface forms to the wildcard walk, measure top-1/top-3 (stem, category) recovery —
     the existing corpora + lexicons are free ground truth. Sena's noun-class system is the hard
     case; Indonesian meN- deletion tests the phonology-unapplication half (the typed word's stem
     is missing its onset — the guesser must restore it).
   - *R3 — discriminating-paradigm generation:* for each category pair in each grammar, compute
     the minimal distinguishing form set; measure how often pairs are distinguishable and with
     how many forms (if most pairs need ≤3 forms, the UI concept works).
   - *R4 — device profile:* memory (mmap base + delta + engine + caches) and battery-relevant
     costs on mobile-representative hardware.

   **Constraints and integration notes:** typed input must segment through the character
   definition table (reject-with-message or grapheme-assist when it doesn't); suppletive
   allomorphs are out of scope for user additions (the regular-stems assumption, above); user
   entries carry a provenance mark so diagnostics/probe reports can separate them from the
   authored lexicon; on stem add, forward-generate its paradigm into the device cache
   (follow-up #6) and version the user-delta cache by user-lexicon hash. Later (note only, not
   planned work): synced user lexicons are pre-validated crowd-sourcing — every accepted form
   carries verify evidence, so proposing stems back to the source FLEx project is a
   review-a-diff workflow, not a data-cleaning one.

## 13. Definition of done (checklist)

- [ ] F0 goldens generated + manifest (oracle ref recorded)
- [ ] §3.1 verified parity: Indonesian chain-off + chain-on, byte-identical, 121/121
- [ ] §3.1 verified parity: Sena full 7,121 (watchdogged), byte-identical
- [ ] §3.1 verified parity: Amharic on the engine-parity subset, exclusion list recorded
- [ ] §3.2 candidate parity per grammar (same scopes)
- [ ] §3.3 structural parity: StateCounts, tier reports (with reasons), advisor reports, overflow counts
- [ ] §3.4 soundness batteries empty on both sides
- [ ] §3.5 toy-test suite ported and green (fixture XMLs committed)
- [ ] §3.6 knob parity pinned by tests
- [ ] Stats battery table (Rust vs C#) recorded in docs
- [ ] Every §4.3 quirk pinned by a test + `// PARITY:` comment
- [ ] Crate docs + CLI usage written; this plan's status block updated with results
