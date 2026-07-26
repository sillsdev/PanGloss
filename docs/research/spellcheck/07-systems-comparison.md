# Top spelling systems vs. PanGloss — comparison

Synthesizes the five per-system profiles in `systems/` (hunspell, aspell, symspell,
divvun, neural) against what PanGloss has/plans (per `00-synthesis.md` + reports 01-06).
Design-only. "Unbounded infl.?" = can it accept an unlimited set of inflected wordforms
WITHOUT enumerating them at build time — the decisive axis for agglutinative languages.

> **CORRECTION (2026-07-24) — this table overstates one row; read with the amendment.**
> The **"Semantic category"** row (PanGloss the only ✓) and **differentiator #3**
> ("semantic-category / selectional detection") oversell us. Semantic domains are per-`LexSense`
> LibLCM data: optional (bimodal coverage — rich only if the project ran Rapid Word
> Collection), reached through two lossy hops (lemma → sense → domain), requiring word-sense
> disambiguation, and explicitly excluded from the `grammar.json` export
> (`docs/grammar-json-export-plan.md:49`, D5 at :71). They are also a document-level signal
> that a 3-word n-gram window structurally cannot see.
> The honest differentiator vs. Divvun is **feature-structure richness**: their tags *are*
> the data model (POS + inflection + valency), whereas PanGloss has full HermitCrab feature
> structures with unification and natural classes above the FST — which is what actually feeds
> the class-backoff LM and the phonological cost matrix. Selectional restrictions
> ("object must be animate") land as **CG rules over inflectional features** — Divvun's
> `valency.cg3` territory, a narrower gap than this table implies. Note that animacy is
> generally a *grammatical feature*, not a semantic domain.
> See `00-synthesis.md` § "Inflectional features ≠ semantic domains". This table is left
> unedited pending followup #10 (a full re-cut).
>
> Also amended by `00-synthesis.md` § "Divvun / GiellaLT — relationship": the Divvun column
> here reflects Sámi deployments only; **GiellaLT** (the shared infrastructure) spans ~100
> language repos across several families, so the architecture is not family-specific.

## Master table (rows = dimension, cols = system)

| Dimension | Hunspell | Aspell | SymSpell | Divvun/hfst-ospell | Neural (Gboard/ByT5) | **PanGloss** |
|---|---|---|---|---|---|---|
| **Architecture** | affix-flag dict + ordered edit cascade | near-miss edit + Metaphone, scores averaged | precomputed delete index + hash lookup | weighted acceptor FST ∘ error FST, then separate CG stage | seq2seq / char-transformer (or reranker) | generative morphology (FST propose→HC confirm) ∘ **one** unified weighted error model; +CG; +class-LM rerank |
| **Lexicon model** | affix-compressed wordlist (.dic/.aff) | flat compiled wordlist | enumerated wordlist + delete table | compiled FST acceptor from lexc/twolc | none (implicit in weights) | **generative grammar** (stems×morphotactics×phonology) from LibLCM/HC |
| **Unbounded infl.?** | ✗ fixed affix rules, 2-deep | ✗ affix compression only | ✗ **enumeration wall** | ✓ full generative FST | ✗ only if seen in training | ✓ full generative FST + HC |
| **Error model** | KEY/TRY/REP/n-gram/PHONE cascade, **incomparable scores** | weighted edit + soundslike, averaged | delete-only Damerau-Lev ≤k | auto-gen Levenshtein+swap FST, weighted; no keyboard geometry | learned from (synthetic) error pairs | **unified weighted composition**: grammar natural-class feature cost + Keyman-derived keyboard prior + edit, one search |
| **Real-word / context detection** | ✗ none | ✗ (static confusion pairs only) | ✗ (compound-split only) | ✓ via **CG** (hand-written rules) | ✓ strong (full sentence) | ✓ CG + **class-LM** + free confusion sets |
| **Uses POS / grammar** | ✗ (fields inert) | ✗ | ✗ | ✓ POS+features+valency (in CG) | implicit only | ✓ **explicit** POS+features (LibLCM), first-class |
| **Uses semantic category** | ✗ | ✗ | ✗ | ✗ (valency, but no sem. domains) | implicit only | ✓ **semantic domains** (LibLCM) available as a factor |
| **Min data for a NEW language** | hand .dic + hand .aff; no corpus | **complete enumerated wordlist** (+opt. hand phonetic table) | frequency wordlist (non-terminating) | **full FST morphology** (big) + opt. CG; no corpus | ~100k sentences, or synthetic errors; loses <~300 real | **the grammar you already have** (LibLCM/HC); no corpus; class-LM works on minimal corpus |
| **Personalization** | flat personal wordlist | flat .pws + .prepl (accumulate, not learn) | mutable hashmap (plan's fst::Map is immutable — mismatch) | none found | Gboard federated (diff. feature); Proofread none | **SuppliedRootOverlay + revisioned LexiconSnapshot** (superset) + planned personal confusion model + cache-LM |
| **Host integration** | **universal** (LibreOffice/Word/Firefox/Chrome/macOS); de facto format | CLI/lib, Emacs; superseded by Hunspell | search-suggest, not office | broad: LibreOffice/Word/GDocs, OS-wide, mobile kbds; .zhfst/.bhfst | Gboard mobile / server APIs | **open** — must pick emit target (Hunspell fmt for reach? divvun-style?) |
| **License** | MPL/GPL/LGPL tri | LGPL 2.1+ | MIT | runtime MIT/Apache; **CG + compiler GPL-3** | ByT5 Apache; Gboard proprietary | own (MIT) |
| **WASM / footprint** | WASM ports exist (hunspell-wasm, zspell) | **no WASM**; mmap/endian complicates | WASM exists; delete table ~15-16× dict, grows quadratically | mmap; **no confirmed WASM of engine**; BHFST for ARM | server TPU; **no sub-5M-param WASM benchmark** | WASM/bounded is a **design target** (.pgpack) |
| **Rust/C** | C++; FFI + pure-Rust zspell | C++; **weakest** (CLI shell-out only) | **mature Rust**, no port needed | Rust (permissive parts reusable); CG GPL C++ = reimplement | candle/ort/burn (not spelling-specific) | **native Rust already** |
| **Hyper-minority verdict** | poor — enumerated wordlist, duplicates grammar in weaker lang | poor — enumerated wordlist + English-shaped phonetics | **not feasible** as core (enumeration wall); ok for bounded caches | works **IF a full FST exists** (its presupposition = our hard early problem); no corpus dep = good | not as generator; maybe reranker w/ synthetic errors | **designed for it** — reuses grammar, no corpus, generative |

## What PanGloss can do that the others structurally cannot

Ordered by how differentiating. "Shared with Divvun" flagged explicitly — Divvun is the
only peer that also has generative morphology, so the honest unique-vs-everyone set is
smaller than unique-vs-wordlist-spellers.

1. **Accept any inflected form without enumeration** (generative morphology). Beats
   Hunspell/Aspell/SymSpell outright (they hit the enumeration wall). *Shared with Divvun.*
2. **Grammar-derived phonological substitution cost** from natural-class **feature
   distance** (pg-featstruct `unif_closure`/`feature_lanes`), not English-shaped Metaphone
   (Aspell) or a generic auto-Levenshtein (Divvun). Per-language-correct by construction.
   *Unique vs. all five* (Divvun's error model is alphabet-generic, no feature model).
3. **Class-backoff LM that boosts UNSEEN wordforms** — P(class|context)·P(w|class) with
   the grammar supplying nonzero P(w|class) for unseen-but-valid forms (see 00-synthesis
   "Design ideas"). Requires analysis-bearing candidates — only possible because the FST
   emits every candidate WITH its analysis. *Unique vs. all five*: wordlist spellers can't
   analyze unseen words; Divvun has CG but no LM layer that licenses unseen forms by class;
   neural has no explicit classes and is data-hungry.
4. **Semantic-category / selectional detection** — LibLCM semantic domains + grammatical
   expectations ("slot expects an animate noun; token is inanimate → flag"). No peer has a
   semantic layer (Divvun has valency, not semantic domains). *Unique* (as a usable signal;
   report 04 warns the semantic-domain part is a weak factor — grammatical factors carry it).
5. **Synthetic error generation from the grammar** — turns "no error corpus" into a
   solvable problem for training/reranking. Neural *needs* this but can't generate from a
   grammar; PanGloss can sample its own HC/foma grammar. *Unique* (untested but ours to try).
6. **Free real-word-error confusion sets** — any two valid analyses one edit apart, straight
   out of the analyzer. *Unique* (falls out of machinery we already have).
7. **One unified weighted search** folding edit + Keyman-keyboard prior + phonological cost
   into a single ranked list — vs. Hunspell/Aspell incomparable-score cascades and Divvun's
   FST-then-CG stacking. Fixes the mixed-error-type + score-incomparability failure the
   incumbents document about themselves.
8. **Reuses the existing LibLCM/HC grammar as the data asset.** Divvun presupposes a
   decades-built FST; for PanGloss that same artifact is the project's existing input, so the
   min-data story is "the grammar you already maintain," not a new authoring project.

## Feasible vs. not-feasible for a hyper-minority language

(Few-hundred to few-thousand speakers, orthography possibly <10yr old, little/no corpus.)

**Feasible (and differentiating):**
- Non-word detection + correction via generative acceptance — *given the grammar exists*.
- Grammar-derived phonological + Keyman-keyboard-prior error model (no data needed).
- Class-backoff reranking of candidates on a minimal corpus (dense at the class level).
- CG-based real-word detection — *if* per-language CG rules are authored (real cost).
- Personal on-device learning (confusion model + cache-LM + personal wordlist).
- Synthetic-error-trained reranker (incl. a small neural reranker) — untested but viable.

**Not feasible / hard (be honest):**
- **Cross-user aggregation** for tiny communities — report 06's RAPPOR-math floor; Tier-2
  novel-item donation infeasible at hundreds of speakers.
- **Neural as a generator** — data crossover (~100k sentences) is above budget.
- **Word-level trigram LM** — type/token sparsity; nearly every trigram unseen.
- **Rich semantic-domain n-gram** — weak signal (report 04); at most a minor factor.
- **The hard gate:** everything above presupposes a HermitCrab/LibLCM **grammar exists**
  for the language. A language with no grammar gets nothing from this design — same
  presupposition Divvun makes, and the project's real early-stage bottleneck.
