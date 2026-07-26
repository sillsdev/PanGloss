# Lexical/delete-distance candidate generation: SymSpell, the Rust `fst` crate, and Levenshtein automata

Research note evaluating Phase 1 of `docs/spell-checking-plan.md` ("Error Transducer FST — Structural
Deletion Layer"), which proposes precomputing, for every dictionary entry, "all unique strings formed
by deleting up to 2 characters ($\frac{n(n-1)}{2}$)" and compiling the result into an `fst`-crate
`ErrorTransducer`. The lead has challenged this specifically on the combinatorics and on whether `fst`
is the right tool. This note grounds that challenge in the primary sources for SymSpell, the `fst`
crate itself, Levenshtein automata (Schulz & Mihov 2002), and the hfst-ospell/divvunspell family, then
asks the harder question the plan does not: PanGloss does not check membership in a wordlist, it
checks whether a string is accepted by a *generative, propose→confirm morphological parser*. Sibling
reports cover phonological substitution cost (`02-phonological-distance.md`), n-gram/factored ranking
(`04-ngram-factored.md`), and cross-cutting gaps including detection-vs-correction and Unicode
normalization (`05-gaps-and-transformers.md`) — this note does not re-cover those.

Labeling convention, matching `02-phonological-distance.md`: **measured** = a number I obtained by
reading a primary source or deriving/cross-checking arithmetic myself; **asserted** = a primary source
states it without showing the derivation or benchmark; **secondary-summary** = I could not open the
primary text (PDF extraction failed) and am relying on an abstract, README, or search-engine synthesis;
**my-synthesis** = a conclusion I am drawing by combining sourced facts, flagged as such.

---

## 1. SymSpell / delete-only model: the real combinatorics, and why it works for English

### 1.1 The algorithm, from the primary source

Read directly from the reference implementation's own README and source
([github.com/wolfgarbe/SymSpell](https://github.com/wolfgarbe/SymSpell),
[SymSpell.cs](https://github.com/wolfgarbe/SymSpell/blob/master/SymSpell/SymSpell.cs)):

> "Opposite to other algorithms only deletes are required, no transposes + replaces + inserts.
> Transposes + replaces + inserts of the input term are transformed into deletes of the dictionary
> term."

This is the "symmetric delete" trick: instead of generating every insertion/substitution/transposition
variant of a misspelled query (expensive, alphabet-dependent — the README's own justification:
"Replaces and inserts are expensive and language dependent: e.g. Chinese has 70,000 Unicode Han
characters!"), the *dictionary* is precomputed once as deletion-only variants, and at query time the
*query* is also reduced to its own deletion-only variants. Any true edit (delete, insert, substitute,
or adjacent transpose) between a dictionary word and a query word of edit distance ≤ k must, by a
symmetry argument, produce at least one exact string match between the two delete-sets when both are
expanded to distance ⌈k/2⌉ or less on each side (in practice SymSpell expands both sides to the full k,
which is what makes the exact-match lookup work without needing the "meet in the middle" halving). The
payoff is that candidate generation becomes an **exact-string equality** problem — the cheapest lookup
primitive there is — instead of an edit-distance computation at query time.

### 1.2 Correcting the combinatorics (measured, cross-checked against the primary source's own example)

The plan's `n(n-1)/2` is `C(n,2)` alone — it silently drops (a) the zero-deletion case (the word itself
still needs an index entry) and (b) every single-character deletion. The correct count of **distinct
deletion operations** performed when precomputing "delete up to k characters" for a word of length n
(counting positions, i.e. multiset of characters is irrelevant to the *operation* count even though
some operations can coincide on the same resulting string when characters repeat) is:

- number of ways to delete exactly *i* of the *n* positions: `C(n, i)`
- total *deletes stored* (excluding the identity/0-deletion case, which SymSpell stores as the word
  itself, not as a "delete"), for max distance k: `sum_{i=1}^{k} C(n, i)`
- total *distinct index keys* including the original word, for max distance k: `1 + sum_{i=1}^{k} C(n, i)`
  — for k=2 this is exactly **`1 + n + C(n,2)`**, which is what the task brief specified and what the
  plan should say instead of `n(n-1)/2`.

This is not just my derivation in isolation — it is **cross-checked against the primary source's own
worked example** and the arithmetic matches exactly: the SymSpell README states "An average 5 letter
word has about 3 million possible spelling errors within a maximum edit distance of 3, but SymSpell
needs to generate only **25 deletes**" ([wolfgarbe/SymSpell
README](https://github.com/wolfgarbe/SymSpell/blob/master/README.md)). Computing
`sum_{i=1}^{3} C(5,i) = C(5,1)+C(5,2)+C(5,3) = 5+10+10 = 25` reproduces that number exactly. I searched
directly for a source stating the general formula symbolically (queries against the SymSpell repo,
README, and general web search) and **found none** — SymSpell's own documentation only ever gives the
worked numeric example, never the closed form. The `1 + n + C(n,2)` formula in this report should be
read as **derived by me, verified against the primary source's own arithmetic**, not as a quote from
SymSpell's own text.

### 1.3 True time complexity

- **Build time**: for a dictionary of D entries with average length n̄, generating the delete sets
  costs `O(D · sum_{i=1}^{k} C(n̄,i))` string-construction operations — polynomial in n̄ for fixed k
  (k=2 means this is `O(D · n̄²)`), not exponential; the plan's own "$O(n^2)$" label for this step is
  directionally correct once the constant is fixed (it's `Θ(n^2)` for k=2, dominated by the `C(n,2)`
  term).
- **Lookup time**: symmetric — generate the query's own `1 + m + C(m,2)` delete variants (m = query
  length) and probe each against the prebuilt index. Each probe is O(1) amortized against a hash map
  (the actual data structure SymSpell's reference C# implementation uses — confirmed by reading
  `SymSpell.cs` directly: `CreateDictionaryEntry` populates `staging.Add(GetStringHash(delete), key)`,
  i.e. a hashtable keyed by a string hash, not an `fst`). Total lookup cost is
  `O(m² )` probes at `O(1)` each (amortized), which is the source of SymSpell's headline numbers:
  **asserted** (not independently re-benchmarked by me) "0.033 milliseconds per word (edit distance 2)"
  and "1,870× faster than BK-tree; 1,000,000 times faster than Norvig's algorithm"
  ([wolfgarbe/SymSpell README](https://github.com/wolfgarbe/SymSpell/blob/master/README.md);
  [Wolf Garbe, "1000x Faster Spelling Correction algorithm (2012)," Medium](https://wolfgarbe.medium.com/1000x-faster-spelling-correction-algorithm-2012-8701fcd87a5f)).
  These are the algorithm author's own reported numbers, not third-party-replicated — treat the
  specific multipliers ("1,870×", "1,000,000×") as **asserted**, though the *mechanism* (hash-exact-match
  beats edit-distance-computation-at-query-time) is sound and not in question.

### 1.4 The real reason it is fast for English: small, bounded n, small alphabet, fixed small k

Three properties of English spelling-correction workloads all point the same direction, and all three
degrade for PanGloss's target languages:

1. **n is small.** English wordforms are typically 3-10 characters. `C(n,2)` grows quadratically, so
   doubling n roughly quadruples the per-word index cost — this is fine at n=8 (`C(8,2)=28`) and still
   fine at n=15 (`C(15,2)=105`), but a HermitCrab-style agglutinative wordform of **20-40 codepoints**
   (a realistic length once several inflectional/derivational affixes stack; PanGloss's own stress-test
   grammars specifically target this scale — per user memory, "50k-entry target, dozens of stress
   grammars" and "every HC construct," not reference-grammar-scale toy examples) pushes `C(n,2)` to
   `C(30,2)=435` or `C(40,2)=780` **per word form**, before multiplying by however many surface forms a
   single lexical entry can synthesize. This is **my-synthesis**: I did not find a published number for
   "delete-table size for 30-40-character wordforms" (searched directly for "agglutinative language
   spell checker edit distance long words SymSpell performance degrade" and found no source addressing
   this beyond generic README claims of "language independence" — flagged below as a genuine gap), but
   the arithmetic is not in dispute: quadratic growth in n compounds with however many wordforms per
   lemma a rich morphology produces, and 40² is 64x the constant-factor cost of 5² even though both are
   "just" O(n²).
2. **k is small and fixed.** SymSpell's own worked numbers stop at k=3; the delete-only trick's value
   proposition (cheap exact-match lookup) directly assumes typos are close, bounded-distance corruptions
   of a correct form. That assumption tracks poorly with the actual error surface for agglutinative
   morphology: a single wrong choice of allomorph, vowel-harmony violation, or misapplied assimilation
   rule can change a wordform's *entire suffix string* while the stem is untouched — a
   linguistically "small" (one wrong morphophonological decision) error that is nonetheless an
   edit-distance-many-more-than-2 change in surface characters. Fixed small k is an assumption about the
   *shape* of errors, not a property SymSpell derives from anything — and it's an assumption calibrated
   to fat-finger/OCR-style character-level noise, which is a different error model than
   "wrong-morphophonological-choice" noise. This point is **my-synthesis**, but it follows directly from
   how the delete-only trick is defined (§1.1) — it fundamentally cannot detect a candidate whose true
   edit distance from the correct form exceeds k, no matter how "linguistically close" that error
   actually is.
3. **Small, roughly uniform alphabet.** SymSpell's own stated rationale for avoiding
   insert/replace/transpose generation is explicitly alphabet-size-driven ("Chinese has 70,000 Unicode
   Han characters"); PanGloss's per-grammar `CharDefTable` alphabets are typically small (segmental
   inventories, not logographic), so this specific concern doesn't bite PanGloss the way it would for
   Chinese — but see §2.3 below for a *different*, PanGloss-specific alphabet problem (multigraphs and
   combining marks) that SymSpell's English-oriented design doesn't have to think about at all.

**Bottom line for §1**: the delete-only trick's speed is a genuine, well-understood, exact-match-lookup
mechanism — nothing about it is broken as an algorithm. But its two implicit assumptions (short words,
small fixed edit distance captures the real error distribution) are calibrated to English/European
short-wordform spelling correction, and PanGloss's target profile (20-40-codepoint agglutinative
wordforms, morphophonologically-driven errors that can span an entire suffix) sits well outside the
regime SymSpell was designed and measured for. This is the strongest, most concrete form of the lead's
challenge, and the primary sources support it directly.

---

## 2. The Rust `fst` crate: transducer-or-not, native Levenshtein reality, and a Unicode correctness problem

### 2.1 Transducer, not just an FSA (measured, read directly from crate docs)

`fst` ([docs.rs/fst](https://docs.rs/fst/latest/fst/), [github.com/BurntSushi/fst](https://github.com/BurntSushi/fst))
ships both: a `Set` (a pure finite-state acceptor over byte strings) and a `Map` (a genuine finite state
**transducer** — a deterministic acyclic FST that "emits a value associated with the specific sequence
of inputs given to the machine"), with `Map`'s values constrained to `u64`. This is the right primitive
for the plan's "map deleted variants to the original dictionary word index" (Step 1.2) — `fst::Map<K,
u64>` is exactly a compact deletion→root-index map, provided the `u64` value budget is enough (it is,
for any reasonable lexicon: a `u64` easily encodes a `LexEntryId`/root-index).

### 2.2 Does it ship Levenshtein-automaton fuzzy lookup natively — yes, but the crate's own docs call it "proof of concept"

The `automaton` module ships `fst::automaton::Levenshtein`
([docs.rs/fst/latest/fst/automaton/struct.Levenshtein.html](https://docs.rs/fst/latest/fst/automaton/struct.Levenshtein.html)),
gated behind the `levenshtein` cargo feature (confirmed by reading the crate's own
`Cargo.toml`: `levenshtein = ["utf8-ranges"]`, at
[github.com/BurntSushi/fst/blob/master/Cargo.toml](https://github.com/BurntSushi/fst/blob/master/Cargo.toml),
crate version pinned there at `0.4.7`, license `"Unlicense/MIT"` — MIT-compatible with PanGloss's own
`license = "MIT"`, `rust/Cargo.toml:29`). Two things the crate's own documentation states directly,
**measured** (i.e., quoted from the primary source, not inferred):

- "Levenshtein automatons use a lot of memory" and "construction of Levenshtein automatons should be
  consider 'proof of concept' quality" — this is the crate's own maintainer's caveat, not a third-party
  criticism.
- Construction "may consume enormous amounts of memory (tens of MB before a hard-coded limit will cause
  an error to be returned)" — i.e. there is a hard-coded state-count ceiling, and hitting it is a runtime
  error condition the caller must handle, not a silent slow-path.

This caveat is corroborated by a since-closed upstream issue reporting a concrete, measured performance
gap: **[BurntSushi/fst#47](https://github.com/BurntSushi/fst/issues/47)** measured the current (at time
of filing) from-scratch DFA construction for a Levenshtein automaton of distance 2 over the word
"Levenshtein" at **2.5ms**, versus **0.09ms** with a precomputed-table approach based directly on Schulz
& Mihov's algorithm (§3 below) — a **~25× measured gap**, plus a secondary effect on traversal speed
itself ("72 ns" vs "8 ns" per step in one reported case) because the from-scratch DFA is also larger and
causes more cache misses. The issue proposes adopting the approach later packaged as the
`levenshtein-automata` crate (used by `tantivy-search`,
[docs.rs/levenshtein-automata](https://docs.rs/levenshtein-automata/latest/levenshtein_automata/), MIT
licensed, explicitly implementing "the 'Fast String Correction with Levenshtein-Automata (2002)' approach
by Schulz and Mihov"). **Important finding, verified by reading `fst`'s own `Cargo.toml` directly**: the
`levenshtein` feature's only dependency is `utf8-ranges` — there is **no** dependency on
`levenshtein-automata` in the current `fst` crate. That means issue #47's proposed fix was **not**
merged into `fst` itself; `fst::automaton::Levenshtein` still builds its automaton from scratch per
query, and the crate's own "proof of concept" warning should be read as still current, not historical.
If PanGloss wants Schulz-Mihov-quality Levenshtein automaton performance in Rust, the `levenshtein-automata`
crate (or `tantivy`'s fork of it) is the better-evidenced choice, used either standalone or via `fst`'s
documented support for external automata implementing its `Automaton` trait (the crate docs note
`regex-automata` DFAs can already be plugged in this way) — not `fst`'s own bundled `Levenshtein` type.

### 2.3 Byte-orientation vs. Unicode scalar vs. orthographic-unit: a real correctness problem, not a hypothetical one

`fst::Map`/`Set` themselves operate over raw bytes (any `AsRef<[u8]>` key) — no Unicode awareness at
that layer at all, which is fine and expected for a generic transducer library. But
`fst::automaton::Levenshtein` specifically **does** interpret its query in Unicode terms: its own docs
state edit distance is computed "based on Unicode characters," i.e. "each character is a single Unicode
scalar value" — confirmed directly from
[docs.rs/fst/latest/fst/automaton/struct.Levenshtein.html](https://docs.rs/fst/latest/fst/automaton/struct.Levenshtein.html).
This is a **measured, quoted fact**, and it creates a genuine mismatch for exactly the orthographies
PanGloss targets:

- **Multigraphs** (digraphs/trigraphs like `ch`, `ng`, `ꞌb` that a grammar's `CharDefTable` treats as one
  orthographic segment) are, to a Unicode-scalar-value Levenshtein automaton, *two or three* separate
  edit units. Deleting or substituting one phonological segment that happens to be spelled with two
  Unicode scalar values costs 2 in scalar-value edit distance, not 1 — meaning a genuinely "one edit
  away" correction (in the grammar's own segment inventory) can silently fall outside a "distance ≤ 2"
  fuzzy search window, or a genuinely "two edits away" error can look like it's within distance 2 purely
  because both edits landed inside one multigraph.
- **NFD combining-mark sequences** (tone marks, etc.) have the identical problem in the opposite
  direction: a base letter + combining diacritic is two Unicode scalar values for what a linguist (and
  PanGloss's own `CharDefTable`) would call one segment carrying one tone feature. `pg-grammar`'s
  `CharDefTable` already carries exactly this distinction explicitly in its own model — it stores both
  `representations` and a separately-computed `representations_nfd: Vec<String>`
  (`rust/crates/pg-grammar/src/chardef.rs:63,105-106`), i.e. PanGloss's own loader already knows that a
  segment's authored surface form and its NFD-normalized form can differ in scalar-value count, and
  treats that as a first-class fact about the grammar, not an incidental detail.
- **This is a correctness problem, not just a tuning knob**, because it means "distance ≤ 2" over raw
  Unicode scalar values is measuring a different quantity than "distance ≤ 2 orthographic/phonological
  units" for any grammar with multigraphs or combining marks in its orthography — which per SIL's own
  orthography-design literature (cited in `02-phonological-distance.md` §4) is common for
  previously-unwritten-language orthographies, not a corner case.
- **What would need to happen instead**: candidate generation would need to operate over a
  grammar-specific tokenization into `CharDefTable` segments (exactly the units `pg-grammar` already
  computes) rather than raw Unicode scalar values, before any delete/Levenshtein step runs — i.e. the
  "character" that gets deleted/substituted must be a `CharDefId`-indexed orthographic unit, not `char`.
  Neither SymSpell nor `fst::automaton::Levenshtein` does this by construction; it would be
  PanGloss-specific pre/post-processing layered around either tool. **This entire orthographic-unit
  argument is my-synthesis**, built from directly-read facts (the `fst` docs' own Unicode-scalar-value
  statement, and `pg-grammar`'s own `representations`/`representations_nfd` fields) rather than from any
  source that discusses this combination explicitly — I found no published work analyzing Levenshtein
  automata specifically for multigraph orthographies.

### 2.4 A naming collision worth flagging explicitly

The plan's Step 1.2 says "use the `fst` crate" — this almost certainly means the external
`BurntSushi/fst` crate discussed above, since **that crate is not currently a PanGloss dependency**
(confirmed: no `fst` entry appears in `rust/Cargo.lock`, and no `Cargo.toml` in the workspace declares
it). PanGloss already has a crate literally named `pg-fst` in its own workspace
(`rust/crates/pg-fst`, listed in `rust/Cargo.toml:10,49,57`), but per the workspace's own layout table
its role is "pattern compile, FSA traversal, registers (CSR arc storage)" (`rust/README.md:28`) — this
is PanGloss's **own hand-written** morphological-rule FSA engine (used by `pg-rules`' cascades), entirely
unrelated to BurntSushi's `fst` crate and not a transducer library in the general-purpose sense the plan
means. Any future spell-checking design doc should say "the `fst` crate (crates.io, BurntSushi)"
explicitly to avoid this ambiguity, since the two names collide exactly in this codebase.

---

## 3. Levenshtein automata (Schulz & Mihov 2002) and the hfst-ospell/divvunspell weighted-composition model

### 3.1 Schulz & Mihov's universal Levenshtein automaton

Klaus Schulz and Stoyan Mihov, "Fast string correction with Levenshtein automata," *International
Journal on Document Analysis and Recognition (IJDAR)* 5(1), 67-85, 2002
([Springer](https://link.springer.com/article/10.1007/s10032-002-0082-8),
[dblp](https://dblp.uni-trier.de/rec/journals/ijdar/SchulzM02.html); PDF at
[dmice.ohsu.edu](https://dmice.ohsu.edu/bedricks/courses/cs655/pdf/readings/2002_Schulz.pdf) **could not
be extracted as text — repeated fetch attempts returned only compressed binary PDF stream data**,
consistent with the PDF-extraction failures reported throughout `02-phonological-distance.md`; flagged
as **secondary-summary** below). Per the paper's own abstract/framing (confirmed via
[dblp](https://dblp.uni-trier.de/rec/journals/ijdar/SchulzM02.html) and corroborating
secondary description): a Levenshtein automaton of degree n for a word W is a finite-state automaton
recognizing exactly the set of strings V such that `Levenshtein(V, W) ≤ n`. The paper's central
contribution is showing this automaton can be built **deterministically, in time linear in the length of
W**, for a fixed bound n — and, further, that the automaton's *transition structure* can be made
**"universal"**: precomputed once per distance bound n, independent of the specific alphabet or word,
parameterized only by a small "characteristic vector" computed from W and the input symbol at each step.
This is exactly the technique later packaged as the Rust `levenshtein-automata` crate (§2.2) and is the
"precomputed datastructures" approach BurntSushi/fst#47 measured as ~25× faster than from-scratch DFA
construction. **I could not independently verify the paper's own complexity proof or exact stated
bounds beyond the abstract-level description** — this should be read as secondary-summary, not
independently confirmed primary-text detail, matching the caveat pattern in `02-phonological-distance.md`
§3 for the same class of PDF-extraction failure.

### 3.2 hfst-ospell / divvunspell: composed weighted transducers, not a delete-table

`hfst-ospell` ([github.com/hfst/hfst-ospell](https://github.com/hfst/hfst-ospell)) is described in its
own README as accepting **two separate transducer inputs** — a lexicon transducer and a weighted error
transducer — passed to the `Speller` constructor ("Pass (weighted!) Transducer pointers to the Speller
constructor"), and correction works by traversing both together, collecting weighted results into a
`CorrectionQueue` that is "a priority queue, sorted by weight." **Measured (read directly from the
README)**: "The library is licenced under Apache licence version 2." This is architecturally the
composed-transducer approach the task brief names: an acceptor/analyzer FST for the lexicon, composed at
lookup time with a separate weighted error-model FST (rather than a precomputed flat delete-table), so
that the error model's cost function (insertion/deletion/substitution/transposition, each independently
weighted, potentially per-symbol) is applied during the same search that also validates lexicon
membership — one composed weighted search, not a generate-candidates-then-filter pipeline.

`divvunspell` ([github.com/divvun/divvunspell](https://github.com/divvun/divvunspell),
[lib.rs/crates/divvunspell](https://lib.rs/crates/divvunspell)) is a from-scratch Rust reimplementation
of the same ZHFST/BHFST-consuming architecture ("a modern reimplementation and extension of
hfst-ospell"), confirmed dual-licensed as **"Apache License, Version 2.0 ... MIT license ... You may
choose either license for library use"** for the library itself, with the command-line tools separately
under GPL-3.0. Both Apache-2.0 and MIT are compatible with PanGloss's own `MIT` license
(`rust/Cargo.toml:29`) for the *library* crate specifically (the GPL-3.0 CLI tools would not be
embeddable, but PanGloss would only need the library). BHFST is described as "THFST files packaged in a
box container with JSON metadata." **I could not extract further architectural detail (exact
composition semantics, weight-combination formula) from the README beyond this** — flagged as a
genuine gap; a deeper read of `divvunspell`'s actual source (not just its README) would be needed before
using it as an implementation reference, not just an architectural precedent.

### 3.3 How this compares to delete-only for morphologically rich languages

The structural difference matters specifically for PanGloss's use case:

- **Delete-only (SymSpell) generates candidates first, validates later** (or not at all — the plan's
  Phase 1 has no validation step; candidates are handed straight to Phase 2's keyboard-distance
  reranking). It has no way to prefer a candidate that is a *morphologically well-formed* wordform over
  one that merely happens to be a low-edit-distance string match, because the delete-table only ever
  stores flat strings mapped to a root index — it has no notion of "this deletion corresponds to a legal
  affix boundary" versus "this deletion happens to collide with an unrelated word."
- **The composed-transducer approach (hfst-ospell/divvunspell) validates and generates in the same
  pass**: because the error-tolerant search runs *through* the lexicon/morphology transducer, every
  candidate the search finds is, by construction, already a string the transducer accepts — there is no
  separate "generate candidates, then check if plausible" step, because implausible (non-lexicon,
  non-morphological) strings are never reached by the search in the first place. This is the more
  principled fit for a morphologically rich language, where "close in edit distance" and "a valid
  wordform" are frequently *not* the same set of strings (§1.4).
- **Cost trade-off**: the composed-transducer approach requires building and maintaining a full
  weighted-transducer representation of the lexicon/morphology (or, for PanGloss specifically, of
  whatever the propose→confirm pipeline's proposer FST already is — see §4), whereas delete-only requires
  only a flat string-to-root map. This is a real engineering cost difference, not just an elegance
  difference, and is the main reason SymSpell-style delete tables remain popular for large flat
  wordlists (search engines, autocomplete) where no morphological structure needs to be respected.

---

## 4. The central tension: spell-checking against a PARSE, not dictionary membership

This is, per the task brief, the least-precedented and most PanGloss-specific question, and it changes
the shape of the problem materially relative to both SymSpell and hfst-ospell as usually deployed.

### 4.1 What "accept" means for PanGloss is already settled, and it is not "in the dictionary"

Per `docs/fst-plan/foma-fst-plan.md` (already in-repo, read directly): PanGloss's acceptance model is
**propose→confirm** — "always use FST to propose and HC to prune. The only question is can we move to
foma completely... There is NO per-grammar fallback to full engine search... FST-only (no-verify)
operation is off the table — propose+prune is the permanent shape" (`docs/fst-plan/foma-fst-plan.md:16-20`).
Concretely, `FomaAnalyzer::new(&Grammar)` builds a foma network that *overgenerates* candidate analyses,
and `pg-foma/src/confirm.rs`'s `confirm(...)` re-derives each candidate against the real HermitCrab
engine semantics before accepting it — "soundness comes from confirm (the real engine re-derives every
emitted analysis)... Over-generation is harmless (fails confirm silently)"
(`docs/fst-plan/foma-fst-plan.md:60-62`). This means "is this string a word of the language" is already
answered by a two-stage propose→confirm *parse*, not a lookup against a flat lexicon — and any spelling
model built for PanGloss needs to either (a) operate entirely upstream of this pipeline (treat spelling
correction as a pure string-similarity problem over surface wordforms harvested from a corpus/lexicon,
ignoring the parser), or (b) be integrated *into* the propose→confirm search itself, the way
Oflazer/hfst-ospell compose an error-tolerance layer with the morphological transducer. These are
genuinely different designs with different cost/precision trade-offs, and the plan as written (Phases
1-3) implicitly assumes (a) — a dictionary of surface wordforms — without saying so.

### 4.2 Oflazer 1996: composing error-tolerance directly with the morphological transducer

Kemal Oflazer, "Error-tolerant Finite-state Recognition with Applications to Morphological Analysis and
Spelling Correction," *Computational Linguistics* 22(1), 73-89, 1996
([ACL Anthology](https://aclanthology.org/J96-1003/), preprint at
[arXiv:cmp-lg/9504031](https://arxiv.org/abs/cmp-lg/9504031)). The arXiv abstract page (fetched and read
directly — this counts as **measured/primary**, unlike the PDF body, which again failed to extract as
text on both the ACL Anthology and arXiv PDF mirrors, consistent with every sibling report's experience
with this exact paper) states the applications directly: "error-tolerant recognition... has applications
in error-tolerant morphological processing, spelling correction, and approximate string matching," and
that the technique "can be applied to morphological analysis of any language whose morphology is fully
captured by a single (and possibly very large) finite state transducer" — i.e. exactly PanGloss's
situation, where the whole grammar compiles to (or is approximated by, in the propose→confirm sense) a
single FST/foma network.

Two facts from the abstract page are directly usable and **measured, not asserted-without-basis** (they
are the paper's own reported experimental numbers, read from the primary abstract text, not a secondary
summary):

- For **edit distance 1**, the error-tolerant recognizer generated "all candidate solutions within **10
  to 45 milliseconds**" on a SparcStation 10/41, for European-language word lists "exceeding **200,000
  forms**." This is a genuinely comparable regime to a compiled lexicon transducer, and the timing is
  fast enough that composing error-tolerance with the transducer (rather than precomputing a flat delete
  table) was practical **on 1996 hardware** — a strong data point that this is not a naive-and-therefore-
  slow approach, even before considering three decades of hardware improvement.
- The paper explicitly demonstrates **"an application of this to error-tolerant analysis of agglutinative
  morphology of Turkish words"** — i.e. this is not a generic edit-distance-over-strings method that
  happens to also work on Turkish; agglutination was a named target case in the original 1996 paper,
  which directly rebuts any framing of "composed error-tolerant FST" as an English-specific technique the
  way delete-only implicitly is (§1.4).

I could not extract the paper's own complexity proof or exact automaton-construction algorithm (state-pair
product construction details, edit-distance bookkeeping) from the PDF body itself, on either the ACL
Anthology or arXiv copy — **flagged as secondary-summary for anything beyond the abstract-page text
quoted above.**

### 4.3 Koskenniemi / two-level morphology as the theoretical substrate

The theoretical case that phonological-rule cascades and two-level morphology denote regular relations
realizable as FST cascades (Kaplan & Kay 1994) is already covered in `02-phonological-distance.md` §3 and
directly underlies why an error-tolerance layer composes cleanly with PanGloss's own rule cascades
(`pg-rules/src/cascade.rs`, `pg-rules/src/rewrite.rs`) — not re-derived here to avoid duplicating that
report; the short version is that Koskenniemi's original two-level model and Oflazer's error-tolerant
extension of it are the same formal family PanGloss's own engine already is an instance of.

### 4.4 Pirinen & Lindén: HFST-based weighted spell-checking for Northern Sámi

Tommi Pirinen and Krister Lindén, "Finite-State Spell-Checking with Weighted Language and Error
Models — Building and Evaluating Spell-Checkers with Wikipedia as Corpus," SaLTMiL 2010
([ResearchGate](https://www.researchgate.net/publication/228543643_Finite-State_Spell-Checking_with_Weighted_Language_and_Error_Models-Building_and_Evaluating_Spell-Checkers_with_Wikipedia_as_Corpus)),
and the related "Effect of Language and Error Models on Efficiency of Finite-State Spell-Checking"
([ACL Anthology W12-6201](https://aclanthology.org/W12-6201.pdf) — **PDF text extraction failed**,
returned only compressed binary stream data; flagged as **not independently read**, secondary-summary
only). Per secondary/abstract-level description: this work builds and evaluates a Northern Sámi
finite-state speller using HFST-LexC and other open-source HFST tools plus Wikipedia dumps as a training
corpus, directly analogous to hfst-ospell's architecture (§3.2) applied specifically to a genuinely
morphologically-rich, minority/low-resource language — i.e. the closest published precedent to
"PanGloss's actual target language profile" that this research turned up, but **I was not able to
extract concrete efficiency numbers, accuracy figures, or the exact error-model weighting scheme from
either paper's primary text** — this is a clearly-flagged gap, and reading these two papers directly
(via a different PDF-extraction path than what this tool has available) is a worthwhile follow-up before
committing to any specific weighted-composition design.

### 4.5 What this means for PanGloss, stated plainly

- A **flat delete-table (SymSpell-style) speller** answers "is this string similar to some known surface
  wordform," entirely independent of whether that wordform (or the query) is morphologically
  well-formed. It is cheap and requires no integration with `pg-foma`/`pg-parse` at all — but it can
  neither reward "this candidate is a valid parse" nor benefit from the propose→confirm pipeline's own
  overgeneration-then-prune discipline; a delete-table candidate is just a string, with no morphological
  status.
- A **composed error-tolerant transducer (Oflazer/hfst-ospell-style)** answers "is this string within k
  edits of *something the morphology can actually produce*," which is the more linguistically correct
  question for PanGloss's target languages, and has 1996-vintage precedent specifically for agglutinative
  morphology (§4.2) plus a modern, license-compatible (§3.2), Rust-native reference implementation
  (`divvunspell`) to study. The cost is architectural: it requires an error-tolerant variant of (or a
  composition with) the propose→confirm search itself, not a standalone precomputed table — i.e., it is
  not a drop-in Phase 1 the way the plan currently frames the problem, but a design that touches
  `pg-foma`'s proposer and `pg-foma/src/confirm.rs`'s confirm step directly.
- **No source found treats this exact tension (candidate generation against a generative/propose-confirm
  parser rather than a static wordlist) as a solved, citable problem** — Oflazer 1996 is the closest
  precedent (composing error-tolerance with a *single* morphological transducer), but PanGloss's
  two-stage propose(overgenerate)→confirm(re-derive-and-prune) architecture is its own design, not
  Oflazer's one-transducer model. Treat "run error-tolerant search through the proposer FST, then confirm
  as normal" as a reasoned extension of Oflazer's precedent, not a citation-backed guarantee — the same
  honesty standard `02-phonological-distance.md` applied to its own most novel proposal.

---

## 5. Incremental update: delete tables and composed transducers vs. a FLEx lexicon edited live

### 5.1 `fst::Map`/`Set` are immutable by construction — confirmed directly from the crate's own docs

`fst::map::MapBuilder`'s own documentation states plainly: keys "must be added in lexicographic order,"
adding a key out of order or a duplicate key is an error, and — the load-bearing sentence — **"once a
key is associated with a value, that association can never be modified or deleted"**
([docs.rs/fst/latest/fst/map/struct.MapBuilder.html](https://docs.rs/fst/latest/fst/map/struct.MapBuilder.html)).
There is no incremental single-key insert API at all; adding one new dictionary entry to an existing
`fst::Map`-backed delete-table means **regenerating and rebuilding the entire map from scratch** (or,
per §5.3 below, building a small separate map and merging/unioning it in — `fst` does support `OpBuilder`
streaming set/map operations like union, which is the mechanism such a merge would use, but that is a
manual architecture choice the plan does not currently make).

### 5.2 SymSpell's own reference implementation sidesteps this by not using an `fst` at all

This is a genuine architecture mismatch worth stating explicitly: SymSpell's C# reference implementation
does **not** use a compressed/immutable transducer for its delete-index. Reading `SymSpell.cs` directly:
`CreateDictionaryEntry` populates deletes into an ordinary hash-based dictionary
(`staging.Add(GetStringHash(delete), key)`), and adding one new word at runtime is described in the
source's own comments as generating only that word's own deletes and inserting them — "edits are
generated only once, no matter how often word occurs," "only as soon as the word occurs in the corpus" —
i.e. **incremental by design**, because the underlying structure is a mutable hash map, not a compiled
FST. The plan's proposal to combine "SymSpell's delete-only algorithm" with "the `fst` crate's compressed
binary map" is combining an algorithm whose reference implementation assumes O(1) mutable incremental
insert with a data structure that explicitly forbids any insert after construction. Neither half is wrong
in isolation; the combination as specified needs an explicit rebuild/merge strategy that the plan
currently does not name.

### 5.3 The standard general answer: immutable segments + periodic merge (Lucene's pattern)

This is well-established, general practice for exactly this class of problem (immutable, highly-
compressed FST-based term dictionaries that nonetheless need to support a live-editing corpus), not
PanGloss-specific: Lucene's inverted index stores its term dictionary as an FST
(`BlockTreeTermsReader`), and "segments are immutable; updates and deletions may only create new
segments and do not modify existing ones" — new/changed documents go into small new segments, and "the
writer merges groups of smaller segments into single larger ones" periodically to reclaim the overhead of
many small segments and dead space from deletions
([Lucene 10.3.1 `org.apache.lucene.index` package docs](https://lucene.apache.org/core/10_3_1/core/org/apache/lucene/index/package-summary.html)).
Applied to PanGloss: a small incremental `fst::Map` of deletes for just-added lexicon entries, unioned
(via `fst`'s own `OpBuilder` streaming union) into the main compiled delete-table on some periodic or
size-triggered schedule, is the directly analogous design — not a novel invention, but the standard
answer the IR/search-index literature already gives for "immutable compact structure, live-editing
corpus."

### 5.4 PanGloss already has the more relevant answer for its own lexicon, independent of any spelling design

This is the most important finding of this section, and it comes from reading PanGloss's own code, not
external literature: **PanGloss already has a live, incrementally-updatable overlay for exactly the
"FLEx user edits the lexicon while the engine is running" problem, at the lexicon level, entirely
independent of spelling correction.** `pg-parse/src/overlay.rs` defines `SuppliedRootOverlay`, built from
an `OverlayTrie` (`OverlayNode`/`OverlayEdge`, `rust/crates/pg-parse/src/overlay.rs:34-50`) that sits
alongside the compiled `Grammar`'s own baked-in root trie and can accept new `SuppliedRoot` entries
without recompiling the grammar. `pg-lexicon/src/runtime.rs` layers a revisioned document model on top —
`LexiconDocument`, `LexiconSnapshot` (carrying the `overlay: SuppliedRootOverlay` field directly,
`rust/crates/pg-lexicon/src/runtime.rs:20-54`), and a `ReconciliationReport` tracking
`inactive_entries`/`superseded_entries` across revisions — i.e. PanGloss already has a working answer to
"how do you add, supersede, and deactivate lexical entries incrementally, with revision tracking, without
a full grammar rebuild." (Per this session's git history, this exact subsystem was recently hardened:
"harden native supplied lexicon runtime," "reuse native batch pool and recover guides.")

**The direct implication for spell-checking, stated as my-synthesis but grounded in the above**: whatever
incremental-update story a lexical-distance speller needs should be designed as an *extension of this
existing overlay/reconciliation mechanism* — e.g., a parallel delete-index (or Levenshtein-automaton
input list) that tracks the same `SuppliedRoot`/`entries` revision stream `pg-lexicon::runtime` already
maintains, rebuilding or unioning-in only the delta since the last reconciled revision — rather than
inventing a separate rebuild-the-whole-`fst::Map`-from-scratch pipeline that has no relationship to how
PanGloss already handles live lexicon edits everywhere else in the engine. This specific integration (spell-index
as a subscriber to `LexiconSnapshot`/`ReconciliationReport` revisions) is not something I found precedent
for in the literature — it is a PanGloss-specific design opportunity, not a citation.

---

## Recommendations for PanGloss

1. **Fix the combinatorics in the plan, and reframe Phase 1's cost model around wordform length, not a
   fixed small-n assumption.** Use `1 + n + C(n,2)` (§1.2), and before committing to any fixed max edit
   distance, measure actual delete-table size against PanGloss's own stress-grammar wordform-length
   distribution (per user memory, "50k-entry target, dozens of stress grammars," 20-40-codepoint
   wordforms expected) rather than assuming English-scale (n≈5-10) costs transfer.
2. **Do not adopt `fst::automaton::Levenshtein` for fuzzy lookup as currently shipped.** The crate's own
   docs call it "proof of concept," a closed upstream issue measured a ~25× construction-time gap against
   a Schulz-Mihov-style precomputed-table approach (§2.2), and — separately and more importantly for
   correctness — its distance metric operates over raw Unicode scalar values, which will silently
   mismeasure edit distance for any grammar with multigraphs or NFD combining-mark sequences in its
   orthography (§2.3), a real risk given PanGloss's `CharDefTable` already tracks
   `representations`/`representations_nfd` as distinct fields (`rust/crates/pg-grammar/src/chardef.rs:63,105-106`)
   specifically because these sequences don't collapse to one scalar value. If a Levenshtein-automaton
   approach is wanted, prefer the `levenshtein-automata` crate (MIT, Schulz-Mihov-based, used by
   `tantivy`) and run it over a tokenization into `CharDefTable` segment units, not raw `char`s.
3. **Name the tool precisely in any future plan.** "The `fst` crate" must mean the external
   `crates.io`/`BurntSushi/fst` crate (not currently a PanGloss dependency — absent from
   `rust/Cargo.lock`), distinct from PanGloss's own `pg-fst` (`rust/crates/pg-fst`, a hand-written
   morphological-rule FSA/pattern-compile engine, unrelated in purpose). This is a real naming collision
   in this specific codebase (§2.4) and worth a one-line disambiguation in whatever plan supersedes this
   one.
4. **Decide explicitly whether spelling correction operates upstream of or inside the propose→confirm
   pipeline, because the plan currently assumes the former without saying so.** A flat delete-table or
   Levenshtein-automaton lookup against a wordform list (upstream, dictionary-membership style) is
   cheap and requires no `pg-foma`/`pg-parse` integration, but cannot distinguish "close in edit
   distance" from "actually a valid parse" (§4.5) — for a HermitCrab-style overgenerating morphology this
   gap is not cosmetic. The better-precedented alternative (Oflazer 1996, hfst-ospell/divvunspell) composes
   error-tolerance directly with the morphological transducer, which for PanGloss concretely means an
   error-tolerant variant of the `FomaAnalyzer` proposer step, still validated by the existing
   `pg-foma/src/confirm.rs` confirm pass (`docs/fst-plan/foma-fst-plan.md:16-20,60-62`) — overgeneration
   from the error-tolerant proposer is exactly as "harmless, pruned by confirm" as the existing proposer's
   overgeneration already is. This is a bigger design commitment than a Phase 1 delete-table, and should be
   scoped as such rather than folded silently into "Phase 1."
5. **`divvunspell` is worth a direct source read, not just an architectural citation, before any
   implementation choice.** It is Rust-native, license-compatible (`Apache-2.0 OR MIT` for the library,
   matching PanGloss's own `MIT`, `rust/Cargo.toml:29`), and is the most mature open implementation of the
   composed-weighted-transducer approach in a language PanGloss already ships in — a stronger build-vs-adapt
   candidate than reimplementing Schulz-Mihov or SymSpell from a paper.
6. **Design incremental update as an extension of the lexicon overlay PanGloss already has, not a
   separate rebuild pipeline.** `pg-parse::SuppliedRootOverlay`/`OverlayTrie`
   (`rust/crates/pg-parse/src/overlay.rs:34-50`) and `pg-lexicon`'s revisioned
   `LexiconSnapshot`/`ReconciliationReport` (`rust/crates/pg-lexicon/src/runtime.rs:20-54`) already solve
   "a FLEx user adds/supersedes/deactivates lexical entries live, without a full grammar recompile." Any
   delete-table or Levenshtein-automaton index should track the same revision stream — incrementally
   folding in just the delta since the last reconciled revision, using `fst`'s own streaming `OpBuilder`
   union if `fst::Map` is used at all (§5.3, the general Lucene-segment-merge pattern) — rather than
   inventing a second, unrelated live-update mechanism. This is the single most actionable, concrete,
   code-grounded recommendation in this report.

---

## What I could not verify (explicit list)

- Schulz & Mihov 2002's own stated complexity proof and the exact mechanics of the "characteristic
  vector" construction — PDF would not extract as text on the one mirror tried; relied on
  abstract/secondary description (§3.1).
- Oflazer 1996's own state-pair-product construction algorithm and complexity bound — PDF would not
  extract as text on either the ACL Anthology or arXiv mirror; the abstract page itself (read directly)
  did yield two genuine measured numbers (10-45ms, 200,000+ forms, SparcStation 10/41, distance 1) which
  I have treated as primary/measured since the abstract page is the paper's own text, not a third-party
  summary (§4.2).
- Pirinen & Lindén's 2010 SaLTMiL paper and Pirinen's 2012 "Effect of Language and Error Models on
  Efficiency" (ACL W12-6201) — both PDFs returned unreadable compressed binary content on every fetch
  attempt; everything in §4.4 beyond the citation itself is secondary-summary only. This is the single
  most valuable follow-up read for anyone taking this design further, since it is the closest published
  work to "weighted finite-state spelling correction for a genuinely low-resource, morphologically rich
  language."
- `divvunspell`'s actual composition/weighting algorithm at the source level — only the README was read;
  the architectural claim (composed weighted transducers, ZHFST/BHFST bundling) is confirmed, but the
  precise scoring formula is not.
- A directly-measured number (rather than derived arithmetic) for delete-table size or lookup latency
  specifically at 20-40-codepoint wordform lengths — no source publishes this because SymSpell-style
  delete tables are not a technique used in the literature for agglutinative-morphology spelling at this
  wordform-length scale; the degradation argument in §1.4 is arithmetic-grounded (`C(n,2)` scaling) and
  architecture-grounded (fixed-k assumption mismatch), not benchmark-grounded.
