# The hybrid FST analyzer — feasibility report

> **Audience:** technical readers (engineers, reviewers, planners) who do not necessarily know
> HermitCrab, morphological parsing, or finite-state theory. This report explains what the hybrid
> FST on the `fst-advisor` branch is, why an ensemble of cooperating finite-state machines works at
> all, why deliberately proposing *wrong* candidates and checking them later is the thing that makes
> it fast, what is measured and proven today, and what remains open — with the bounds on each open
> item. Companion documents: `FST_FAST_PATH_PLAN.md` (architecture reference),
> `FST_FULL_GRAMMAR_PLAN.md` (execution history, Phases A–I), `HYBRID_FST_RUST_PLAN.md` (the port
> plan this report supports).

## 1. Executive summary

HermitCrab (HC) is SIL's rule-based morphological parser: given a grammar written by a linguist, it
takes a surface word like Indonesian *menulis* and returns its analyses — "the root *tulis* 'write'
with the active-voice prefix *meN-*". It does this by **searching**: un-applying every rule that
might have produced the word, in every order, and checking each hypothesis. That search is
combinatorial. On real grammars it ranges from fast to catastrophic: on the Sena (Mozambique)
grammar, single words have been measured taking 12–90+ seconds *just to prove no parse exists*, one
word out-of-memory-crashed the test host, and a step profile of one pathological word counted ~15
million rule applications, ~98% redundant.

The hybrid FST replaces that search on the fast path with a **propose-and-verify** pipeline:

1. A small ensemble of finite-state machines and scanners (**proposers**) reads the surface word
   and emits a handful of *candidate* analyses in microseconds-to-milliseconds. The proposers are
   deliberately allowed to over-generate — some candidates are wrong.
2. Each candidate is **verified** by the real HC engine, restricted to exactly that candidate's
   root and rules (`FstReplay.Confirm`). Restriction collapses the engine's combinatorial fan-out
   to a single path, so each check costs a few milliseconds — and because it *is* the real engine,
   a confirmed analysis is a real HC analysis with every constraint enforced.

The result is **sound by construction** (no false positives, ever — a wrong candidate is simply
discarded), **known-incomplete on negatives** (a missed parse falls back to the engine or is
reported; it is never silently wrong), and fast: measured 22–72× per-word speedups over the pooled
engine on Sena slices, with the engine's pathological words answered in milliseconds.

**Proven today** (all measured on this branch, test suite 144/144 green):

| | Indonesian | Sena | Amharic |
|---|---|---|---|
| Corpus | 121 words | 7,121 words | 673 words |
| Coverage (exact set parity vs engine) | **121/121, 0 unsound, 0 false positives** | **99.2% of engine-parseable** (200-word random sample; the one gap, `ndikhali`, since closed at 8/8 exact parity; guarded 60-word slice 57/57) | census + rule-tier measured; end-to-end run still queued (§9) |
| FST states | 547 | 18,871 | 3,739 (bare) |
| Build time | ~0.3–0.4 s | ~1.3–1.5 s | 40 ms bare FST; **~112 s with junction probing** (a known cost bug on 417-segment alphabets — §8.3) |
| Verified composite p50 / p95 per word (full corpus, sequential) | 1.4 / 6.0 ms | 31 / 173 ms | — |

The remaining open items are enumerated in §8–§9. Each is *bounded*: either by construction (caps
that cannot hang), by mathematics (the one provably-impossible construct has a documented
workaround used by every FST toolkit ever shipped), or by the soundness contract (every possible
failure mode costs coverage or speed, never a wrong answer).

> **Terminology used throughout** (kept minimal; everything else is defined where it appears):
> a **morpheme** is a minimal meaningful word-part (root, prefix, suffix); **morphotactics** is the
> grammar of how morphemes combine; **strata** are HC's ordered layers of rules. **p50/p95** are
> the 50th/95th latency percentiles per word. An **FST** (finite-state transducer) is an automaton
> that maps strings to strings; **ε-moves** (epsilon) are automaton transitions that consume no
> input. HC segments are **feature structures** (attribute bundles like "voiceless obstruent")
> matched by **unification** (compatibility-merge) rather than symbol equality. **SPE** is the
> classical generative-phonology rule formalism (`φ → ψ / λ _ ρ`); **MPR features** are HC's
> morphophonemic gating flags on rules; an **α-variable** is a rule variable that must take the
> same feature value at two positions (e.g. "nasal agrees in place with the next consonant").
> "**Pooled engine**" means the stock HC search engine run from a pool of engine instances, one
> rented per word, so the comparison baseline is itself parallel-friendly. All performance numbers
> in this report are Debug-build; engine-vs-FST *ratios* compare like against like.

## 2. Background: what the engine does and why it is slow

### 2.1 The task

A morphological analyzer answers: *given this surface word, which morphemes is it made of?* For
Indonesian *menulis*:

- The grammar says the root is *tulis* ("write") and the prefix is *meN-* (active voice), where
  *N* is an underspecified nasal placeholder.
- Two of the grammar's five phonological rules apply during word formation ("synthesis"): the
  nasal assimilates to the place of the following consonant (`meN+tulis → mentulis`), and then the
  voiceless obstruent *t* deletes after the nasal (`mentulis → menulis`).
- Analysis is the inverse problem: from the observed *menulis*, recover `meN- + tulis`. Note that
  the *t* of the root is **not present in the input** — an analyzer must hypothesize that a segment
  was deleted and guess what and where.

HC's grammars express this with lexical entries (roots), morphological rules (affixation,
compounding, reduplication, infixation, templates with slots), and ordered phonological rewrite
rules of the classical form `φ → ψ / λ _ ρ` ("φ becomes ψ between left context λ and right context
ρ"), organized in strata. Rules interact: in the example above, assimilation *feeds* deletion — the
second rule's context exists only because the first rule fired. Analyzing such "opaque" interactions
correctly is the hard part of the problem.

### 2.2 Why search blows up

HC analyzes by running its rule system backwards: try to un-apply every rule whose output shape
matches, recurse on each result, and finally check each surviving hypothesis by re-synthesizing it
forwards. Every un-application is a *guess* (was a segment deleted here? which rule ran last?), and
guesses multiply. Measured consequences on this branch:

- Sena pooled-engine average: ~445–837 ms/word on 60-word slices — vs 12–20 ms/word for the
  verified FST on the same slices. (§7's 31 ms p50 is a different, heavier measurement: the full
  composite with every generator running, over the full 7,121-word corpus including its tail.)
- Proving a **non**-word has no parse is the worst case — the search must exhaust every hypothesis.
  Six words in a 200-word random Sena sample each burned 12–90+ s; one crashed the test host with
  an out-of-memory error.
- A step-level dissection of one pathological Sena word counted ~15M rule-application steps, ~98%
  of them redundant re-derivations of states already explored in a different order.

A grammar-tuning tool cannot sit on top of that: a linguist who edits one rule needs to see the
effect on a whole corpus in seconds, and a service cannot tolerate unbounded per-word cost. That is
the gap the hybrid FST fills.

## 3. The core idea: propose cheaply, verify exactly

### 3.1 The contract

The design commits to one invariant (stated in `FST_FAST_PATH_PLAN.md` §1 and enforced everywhere):

- **Sound on positives.** Every emitted analysis is confirmed by the real engine. No false
  positives, ever.
- **Known-incomplete on negatives.** A missed parse is acceptable and visible (diagnostics name
  what isn't covered and why); a wrong parse is not acceptable.
- **Opt-in.** The hybrid never replaces the engine silently; it is an explicit fast path.

### 3.2 Verification by restricted re-analysis

`FstReplay.Confirm` (src/SIL.Machine.Morphology.HermitCrab/FstReplay.cs) is the linchpin. Given a
candidate — an ordered list of morphemes with a designated root — it runs HC's own
`Morpher.AnalyzeWord` with two selectors pinned:

- `LexEntrySelector`: only this candidate's root (plus a compound's second root, if any);
- `RuleSelector`: only this candidate's morphological rules — templates, strata, and *all*
  phonological rules stay open, because those are obligatory deterministic rewrites, not fan-out
  choices.

Two properties make this both **correct** and **fast**:

1. *Restriction can only remove search paths, never fabricate one.* The selectors are pure
   admission filters: they control which lexical entries and which morphological rules the search
   is allowed to *consider*, and nothing else — the engine still runs its full analysis and
   re-synthesis with every constraint (category gating, morpheme co-occurrence, MPR features,
   allomorph environments, obligatoriness) over whatever the filters admit. A filter cannot cause
   the engine to accept a derivation it would otherwise reject; it can only hide alternatives. So
   "the restricted engine produced this analysis" is exactly equivalent to "this is a valid HC
   analysis". Soundness is inherited from the engine, not re-implemented — and it is also pinned
   empirically: per-word *set-parity* measurements against the unrestricted engine (§7) would
   expose any analysis the restricted run fabricated, and the 50-word negative battery has never
   produced a false positive.
2. *Pinning the root and rules collapses the fan-out.* The combinatorial explosion in §2.2 comes
   from trying every root in a 1,400-entry lexicon against every rule sequence. With one root and
   ~3 rules admitted, the same search finishes in a few milliseconds.

### 3.3 Why over-generation is a feature, not a bug

This contract inverts the usual economics of correctness. Because verify is cheap and exact, a
proposer does **not** need to be right — it needs to be a *superset*: every true analysis must be
among its candidates, and any junk it adds costs one cheap verify call, never a wrong answer. The
codebase calls this the governing principle: **"superset, never silent skip."**

That is why "grab a few candidates, some of them bad, and check later" is precisely what makes the
system fast *and* buildable:

- The proposers can use aggressive approximations (probe one representative segment per class,
  drop a gate they can't express, propose both head choices of a compound) without any correctness
  analysis — verify absorbs the slop.
- The expensive inverse problem ("what did deletion remove?") becomes cheap: propose the plausible
  restorations that the *lexicon* can accept (§5.3) and let verify pick the real ones.
- Every rule compiles at a declared tier — **Exact** (compiled precisely), **Permissive** (some
  gating dropped; more verify traffic), or **Identity-skip** (not compiled; words needing it fall
  to the engine) — and the per-rule tier report tells a grammar author exactly what is costing
  what. A rule the compilers can't handle degrades coverage or speed, visibly; it cannot produce a
  wrong answer.

The measured cost of this tolerance is small: on Indonesian, the verified composite runs at
1.4 ms/word p50 *including* verification of all discarded junk candidates, and the
soundness battery (50 deliberately-wrong near-miss words) yields 0 false positives.

## 4. The proposer ensemble: which machine does which job, and why

"Multiple FSTs working together" is not an accident of history — it is the load-bearing design
decision. Each construct class gets the one mechanism that is *bounded* for it
(`FST_FAST_PATH_PLAN.md` §2):

| Construct class | Mechanism | Why this one is bounded |
|---|---|---|
| Concatenative morphotactics: lexicon, affixes, templates/slots, derivation, compounding | One **shared trie/NFA** built eagerly (`FstTemplateAnalyzer`) | Tries are additive: states ≈ lexicon + affix inventory. They cannot multiply. Sena: 18,871 states for 1,463 root allomorphs + 24 templates. |
| Phonology (rewrite rules: substitution, deletion, epenthesis, metathesis) | **Per-rule inverse transducers, composed lazily at walk time** — never materialized (§5.2) | Build is per-rule, independent of the lexicon. Walk cost = word length × live frontier, beam-capped. |
| Boundary-conditioned phonology at affix junctions (the common case in practice) | **Build-time junction probing** (`SurfacePhonology`): run the real synthesis cascade on affix+neighbor windows once, bake the resulting surface variants (mem-/men-/meng-/meny-) and deletion junctions into the trie as arcs | Bounded by \|affixes\| × alphabet (×alphabet² for a two-neighbor fallback) — hundreds of probes on a normal ~30–40 segment alphabet, independent of lexicon size. The bound is real but alphabet-sensitive: a 417-segment syllabary broke it (measured; §8.3), which is a known cost bug with queued fixes, not a soundness issue. The general chain (below) covers the same shapes and is the principled replacement path. |
| Reduplication and infixation (copying) | **Runtime peels**: scan the surface for a copy/infix, strip it, re-analyze the residual through the FST, wrap with the morpheme (`ReduplicationProposer`, `InfixProposer`) | Unbounded copying is *provably not finite-state* (§5.4) — no FST of any size can do it. The peel is an O(n²) surface scan, verify-gated like everything else. |

The `CompositeProposer` unions all candidate streams, dedups by signature, and feeds one stream to
the verifier (`VerifiedFstAnalyzer`). A grammar that lacks a construct pays nothing for its
proposer (each is inert when it holds no matching rules).

Equally important is what the design **forbids**, each item a measured blowup from this branch's
own history:

- No eager composition of `lexicon ∘ rule₁ ∘ … ∘ ruleₙ` into one automaton (multiplicative state
  growth — see §5.2 for why classic toolkits get away with it and HC cannot).
- No materialized root × affix-permutation surface tables (measured: 5 s build at depth 2, 45 s at
  depth 3 on a 2,283-entry grammar; survives only as an opt-in flag).
- No phonology inversion over the bare surface *before* the morphotactic walk: without morpheme
  boundaries on the tape, boundary-conditioned rules fire everywhere (the recorded `ⁿmeⁿnⁿpuⁿlis`
  garbage). Lockstep composition (§5.3) is the fix.
- No determinize/minimize across unification arcs (it merges paths and destroys multi-analysis
  enumeration).

## 5. Why this works at all — the theory, with references

### 5.1 Rewrite phonology is regular (Kaplan & Kay)

The foundational result: a context-sensitive rewrite rule `φ → ψ / λ _ ρ` with regular φ, ψ, λ, ρ,
applied directionally and not recursively into its own output, denotes a **regular relation** —
representable by a finite-state transducer, *no matter how long the contexts are* (Kaplan & Kay
1994). Regular relations are closed under composition, so an ordered cascade of such rules is one
regular relation, and its **inverse** (surface → underlying, the analysis direction) is also
regular. HC's `RewriteRule` is exactly this SPE-style form.

This is not a research bet. Two-level morphology (Koskenniemi 1983) and the xfst/lexc toolchain
(Beesley & Karttunen 2003), and their open-source successors HFST (Lindén et al. 2011) and foma
(Hulden 2009), have compiled full production morphologies — lexicon composed with phonology — as
finite-state transducers for four decades. "Multiple FSTs working together" is the *textbook*
architecture for this problem; the hybrid's departure from the textbook is only in *how* they are
combined (next section).

### 5.2 Why lazy composition instead of one big compiled FST

Classical toolkits eagerly compose lexicon and rules into one machine and keep it small by
determinizing and minimizing over a **concrete alphabet**. HC cannot do that: its arcs are labeled
with *feature structures* matched by unification (a segment can be "any voiceless obstruent"), and
determinizing/minimizing across unification arcs merges genuinely distinct analysis paths —
destroying the multi-analysis enumeration the product needs. Eager composition without minimization
was tried on this branch and exploded, exactly as theory predicts.

The hybrid's answer is **lazy (on-the-fly) composition**: the composed machine is *never built*.
The walker maintains configurations `(ruleState₁ … ruleStateₙ, trieState, tokens)` and advances all
coordinates in lockstep as it consumes the surface word; only the states actually reachable from
this word are ever instantiated. Lazy composition is itself standard, proven technology — it is how
large-vocabulary speech recognizers compose their transducer cascades at decode time (Mohri,
Pereira & Riley 2002; OpenFst's delayed composition, Allauzen et al. 2007). Its effect here:

- **State explosion is structurally impossible** — there is no product automaton to explode. The
  risk moves to walk-time frontier width, which is capped (§8.1).
- Composition of unification arcs never needs a general algorithm: the walk checks
  `IsUnifiable(pinvOutput, trieArc)` pairwise, per step.

### 5.3 The lexicon-constrains-restoration argument (why deletion doesn't explode)

Inverting deletion means *inserting* hypothesized segments — in principle, anywhere, from the whole
alphabet. The reason this stays tractable is the lockstep product: a restoration ε-arc proposed by
a rule-inverse survives **only if the lexicon trie has a matching arc at that exact position**. The
hypothesis space is pruned by the strongest available constraint — what words actually exist — at
the moment each hypothesis is generated, not after. The branch proved this end-to-end, including
the hardest known shape: a two-rule *counterbleeding opacity* cascade — assimilation feeding
deletion, where the segment that *triggered* the first rule is itself deleted by the second, so
the surface form no longer shows why the first rule fired — is recovered exactly, with
the lexicon admitting exactly one restoration (`LEVER_2.md`, `LeverTwoSpikeTests`). The same
argument bounds boundary-marker insertion on the intermediate tapes (the "boundary tape";
I-numbered labels like the I8 mentioned later are Phase-I milestones from
`FST_FULL_GRAMMAR_PLAN.md`).

### 5.4 The honest mathematical boundary: copying

Unbounded reduplication (copy an arbitrarily long stem: `w → ww`) is **provably not a regular
language** (pumping lemma; Hopcroft & Ullman 1979) — no finite-state machine of any size
represents it. Every FST toolkit ever shipped has the same carve-out (xfst's `compile-replace` is a
two-pass preprocessing trick, not a counterexample — Beesley & Karttunen 2000). The hybrid handles
copying with the runtime peel (§4): detect the copy on the surface, strip it, analyze the residual
with the FST, and let verify confirm the whole. This is a *design-matched-to-mathematics* boundary,
not a gap: the peel is bounded (O(n²) scan, ≤2 applications), verify-gated, and measured — it is
what closed Indonesian's seven `-X-X` reduplicated corpus words, including a suffix stacked outside
the copy (`mengamat-amati`).

### 5.5 Where soundness actually comes from

Note what the theory is — and is not — asked to carry. Kaplan–Kay guarantees an exact inverse
*exists*; the implementation does not depend on constructing it perfectly. The compilers only need
supersets (§3.3), and the verifier supplies exactness. So a compiler bug, an approximation, or an
unmodeled rule interaction costs coverage or verify time — *measurably, visibly* — and can never
produce a wrong analysis. This split (theory for reach, engine for truth) is what makes the system
robust to its own immaturity, and it is why every phase of the work could ship incrementally with
the suite green.

## 6. The flow, end to end

**Build time** (once per grammar; ~0.3 s Indonesian, ~1.3–1.5 s Sena):

1. Build the shared trie from the lexicon: one prefix-shared network over per-segment feature
   structures, entered by every template/derivation site; accepting nodes carry the lex-entry
   tokens (homographs share nodes). Morpheme-boundary markers become real arcs (the "boundary
   tape"). A bounded compound loop (one join state per attachment site) covers two-root compounds.
2. Lay affix arcs, template slots, and derivation chains over it, gated by a category-reachability
   BFS (`DerivableToCategory`, which treats compounding as a category edge).
3. Probe the real synthesis cascade per affix (`SurfacePhonology`): precompile each affix's surface
   variants (assimilation outcomes) and deletion junctions (when the cascade eats the root's first
   segment), wiring "skip the deleted onset" arcs gated to roots whose onset actually matches.
   Memoized and capability-gated (a grammar with no deletion rules skips all of it).
4. Compile each phonological rule to its own small inverse transducer (`RuleInverseCompiler`),
   reporting the tier (Exact/Permissive/Identity-skip) and reason per rule. Deletion gets
   structurally-capped restoration "floors"; epenthesis gets ε-output arcs; metathesis a bounded
   window swap with a 256-combo compile cap.

**Analysis time** (per word; p50 1.4 ms Indonesian, 31 ms Sena):

1. The bare FST walk consumes the surface word segment by segment through the trie (NFA walk with
   ε-closure, beam-capped), emitting candidate token sequences → candidate analyses.
2. Sibling proposers contribute their candidates: the reduplication/infix peels scan the surface;
   the phonology proposers un-apply rules (default: the v1 merged-automaton lockstep proposer;
   opt-in: the general per-rule chain walking all rule-inverses + trie in lockstep).
3. The composite unions and dedups all candidates by signature.
4. Each candidate is verified by restricted re-analysis (§3.2); confirmed analyses are emitted —
   they are genuine engine analyses, carrying the real category and all constraint checks.

Worked example, *menulis*: the trie walk fails to find `menulis` directly (no such root), but the
junction-probed variant arc `men-` (the assimilated surface of `meN-` before an alveolar) plus the
deletion-junction skip arc ("a root beginning with voiceless alveolar *t* may have lost it here")
lead into root `tulis`'s chain minus its first segment. Result: one candidate,
`meN- + tulis` (root index 1). Verify pins `tulis` and the `meN-` rule, re-runs the engine, which
re-synthesizes `meN+tulis → mentulis → menulis` — match — and emits the confirmed analysis. Wrong
candidates arising the same way (e.g. a root in *p* whose junction class doesn't fire for this
word) cost one failed verify each, a few ms.

## 7. What is proven today

All numbers measured on this branch (Debug builds; this machine), with the standing stats battery
(state count, build time, walk p50/p95, coverage, soundness) recorded per milestone in
`FST_FULL_GRAMMAR_PLAN.md`. Two different kinds of claim appear below and are labeled as such:
*by-construction* properties (soundness — §3's argument, which no corpus can falsify but tests
pin) and *empirical* measurements (coverage and speed — true for the corpora measured, extrapolated
nowhere beyond them; §9 lists exactly which measurements are still missing).

**Coverage and soundness (the headline):**

- **Indonesian, full 121-word corpus: 121/121 fully covered** — *exact per-word analysis-set
  parity* with the engine, not just "some parse" — **0 unsound, 0 false positives** (50-word
  negative battery clean). This corpus exercises assimilation + deletion feeding (opaque `meN-`),
  reduplication with separators, a suffix stacked outside a reduplication, and compounding rules
  that must stay silent.
- **Sena (7,121-word corpus): 99.2% of engine-parseable words** on a seeded 200-word random sample
  (per-word isolated oracle processes; the engine itself cannot parse a large fraction of the raw
  list — loanwords, typos, proper nouns — and the FST correctly rejects those in ms). The single
  genuine gap found, the two-root compound `ndikhali` (8 engine analyses), was subsequently closed
  with **8/8 exact set parity**; the "guarded" 60-word slice (guarded = the oracle side runs under
  a per-word engine timeout, since the *engine* is what hangs) stands at 57/57 covered, 0 unsound.
- **Test suite: 144/144 green**, including toy-grammar tests for every mechanism: word-internal
  rules, two-rule feeding chains, long-distance harmony (quantified environment spans),
  deletion/epenthesis inverses with caps, metathesis, boundary-conditioned substitution, compound
  loop bounds, junction two-neighbor probes, separator/suffix-peel reduplication, beam overflow.
- **The general chain** (per-rule inverse transducers in lockstep — the "true FST" path) holds
  Indonesian coverage exactly (121/121, 0 unsound) with junction probing disabled — proving the
  general mechanism *subsumes* the grammar-specific special case (46/46 non-reduplicated meN-
  words). It ships opt-in because it currently costs ~37× walk p50 vs the v1 default (58.6 ms vs
  1.6 ms in the dedicated chain-vs-v1 battery; the headline table's 1.4 ms comes from the earlier
  full-corpus probe run — same configuration, different measurement session) — a performance gap,
  not a correctness gap, and the measured basis of the default choice.

**Performance:**

| Measure | Indonesian | Sena |
|---|---|---|
| FST states (post-boundary-tape) | 547 | 18,871 |
| Build (grammar → ready analyzer) | ~0.3–0.4 s | ~1.3–1.5 s (was 9.3 s before the Phase-H fix) |
| Verified composite p50 / p95 (full corpus, sequential) | 1.4 / 6.0 ms | 31 / 173 ms |
| vs pooled engine (60-word slices, 16 threads) | — | 22–72× faster/word |
| Engine-pathological words (12–90 s+ each, one OOM) | — | answered in ms |

**Rule-compilation tier reports** (the per-rule honesty instrument, pinned by a standing test):
Indonesian `Exact=2, Permissive=3, IdentitySkip=0`; Amharic `Exact=2, Permissive=4, IdentitySkip=1`
(6 of 7 rules compile; the holdout is an α-variable boundary CV merger, a documented residual);
Sena has zero phonological rules (confirmed no-op).

**Third-grammar generalization evidence:** Amharic — a typologically different case (Semitic,
417-segment syllabary alphabet, templatic-leaning, 3 infixation escapes) — was added late and
immediately exercised the machinery: its census, tier report, and build-cost attribution all
produced actionable, correct diagnoses (§8.3), which is exactly the advisor behavior the design
promises on an unseen grammar.

## 8. Remaining issues, and how each is bounded

Every open item falls into one of three bounded categories: *capped by construction* (cannot hang
or grow unboundedly), *visible-by-design* (degrades to a counted, named diagnostic), or *carved out
by mathematics* (with the standard industry workaround in place).

### 8.1 Walk-time blowup → the beam cap (capped by construction)

A pathological grammar/word pair could explode the lazy walk's frontier. One per-word `BeamBudget`
(default 1,000,000 work units, empirically calibrated at three points on the largest grammar;
knob available) is debited on both the frontier axis and the within-symbol enumeration axis,
latches on overflow, and drops the word to "unparsed, counted" (`BeamOverflowCount`) — never a
throw, never a hang. Measured: an engineered 12-rank × 8-branch pathological chain falls out in
~22 ms; on real corpora the default stops exactly the 2 known pathological-tail Sena words and
clips nothing healthy. Per-grammar calibration is delegated to the complexity-cap plan.

### 8.2 Deletion/insertion hypothesis growth → structural caps (capped by construction)

Deletion restoration is bounded by automaton "floors" (cap+1 copies, restorations strictly ascend,
top floor has none — the walker *cannot* loop), defaulting to the engine's own
`DeletionReapplications + 1`. Boundary insertion is bounded by a per-word cap baked into the config
key. Metathesis compilation is bounded by a 256-combination cap with an honest tier downgrade.
Known narrowing: one engine "round" restores multiple sites at once while the chain counts events,
so the chain's default is narrower on multi-site words — under-coverage (falls to engine), pinned
by a test, never wrong.

### 8.3 Build-time probing cost on huge alphabets (visible-by-design; fix paths queued)

Amharic's 417-segment syllabary broke `SurfacePhonology`'s "alphabet², dozens²" design assumption:
the morpher-based build measured ~112 s, almost all of it `DeletionJunctions` probes that found
*zero* junctions (the pure FST build is 40 ms). This is a cost bug, not a correctness bug, with
three queued mitigations, in order of principle: (a) the chain path makes junction probing
redundant (its retirement's *coverage* half is already proven — §7 — but actually retiring it is
blocked on the chain's walk speed, §8.4, so both mechanisms stay for now); (b) feature-quotient the probe
alphabet (probe one representative per class the rules can distinguish — restores the design
assumption; syllabary segments mostly differ in features no rule mentions); (c) a static pre-gate
(skip probing affixes no deletion rule can touch — Amharic's measured 0-junction result means this
alone eliminates ~all probes). Bounded meanwhile by the complexity-cap plan's instruments.

### 8.4 Chain walk performance (visible-by-design; the default-flip blocker)

The general chain is ~37× slower than the v1 merged automaton at p50 on Indonesian (5 per-segment
rule cascades vs 1; +43% allocations/word profiled and recorded). This blocks making the
theoretically-cleaner path the default, and blocks retiring the v1/junction-probing mechanisms —
all retirement decisions were made by measurement and all landed "keep". It is a pure optimization
target with the profile already in hand (config-key hashing, per-arc array clones), and the Rust
port is expected to change this calculus materially (`HYBRID_FST_RUST_PLAN.md`).

### 8.5 Named residuals (visible-by-design, each pinned in code or KNOWN_GAPS)

- **Self-feeding iterative rules** may under-cover via the chain (one pass models one sweep);
  detection was researched and deliberately dropped — the honest criterion would be vacuous on HC
  grammars in hand. Falls to engine; documented on `Compile`.
- **α-variable per-binding enumeration** not implemented (one representative probed per class →
  Permissive tier). Amharic's one IdentitySkip rule is the motivating case.
- **Clitics and process/simulfix morphs** (`MorphOp.Clitic`/`Process`) have no proposer — those
  words fall to the engine entirely. No grammar in hand uses them (I8 backlog, spec-on-demand).
- **MPR features / allomorph environments / stem names are not build-time gated** — a precision
  gap (more candidates verified-and-rejected than strictly necessary), explicitly not a soundness
  gap.
- **Compounding is bounded at 2 roots** (the loop's bound; lift to `MaxStemCount` if a grammar
  needs 3 — with the recorded caveat that lifting it via a genuine cycle in the trie would create
  a walk that can loop while accumulating tokens, defeating the dedup that makes the walker
  terminate; today's construction is a DAG so this cannot happen, but a future lift must add an
  explicit defense).

### 8.6 The mathematical carve-out (bounded by theory)

Unbounded copying stays with the peel (§5.4). This is permanent and shared with every finite-state
system in existence; it is listed here so no reader mistakes it for an implementation gap.

## 9. What still needs to be verified

In order of value:

1. **Amharic end-to-end.** The census, tier report, and build diagnostics exist; a full
   propose-and-verify coverage run against its 673-word corpus does not (blocked so far on the
   probing cost of §8.3 and on oracle hygiene — the engine itself blows up on Amharic analysis,
   which is precisely why the fast path matters there). The apparent circularity — "the reference
   is the thing that hangs" — is resolved the same way it was for Sena: *soundness* needs no
   corpus oracle at all (verify runs restricted analysis, which is tractable even where the
   unrestricted search is not), and *coverage* is measured word-by-word in isolated, watchdogged
   oracle processes, with words the engine cannot finish reported as an explicit exclusion list
   rather than folded into the denominator. This is the strongest single validation remaining: a
   third typology, hybrid tier, real infixation.
2. **Full-corpus Sena oracle parity.** Coverage is measured on a seeded 200-word sample + guarded
   slices; the full 7,121-word oracle comparison needs the engine-side watchdog/heap-limit harness
   (the engine, not the FST, is what hangs).
3. **Chain-walk optimization** to close the 37× gap and revisit the default + retirements
   (§8.4) — the recorded allocation profile is the starting point; the Rust port is the natural
   venue.
4. **Per-grammar beam calibration** (complexity-cap plan) — replace the one-size default with a
   percentile-calibrated per-grammar budget.
5. **I8 constructs** (clitics, process/simulfix) — spec when a grammar needs them.

None of these gate soundness; all are coverage, cost, or generality work on top of a contract that
already cannot emit a wrong answer.

## 10. Path to 100% fast-path coverage — plan additions

§8–§9 describe the system as architected: sound, visibly incomplete, engine fallback as the safety
valve. A stronger goal — **100% of engine-parseable words answered by the fast path alone, on any
HC grammar** — does not require overturning any of that architecture, but it does require promoting
completeness from a *measured aspiration* to an *engineered property*. Nothing in §8's list is a
categorical blocker (the only mathematical carve-out, unbounded copying, is already handled outside
the automaton by the peel). What follows is what the plan must add, in priority order. Items 10.1
and 10.8 are new instruments the plan previously lacked; the rest re-scope existing residuals whose
"on demand" or "hygiene" status is incompatible with the 100% goal.

### 10.1 A completeness instrument: the forward-generation oracle (the biggest gap)

Soundness is by-construction; completeness ("superset, never silent skip") is per-compiler
*discipline*, verified only by corpus oracles — and the oracle is engine **analysis**, which is
exactly the thing that hangs (§9's watchdog contortions exist because of this). The missing
instrument is cheap and closes the loop: **generate words forward**. Engine *synthesis* is
tractable and deterministic — enumerate root × rule-sequence combinations to a bounded depth,
synthesize each to its surface form with the real engine, then assert the fast path recovers that
exact analysis. This yields a completeness oracle with no pathological search anywhere in the loop,
it is fuzzable per construct and per rule tier, and it directly catches the one failure mode the
contract admits it cannot see: a compiler that silently under-generates *without* downgrading its
tier (a proper-subset bug would pass every soundness test and every tier report, and only a
coverage oracle can expose it). This should run as a standing generative battery per grammar, the
same way the tier report is pinned today.

### 10.2 Proactive proposers for every uncovered construct

The I8 "spec when a grammar needs them" posture is the gap: any FLEx/FieldWorks grammar in the wild
can use these, so 100% requires building them ahead of demand, not behind it:

- **Clitics** (`MorphOp.Clitic`) — concatenative, trie-shaped; extend `FstTemplateAnalyzer` arcs.
- **Process/simulfix** (`MorphOp.Process`, `ModifyFromInput`) — bounded rewrites; compile like
  affix allomorphs with a modification transducer.
- **Templatic multi-slot infixation** (the Phase-4.2 deliberate residual).
- **Circumfixes without the forward-synthesis opt-in** — the Phase-4 construct sweep records that
  `MorphOp.CircumfixPrefix/CircumfixSuffix` are covered *only* when `forwardSynthesis` is on;
  otherwise those words fall to the engine. §8.5 does not currently list this; it is a residual.

### 10.3 α-variable per-binding enumeration, feature-quotiented

The one IdentitySkip tier (Amharic's CV merger) needs per-binding enumeration. It is finite over
HC's feature system, but on a 417-segment alphabet it must reuse §8.3's fix (b) — enumerate over
feature-equivalence classes the rule can actually distinguish, not raw segments — or the
combinatorics recreate the probing blowup in a new place.

### 10.4 Self-feeding iterative rules: iterate to the engine's own bound

Static detection was researched and rightly dropped (§8.5) — but that was the right answer for the
fallback architecture, not for 100% fast path. The fix is dynamic, not diagnostic: iterate the
rule-inverse to a fixpoint capped at the engine's own reapplication limit, mirroring what the
engine actually does rather than trying to classify the rule.

### 10.5 Derive every cap from the grammar, not from a constant

Each fixed cap is a place where a healthy word can silently become "uncovered":

- **Deletion floors**: the chain counts restoration *events* while an engine round restores
  multiple *sites* (§8.2's pinned narrowing) — count rounds the way the engine does.
- **Reduplication peel** ≤2 applications: derive the bound from the grammar's actual reduplication
  rule structure.
- **Compound roots** bounded at 2: lift to `MaxStemCount` *with* the §8.5-documented cycle defense
  (token-accumulation dedup), which becomes mandatory the moment the trie gains a genuine cycle.

### 10.6 Beam overflow must stop being a coverage sink

Latch-and-drop (§8.1) is correct triage for a fallback architecture; under a 100% goal an overflow
is a word *lost*. Overflow should trigger bounded escalation inside the fast path — iterative
deepening of the budget, and/or re-walking with proposers restricted per construct class (frontier
blowups are usually attributable to one mechanism). Per-grammar calibration (§9 item 4) makes
clipping rare; only an escalation path makes it recoverable.

### 10.7 A verify-side budget and termination story

§3.2's speed argument (fan-out collapses with one root pinned) is empirical, not proven:
phonological un-application inside the restricted search is still a search, and nothing bounds it.
Under fast-path-only, a hanging verify is a hanging system. Add a verify-side work budget with the
same latch-and-report semantics as the beam cap, plus a per-construct-class measurement battery (or
argument) for why restricted analysis stays bounded.

### 10.8 The chain becomes the default — its 37× gap is on the critical path, not hygiene

The v1 merged automaton and junction probing are grammar-shape-specific mechanisms: junction
probing already broke on Amharic's alphabet (§8.3), and v1's gaps are only "covered by the chain
being wireable where they bite." A universal claim cannot rest on hand-picking mechanisms per
grammar. That reframes §8.4: chain-walk optimization (and the Rust port as its venue) is the
coverage-generality prerequisite for retiring the special cases, not a performance nicety.

### 10.9 Build-time gating graduates from precision work to requirement

MPR features, allomorph environments, and stem names un-gated at build time (§8.5) are harmless on
the grammars in hand — but a grammar with heavy homography and open gating multiplies candidates
until verify traffic dominates, and fast-path-only removes the "fall back if slow" valve. The
gating must land before the 100% claim, because throughput cliffs are coverage cliffs once there is
no fallback.

### 10.10 Define "done": the construct-completeness matrix

The plan currently has no criterion for fast-path completeness. Add one, enforced as a standing
test the way the tier report already is: **every HC construct in the Phase-4 sweep table has a
proposer path at Exact or Permissive tier; zero IdentitySkip rules; zero uncovered-op words in the
grammar census; the forward-generation battery (10.1) green.** Corollary: a fast-path-only "no
parse" answer is only *authoritative* once this matrix holds — today it is trustworthy only modulo
the superset discipline, and the plan should state explicitly when that flips.

### 10.11 Sequencing

Amharic end-to-end stays first (§9 item 1) — it is the only cheap falsifier for the generalization
claims, and it is the motivating case for 10.3, 10.8, and the quotienting in both. Then 10.1 (the
oracle instrument, since every subsequent item is validated with it), then 10.8 (chain default,
unblocking retirements), with 10.2–10.7 and 10.9 landing behind the instrument, and 10.10 as the
exit criterion.

## 11. References

- Kaplan, R. M. & Kay, M. (1994). Regular Models of Phonological Rule Systems. *Computational
  Linguistics* 20(3), 331–378. — rewrite-rule cascades are regular relations; the theoretical
  license for the whole approach.
- Koskenniemi, K. (1983). *Two-Level Morphology: A General Computational Model for Word-Form
  Recognition and Production.* University of Helsinki. — the first industrial finite-state
  morphology.
- Beesley, K. R. & Karttunen, L. (2003). *Finite State Morphology.* CSLI Publications. — the
  xfst/lexc toolchain; the textbook for lexicon∘phonology FST compilation. Beesley & Karttunen
  (2000), Finite-State Non-Concatenative Morphotactics (SIGPHON), for the `compile-replace`
  reduplication workaround.
- Hulden, M. (2009). Foma: a finite-state compiler and library. *EACL demos.* — open-source xfst
  successor.
- Lindén, K., Silfverberg, M., Axelson, E., Hardwick, S. & Pirinen, T. (2011). HFST — Framework for
  Compiling and Applying Morphologies. *SFCM.* — open-source FST morphology framework.
- Mohri, M., Pereira, F. & Riley, M. (2002). Weighted finite-state transducers in speech
  recognition. *Computer Speech & Language* 16(1). Allauzen, C., Riley, M., Schalkwyk, J., Skut, W.
  & Mohri, M. (2007). OpenFst. *CIAA.* — lazy/delayed composition as standard, production-proven
  practice.
- Hopcroft, J. E. & Ullman, J. D. (1979). *Introduction to Automata Theory, Languages, and
  Computation.* Addison-Wesley. — pumping lemma; `ww` (unbounded copy) is not regular.
- HermitCrab: SIL International's rule-based morphological parser in the SPE tradition (this
  repository, `SIL.Machine.Morphology.HermitCrab`), used by FieldWorks Language Explorer.
