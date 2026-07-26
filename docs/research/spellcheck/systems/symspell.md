# System profile: SymSpell (and the SymSpellCompound / delete-only family)

Profile of SymSpell against the standing PanGloss comparison rubric. PanGloss's *original*
spell-checking plan (`docs/spell-checking-plan.md`, Phase 1) copied SymSpell's delete-only model
directly (precompute "all unique strings formed by deleting up to 2 characters," compile into an
`fst`-crate `ErrorTransducer`). `01-lexical-distance.md` already challenged that plan hard, against
SymSpell's own primary sources; this profile is built on that analysis, not independent of it — read
`01-lexical-distance.md` and `00-synthesis.md` first if you have not.

**Labeling convention** (matching the sibling reports): **measured** = read directly from a primary
source (repo README, source file, crate docs) or arithmetic I derived/cross-checked myself against the
primary source's own numbers; **asserted** = a primary source states it without showing derivation or
independent benchmark; **synthesis** = a conclusion drawn by combining sourced facts, not stated by any
single source; unfetchable sources are flagged explicitly where they occur.

Primary sources used: [wolfgarbe/SymSpell](https://github.com/wolfgarbe/SymSpell) README (fetched via
the `raw.githubusercontent.com` mirror — the rendered `github.com` page itself returned only
navigation chrome on fetch, flagged below), `SymSpell.cs` (already read directly in
`01-lexical-distance.md`, not re-fetched here), Wolf Garbe's ["1000x Faster Spelling Correction
algorithm (2012)"](https://wolfgarbe.medium.com/1000x-faster-spelling-correction-algorithm-2012-8701fcd87a5f)
Medium post, [wolfgarbe/symspell_rs](https://github.com/wolfgarbe/symspell_rs) (the official Rust
port), and the [`symspell`](https://lib.rs/crates/symspell) third-party Rust crate (via its `lib.rs`
mirror, since `crates.io`'s own page and API both refused this environment's fetches — flagged
below).

**Unfetchable / degraded sources**: `https://github.com/wolfgarbe/SymSpell` rendered page returned only
GitHub chrome, not README content — worked around via the `raw.githubusercontent.com` README mirror,
so README content below is still primary-source, just fetched through a different URL. `crates.io`'s
own crate page (`https://crates.io/crates/symspell`) and its JSON API
(`https://crates.io/api/v1/crates/symspell`) both refused this environment's requests outright ("unable
to process your request... API data access policy"); worked around via the `lib.rs` community mirror
of the same crate metadata, which is a secondary aggregator, not the registry itself — flagged
inline where used.

---

## ARCH

Precomputed deletion index + exact hash lookup, not a search or edit-distance computation at query
time. **Measured**, read directly from the README: "Opposite to other algorithms only deletes are
required, no transposes + replaces + inserts. Transposes + replaces + inserts of the input term are
transformed into deletes of the dictionary term." Both the dictionary (build time) and the query
(lookup time) are reduced to their own deletion-only variant sets; any true edit distance ≤ k between
two words guarantees at least one exact string collision between the two delete-sets. The reference
C# implementation stores deletes in an ordinary hash table (`staging.Add(GetStringHash(delete), key)`,
confirmed by direct source read in `01-lexical-distance.md` §1.1-1.2) — **not** a compressed
transducer; PanGloss's plan to pair this algorithm with the external `fst` crate's immutable `Map` is
PanGloss's own addition, not part of SymSpell's own design (see PERSONALIZATION below).

## LEXICON

Must be an enumerated wordlist (a flat list of surface strings, typically frequency-annotated) plus a
derived deletion dictionary computed from it. There is no notion of a stem, an affix, or a generative
rule anywhere in the model — every acceptable output string must appear literally in the input
wordlist before the index is built. **Measured**: the README's own worked example treats "an average
5 letter word" as the atomic unit being indexed, i.e. the unit of the lexicon is the whole surface
form, not a morpheme.

## MORPHOLOGY

None — and it cannot accept unbounded inflection without enumerating every wordform; this is the
crux, and the reason is structural, not a missing feature that could be bolted on. The delete-index
maps *specific strings* to a root entry; there is no operation in the algorithm that generates or
recognizes a wordform it wasn't given at build time (contrast HermitCrab-style propose→confirm, which
recognizes a combinatorially large wordform space from a bounded stem+rule grammar it was never
handed the surface forms of). For a language where inflection is enumerable and small (English:
handful of affixes, mostly regular), pre-enumerating the paradigm and feeding every wordform into the
wordlist is viable and is in fact standard practice for SymSpell-based English spellcheckers. For an
agglutinative language, that same move is a non-terminating enumeration (see DATA_REQ) — the wordlist
that "would" make SymSpell correct does not have a fixed size to converge to. This is **synthesis**,
built directly on `01-lexical-distance.md` §4.1's point that PanGloss's own acceptance model is already
settled as propose→confirm *parsing*, not wordlist membership — SymSpell is architecturally the
opposite of that model, not a variant of it.

## ERRORMODEL

Delete-only Damerau-Levenshtein ≤ k, symmetric between dictionary and query. Correct combinatorics,
**measured** and cross-checked against the README's own worked numbers in `01-lexical-distance.md`
§1.2: total distinct index keys per word of length n at max distance k is `1 + sum_{i=1}^{k} C(n,i)`,
which for k=2 is `1 + n + C(n,2)` (the plan's `n(n-1)/2` alone silently drops the identity case and
every single-deletion case). For k=3, n=5: `1 + 5 + 10 + 10 = 26` deletes stored per word — one more
than the README's own stated "25," because the README's phrasing counts stored deletes excluding the
identity/original word ("SymSpell needs to generate only 25 deletes" — the word itself is stored
separately, not as a "delete"); either way the arithmetic (`C(5,1)+C(5,2)+C(5,3) = 5+10+10 = 25`)
reproduces the README's number exactly (**measured**, cross-checked). Growth is polynomial in n for
fixed k (`Θ(n^k)`, i.e. `Θ(n^2)` at k=2), not exponential — but the constant factor is what bites at
length. Degradation at 20-40-character agglutinative wordforms (the length PanGloss's own stress
grammars specifically target, per user memory): `C(30,2)=435`, `C(40,2)=780` deletes per wordform at
k=2 alone, versus `C(5,2)=10` or `C(8,2)=28` for typical English wordforms — a 15-80x per-word index
cost multiplier before any question of how many surface forms a single lemma must contribute. This
specific degradation number is **synthesis** (derived arithmetic, not published by SymSpell or anyone
else for this wordform-length regime — `01-lexical-distance.md`'s own "what I could not verify" list
names this exact gap). Separately, fixed small k is a real assumption about *error shape*, not a
property SymSpell derives from anything: a single wrong-allomorph or vowel-harmony choice can change
an entire suffix string (edit-distance-many, in raw characters) while being a "one wrong
morphophonological decision" error linguistically — outside what k=2 or k=3 can catch at all
(**synthesis**, following directly from the delete-only definition per `01-lexical-distance.md` §1.4).

## DETECTION

SymSpell is correction-only, and real-word errors (a wrong-but-valid word substituted for the
intended one) are outside single-word delete-index lookup by construction, because a real-word error
is, by definition, already an exact hit in the dictionary — nothing about looking a valid word up
against a delete-index flags it as suspicious. **SymSpellCompound provides some context, but through
compound-splitting, not real-word-error detection**: **measured**, from the README (fetched via the
raw-mirror workaround above), `LookupCompound` handles three specific cases — "mistakenly inserted
space within a correct word," "mistakenly omitted space between two correct words," and "multiple
input terms with/without spelling errors" — i.e. word-segmentation-with-correction, not "is this
valid-looking word actually the wrong word for this sentence." That is a different problem than
real-word-error detection in the Constraint-Grammar/agreement-violation sense `00-synthesis.md`'s
corroborated finding #2 names as the actual detection gap.

## CONTEXT

SymSpellCompound supports an optional bigram dictionary for exactly one purpose: picking the best
compound split, not general sentence-context correction. **Measured**, from the README: "Even better
SymSpell.LookupCompound correction quality, when using the optional bigram dictionary in order to use
sentence level context information for selecting best spelling correction," and version 6.5 added
"Better SymSpellCompound correction quality with existing single term dictionary by using Naive Bayes
probability for selecting best word splitting" (**asserted**, versioned changelog entry, not
independently benchmarked here). This is a narrow, single-purpose n-gram use (disambiguating a
compound-split candidate set), not a general word- or morpheme-n-gram language model of the kind
`04-ngram-factored.md` evaluates — SymSpellCompound's bigram use and `04`'s factored-LM discussion are
not the same mechanism and should not be conflated.

## SEMANTICS_POS

None. Nothing in the delete-index, the hash lookup, or SymSpellCompound's bigram-assisted split
carries or consumes part-of-speech, inflectional-feature, or semantic-domain information — the entire
model operates over bare strings and (optionally) their bigram co-occurrence frequency. **Measured**
by absence: no field, parameter, or README section in any fetched source mentions POS or semantic
annotation anywhere in the pipeline.

## DATA_REQ

Minimum viable data is a frequency-annotated flat wordlist (surface forms + counts) — nothing else is
structurally required; SymSpellCompound additionally wants a bigram frequency dictionary as an
optional add-on. For English this is a bounded, off-the-shelf artifact (word-frequency lists from
corpora of a few hundred thousand distinct forms). For an agglutinative language, the wordform list
this same recipe would need is **non-terminating**, not merely "large": if a single lemma combines
with even a modest inventory of person/number/case/aspect/mood affixes multiplicatively, and
derivational affixes can themselves recurse (a common HermitCrab-grammar pattern), the set of
grammatical surface wordforms for one lemma is unbounded in principle and, even where practically
bounded by realistic usage, is orders of magnitude larger than any corpus-derived frequency list will
ever cover — meaning SymSpell's "minimum viable data" is not achievable at all for the actual target
language class, only an ever-incomplete approximation of it, with new valid wordforms permanently
falling outside the delete-index no matter how the wordlist is grown. This is **synthesis**, directly
extending `01-lexical-distance.md` §1.4/§4.5's finding that PanGloss's acceptance model is a generative
parse specifically because a flat wordlist cannot terminate for this language class.

## PERSONALIZATION

SymSpell's own reference design supports cheap incremental update; the architecture PanGloss's
original plan proposed on top of it does not. **Measured**, from direct source read (already performed
in `01-lexical-distance.md` §5.2): the C# reference implementation's `CreateDictionaryEntry` inserts
into an ordinary mutable hash table, and the source's own comments describe adding a new word at
runtime as generating only that word's own deletes and inserting them ("edits are generated only
once, no matter how often word occurs... only as soon as the word occurs in the corpus") — i.e.
SymSpell itself is incremental-by-design, because its underlying structure is a mutable hashmap. The
problem is specific to the plan's proposed pairing with the external `fst` crate: `fst::Map` is
immutable once built (`MapBuilder`'s own docs, quoted directly in `01-lexical-distance.md` §5.1:
"once a key is associated with a value, that association can never be modified or deleted"), so
adding one lexicon entry to an `fst`-backed delete-table means a full rebuild (or a manual
merge/union via `OpBuilder`, which the plan does not name). **The mismatch is PanGloss's plan's own
combination, not a property of SymSpell** — SymSpell's algorithm and SymSpell's reference data
structure are two separable things, and the plan adopted the algorithm while pairing it with a data
structure (`fst::Map`) whose mutability model is the opposite of what SymSpell's own reference
implementation assumes.

## INTEGRATION

Search-suggest and fuzzy-lookup contexts, not office-suite spellcheckers. **Asserted/synthesis** (web
search, not a single authoritative directory of deployments): the clearest concrete production
example found is [SeekStorm](https://seekstorm.com/blog/1000x-spelling-correction/), a
search-as-a-service/crawling engine built by the same author (Wolf Garbe), which states real-time
spell-checking at "5µs average lookup time" for search-as-you-type use — i.e. the algorithm's own
inventor's follow-on product is a search engine, not a word processor. `symspell_complete_rs`
("SymSpellComplete, a typo-tolerant autocomplete library in Rust") is explicitly built for
autocomplete and names SeekStorm as an intended consumer (**asserted**, from the repo's own framing).
No source found describing SymSpell integrated into a word-processor/office-suite spellchecker (the
Hunspell/LibreOffice/Word niche `03-keyboard-keyman.md` and `05-gaps-and-transformers.md` describe
belongs to a different tool family) — this profile did not find such an integration, which is worth
reading as "did not find," not "confirmed absent," since no exhaustive deployment registry exists to
check against. API shape, from the reference implementation and its ports: construct/load a
dictionary, call a single-word `Lookup` (returns ranked suggestions within max edit distance) or a
multi-word `LookupCompound`/`WordSegmentation` call — a small, synchronous, in-process API, not a
client-server protocol.

## LICENSE

MIT. **Measured**, from the README (via the raw-mirror fetch): "MIT License... Copyright (c) 2025
Wolf Garbe." The official Rust port `symspell_rs` is also MIT (**measured**, its own README/LICENSE),
and the independent third-party Rust crate `symspell` (on crates.io, mirrored via `lib.rs` since
`crates.io`'s own page/API refused this environment's fetch) is also listed MIT. All three are
compatible with PanGloss's own `license = "MIT"` (`rust/Cargo.toml:29`, per `01-lexical-distance.md`
§2.2's already-confirmed reading).

## FOOTPRINT

Deletion table roughly 10-16x the dictionary's own entry count at typical English word lengths (k=2),
growing with word length per ERRORMODEL above; WASM feasibility is good at English/European scale,
untested and likely poor at PanGloss's target wordform lengths without a length cap. **Measured**,
from the Medium post: "for a maximum edit distance of 2 with an average word length of 5 and 100,000
dictionary entries we need to additionally store 1,500,000 deletes" — 15x the entry count, which
matches `1 + n + C(n,2)` at n=5,k=2 (`1+5+10=16`, off by one for the same identity-case counting
convention noted in ERRORMODEL) almost exactly, confirming the derived formula against a second
independent primary-source number. Speed reputation is very strong at this scale: **asserted**
(author's own numbers, not independently re-benchmarked by this profile or by `01-lexical-distance.md`)
"0.033 milliseconds/word (edit distance 2)," "0.180 milliseconds/word (edit distance 3)," "1,870x
faster than BK-tree," "1,000,000x faster than Norvig's algorithm" (README, Medium post). WASM
feasibility specifically: no source found benchmarking SymSpell's WASM footprint or latency directly;
the third-party `symspell` crate and `symspell_complete_rs` both advertise wasm32 compilation as a
supported target (**asserted**, crate descriptions), which is at least an existence proof that the
algorithm compiles and runs under wasm32 — but the delete-table's size problem at 20-40-character
agglutinative wordforms (ERRORMODEL) is a memory-budget concern specifically relevant to PanGloss's
bounded WASM deployment (`CONTEXT.md`'s "inference deployment — browser/WASM... bounded analysis") that
no source addresses, because nobody publishes SymSpell benchmarks at that wordform-length regime
(same gap `01-lexical-distance.md`'s "what I could not verify" list already names).

## RUST_C

Yes, multiple: `symspell_rs` ([github.com/wolfgarbe/symspell_rs](https://github.com/wolfgarbe/symspell_rs))
is the **official** Rust port, MIT licensed, maintained by the algorithm's original author, and
supports `LookupCompound`, word segmentation, bigram-dictionary context, and Chinese text segmentation
(**measured**, its own README). It has no WASM bindings itself as of this fetch (**asserted**, not
mentioned in its README). Independently, at least three third-party crates exist on crates.io:
`symspell` (a from-scratch Rust reimplementation, MIT, ships `UnicodeStringStrategy` and
`AsciiStringStrategy` string handling plus explicit `wasm32` compilation support and JS bindings,
v0.5.2 as of this fetch, ~6,571 monthly downloads per `lib.rs`'s mirror — **asserted**, third-party
aggregator numbers, not crates.io's own page directly since that page refused this environment's
fetch), `fast_symspell`, and `symspell_complete_rs` (typo-tolerant autocomplete specifically, MIT,
targets SeekStorm). Per this project's stated build philosophy (`00-synthesis.md`: "first-class Rust
implementations of each engine... code is cheap"), SymSpell specifically needs no porting work at
all — it is already a mature, actively maintained, author-endorsed Rust ecosystem, unlike several
other engines named across the sibling reports (SRILM's FLM, a Constraint Grammar engine, KMX/LDML
confusion-matrix tooling) that do need a port.

## MINORITY_VERDICT

Not feasible as the acceptance/candidate-generation mechanism for a hyper-minority agglutinative
language, for a structural reason, not an engineering-effort or performance-tuning reason: the
enumeration wall. SymSpell requires every acceptable output to be a literal entry in a precomputed
wordlist before the index exists; an agglutinative grammar's true wordform space is non-terminating
(DATA_REQ), so there is no wordlist size at which "add more words" converges to "covers the language."
Making SymSpell's index cover more of the language means enumerating more wordforms from the grammar
and re-indexing them — which is not spell-checking against a language, it is spell-checking against
whatever finite sample of the language someone remembered to enumerate, with every un-enumerated
correct wordform permanently invisible to lookup, indistinguishable in the index from a genuine typo.
This is the same wall `01-lexical-distance.md` §4 already names as PanGloss's actual acceptance model
being a generative parse, not dictionary membership — SymSpell is dictionary-membership's purest,
fastest form, which is exactly why it cannot be adapted to close this gap without ceasing to be
SymSpell (at that point the design is Oflazer/hfst-ospell-style composed error-tolerant transduction
through the morphology itself, a different architecture `01-lexical-distance.md` §3-4 already covers).
Where SymSpell **does** remain a legitimate PanGloss component: as a fast layer over a *bounded,
enumerable* surface set that is not the whole language — e.g. a personal/session cache of a user's
own recently-typed forms (PERSONALIZATION-overlay style, per `00-synthesis.md`'s personalization axis),
or a closed list of known loanwords/proper nouns/frequent function words — never as the mechanism
covering open-class inflected vocabulary.

## HEADLINE

**Strengths**: (1) Genuinely fast, exact-match lookup at bounded wordform lengths and dictionary
sizes — asserted sub-millisecond per-word correction, verified-by-arithmetic combinatorics, and this
is not in dispute as a mechanism. (2) Mature, actively maintained, MIT-licensed, author-endorsed Rust
ecosystem already exists (`symspell_rs` official port plus independent crates) — zero porting cost,
unusual among the engines this research has surveyed. (3) SymSpellCompound's bigram-assisted
compound-splitting is a real, narrow context mechanism worth studying even outside a full SymSpell
adoption, for languages with word-boundary ambiguity.

**Weaknesses**: (1) The morphology wall — architecturally requires an enumerated wordlist, and an
agglutinative language's true wordform space does not terminate, so SymSpell can only ever cover a
shrinking, silently-incomplete fraction of the actual language, with no path to closing that gap
without becoming a different algorithm. (2) Fixed small edit-distance-k assumption mismatches the
actual error shape of morphophonological mistakes (one wrong allomorph/harmony choice = many changed
characters, linguistically "small" but numerically large), and the delete-table's own size scales
`Θ(n^k)` per word, directly punishing the 20-40-character wordforms this project targets. (3) Zero
POS/semantic/context awareness beyond SymSpellCompound's narrow bigram-for-splitting use — no
real-word-error detection, no agreement/case checking, nothing usable as PanGloss's actual detection
layer (`00-synthesis.md` finding #2).

---

## Filled rubric (compact field:value block)

```
ARCH: precomputed delete-only index (dictionary + query both reduced to deletion variants) + exact hash/string-equality lookup; no query-time edit-distance computation [measured]
LEXICON: enumerated flat wordlist (surface strings, optionally frequency-annotated) + derived deletion dictionary; no stems/affixes/rules [measured]
MORPHOLOGY: none; cannot accept unbounded inflection without enumerating every wordform — the index only ever maps specific precomputed strings to a root entry, it has no generative mechanism, so any wordform not enumerated at build time is permanently unreachable [synthesis, grounded in 01-lexical-distance.md §4.1]
ERRORMODEL: delete-only Damerau-Levenshtein <=k, symmetric dict/query; correct count = 1+n+C(n,2) at k=2 [measured, cross-checked against README's own "25 deletes for 5-letter word, k=3" example]; Theta(n^k) growth — C(30,2)=435, C(40,2)=780 per 20-40-char wordform vs C(5,2)=10 for English, a 15-80x per-word cost multiplier [synthesis, no published number at this length exists]
DETECTION: correction-only; real-word errors invisible to single-word lookup by construction (a wrong-but-valid word is an exact dictionary hit); SymSpellCompound's context is compound-split selection, not real-word/agreement-error detection [measured]
CONTEXT: SymSpellCompound optional bigram dictionary, used only to pick the best compound split (Naive Bayes over splits since v6.5) — not a general sentence-context correction model [measured/asserted]
SEMANTICS_POS: none anywhere in the pipeline [measured by absence]
DATA_REQ: a frequency wordlist (+ optional bigram dict for compounds); for an agglutinative language this wordlist is non-terminating in principle and, even bounded practically, orders of magnitude short of the true wordform space — "minimum viable data" is unachievable, only ever an incomplete approximation [synthesis]
PERSONALIZATION: SymSpell's own reference implementation is incrementally mutable (plain hashmap, generate-deletes-for-one-new-word-only) [measured, SymSpell.cs]; PanGloss's plan pairs the algorithm with fst::Map instead, which is immutable-once-built (full rebuild or manual OpBuilder union needed) — a mismatch in the plan's own combination, not in SymSpell itself [measured, 01-lexical-distance.md §5]
INTEGRATION: search-suggest/autocomplete/fuzzy-search contexts (SeekStorm search engine, symspell_complete_rs autocomplete), not office-suite spellcheckers; no office-suite integration found [asserted/synthesis]; API = in-process Lookup/LookupCompound/WordSegmentation calls, not a protocol
LICENSE: MIT (SymSpell C# original, symspell_rs official Rust port, and the third-party symspell Rust crate all MIT) [measured]; compatible with PanGloss's own MIT license
FOOTPRINT: delete table ~15-16x dictionary entry count at English word lengths (k=2) [measured, Medium post's own "100,000 entries -> 1,500,000 deletes"], growing with word-length^2; reputed sub-millisecond lookup, "1,870x faster than BK-tree," "1,000,000x faster than Norvig" [asserted, author's own numbers]; WASM: existence-proof only (crates advertise wasm32 target), no benchmarks found at PanGloss's wordform-length regime, and the table-size problem is a real memory-budget risk for bounded WASM deployment that no source addresses
RUST_C: yes, mature and actively maintained — symspell_rs (official, author-maintained, MIT) + at least 3 independent crates.io crates (symspell v0.5.2 MIT with wasm32 support, fast_symspell, symspell_complete_rs); zero porting work needed per project build philosophy
MINORITY_VERDICT: not feasible as the core acceptance/candidate-generation mechanism for a hyper-minority agglutinative language — the enumeration wall is structural, not a tuning problem; may remain legitimate as a fast layer over a genuinely bounded, enumerable surface set (personal-cache overlay, closed loanword/proper-noun list) but never as the mechanism covering open-class inflected vocabulary
HEADLINE: strengths = fast exact-match lookup at bounded scale, mature MIT Rust ecosystem already exists (no port needed), SymSpellCompound's bigram-assisted compound-split is a useful narrow mechanism; weaknesses = the morphology/enumeration wall (cannot cover a non-terminating wordform space), fixed-small-k mismatches morphophonological error shape and Theta(n^k) table cost punishes long agglutinative wordforms, zero POS/semantic/real-word-error detection capability
```
