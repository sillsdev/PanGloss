# Spell-checking plan (v2) — accreting

**Status:** this file holds the *decided* parts of the spell-checking design. It accretes as
decisions firm up out of the research (`00-synthesis.md` + reports 01-10). It will eventually
replace `docs/spell-checking-plan.md`, whose 5-phase delete-table design does not survive the
research — see `00-synthesis.md` § "The challenge to the original plan". That file is left in
place, unedited, until this one is complete enough to replace it.

Everything here is **design**. No code, no spikes.

> ## READ THIS BEFORE ANY NUMBER IN THIS FILE — D16, 2026-07-25
>
> **The four grammars and their texts are NOT representative and MUST NOT drive design.** They are
> small, incomplete sample projects. We have **no complete grammar or lexicon today**, so nothing
> here is calibration and nothing here is implementation-ready — **it is all research, plus plans
> for what to do once real data arrives.**
>
> Every measured number below (report 13's rung cardinalities, `mpr` emptiness, coverage rates,
> ambiguity distributions, corpus and gold-set sizes from report 18, **and report 27's completion
> containment, ranks and latencies**) is **research signal about four samples** — in report 27's case
> **two** — never a fact about grammars, languages, or what we should build. Where such a
> number has been allowed to narrow a design, that narrowing is **provisional and marked**. Design
> for full-scale data — see D16 for the rule and § "What data we need" for the requirements this
> plan is now written to produce.

| Decision | Status |
|---|---|
| **D1 — Factor sources: parse-determined only; semantic out of scope** | **DECIDED** (2026-07-24) |
| **D2 — Error model: the training pairs are synthesized by corrupting the grammar's own output** | **DECIDED** (2026-07-25, report 20); the composition weights remain untuned |
| **D3 — Constraint Grammar: deferred, not adopted; not required for the speller** | **DECIDED** (2026-07-24) |
| **D4 — Two-scale class n-gram is the ranking layer that ships** | **DECIDED** (2026-07-24) |
| **D5 — Neural reranker is a bounded late ablation, not the design** | **DECIDED** (2026-07-24) |
| D6 — Tokenization driven by the writing system's word-forming set | required, undesigned |
| D7 — Personal on-device overlays; no cross-user aggregation for small communities | direction settled |
| **D8 — Emit target: Keyman lexical model. NOT Divvun `.zhfst` — architecturally impossible** | **DECIDED** (2026-07-25) |
| **D8a — We ship the engine in a `custom-1.0` model; Keyman owns key-adjacency and the seen-word store** | **DECIDED** (2026-07-25); Keyman's user-dictionary epic is **unbuilt** |
| **D8b — We own the tier-0 cache in-worker; only *authored* words need a durable store** | **DECIDED** (2026-07-25); row added 2026-07-25 (report 21 finding #5 — the section existed, the index row did not); one open spike (`file:`-origin IndexedDB) with **no named fallback** — ledger row **C9** |
| **D9 — Tiered candidate supply; unseen forms allowed but ranked strictly below seen** | **DECIDED** (2026-07-24) |
| **D10 — Tier thresholds are per-grammar calibrated + on-device adaptive, never fixed constants** | **DECIDED** (2026-07-24); the calibration itself is unbuilt |
| **D11 — All accepting languages are kept; narrowing to one is an optimization, never a correctness step** | **DECIDED** (2026-07-24) |
| **D12 — Languages without a well-defined orthography are out of scope, explicitly deferred** | **DECIDED** (2026-07-25) |
| **D13 — The speller ships only for languages meeting the then-current certification bar** | **DECIDED** (2026-07-25); re-expressed as a principle 2026-07-25 for the multi-FST rewrite |
| **D14 — A warm cache ships with the language pack; runtime generation for uncached words is shelved** | **DECIDED** (2026-07-25); one reading assumed, flagged for correction; the shelved bucket **measured for the first time** 2026-07-30 (report 27) — see D14 § "Measured 2026-07-30" |
| **D15 — Layer boundary: this whole plan is a corpus-trained add-on, not part of the analyzer pack** | **DECIDED** (2026-07-25); the training corpus is now the top unknown |
| **D16 — The reference grammars are unrepresentative samples; this plan is research + plans, never calibration** | **DECIDED** (2026-07-25); **governs every other entry in this table** |
| **D17 — The deliverable is a two-column ledger: analytically eliminated vs. empirically deferred. Carrying 2-3 live candidates is success, not indecision** | **DECIDED** (2026-07-25); **governs the shape of every other entry** |
| **D18 — A cache miss is never grounds to flag. Flagging requires an attempted parse that failed** | **DECIDED** (2026-07-25, report 20); coupled to D14's challenged traffic model |
| **D19 — Prefix-constrained completion, if ever built, is a separate top-k entry point with no recall claim — never a mode of the proposer** | **DECIDED by invariant** (2026-07-30, report 27); a *constraint on* any future build, **not authorization** to build one |

---

## D1 — Factor sources: everything the parse determines is load-bearing; authored lexical semantics is out of scope

**Decided 2026-07-24.** This governs D4 (the class-backoff LM), D5 (whatever the reranker
consumes), and D3 (what CG rules may condition on).

### The criterion

A factor is **load-bearing** iff it is a **deterministic function of the parse** — the analysis
fixes it, with no additional authored data and no disambiguation step. Everything else is out of
scope for the speller.

This is not a new boundary. It is rung 1 of the ladder already ratified in
`docs/grammar-json-export-plan.md:45`:

> **Parser needs it** (`compile_project` consumes it) → required core. Objective test: strip it
> and the grammar no longer compiles/parses identically.

That test is what makes these factors trustworthy, and the reason is worth stating precisely:
**data the parser needs is guaranteed present, guaranteed consistent, and guaranteed maintained**,
because the grammar does not parse otherwise. Inflection class is authored per lexeme just as
semantic domain is — the difference is that a wrong inflection class breaks parsing, so it gets
fixed, whereas semantic domain is *inert* to parsing and nothing forces it to exist or be right.

Semantic domain sits at rung 3 of the same ladder — "only a dictionary needs it → out of scope;
LIFT/MiniLcm/Webonary territory" (`:48-50`). So the speller inherits an already-ratified
boundary instead of inventing one, and needs no schema change.

### The load-bearing set, grounded in the actual parse output

`WordAnalysis` (`rust/crates/pg-parse/src/lib.rs:25-44`) is, field for field, the deterministic
tier. Note what it does **not** contain: no sense, no gloss, no definition, no semantic domain.
The parse output already structurally excludes the thing we are ruling out.

| Field | Factor(s) it supplies |
|---|---|
| `morpheme_ids: Vec<u32>` | morpheme identity; the ordered decomposition; morpheme-sequence n-grams (the realized slot pattern); morpheme count |
| `root_morpheme_index: i32` | which morpheme is the root → prefix/suffix partition, affix counts on each side |
| `pos_id: Option<u32>` | lexical category (and its FLEx category hierarchy, which gives free coarser backoff rungs) |
| `syn_fs: FeatureStruct` | **the full morphosyntactic feature bundle** — tense, aspect, mood, person, number, case, gender/noun class, animacy where grammaticalized, definiteness. Unifiable, so subsets are principled backoff rungs rather than ad-hoc groupings |
| `mpr: MprSet` | MPR features (`pg-grammar/src/model.rs:120-125`) — a `u64` bitset. ~~≤6 members across the reference grammars. Low cardinality, therefore **dense even on tiny corpora**: a naturally good backoff rung~~ **WRONG — measured and refuted 2026-07-25, see report 13.** `mpr` is nonempty in **zero** confirmed analyses in Sena, Amharic and Indonesian; only Aweti populates it (38.5%). It is not reliably dense, it is reliably *empty*. Aweti also declares **9** `mpr_names`, exceeding the "≤6" claim |
| `guessed: bool` | **the unknown-root signal** — see below |
| `provenance`, `supplied_root` | whether the root came from the shipped lexicon or a runtime-supplied overlay → distinguishes "in the base lexicon" from "in this user's personal overlay" (feeds D7) |

Plus, from the grammar rather than the analysis, and equally deterministic:

- **Segment-level natural-class features** — `CharDefTable`'s feature lanes / unification closure.
  This is what the phonological substitution-cost matrix is derived from (report 02) instead of a
  hand-authored confusion table. Same criterion: the grammar needs it to parse.
- **Orthographic units** — multigraphs and PUA already defined as single collation units, and
  per-writing-system combining classes (`sil-primary-sources.md`). The edit unit is existing
  LibLCM data, not something to invent (report 01's correctness bug).
  **CAVEAT, verified 2026-07-24 `[M]`: that data exists in the FLEx project but PanGloss does
  not extract it.** `pg-fwdata` pulls only the space-separated writing-system *tags* from
  `CurVernWss`/`CurAnalysisWss` (`rust/crates/pg-fwdata/src/extract/project.rs:25-45`,
  `extract/mod.rs:38-40`) — no collation tailoring, no multigraph or PUA definitions, no
  per-writing-system combining classes. A repo-wide search for `ldml` / `WritingSystems/`
  returns **nothing** outside the spellcheck research docs, which is expected: writing-system
  definitions live in the project's `.ldml` files, not inside `.fwdata`, so this is a new
  *source*, not a missed field. Consequence: the orthographic edit unit, D6 (tokenization),
  and the multilingual script gate all currently have **no data source**. This is a
  prerequisite change, not a detail — see followup 18.

### `guessed` is a found asset — call it out

`WordAnalysis.guessed` is true exactly when the analysis came from the guess branch on a total
normal-lexicon miss. That is a **first-class unknown-word signal, already computed, per analysis**.

Divvun needs a separate executable for this (`divvun-cgspell`, which spells unknown forms and
injects them as new CG readings — `systems/divvun.md` § DETECTION). We get the discrimination for
free from the parser, which means the "unknown word" path and the "known word, wrong form" path
can be separated without a second component. This bears directly on the open question
*precision under overgeneration*: a guessed-root analysis is not evidence of correctness, and must
never be scored as if it were.

### Deterministic derived factors

All are pure functions of the fields above, so all are in scope by the criterion:

- Morpheme/affix counts; prefix vs. suffix counts; wordform length in orthographic units.
- Root identity and root frequency (frequency is corpus-derived but keyed on a deterministic id).
- Realized slot/template pattern as a sequence — the morphotactic shape independent of which
  specific roots filled it.
- Feature-bundle subsets via unification — the principled backoff rungs.
- Open- vs. closed-class, derivable from POS.

### Backoff ladder for D4 (first cut)

Report 04 identified choosing the factors and the backoff graph as *the* factored-LM design
problem, and warned that too fine re-sparsifies while too coarse loses predictive power. D1
fixes the candidate rungs; the ordering below is a starting hypothesis to be tuned empirically,
densest-last:

1. ~~full decomposition (morpheme sequence) + full `syn_fs`~~ — **DEAD, measured**
2. POS + full `syn_fs`
3. POS + a selected feature subset (the features that govern local agreement in this language)
4. ~~POS + `mpr` — ≤64 values, so reliably dense~~ — **DEAD on 3 of 4 grammars, measured**
5. POS alone, then its coarser FLEx hierarchy ancestors
6. open/closed class — the floor

> **PROVISIONAL — superseded in force by D16, 2026-07-25.** Everything in this subsection is an
> observation over **four small, incomplete sample projects**, not a finding about grammars. It may
> not shorten the ladder, retire a rung, or fix the rung-3 selection rule. **Build all six rungs**;
> let a per-grammar pre-flight pick the operating rung at install time (D10's existing pattern). The
> numbers stay because they are useful research signal and useful synthetic-profile targets — read
> "measured" throughout as "observed once, on a sample."

**Measured 2026-07-25 (report 13) — the ladder is shorter than designed** *on these four samples*:

- **Rung 1 has zero statistical power, universally.** 93.5%–100% of rung-1 classes are
  **singletons** across all four certified languages, independent of corpus size. A class with one
  member cannot be estimated. Rung 1 is decoration.
- **Rung 4 is byte-identical to rung 5** on Sena, Amharic and Indonesian, because `mpr` is never
  populated there. Only Aweti distinguishes them (18 vs 16 classes).

So the usable ladder ~~is~~ **was, on these samples, four rungs rather than six**: POS+`syn_fs` →
POS+feature-subset → POS → open/closed. Note rung 6 is also the least trustworthy number in report
13 — it has no backing field in the grammar data and was derived post-hoc, so the real floor may be
POS. **Per D16 this does not shorten the shipped ladder.** A complete grammar with a fully populated
feature inventory could give rung 1 real class sizes, and a linguist who uses MPR features would
populate rung 4. Both rungs stay in the design; whether they earn their keep is a per-grammar,
install-time question, and the corpus-size-versus-rung sweep is answerable on synthetic data now.

**The rung-3 selection rule changes: per-POS, not per-grammar.** The measurement found that
`syn_fs` richness concentrates on whichever POS category a grammar's authors happened to populate
`HeadFeatures` for — nouns in Sena, verbs in Amharic and Aweti. So an aggregate "is this grammar
feature-rich?" number is an artifact of which POS dominates the sampled corpus, and choosing one
feature subset per grammar averages across categories that differ. Language-specific rung selection
was expected; **category-specific selection was not, and is the actual requirement.**
**Softened by D16:** one observation across four samples makes this a strong *hypothesis*, not a
requirement. The design must permit per-POS selection; it must not assume per-POS is always right.

**D1's stated collapse risk is real but not universal.** Only Indonesian showed the total collapse
D1 feared — rungs 2-5 byte-identical, 0% `syn_fs` population. Sena, Amharic and Aweti all retain a
meaningfully finer rung 2/3 than rung 5, so the class-backoff LM has real signal on three of four.

### Out of scope

**Semantic domain — discarded, not parked.** Superseding the earlier "keep it as an optional
document-level topic prior" position in `00-synthesis.md`. Reasons, in order of force:

1. **Not parse-determined.** It hangs off a `LexSense`, reached by lemma → sense → domain — two
   lossy hops — and requires **word-sense disambiguation** to pick among a polysemous entry's
   competing domains. That is an unsolved problem sitting upstream of the one we are solving.
2. **Not guaranteed present.** Coverage is bimodal: rich if the project ran Rapid Word Collection,
   near-empty otherwise. A factor that is absent for most projects cannot be load-bearing, and one
   that is present for some invites a design that silently differs per project.
3. **Structurally invisible at the window that matters.** Semantic coherence is a document-level,
   long-distance phenomenon — Hirst & Budanitsky used whole-document lexical chains and still got
   F1≈0.26 (report 04). In a 3-word window there is no signal: "the tree fell yesterday" spans
   plant/motion/time and that is normal text. Grammatical features are the opposite — agreement,
   government, and morphotactic sequencing are local *by definition*, distance 1-3. Widening the
   window to serve semantics re-loses the sparsity war the class backoff just won.
4. **No positive precedent.** No paper builds an n-gram over semantic-domain tokens; FLEx's ~1,800
   categories reintroduce the sparsity the classing was meant to cure (report 04).
5. **It would require reopening a settled schema decision** — D5 of the export plan
   (`grammar-json-export-plan.md:71`) admits enrichment content only when a named deployment
   consumer needs it *to display a parse*. A speller factor is not that.

Also out, by the same criterion — authored lexical data that is inert to parsing:
**glosses, definitions, examples, pronunciations, etymologies, reversals**, and
**valency / subcategorization frames**.

Note the last one is a real, deliberate cost: valency is the one semantic-adjacent thing Divvun
*does* have (`valency.cg3`), and we are choosing not to use it in the first design. It is authored
per lexeme, inert to parsing, and therefore unguaranteed — the same failure mode as semantic
domain. If it later proves necessary for real-word-error detection it can be reconsidered **as a
CG rule input**, which is where Divvun puts it, not as an LM factor.

### Note on the earlier "it's growing → it's likely alive" example

That intuition was already about a **feature**, not a domain. Animacy is a grammatical feature in
every language that grammaticalizes it — Bantu noun class, Algonquian animate/inanimate gender,
Slavic animacy — and it lives in `syn_fs`. "tree → plant → alive" is a lexicographic taxonomy
encoding overlapping information through more hops, less reliably. The idea survives D1 intact;
only its data source changes.

### Consequences

- **D4 (class-backoff LM)** draws its classes exclusively from the table above. The backoff ladder
  is a real design deliverable, not a config flag.
- **D5 (reranker)** takes the same factors as input — which is what makes a tiny tag vocabulary
  possible (hundreds of tag types, not tens of thousands of subwords). D1 fixes the vocabulary.
- **D3 (CG)** rules may condition on parse-determined features only. This is *narrower* than
  Divvun, which allows valency tags — an accepted, stated cost.
- **No export-schema change is needed** for the speller. That removes a dependency on ratifying a
  schema exception, and it is a direct consequence of choosing this boundary.
- **`07-systems-comparison.md` overstates us** and needs re-cutting (followup #10): the honest
  differentiator vs. Divvun is feature-structure richness — their tags *are* the data model,
  whereas we have unification and natural classes above the FST — not semantics.

### To verify before building

- ~~The exact `syn_fs` feature inventory actually populated in real field grammars, per language.~~
  **MEASURED 2026-07-25 — report 13.** `syn_fs` beyond bare POS: Sena 30.99%, Amharic 85.33%,
  Aweti 45.27%, **Indonesian 0.00%**. The risk was real for exactly one of four grammars. The
  sharper finding is that richness concentrates **per POS category**, not per grammar — see D4's
  revised ladder.
- Whether `pos_id` exposes the FLEx category hierarchy or only a flat id — rung 5's coarser
  ancestors depend on it.
- Whether `morpheme_ids` distinguishes morpheme *type* (prefix/suffix/infix/circumfix/clitic)
  directly or only positionally via `root_morpheme_index`.

---

## D2 — The error model: synthesize training pairs by corrupting the grammar's own output

**Decided 2026-07-25**, on report 20's finding. This section existed only as a table row —
*"direction settled, not designed"* — for the entire life of the plan, while D4 § "The design: two
n-grams at two scales, linked by the whole-word analysis" composed both
of its terms into it. Report 20's verdict was **BROKEN**, and its most useful observation is that
**the answer was already written and never promoted**: `09-training-without-data.md` is a complete
report on exactly this problem.

### The problem, stated exactly

`error_cost` is the first term of D4's composition. Every published way of fitting one —
Kernighan/Church/Gale 1990, Brill & Moore 2000, Toutanova & Moore 2002 — **learns it from a corpus
of real spelling errors**, and we have none, will have none, and cannot collect one for a language
with a few hundred speakers before shipping. That is the actual constraint, and it is not a data-
collection problem to be scheduled; it is permanent for the first release of any language.

### The decision

**Generate the error corpus.** Sample the grammar's own confirmed generative output and perturb it,
then fit the error model on the resulting (corrupted, correct) pairs. Two layers, in order of
cost and evidence:

1. **Character-level corruption** — deletion, insertion, substitution, transposition over the
   orthographic edit unit (D6). Cheap, needs nothing but the grammar.
2. **Feature-level corruption** — perturb a morphosyntactic feature and keep the result only if it
   still parses, yielding structurally near-miss negatives. This is `09-training-without-data.md:193-231`'s
   PanGloss-specific extension of hard-negative sampling, and no competitor can do it, because it
   requires owning the generator.

### Why this is credible rather than a hope

> **⚠ CORRECTED 2026-07-25 by report 24, re-verified at primary source by the parent session.** The
> claim below was **misattributed and cherry-picked**, and it was the single most-touted number in
> this decision. The corrected version still supports D2 — but it must be quoted correctly, because
> this is the number anyone auditing D2 will check first.
>
> ~~**MAGEC** (Grundkiewicz & Junczys-Dowmunt 2019) built a system from **zero real error data** —
> confusion sets mined by inverting a spellchecker over clean text — and reached **~92% of a
> labeled-data sibling's score** on a real shared task.~~
>
> **What is actually true, verified `[A]`:**
>
> - **Two different papers were conflated.** The **64.24 / 69.47** figures behind "~92%" are not
>   MAGEC's. They belong to *Neural Grammatical Error Correction Systems with Unsupervised
>   Pre-training on Synthetic Data* — Grundkiewicz, Junczys-Dowmunt **& Heafield**, BEA 2019,
>   `W19-4427` — the BEA-2019 shared-task system description, which placed **first in both tracks**:
>   **64.24 F₀.₅ low-resource** and **69.47 F₀.₅ restricted**, both on W&I+LOCNESS test
>   `[A, aclanthology.org/W19-4427/, fetched and confirmed]`. The ratio is real (92.5%) and it is a
>   genuine low-resource-vs-full-data comparison — it is simply **not the MAGEC paper**, which is
>   Grundkiewicz & Junczys-Dowmunt, W-NUT 2019, `D19-5546` `[A, confirmed]`.
> - **"Zero real error data" overstates the setting.** The BEA-2019 low-resource track permitted the
>   W&I+LOCNESS **dev** set, which is real annotated data `[S, report 24 — not independently
>   confirmed by the parent session]`. The right phrase is *"without real error-annotated **training**
>   data"*, which is MAGEC's own wording.
> - **"~92%" is the best of three languages.** Report 24 reads MAGEC's own Table 4 as **75% (German),
>   77% (English), 92% (Russian)** `[S, not independently verified]`. English — the shared task's
>   language — is the 77%.
>
> **What survives, and it is still enough to carry D2.** MAGEC's abstract says verbatim that it
> *"outperform[s] the current state-of-the-art results for German and Russian GEC tasks by a large
> margin **without using any real error-annotated training data**"* `[A, D19-5546, confirmed]`. That
> is the claim D2 needs, at the correct paper, in the authors' own words. **The honest anchor is
> "75-92% of a labeled-data system depending on language, with the best available English figure
> around 77%" — not "~92%".** D2 is unchanged; its headline number is not.

- **MAGEC** (Grundkiewicz & Junczys-Dowmunt, W-NUT 2019, `D19-5546`) built a system from **no real
  error-annotated training data** — confusion sets mined by inverting a spellchecker over clean text
  — and beat the then-SOTA for German and Russian `[A]`. Synthetic-error systems in this family
  reach roughly **75-92% of a full-labeled-data system's score depending on language** (see the
  correction box above for the exact provenance of each figure). That range, not the 92% endpoint,
  is the anchor for what D2 can be expected to buy.
- **Zarma** — a genuinely small West African language built from nothing — used synthetic
  deletion/insertion/substitution/transposition noise, four corrupted variants per sentence,
  250k+ examples, and a **non-neural** Levenshtein+Bloom-filter baseline **beat the neural model
  outright on exactly the error class a speller cares about** (100%/96.27% vs 95.82%/78.90%)
  `[A, 09-training-without-data.md:119-133]`. Both halves of that result support this plan: the
  synthetic-corruption method, and the classical-ranker choice of D4.

### Live candidates, per D17

D2 is a **leading candidate**, not an architectural necessity. The ledger row:

| Candidate | Status | What would decide it |
|---|---|---|
| **A. Grammar-derived synthetic corruption** (above) | leading | recall@k of the generator on held-out real typos, once any exist |
| **B. A generic weighted-Levenshtein model** with weights from keyboard adjacency only, no corpus | live — it is the honest floor, and cheap | if A cannot beat B, A's complexity is unjustified |
| **C. Transfer an error model fitted on a related, better-resourced language** | live, unexplored | whether error patterns are orthography-driven (transferable) or language-driven (not) |

**B is the baseline A must beat**, and the plan has no result showing it does. Note the known
accuracy cost of the whole family versus a genuinely learned model is real but bounded — MAGEC's
~92% is the best available anchor — and no controlled study isolates the transfer gap for *spelling*
specifically (`09-training-without-data.md:135-163`, an explicit unfilled gap).

### What remains open

**The sampling weight is unspecified, and it is the same unsolved problem D14 names one section
over.** Added 2026-07-25 (report 23, T3). D2 says *"sample the grammar's own confirmed generative
output and perturb it"* — and never says **how the sample is drawn.** A grammar's generative output
is not a distribution over what people type: it is unbounded, and uniform sampling over it is
dominated by long, rare, morphologically elaborate forms. Fit an error model on that and the error
model learns the typo behaviour of a population that does not exist.

This is *exactly* the defect D14 already names for the warm cache — *"frequency for a generated
entry is not observed"* — and the plan solves it there (rank by D4's class model) while leaving it
unsolved and unmentioned here. **The two are one problem and should have one answer.** Worse, the
answer D14 uses is the one F18 shows is biased against complex forms, so importing it wholesale
imports the bias into the error model too. Recorded as an open item on D2, not a defeater: the
candidates are (a) weight by D4's class model and accept F18's bias, (b) weight by observed
*character-level* frequency from any running text, which needs no analysis and is therefore immune
to coverage bias, or (c) sample uniformly and accept the mismatch, with the mismatch measured.
**(b) is the interesting one and appears nowhere in the plan.**

The **weights** `w_err`, `w_inter`, `w_intra` are untouched by this decision. D4 § "Interpolation
weights" owns them, and report 22 **has since audited** whether the proposed grid search over ~147 gold tokens
is a valid procedure at all.

---

## D4 — The ranking layer that ships: a two-scale class n-gram

**Decided 2026-07-24.** This is the ranking layer. D5 (anything neural) is an ablation
measured against it, not a successor assumed to replace it.

### Why an n-gram and not a learned ranker

Every measured comparison found across reports 08 and 09 — four unrelated task families,
no exceptions — says a tuned classical model wins at our data scale:

| Evidence | Result |
|---|---|
| Filipino spelling normalization, 300 samples (09) | n-gram + edit distance **77%** acc@1 vs ByT5 **31%** |
| Zarma GEC (09) | rule-based baseline wins outright on spelling-class errors |
| GBDT vs. neural nets, 176 tabular datasets (09) | "the debate is overemphasized — tune the simple model first" |
| gzip+kNN vs. BERT, low-resource text classification (09) | classical wins few-shot |
| Neural morphological disambiguation (08) | every measured result trains on 300K-1.8M gold tokens; the one architecture proven at our ceiling (100K, LEMMING) is a linear-chain CRF |

Report 04 separately established that modified Kneser-Ney wins at **every** training size
tested (Chen & Goodman 1999), so the smoothing formula is not the thing to redesign — the
token is. And report 09's re-read of the Filipino paper shows its winning system was itself
a generate-then-rank pipeline with **both stages non-neural** — direct architectural
validation for exactly this.

### The design: two n-grams at two scales, linked by the whole-word analysis

The standing class-backoff formula (`00-synthesis.md`) had a hole in it:

    P(w | context) ~= P(class(w) | context) * P(w | class(w))

The first term was well-specified. The second was hand-waved as "the grammar supplies a
nonzero value." **It needs real content, and the two-scale decomposition supplies it.**

**Inter-word (cross-word) term — `P(class(w) | context)`.** A trigram over *whole-word
analyses*: the class of this word given the classes of the preceding words. Dense on tiny
corpora because a class is shared by thousands of wordforms. Backoff ladder per D1
(decomposition+`syn_fs` -> POS+`syn_fs` -> POS+feature subset -> POS+`mpr` -> POS ->
open/closed).

**Intra-word term — `P(w | class(w))`.** An n-gram over the *morpheme sequence within the
word*, conditioned on the whole-word analysis. This is the "for each segment, weight the
morphology of the whole word" idea, and it is what makes `P(w | class)` estimable rather
than assumed.

Note carefully what this term does and does not do. The morphotactics **already** decide
legality — the FST will not produce an illegal morpheme sequence, so an intra-word n-gram
adds nothing on that axis. What it adds is **probability over legal forms**: of the many
well-formed realizations of a given class, some are far commoner than others, and nothing
in the FST knows that. That is a distinct job from the FST's, not a duplicate of it.

**Composition.** Both terms enter the same unified weighted composition as the error-model
cost (D2), as additive log-space terms with tuned weights:

    score(candidate) = w_err   * error_cost
                     + w_inter * log P(class | context)
                     + w_intra * log P(morphemes | class)

Which mirrors divvunspell's own three-term shape (`lex + mut + rew`, `systems/divvun.md`) —
a precedent that already works in production, including its positional reweighting.

### Why this handles unseen wordforms — the thing no competitor can do

For any other speller `P(w | class)` is zero for a form never seen, which is what kills them
on agglutinative languages. Here both terms are computable for a form that has **never
appeared in any corpus**, because the candidate arrives with its analysis attached and the
intra-word term is estimated over *morphemes*, which recur even when wordforms do not. This
is the mechanism behind the original intuition ("all the words that follow this n-gram are
future-tense verbs of this class — boost those, even unseen").

### Consequences and open items

- **Estimated from morpheme/tag tokens, never word tokens** — report 04's core correction.
  Finnish: 20% OOV at word level -> 0% at morph level, WER 56%->32%.
- **The factored-LM engine is still MUST-PORT** (report 10: 4-8 person-weeks for a fixed
  backoff graph; 8-16 if the graph is itself searched). SRILM's FLM module is dead; KenLM
  dropped factors. **Start with a fixed, hand-chosen backoff graph** — the searched-graph
  version is the doubling, and is not needed to ship.
- **Ambiguity is marginalized, not resolved.** Context words carry multiple analyses. The
  n-gram scores over the analysis lattice (summing over context analyses weighted by their
  own scores) rather than requiring a hard disambiguation pass first. This is standard
  lattice LM scoring, and it is the reason D3 (CG) is not a prerequisite — see D3.

  > **Warning added 2026-07-30 (report 27) — the neighbouring operation that looks identical and is
  > not.** Marginalising over the analyses of the **context** (this bullet) is standard and stands
  > unchanged. Marginalising over the analyses of the **candidate being scored** was measured and is
  > **harmful**: summing a candidate's probability over every analysis that yields it rewards
  > analysis *multiplicity*, so a junk surface reachable 50 ways outranks a real word reachable
  > twice. Switching to the single best analysis moved the true word from rank 114 to **rank 1** and
  > top-3 acceptance from 0% to **100%** on one grammar, changing nothing else `[M]`. The two
  > operations are one word apart in prose and opposite in effect, so **state which one is meant
  > wherever a score is specified.** Per D16 this is one sample and elimination-shaped, not a
  > settled default — ledger row **C15**, sweep **N12**.
- **Interpolation weights** are tuned by low-dimensional grid search on the small gold set
  (report 09's evaluation apparatus), defaulting conservatively toward the error-model term.
  **Premise corrected 2026-07-25 `[M]` (report 18, re-verified):** the gold set for *contextual*
  tuning is **~147 tokens, not ~760**. The 760 `WfiAnalysis` records are type-level; only 147
  Sena text positions carry a disambiguated reading. Grid search over 3-5 scalars on 147 tokens
  is thin but not hopeless — report 16 argues it stays defensible precisely because the
  dimensionality is tiny. Two mitigations, in order of cost: **(a)** project type-level analyses
  onto tokens wherever a wordform type has exactly one analysis, yielding a free silver set of
  unmeasured size — **the cheapest measurement now available**; **(b)** hand-annotate, which
  §"Two mitigations" already contemplates at 50-400 sentences. Do (a) before costing (b).
  **Per D16, do not design around 147.** That is what one incomplete sample happens to hold; a
  properly interlinearized project has orders of magnitude more. The durable output is the
  *requirement* — how large a gold set must be before grid-searched weights stop moving — which is
  measurable on synthetic data today. Recorded in § "What data we need".
- **Open:** which feature subset forms rung 3 is language-specific and must be chosen per
  grammar. Related risk carried from D1: if real field grammars carry thin `syn_fs`, the
  upper rungs collapse and the inter-word term weakens.

---

## D5 — Anything neural is a bounded late ablation, not the design

**Decided 2026-07-24.** Supersedes the framing of "a mini-transformer plan": the research
does not support building one first, and D4 is the thing that ships.

**What the research actually returned.** The idea is *feasible* — report 10's verdict was
"yes-with-conditions" on the WASM envelope, and the tag-vocabulary arithmetic is genuinely
favourable (embedding + output projection falls from ~31-47% of parameters with a real
subword vocabulary to under 1% at ~10M params with ~300 tags; sub-2.5MB after INT8;
`ternlight` calibrates a comparable in-browser encoder at ~2.5-5ms/forward-pass).
Feasibility was never the blocker. **Expected value at our data scale is.**

**Why it is not the design:**
1. Report 08: no neural morphological disambiguator has a measured result inside our data
   range; the architecture proven at our ceiling is a linear-chain CRF.
2. Report 08: reranking's proven gain ceiling is small even in ideal conditions — the best
   documented GEC transformer reranker (on a 248M-param T5 with 10.5B tokens of extra
   pretraining) bought **0.36-0.91 F0.5**, while a context-blind variant of the same
   architecture lost 9.75-18.25.
3. Report 09: **no controlled experiment anywhere** quantifies how much less data a reranker
   needs than a generator. The bounded-hypothesis-space argument is sound and is why
   reranking is the field-wide default, but it is architecture-motivated reasoning, not a
   measured learning curve. Whoever builds this measures it first, for this task shape.

**If it is built, the shape is CRF-first.** Report 08's recommendation: a CRF-style listwise
scorer over grammar-supplied candidates (LEMMING/MarMoT-shaped) — a log-linear scorer over
templated features of each candidate's analysis crossed with context, normalized across the
small candidate set. This consumes exactly D1's factor set and needs **no tag-embedding
vocabulary at all**. A transformer, if prototyped, is a bounded ablation (2 layers, 2-4
heads, 64-128 dim, listwise softmax, no positional encoding over unordered feature bundles)
measured against the CRF, not assumed to beat it.

**The bar it must clear**, per report 09's minimum evaluation apparatus — all four required
*before* any reranker code:
1. **recall@k of the candidate generator alone.** Buildable now, before any ranker exists;
   tells you whether ranking even has a solvable problem for a given language.
2. A **50-400 sentence hand-annotated gold set** per language, reserved for final evaluation
   and lambda sanity-checking only.
3. A **held-out-from-synthetic dev split**, documented as biased, never the shipping number.
4. **D4 measured on the same split first.** Beating it is the bar for shipping a neural
   component at all — not a formality on the way to shipping one regardless.

**Synthetic error generation** (report 09) is real and buildable, and is the enabling move
for any learned component. Closest precedent: MAGEC's zero-real-error-data system reached
~92% of its labelled-data sibling's F0.5 (64.24 vs 69.47) `[A, snippet-level]`. Generating
negatives by perturbing a correct analysis into a nearby wrong one is unattested for
morphology but well-precedented in semantic-parsing/SQL rerankers. Note this benefits D4
too — it is not neural-specific.

**Latency risk to measure first if built** (report 10): nobody has measured WASM
bounds-checking overhead on `gemm`-heavy inference at this shape (small `d_model` 64-320,
short sequences, small batch), which is exactly the regime where fixed per-call overhead
fails to amortize. The one figure found (20-220%) is worst-case on `gemm`.

---

## D3 — Constraint Grammar: deferred, not adopted; not required for the speller

**Decided 2026-07-24.** CG stays on the map as the eventual grammar-checker tier. It is
**not** a prerequisite for spell-checking and is **not** on the critical path.

### What CG is

A rule formalism for *reductive* disambiguation (Karlsson 1990; CG-2 Tapanainen; engine
`vislcg3`). The unit is a **cohort** — one token plus every reading the analyzer produced.
The analyzer overgenerates deliberately; hand-written rules delete readings that context
rules out, then a second rule layer *adds* error tags rather than removing readings:

    REMOVE (V Imprt) IF (-1 (N Nom)) ;
    SELECT (V Ind Prs Sg3) IF (-1 (Pron Sg3)) ;
    ADD (&err-agreement) (N Sg) IF (-1 (Det Pl)) ;

Divvun runs `vislcg3` via `libdivvun`, downstream of the FST — a separate rule engine, not
fused into the weighted composition (`systems/divvun.md` section ARCH).

### What it buys — three separable jobs, with different answers

1. **Disambiguation** (pick among competing analyses). **Not needed for the speller.** D4
   marginalizes over the analysis lattice instead of resolving it, which also dodges the
   circularity problem (context must be analyzed to score, but context may contain the
   error). Resolution is only needed if something downstream requires a single reading.
2. **Real-word error detection** (a correctly-spelled word used wrongly — agreement, case).
   **This is what CG uniquely buys**, and it is a genuine capability gap without it.
3. **Precision under overgeneration** (filtering analyses the morphology licenses but
   context forbids). Partially addressed more cheaply: `WordAnalysis.guessed` already
   separates guessed-root analyses from lexicon-backed ones (D1), which is the largest
   single source of spurious acceptance.

**Decisive point:** a Divvun-style *speller* needs only acceptor + error model. CG is the
*grammar-checker* tier, separately authored and optional (`systems/divvun.md` section
DATA_REQ). Divvun ships working spellers with zero CG investment. So do we.

### The evidence for it, and its limits

Report 08 found the head-to-head the series had been missing — Chanod & Tapanainen 1995,
same analyzer, same task, one month of development given to *each* side:

| Test | HMM error | CG error |
|---|---|---|
| Clean newspaper (255 sentences) | 3.2% | **1.3%** |
| Noisy, typos + lexicon mismatches (12k words) | 5.0% | **2.5%** |

From **75 rules and 50 example sentences, no training corpus**. Their combination experiment
(CG first, HMM breaking residual ties) was **worse than CG alone**.

Hold this loosely, for three reasons: it is 1995 French POS tagging, not candidate ranking;
a Basque result went the **opposite** way (CG+HMM 14%->3.5%), an unresolved contradiction in
the literature; and **no CG-vs-neural comparison exists anywhere**.

### Alternatives to CG, per job

- **Job 1** — lattice marginalization (D4, chosen); or a CRF tagger (LEMMING, proven at
  100K tokens); or an HMM (measured to lose to CG in the one direct comparison).
- **Job 2** — free confusion sets from the analyzer (any two valid analyses one edit apart,
  report 04) scored by D4's inter-word term. Weaker than CG rules but costs nothing and
  needs no rule authoring. This is the fallback if CG never arrives.
- **Job 3** — `guessed`, plus error-model cost thresholds.

### Can the rules be auto-derived from LibLCM? Mostly no — and the reason is structural

Split the rules by what they constrain:

- **Intra-word constraints** (which affix follows which, which feature combinations are
  legal). Derivable from LibLCM in principle — but **pointless**: HermitCrab already
  enforces them, so the FST never emits an analysis violating them. Auto-generated rules
  here would be rules that can never fire.
- **Cross-word constraints** (agreement, government, case selection). These are what CG is
  *for* — and **LibLCM has no syntactic layer to derive them from.** FLEx models lexicon,
  morphology, and phonology; it does not model syntax. There are no dependency relations,
  no phrase structure, no agreement rules between words.

**So the rules that matter cannot be auto-derived from the grammar, because the data that
would license them does not exist in FLEx.** This is a structural fact about the source, not
a tooling gap, and it will not be fixed by better extraction.

Two things that *are* available instead:
1. **Learn cross-word regularities from interlinear text rather than derive them from the
   grammar** — which produces a statistical model, i.e. **D4**, not CG rules. This is an
   independent argument that the class n-gram is the right first move.
2. **Auto-generate rule scaffolding** — enumerate the agreement-bearing features present in
   a grammar and emit stub rules for a linguist to fill in. Scaffolding, not automation;
   modest value, low cost, deferred.

### Found asset: FLEx interlinear text is a gold-annotated corpus we are not importing

Field linguists' primary output is **interlinearized text** — human-approved analyses per
wordform. That is gold annotation: the scarcest resource in this entire problem, and exactly
what report 08 said neural disambiguators need 300K-1.8M tokens of (we will have far less,
but it is free and it is gold).

`docs/fwdata-import-plan.md:81` excludes it: the snapshot carries "only parser-relevant data
(**no texts, wordform analyses**, semantic domains, styles...)". So it exists upstream in
`.fwdata` and is simply not imported today.

**This does not conflict with D1.** D1 governs which *factors a model may condition on*;
this is a *corpus to estimate from* and a *gold set to evaluate against*. Different question,
different ladder. It is also categorically unlike semantic domain: not authored-but-inert,
but human-verified ground truth.

> **MEASURED AND HALF-REFUTED, 2026-07-25 `[M]`** — report 18, re-verified independently by the
> parent session against the raw `.fwdata`. The asset is real but it is **not** the asset this
> section describes, and the two halves separate cleanly:
>
> - **As a corpus to estimate from: confirmed, and bigger than anything we had.** `Sena 3.fwdata`
>   carries 4,487 `Segment` records and **31,682 word tokens** of sentence-segmented running text
>   (40,255 analysis slots less 8,573 `PunctuationForm`). That is ~4.5x the 6,973-form type list
>   and it is the first running text this project has had access to at all.
> - **As a gold set to evaluate against: refuted for Sena.** Only **147 of those 31,682 word
>   tokens (0.46%)** carry a token-level link to a disambiguated reading. The 760 `WfiAnalysis`
>   records this plan quotes elsewhere are owned by wordform **types**, not anchored at text
>   positions. "Human-approved analyses per wordform" is true of the *lexicon*, not of the text.
>   `aweti.fwdata` is the inverse — 87.9% of its word tokens are glossed, but there are only 555
>   of them.
> - `Sena_InterlinearTraining.fwdata` is a near-duplicate of `Sena 3` (+114 word tokens, 66
>   analyses against 760), not a second corpus. De-duplicate; do not add.
>
> Per D15, training needs raw text plus the analyzer and **not** gold annotation — so the
> confirmed half is the half that matters for D4. The refuted half lands on evaluation, and is
> recorded against D4's weight-tuning premise below. Still not extracted:
> `rust/crates/pg-fwdata/src/xml.rs:1-6` names `WfiWordform`/`StText` among the classes skipped
> without ever being parsed.

**Action:** scope what it would take to import texts + wordform analyses as a separate,
optional artifact (not into the parser snapshot). It feeds D4's estimation, report 09's
evaluation apparatus items 1-2, and any future CRF. Recorded as a followup, not yet decided.

### The engine question is a licensing question, not an engineering one

Report 10 resolved the long-standing open question: **`github.com/divvun/cg3-rs` (crate
`cg3`) exists** — a Rust port of VISL CG-3 by a Divvun engineer, first commit 2026-07-12,
v0.2.0 released 2026-07-20, claiming full grammar-source and `.cg3b` binary-ABI
compatibility, `unsafe_code = "forbid"`. Verified independently against the crates.io API.

**License: `GPL-3.0-or-later`. PanGloss is MIT** (`LICENSE`; `rust/Cargo.toml:29`).

Finding a Rust port **does not change the licensing math** — FFI-wrapping the C++ `cg3` had
the identical problem. It changes only the engineering math. And the constraint binds hardest
exactly where we need it: native deployment could plausibly ship CG-3 as a separate
GPL-licensed process over IPC, but **WASM has no separate-process model** — everything links
into one module.

**Caution on the effort estimate.** Report 10 gives 8-14 person-weeks for a from-scratch MIT
engine, while noting `cg3-rs` itself was apparently built in ~9 days for a *larger* scope.
These do not reconcile until you read `cg3-rs`'s own README: "literal, bug-for-bug 1:1
translation." That is **transpilation of the C++**, which explains the speed and **does not
transfer to us** — a line-by-line translation of GPL source into an MIT codebase is a
derivative work. The 8-14 week estimate stands, and it stands for a licensing reason, not an
engineering one.

`cg3-rs` remains legitimately useful as a **behavioural reference and validation target**.
Its shipped upstream conformance corpus is test *data* — a separate licensing question from
the code, worth an actual check rather than an assumption.

**Decision:** defer. Ship the speller without CG (D4 + D2). Revisit when the grammar-checker
tier is actually wanted, at which point the first question is legal, not technical: can we
use `cg3-rs`, negotiate a licence with Divvun (a partnership conversation already worth
having per the Divvun section of `00-synthesis.md`), or must we build MIT-from-scratch?

---

## D9 — Tiered candidate supply: unseen forms are allowed, and ranked strictly below seen forms

**Decided 2026-07-24.** Resolves the contradiction between "propose an animate noun of this
class — I see the stem for *squirrel*, build it out" and "really, they need to have seen that
exact word before." The answer is **both, tiered**: unseen forms are supplied, but they can
never outrank a form the user has actually typed.

**Prediction and correction are one machine, not two designs.** Next-word prediction,
completion of a partially-typed word, and spelling correction differ only in how wide the
candidate net is opened and what the prefix constraint is. This is a single pipeline with a
tier policy, not three products.

### The tiers

| Tier | Candidate source | Rough cost | When it runs |
|---|---|---|---|
| 0 | **Cache of words SEEN** — typed by this user, or present in this document; persisted across sessions | sub-10ms, hash lookup | always, emitted immediately |
| 1 | Lexicon stems + **grammar-generated inflections**, prefix-constrained | tens of ms | always, refines tier 0 (frequency governed by D10) |
| 2 | **Error-tolerant** generation — the typed prefix may itself be misspelled | 100ms+ | only when tiers 0-1 come up thin, or on an explicit correction request |

**Amended 2026-07-25 by D14.** Tier 0 is no longer a cold cache — a ~10k-entry warm cache ships in
the language pack. Tiers 1 and 2 are **shelved at runtime**: generation moves to pack-build time,
where it fills the warm cache offline. Error tolerance *over the finite cache* is not shelved — it
is the second-largest traffic bucket. Read the table below as the tier *architecture*, which D14
does not repeal; read D14 for which parts execute on a keystroke.

~~**The cache is of words seen, never of words constructible.**~~ **REPEALED 2026-07-25 by D14**
(review-campaign finding P0-1; this sentence was the last unamended statement of the pre-D14
design and it contradicted D14 outright). The *termination* argument it carried is correct and
survives: the wordform inventory is unbounded (10^4-10^8 per stem), so any design that
materializes it does not terminate. **What D14 changed is that the cache no longer has to be of
words seen** — it is a *budgeted sample* of constructible words, generated offline, which
terminates precisely because it is budgeted rather than exhaustive. See D14 § "Why build-time
generation is safe here". A cache covers the Zipf head; tiers 1-2 cover the tail, generatively,
on demand. Tier 2 results are cached **keyed on the observed error string**, so a repeated typo
is a tier-0 hit the second time.

### The ranking rule

> **AMENDED 2026-07-25 by D14 item 2 — read that first.** The binary rule stated below is
> superseded: there are now **three** populations (typed-by-this-user > shipped-warm-cache >
> generated-on-demand), and the large fixed penalty belongs between rungs 2 and 3, not between
> "seen" and "unseen". The text below is retained for its rationale, which is unchanged.

Unseen forms carry a **large fixed penalty** — a constant, not a learned weight — so a
grammar-generated form never outranks a form the user has typed, regardless of what the
n-gram thinks. Rationale: at 50k tokens *everything* is rare, so a learned penalty would be
estimated from the same starved data it is supposed to correct for. Hard-code the ordering
and let D4's terms rank *within* a tier.

### Tiers govern supply, never flagging

A tier is a statement about where candidates came from and what they cost — **never** about
whether anything is an error. This makes "unlikely != wrong" structural rather than a tuning
guideline: no code path turns a low LM score or an empty cache into a diagnostic. This is the
same argument that rules out LM-threshold detection — in a 50k-token language,
correct-but-rare text is the norm, so a probability threshold flags correct text constantly.
That is the data regime, not a knob.

### Consequences

> **AMENDED 2026-07-25 by D14.** The first bullet's *mechanism* is superseded — D14 shelves
> runtime tiers 1-2 and relocates zero-count generation to pack-build time. The **conclusion**
> (D4's intra-word term is load-bearing) survives, and in fact hardens: the warm cache is
> generated, so nothing in it has an observed frequency, and D4's class model is the only thing
> that can rank it. What changes is *when* it runs, not *whether* it is needed.

- **D4's intra-word term earns its keep.** Tiers 1-2 emit forms with zero corpus count, and
  `P(w | class)` is the only thing that can rank them. Had this gone seen-only, D4 would
  collapse to a one-scale surface-word n-gram and the morphology would be decoration for
  prediction — a real risk that was on the table and is now closed.
- **Tier 1 keeps the grammar load-bearing** — it is exactly where "I see the stem, build out
  the inflected form" happens, which no cache and no surface n-gram can do.
- **Tier 2 is the latency risk, and it cannot be trie-pruned.** Error-tolerant prefix search
  must not treat the typed prefix as a hard filter — tolerating an error *in the prefix* is
  the point — so it is Oflazer-style error-tolerant FST traversal, not a lookup. Report 01's
  calibration point: 10-45ms over 200k Turkish wordforms on 1996 hardware `[A]`.
- **Anytime contract.** Tier 0 is emitted immediately and refined; a slow tier 1/2 degrades
  the result set, never the response. Partial output beats correct-but-late.

---

## D10 — Tier thresholds are per-grammar calibrated and on-device adaptive, never fixed constants

**Decided 2026-07-24** (John): *"the decisions about Tier-1 and Tier-2 are really about how
fast PanGloss is for that specific language. We likely need some form of calibration and
adaptation based upon the language, how much is typed, etc. This will be something designed
empirically (or synthetic-empirically — or through searching for studies on how to optimally
do this)."*

So "does tier 1 run on every keystroke or only on a thin cache?" is **not** a design constant
to be chosen in this document. It is a calibrated policy, and the deliverable is the
calibration mechanism plus a conservative default.

> **AMENDED 2026-07-25 by D14, and re-widened the same day by the report-20 challenge. Read this
> before building anything in D10.** D14 narrowed D10 sharply — with tiers 1-2 shelved at runtime
> there is no tier-2 invocation threshold to calibrate and no anytime refinement to schedule, and
> report 11's "value-of-continuing estimate" finding parks with tier 2. **Everything below this
> banner describing runtime tier-1/tier-2 threshold calibration is written against the pre-D14
> design.** But D14's own traffic model is now challenged (see D14's ⚠ box), and the resolution —
> ledger row **C4** — makes *whether generation runs at all* a per-grammar calibrated choice.
> **That is D10's job, and it is the biggest one in the document.** D10's post-D14 scope is
> therefore: (i) choose the runtime-generation operating point per grammar from a measured
> uncached-token rate; (ii) size the warm cache per grammar; (iii) if generation is on for a
> grammar, the pre-D14 tier-threshold material below becomes live again for that grammar. Do not
> read the narrowing as a deletion.

### This is already the repo's standing policy, not a new idea

- `docs/fst-plan/foma-fst-plan.md:526-528` — paths are "chosen per grammar by a measured
  **pre-flight determination** (`composite_scale_hint` pattern), **never by language
  identity**."
- `docs/fst-plan/synthetic-stress-grammar-plan.md:20-24` — "*measure, don't guess*: every
  grammar gets a cheap pre-flight census, strategy selection happens per grammar, and
  all-strategies-explode is an honest typed error, never an OOM."

The tier policy is the same shape one layer up: per-grammar measurement decides strategy, and
running out of budget is an honest degraded mode rather than a hang.

### Two distinct knobs — do not conflate them

1. **Per-grammar calibration (build/install time).** Measure this grammar's actual tier-1 and
   tier-2 cost, then set that grammar's tier policy and candidate budgets. Ships as data in
   the language pack, not as constants in the binary.
2. **Runtime adaptation (per user, per session, on device).** Adjust within the calibrated
   envelope from observed behaviour — typing speed, how much has been typed, suggestion
   accept rate, cache hit rate, and the device's measured throughput. This is what "based
   upon the language, how much is typed" asks for, and it is also where D7's personal overlay
   lives.

### The known trap: cheap static predictors have already failed here once

`docs/fst-plan/morphotactic-composite-pruning.md:74-77` records that `composite_scale_hint`
(should_run / candidate-rule-count / root-count) **did not predict Aweti's explosion — Aweti
looked ordinary on all three signals.** Consequence for this decision: tier policy must be set
from **measured** cost on the actual grammar, never inferred from grammar statistics, and never
from language family. Budget for the measurement; do not look for a formula.

### Harness: reuse, do not invent

`openspec/changes/calibrate-fst-resource-envelopes/` already defines the shape — sweep the
vectors, binary-search each one's safe cliff in supervised child processes, record elapsed
time and sampled peak RSS and artifact size, then **version conservative defaults from
measured evidence**. The speller's latency calibration should be another consumer of that
harness rather than a parallel one. And per `docs/fst-plan/synthetic-stress-grammar-plan.md`,
find the cliffs on **synthetic** grammars we generated, not on a linguist's machine.

### What must NOT be adaptive

Recall. Tier policy may trade latency for candidate-set size; it may never silently drop a
correctness guarantee. If every tier blows its budget, the honest outcome is a **stated**
degraded mode (tier 0 only, said out loud) — never an unbounded hang and never a silent
recall loss. Same rule as all-strategies-explode.

### Settled by the literature search (report 11, run 2026-07-24)

The search John named as a valid route has been run — `11-latency-policy.md`. Four findings are
adopted here:

1. **The tier design is already an *interruptible anytime algorithm*** in Zilberstein & Russell's
   precise technical sense, not by analogy: tier 0 always holds a valid answer and tiers 1-2 only
   refine it. Promoted from open question to a **stated property of the design**. It also
   dissolves an apparent tension — "recall is never adaptive" and the anytime framing are the same
   idea, since every partial state is an honest incomplete answer and full recall is simply what
   running to completion means.
2. **Tier-2 invocation must be a value-of-continuing estimate, not a confidence threshold.** The
   early-exit literature's own recent self-correction (*Rethinking Calibration for Early-Exit
   Neural Networks*, arXiv:2508.21495 — abstract verified directly) finds calibration alone
   insufficient precisely because it says nothing about the *cost of continuing*; Hansen &
   Zilberstein's older `V_c` formulation reaches the same shape independently. So "when does tier 2
   run" is specified as expected quality gain weighed against the cost of running it.
3. **Selective-classification / risk-coverage schemes are ruled out as the exit mechanism.** Their
   standard form trades accuracy for coverage — exactly what this decision forbids. If a future
   tier ever needs to *prune* rather than only *add*, the safe pattern is the WAND-style
   score-upper-bound one: prune only when a computed bound proves the candidate cannot matter,
   never on a probabilistic score.
4. **The percentile is p90, single-stream**, by explicit analogy to MLPerf Mobile's interactive
   scenario. Stated as a convention we align with, not an optimum the literature proves — nothing
   does. This answers the percentile half of followup 16.
   **Refined 2026-07-25 by report 12:** Keyman does not override this, it *complements* it with a
   narrower, host-enforced number — `DEFAULT_ALLOTTED_CORRECTION_TIME_INTERVAL = 33` ms
   (`correction/distance-modeler.ts:665`). Crucially that budget scopes only to Keyman's own
   correction search over `LexiconTraversal`; a model that does its work in `predict()` has **no
   host-enforced timeout at all**. So p90 survives as our own target, and 33 ms becomes a hard
   constraint on one specific path — see D8's `traverseFromRoot` fork.

The report also upgrades the harness reuse: `calibrate-fst-resource-envelopes`'s
sweep-and-binary-search-the-cliff output *is* a conditional performance profile in the anytime
sense, which is exactly the input the meta-level control rule in (2) consumes. That makes the reuse
a precision upgrade rather than a convenience.

### Open — this is the unbuilt part

- **The reference device is still ours to name.** Report 11 settles the percentile (p90,
  single-stream — see above) but found that **no source states a method for choosing a low-end
  reference device**; everyone who tests one just names a current SKU. We name one, or a small
  fixed panel, and measure on the physical unit.
- **The calibration workload distribution is unvalidated.** Pairing synthetic stress grammars with
  a real-language matrix for *latency* calibration (as opposed to the FST-compilation-cost
  calibration the harness already does) is a reasonable extension, not a literature-backed one.
- **Which runtime signals actually drive adaptation** remains a hypothesis, and report 11 makes it
  sharper rather than softer: of the five signals listed above, only **accept rate** has any
  precedent at all — and that precedent adapts an *accuracy* budget, not a latency one. The other
  four have no precedent as latency drivers anywhere. Measure from our own telemetry once tiers
  exist.
- **The per-grammar, per-tier conditional performance profile** — what tiers 1 and 2 actually cost
  and what quality they add on our grammars — is the measurement to budget for. Unchanged in
  substance from "budget for the measurement, do not look for a formula," but now with a named
  target to measure *toward*.
- **A clean negative result worth remembering:** the keystroke-savings literature and the
  latency-budget literature never intersect. The newest paper report 11 found assumes
  instantaneous suggestion delivery. Nobody has published the tradeoff we are calibrating.
- **Interaction with the multilingual case** (several languages resident, one checker running)
  multiplies tier-1 cost by the number of candidate languages unless language identification
  prunes first. Tracked in `openspec/changes/define-multilingual-spellcheck-runtime/`.
  **Now constrained by D11**: pruning is a budget decision, and dropping a candidate language is a
  *stated* degraded mode, never a silent one.

---

## D11 — All accepting languages are kept; narrowing to one is an optimization, never a correctness step

**Decided 2026-07-24** (John): *"Prefer all languages — one language is just 'faster' or 'better
options'."* This settles open question 5 of
`openspec/changes/define-multilingual-spellcheck-runtime/`, and it settles it by **inverting** what
that change assumed.

### The inversion

The multilingual design treated multi-language tagging as the *last resort* after the session prior
and a cross-language score comparison both failed to force a single pick. D11 makes it the
**default**. A word that parses in three loaded languages is a word in three loaded languages; that
is the honest result, and PanGloss returns it.

Identifying a single language is therefore not a correctness objective at all. It buys exactly two
things, both of which are quality-of-service:

1. **Speed** — fewer grammars to run propose→confirm against, and fewer tier-1 generative expansions
   (D9), which is the multiplier D10 was worried about.
2. **Better options** — a candidate list drawn from one language is less diluted than a merged list
   drawn from three, so the top-k a user actually sees is more useful.

Neither is correctness. So neither may cost a real analysis.

### The rule this produces: hard gates may eliminate, soft priors may only rank

This is the operative consequence, and it changes the cascade in the multilingual change:

| Signal | Kind | May it eliminate a language? |
|---|---|---|
| Host declares the writing system / input language | **authoritative external input** | **Yes** — the host has stated what is being typed; this is data, not inference |
| Script / character-set feasibility gate | **hard fact** | **Yes** — a writing system that cannot contain the word's characters cannot have produced it. Requires the writing-system data that has no source today (see D1's caveat, followup 18) |
| Session / document language prior | **soft signal** | **No — ranking only** |
| Cross-language class-LM score comparison | **soft signal** | **No — ranking only** |

The two soft signals were previously written as elimination steps 3 and 2 of the tie-break. They
become ordering steps over a result set that keeps every accepting language.

### What this de-risks

**The cross-language score-comparability bet stops being load-bearing.** The multilingual change
flagged it honestly as an unvalidated bet with no support in reports 04-10 — and under the old
design it was load-bearing, because a mis-normalized comparison would *force the wrong single
language* and discard a correct analysis. Under D11 the same error only mis-*orders* a result set
that still contains the right answer. It drops from a correctness risk to a ranking-quality risk.
It still needs measuring; it no longer blocks.

### What this costs, stated plainly

- **Cost scales with the number of accepting languages**, not with the number of loaded ones — the
  hard gates still prune, and most words in most documents will accept in exactly one language.
  But closely related languages or dialects sharing an orthography are precisely the case where
  several accept, and that is a real, common deployment (it is why the multilingual change exists).
- **Merged candidate lists are diluted.** This is the "better options" half of John's framing, now
  an explicit tradeoff rather than a hidden one: PanGloss ranks across languages instead of
  choosing between them, so ranking quality across languages matters more than it did.

### Interaction with D10 — and the one thing that must not become silent

D10 says tier policy may trade latency for candidate-set size but **never for a correctness
guarantee**, and that running out of budget is a *stated* degraded mode. D11 puts the candidate
language set under that same rule:

- How many languages can be kept live is a **measured** budget (D10's per-grammar calibration,
  summed over the resident set) — not a fixed constant and not inferred from language identity.
- If the budget cannot cover every accepting language, dropping one is a **stated degraded mode**,
  reported the same way a tier-0-only fallback is. **Never a silent drop.** A silent drop is
  precisely the recall loss D10 forbids.

### Consequences for the multilingual change

- D-LangID-1's "resolution order" becomes a **ranking** order, and its third scenario ("tagged with
  all tied languages") is promoted from fallback to default.
- D-NGram-3 (cross-language normalization) is downgraded from load-bearing to ranking-quality, per
  above — still a bet, still to be measured, no longer blocking.
- D-NGram-4 (next-word prediction merges per-language proposals) is *already* D11-shaped: it merges
  rather than choosing. It needed no change, which is corroboration that merge-and-rank is the
  natural shape here.
- The seen-word cache already carries "the language (or languages, when ambiguous)" per entry, so
  the data model needed no change either.
- The distinct "no loaded language could account for this word" outcome is **unaffected** — D11 is
  about not discarding languages that *do* accept, not about inventing one when none does.

---

## D8 — Emit target: a Keyman lexical model. Divvun `.zhfst` is not deprioritized, it is architecturally impossible

**Decided 2026-07-25** (John): *"We will not ship with Divvun... we may ask them to join us, but we
will not be joining them."* Verified against both architectures before recording, on instruction.

### Two facts settle it

**1. Keyman is the declared first integration.** SIL owns Keyman; mobile is the largest market. A
Keyman **lexical model** "powers predictive text **and autocorrect**" `[M]`, authored as a
`.model.ts` TypeScript/JavaScript module with documented extension points for custom search-key
functions, **custom word breakers**, and punctuation handling `[M]`. Both products flow through one
plugin contract — matching John's framing that prediction and correction are the same engine looking
at different parts of the document.

**2. A `.zhfst` acceptor must be exact; the PanGloss FST overapproximates by invariant.** This is
the disqualifying fact, and it is not about licensing or file formats:

- `CONTEXT.md:195-196`, **Propose-and-confirm invariant**: "The PanGloss FST **may safely
  overapproximate** by proposing analyses that the matched Rust HermitCrab runtime rejects." Its
  own *Avoid* line reads **"FST-only correctness, free false positives."**
- `rust/crates/pg-foma/src/composite.rs`, doc comment on `candidates_generated`: "**confirm only prunes, never invents**" — the
  confirmed set is a strict subset of the proposed set.
- `systems/divvun.md` § LEXICON: the ZHFST acceptor **is** the correctness relation — it accepts
  iff the wordform is well-formed.

**Therefore a `.zhfst` emitted from our proposer accepts misspelled words by construction.** The
planned spike (followup 9) asked whether we could write HFST binary format without GPL `libhfst`.
Answering it would not have helped — the obstacle sits one level above the format.

### Why it cannot be repaired

To emit an exact acceptor we would have to compile out everything confirm rejects. But
`CONTEXT.md:47-48` enumerates construct dispositions as "compiled, safely overapproximated, peeled
outside the FST, **confirm-only**, or detected unsupported" — **there is a named class of constructs
that cannot be in the FST at all**. Any grammar using one can never produce an exact acceptor, and
forcing the remainder in is precisely the compilation this architecture exists to avoid. The Aweti
history (`morphotactic-composite-pruning.md`) says that compilation sometimes cannot be done at any
price.

Three escape routes were checked; all fail but one:

| Route | Outcome |
|---|---|
| Emit only for grammars where every construct compiles | A speller that works only for grammars simple enough not to need PanGloss |
| Ship the proposer, let Divvun call back into confirm | ZHFST is a data format with no callback into a foreign runtime; `divvunspell`'s CLI is GPL against our MIT |
| Reverse the direction — Divvun consumes *our* analyses | **This one works.** See below |

### The corroborating structural point

`CONTEXT.md:224` treats duplicate analyses as expected evidence of "overlapping proposal paths." A
spelling acceptor has no such property — it accepts or rejects a string. **Ours is a search and
indexing structure; theirs is a language definition.** Different kinds of object, and only one of
them can be a `.zhfst`.

### What survives

Institutional collaboration, in one direction only. Our propose→confirm emits POS plus full
inflectional feature bundles; their CG layer consumes analyses. We are naturally **upstream** of
them, and the complementarity recorded in `00-synthesis.md` — they solved deployment and have no
authoring-at-scale answer; SIL has thousands of field linguists' analyses that will never become
lexc — is unchanged. That is a story to bring them, not infrastructure to adopt.

**Struck from the plans, permanently: "adopt / inherit / emit into Divvun's infrastructure."** The
inheritance *argument* — one emit target buys many hosts — remains sound and now points at Keyman.

### Consequences

- **Followup 9 is closed without running it**, replaced by: read the Keyman lexical-model API
  properly. It determines the emit target, the latency contract, D6's word-breaker consumer, and
  whether a Rust/WASM module loads inside Keyman's model worker on a low-end Android device.
- **D9/D10's tier policy may not be ours to set.** Shipping inside Keyman means the latency budget
  and call cadence are the host's. Report 11's p90 figure is provisional until Keyman's contract is
  read.
- **`pg-wasm` + `make-wasm-analysis-only` are already the right shape** — a self-contained versioned
  analysis artifact loaded by an analysis-only runtime is exactly what a `.model.ts` would wrap.

### The contract, read and independently re-verified (report 12, 2026-07-25)

`12-keyman-integration.md` read the API. I re-verified its load-bearing claims directly against
`keymanapp/keyman` source rather than accepting them `[M]`:

- **A custom model is not constrained to a wordlist.** `LexicalModel`
  (`common/web/types/src/lexical-model-types.ts:107`) requires only `configure(capabilities)` and
  `predict(transform, context): Distribution<Suggestion>`. `toKey?`, `wordbreaker?` and
  `traverseFromRoot?` are all optional. No wordlist shape appears anywhere in the required surface.
  **D8's premise survives its own biggest risk** — the generative advantage fits the host.
- **Three declared model formats** (`developer/src/kmc-model/src/lexical-model-compiler.ts:181-191`):
  `trie-1.0`, `custom-1.0` (arbitrary TS/JS, real and working), and — remarkably —
  **`fst-foma-1.0`, which is declared in the type union but throws
  `Error_UnimplementedModelFormat`.** Keyman reserved a format name for foma FSTs and never built
  it. PanGloss is foma-backed. That is a conversation to have with the Keyman team, not a format to
  wait for.
- **`predict()` is synchronous** — it returns `Distribution<Suggestion>`, not a Promise. This rules
  out `wasm-bindgen`'s default async `init()` boilerplate and is a real constraint on how `pg-wasm`
  would be embedded.

### The `traverseFromRoot` fork — and where D8's exactness trap reappears

Keyman's own correction search requires the optional `LexiconTraversal` interface and **throws
without it** (`correction/distance-modeler.ts:729-730`). Its budget,
`DEFAULT_ALLOTTED_CORRECTION_TIME_INTERVAL = 33` ms (`:665`), scopes to *that search only*. So there
is a genuine design fork, and it maps onto D9's tiers:

| Option | What we get | What we pay |
|---|---|---|
| Implement `traverseFromRoot` | Keyman drives correction search over our lexicon, with its tuned search and its enforced 33ms budget | Confirm cost inside 33ms; the upper-bound problem below |
| Do everything in `predict()` | Full control of D9's tier policy | No host correction search, no host-enforced timeout — we own the latency contract after all |

Three things follow that report 12 did not draw, from reading `LexiconTraversal` directly:

1. **Exactness is ours to control, so D8's trap is avoidable here.** A traversal node exposes
   `entries: TextWithProbability[]` — the valid words keyed at that prefix. Keyman does not decide
   validity; we populate `entries`. So unlike a `.zhfst` acceptor, an over-approximating proposer
   need not leak invalid forms — **provided `entries` is confirm-backed**.
2. **But `entries` is an eager property, not a lazy function.** `children()` is a lazy `Generator`
   and child traversals are thunked, so the *walk* is lazy — the entries at each visited node are
   not. Confirm must therefore run per visited node, inside the 33ms budget. That is the concrete
   engineering cost, and it is measurable.
3. **`p` must be a true upper bound over an unbounded subtree — and this is unsolved.** Each
   traversal node carries `p`, "the probability of the highest-frequency lexical entry that is
   either a member or descendent" of that node. That is a **score upper bound**, and Keyman's
   correction search is therefore a WAND-style safe-pruned search — precisely the recall-preserving
   cascade mechanism report 11 identified as the only family that does not trade accuracy for
   coverage. The convergence is welcome. The problem is that for a generative grammar the subtree
   below a prefix is **unbounded** (infinite inflection), so computing a genuine max over all
   descendants is not obviously tractable. **If `p` is not a true upper bound, Keyman's search
   prunes incorrectly and silently loses recall** — the exact failure D10 forbids. Nobody has named
   this; it is now the sharpest open engineering question in the integration.
   **Largely dissolved 2026-07-25 by D14** — with runtime generation shelved, `traverseFromRoot` is
   backed by the finite warm + seen cache, so `p` is an exact max over a finite subtree,
   precomputable at pack-build time. The problem returns only if generative forms are ever exposed
   through the traversal; see D14 § "What this de-risks".

### D8a — Integration shape: we ship the engine; Keyman owns storage and key-adjacency

**Decided 2026-07-25** (John): *"we need to have the HermitCrab rust port trim bad words — that is
fundamentally incompatible — we will need to ship our engine. It will be the cost of integrating it.
We will likely have our own weights for the n-gram, etc. but accept Keyman's key-adjacent mechanisms
and rely on it to cache added words, common words, already-typed words... That needs to be stored and
is not done by PanGloss."*

**Keyman is the same organisation, a different team.** The obstacles below are therefore
**coordination items, not technical blockers** — but they are real work that must be scheduled by a
team we do not control, and two of them do not exist today. Documented here so the ask is concrete.

#### The context window — a third coordination item, found 2026-07-25 (report 23 T2, verified `[A]`)

D4's inter-word term needs preceding words. Keyman supplies them through the `Configuration` a
lexical model returns from `configure(capabilities)`, and the relevant field is a **codepoint**
budget, not a word count. Keyman's own published worked example — the one written for *"polysynthetic
languages or those with complex morphologies... it is not practical to list all possible word
forms"*, i.e. precisely our case — requests:

```typescript
configure(capabilities: LexicalModelTypes.Capabilities): LexicalModelTypes.Configuration {
  return { leftContextCodePoints: 16, rightContextCodePoints: 0, wordbreaksAfterSuggestions: false }
}
```

`[A, blog.keyman.com, "Creating an advanced custom lexical model with Keyman", March 2026 — fetched
and quoted verbatim by the parent session; report 23 described this as the official tutorial, which
it is not — it is a blog post, and the correction is recorded here rather than silently applied.]`

**Why 16 codepoints is the finding.** For an Inuktitut- or Aweti-shaped language, *one* word can
exceed 16 codepoints on its own. A left-context budget denominated in codepoints does not scale with
morphological complexity, so **the languages that most need the inter-word model are the ones least
able to feed it.**

> **The pattern, stated precisely — narrowed 2026-07-25 by cross-check B.** The parent session
> originally generalised this across three findings as *"every fixed-size resource is sized in units
> that shrink as morphology grows."* That is **two findings, not three**, and the audit was right to
> split them:
>
> - **Genuinely the same mechanism:** the 10k warm cache (F8) and this 16-codepoint window (F23).
>   Both are *fixed-size resources denominated in a morphology-blind unit* — entries, codepoints —
>   so their effective capacity, measured in *linguistic* units (distinct wordforms served, words of
>   context supplied), falls as morphology grows. **The fix for both is the same: size per grammar,
>   never by constant** (D10).
> - **Related but different:** F18's training-corpus bias is **selection bias, not exhaustion.** The
>   corpus is not a fixed-size resource being consumed; it is a *filtered* one, where the filter
>   (grammar coverage) removes complex forms. Same *direction* of harm — complex morphology loses —
>   but a different mechanism and a different fix (N4 measures it; better coverage reduces it).
>
> The common thread that does survive all three, and it is worth keeping: **every one of them errs
> in the direction that makes the design look adequate on simple languages and fails on the ones
> this project exists for.** That is a reason to distrust favourable numbers, not a unified defect.

**What is and is not established.** The example value is verified; that it is a *ceiling* is not.
`configure` receives `capabilities` and returns a requested `Configuration`, so 16 is plausibly the
example author's choice rather than a host limit — whether the host will grant more, and how much,
is an unanswered question for the Keyman team and is the concrete ask. **Continuous typing is
largely defended anyway** by a rolling in-session buffer we maintain ourselves; the exposed case is
**cold start** — a cursor placed into existing text, where the host's window is all there is.
Ledger row **C12**.

#### Why we must ship the engine

No static lexicon format can express "confirm trims this." The Rust HermitCrab port is the authority
on which proposed forms are real (`CONTEXT.md` § propose/confirm; `composite.rs`, `candidates_generated` doc comment, "confirm only prunes,
never invents"), so **every** Keyman model format that stores a finished word list — `trie-1.0`, and
`fst-foma-1.0` if it were ever built — is fundamentally incompatible with our architecture, for the
same reason `.zhfst` was (D8). The resolution is different only because `custom-1.0` exists: we ship
`pg-wasm` inside the model and confirm at query time. **That is the cost of integration, accepted.**

#### The ownership split

| Concern | Owner | Status |
|---|---|---|
| Morphological generation, confirm/trim, analysis | **PanGloss**, shipped as `pg-wasm` in a `custom-1.0` model | ours to build |
| N-gram weights and ranking (D4) | **PanGloss** — our own weights, not Keyman's | ours to build |
| Word breaking (D6) | **PanGloss** via the optional `wordbreaker` hook, fed by `import-writing-system-data` | ours to build |
| **Key-adjacency / fat-finger correction** | **Keyman** — accepted as-is, not rebuilt | **exists** |
| **Seen-word cache: common words, already-typed** (D9 tier 0) | **PanGloss**, in-worker — see D8b | ours, no Keyman change needed |
| **User-*authored* added words** | **Keyman** (durable store) | **DOES NOT EXIST YET** (#12124) |

#### Verified: accepting key-adjacency forces the `traverseFromRoot` fork

`ModelCompositor.predict()` takes `Transform | Distribution<Transform>`
(`model-compositor.ts:79`) — a probability distribution over what the user may have meant to type.
That distribution **is** the key-adjacency mechanism, and it is consumed by `correctAndEnumerate`
(`:142`), which runs Keyman's correction search — which **throws without `traverseFromRoot`**
(`distance-modeler.ts:729-730`).

So John's "accept Keyman's key-adjacent mechanisms" **resolves the fork above, and selects the
harder branch**: we implement `traverseFromRoot`, we inherit the 33 ms budget on that path, and the
`p`-upper-bound problem becomes load-bearing rather than optional. That is the right trade — it buys
a whole error-model component we would otherwise build (most of report 03's territory) — but it is a
trade, not a free win.

Note the asymmetry, which confirms the split: Keyman consumes the transform *distribution* and calls
into **our traversal**; our own `LexicalModel.predict()` receives only a single `Transform`
(`lexical-model-types.ts:194`).

#### The Keyman-side ask: user dictionaries do not exist yet

Relying on Keyman to store added, common, and already-typed words is **an ask, not an integration**.
Verified `[M]` against the Keyman issue tracker, 2026-07-25:

- **#12124 "epic: user dictionaries" — OPEN.**
- **#11872 "feat(web): add support for OS-provided user dictionaries" — OPEN.**
- #15100 "feat(android): Prompt User to Select Dictionary when Multiple Downloaded" — OPEN.

There is also **no learn/persist/accept hook anywhere on the `LexicalModel` interface** — the model
is never told that a suggestion was accepted, so it cannot maintain this store itself even if it
wanted to.

**Consequences for D9 and D7, which must not be papered over:**

- **D9 tier 0 has no implementation on either side of the boundary today.** D9 specifies it as a
  cache of words seen, "persisted across sessions" — PanGloss is now explicitly not the persister,
  and Keyman's persister is an open epic. Until #12124 lands, the tier-0 sub-10ms path that D9's
  entire anytime contract rests on does not exist. **This is the single highest-value coordination
  item with the Keyman team.**
- **D7 (personal on-device overlays) partly relocates.** If Keyman owns the personal word store, D7's
  privacy and data-governance reasoning applies to *Keyman's* storage, not ours — which is better
  for us (one less sensitive store to own) but means D7's guarantees are only as strong as
  Keyman's implementation, and we should have an opinion on #12124's design rather than inheriting
  whatever ships.

### D8b — We own the tier-0 cache in-worker; only *authored* words need a durable store

**Decided 2026-07-25** (John): *"We may be able to implement a package for Android and iOS that
handles caching, etc. That shouldn't be a burden."*

Correct, and it is cheaper than a package. Two verified facts collapse most of the D8a ask:

1. **The LMLayer runs in a real Web Worker** (`worker-main/src/lmlayer.ts:40,53`: "The LMLayer proper
   runs within a Web Worker"). **IndexedDB is available inside Web Workers** (`localStorage` is not).
   So a `custom-1.0` model can persist its own store **with zero Keyman code changes** — this is not
   a native package, it is part of the artifact we already ship.
2. **The learn signal already exists, disguised as context.** D8a noted there is no learn/accept hook
   on `LexicalModel`. But `predict(transform, context)` delivers `context.left` (up to
   `Capabilities.maxLeftContextCodePoints`) on **every keystroke** — so every word the user types
   passes through our hands anyway. "Already-typed words" and "common words in this session" are
   observable without any hook at all; we accumulate frequency counts from context.

#### The distinction that decides where data lives: regenerable vs. authored

| Data | Regenerable? | Store | Loss on eviction |
|---|---|---|---|
| Tier-0 seen-word cache, frequency counts | **Yes** — rebuilt from context as the user keeps typing | **Ours, IndexedDB in-worker** | Acceptable: a cold cache, not lost data |
| Words the user explicitly *added* to their dictionary | **No** — authored, one-off, unrecoverable | **Must be durable** — Keyman #12124, or a native package | **User-visible data loss** |

**Never put authored data in an evictable cache.** WebView IndexedDB can be cleared by the OS under
storage pressure, by the user clearing app data, and possibly across app updates. That is fine for a
cache and unacceptable for something a person typed in once on purpose. This is the real reason a
native package might still be wanted — not for volume, but for **durability of the authored subset**.

#### Risk to spike before relying on this

`lmlayer.ts:75` documents that the worker may be started from a **`file:` URI**. A `file:`-origin
worker may get an opaque origin, under which **IndexedDB is restricted or unavailable**. This is the
one thing that would invalidate D8b, it is cheap to test, and it should be tested before any design
depends on in-worker persistence. Quota limits are a secondary, lesser unknown.

#### What this does to D7

D8a said D7's privacy guarantees relocate to Keyman's storage. **D8b takes them back** for the
regenerable tier, and that is a feature rather than a cost: the seen-word cache is the most
personally revealing data in the system, and keeping it inside our own on-device store means D7's
"personal on-device overlays, no cross-user aggregation" holds by construction rather than by
inheriting whatever #12124 ships. The residual Keyman dependency shrinks to authored words plus
OS-level dictionary sharing (#11872).

#### Second Keyman-side item, non-blocking

`fst-foma-1.0` is declared in the model-format union and throws `Error_UnimplementedModelFormat`
(`lexical-model-compiler.ts:189-190`). We do **not** need it — `custom-1.0` is the right vehicle
precisely because it lets confirm run — but its existence shows the Keyman team already contemplated
FST-backed models. Worth raising as context when opening the user-dictionary conversation, and worth
saying plainly that we do not want it built for us: a compiled foma model without confirm would
reintroduce the exact over-approximation defect that killed the ZHFST route.

---

## D12 — Languages without a well-defined orthography are out of scope

**Decided 2026-07-25** (John): *"Explicitly defer orthographies that are not well defined. We
inherently are a rule based system with some statistics tacked on, not the other way around. If
there is no established orthography, our rules collapse."*

**The gap this closes.** None of reports 01-11 asked whether a spelling norm exists. Tone appears
only as an *encoding* problem (word-forming classification, U+A700 homoglyphs), never as a
*normative* one. Yet spell-*checking* presupposes a correct form the user deviated from — the
premise silently underwriting reports 01-03 and all of D2.

**Why deferral rather than a graded fallback.** Making this a per-pack attribute with three regimes
was the wrong instinct, because the collapse is upstream of the speller: HermitCrab rules apply over
graphemes, so an unstable orthography degrades the *parser*, not merely the ranking. There is no
speller-level mitigation for a grammar that cannot parse its own language's text.

**Side effect worth stating:** this also resolves an ethical exposure nobody had named. Flagging a
community member's spelling in a language whose orthography is still being negotiated is imposing an
orthography. Scoping those languages out removes the exposure rather than managing it.

---

## D13 — The speller ships only for languages certified under the existing corpus-recall bar

**Decided 2026-07-25** (John): *"The n-gram assumes a large (few thousand words min) body of text
that we can analyze at near 100%... If we can't get near 100% parsing, we don't have a good base
model anyway."*

### Coverage is required; disambiguation is not

- **Coverage** (≥1 analysis per token) — genuinely required, and worse than it appears: an unparsed
  token breaks the *context* for its neighbours too, so holes cost superlinearly in a trigram.
- **Disambiguation** (exactly the right analysis) — **not** required. D4 marginalizes over the
  lattice, and fractional counts give a soft estimate. This is why the reference project's 760
  human-approved analyses are not the blocker they first appear to be.

### ~~This is not a new gate — it is an existing one~~ → It IS a new gate, and D13 must own it

> **CORRECTED 2026-07-25 by report 21 (finding #2), verified in-repo by the parent session `[M]`.**
> The argument below was false at the moment it was written, and the correction changes who owns
> the admission bar.

~~`openspec/changes/certify-four-language-matrix/` already certifies a language **"only when
analysis-level corpus recall is complete."** That is exactly this precondition, already defined and
already measured, for Sena, Indonesian, Amharic, and Aweti. The speller declares it as an admission
criterion rather than inventing a parallel one.~~

**What is actually true.** `openspec/changes/certify-four-language-matrix/` does not exist. It was
renamed and rewritten by commit `bf3d12c` — *"docs: rename certify-four-language-matrix ->
run-synthetic-conformance-matrix (Stage 4)"* — on the morning of 2026-07-25, **hours before D13 was
written**. The quoted phrase *"only when analysis-level corpus recall is complete"* appears nowhere
under `openspec/` `[M, grep-confirmed]`. The replacement change,
`openspec/changes/run-synthetic-conformance-matrix/proposal.md`, argues the *opposite* of what D13
quoted it for:

> *"there is no terminal certification stage and no external reference languages to certify
> against... **Retire the "certify a language" framing: there are no external reference languages
> to certify against.**"*

So the citation is not merely stale — it points at an artifact that now disclaims the framing it
was cited to supply. This is also a live instance of the D16 problem: the retired framing was
"certify against four reference languages", i.e. certification against exactly the four
unrepresentative samples D16 governs. The rename was the right call and D13 leaned on the version
that predated it.

**The consequence, and it is not cosmetic.** D13's rhetorical move was *"the speller is not
inventing a parallel admission criterion, it is declaring an existing one."* There is no longer an
existing one to declare. **The coverage bar is now D13's own requirement, owned by this plan, and
it must be justified on its own merits rather than inherited.** What survives unchanged is the
substance: a speller must not ship for a language whose grammar cannot analyze that language's
text, because D18 then has nothing to flag with and D14's cache is built from a partial generator.
What does not survive is the claim that someone else already decided this. Under D17 this is a
**product/scope call — John's**, not a technical inheritance.

### Two mitigations that soften the bar without weakening it

1. **Guessed parses are partly usable.** The guess branch is all-or-nothing and the root becomes a
   sentinel, but affix morpheme ids pass through as real (`pg-parse/src/lib.rs:33-38`). In an
   agglutinative language most class-bearing information — tense, aspect, person, number, case —
   lives in the affixes. A guessed parse therefore still carries a real feature bundle: useless for
   the root/lexical term, usable for the class term, **provided it is flagged so it never counts as
   lexical evidence**. Coverage holes become partial credit rather than zeros.
2. **Poor coverage degrades exactly like thin features do.** Both collapse the backoff ladder toward
   its coarser rungs rather than killing the model. D1 already names that failure mode for thin
   `syn_fs`. One risk, one mitigation, not two.

### Measured on the reference project (`Sena 3`, 2026-07-25)

Indicative, not typical — Sena 3 is a FieldWorks demo project, but it is the one `pg-fwdata`'s tests
use.

| Record | Count |
|---|---|
| `WfiWordform` | 6,973 |
| `Segment` (≈ sentences) | 4,487 |
| **`WfiAnalysis` (human-approved)** | **760** |
| `WfiGloss` | 520 |
| `LexEntry` | 1,464 |

D4's ~50k-token assumption is roughly right for raw text. But followup 12 called human-approved
interlinear analyses "the scarcest resource in this entire problem," and here that resource is 760
records — three orders of magnitude below the 300K-1.8M gold tokens report 08 found neural
disambiguators needed.

### Measured 2026-07-25 (report 13): the admission criterion currently admits nothing

D13 says the speller ships only for languages with complete analysis-level corpus recall. First
measurement of where those languages actually stand:

| | Sena 3 | Amharic | Indonesian | Aweti |
|---|---|---|---|---|
| Coverage (≥1 confirmed analysis) | 49.20% | 24.37% | 85.12% | 48.56% |
| step-capped (200k steps) | 12.42% | 0.00% | 0.00% | 40.87% |
| timed out | 0.00% | 9.81% | 0.00% | 6.73% |
| Ambiguity mean / median / p90 / max | **4.61 / 4 / 9 / 78** | 1.12 / 1 / 2 / 2 | 1.03 / 1 / 1 / 2 | 1.47 / 1 / 2 / 4 |

**Read these as a floor, not as the coverage figure.** Four caveats, all from report 13 itself:
the **guess branch was never exercised** (`guess_root=true` untested), so real coverage is higher;
19% of Sena's inventory is `invalid_shape`, i.e. unsegmentable strings that are probably punctuation
and numerals rather than failures; step-caps and timeouts are *resource* outcomes, not correctness
failures; and the run used the **Rust-HermitCrab-only** pipeline, not the deployable foma-propose +
confirm one. Amharic and Indonesian were also measured on 673 and 121 wordforms, far too few to
generalize from.

**Even read generously, nothing here is near "complete."** And report 13 found that the change's
tasks were **all unchecked** — no certified baseline existed then, and the change has since been
renamed to `openspec/changes/run-synthetic-conformance-matrix/` and rewritten to retire the
certification framing entirely (see the correction above).

### Superseded 2026-07-25: PanGloss is being rewritten to a multi-FST topology

**John:** *"PanGloss is being completely rewritten to have a different certification criteria — not
just the 4 but a multi-FST topology. It is not there right now, but we 'assume' that we will get to
very high coverage (or that if we can't we have absolutely nothing)."*

So high coverage is a **planning axiom**, and it is existential: the project has no product without
it. Three consequences, and they do not all point the same way.

**1. D13 is re-expressed as a principle, not a pointer.** The criterion is "ship only for languages
meeting the **then-current** certification bar," not "the four in the retired
`certify-four-language-matrix`." The gate is being replaced; the rule that a language must clear it
survives unchanged. Likewise the coverage arithmetic above describes an architecture being retired —
it is a snapshot, not a verdict. **Sharpened 2026-07-25:** the correction above shows the gate is
not merely *being* replaced — it was already gone when D13 was written, and there is no successor
gate. So "the then-current certification bar" currently resolves to nothing, and **D13 must state
its own bar**. That is the open item, recorded in the candidate ledger as **C8**.

**2. Report 13's findings partition, and the rewrite invalidates only half of them.**

| Engine-dependent — **re-measure after the rewrite** | Grammar-data-dependent — **stable across any rewrite** |
|---|---|
| Coverage (24–85%) | `mpr` nonempty in 0 of 4 grammars |
| Step-caps, timeouts | `syn_fs` beyond POS: 31 / 85 / 0 / 45% |
| D13's admission arithmetic | Rung-1 singleton rate 93.5–100% |
| Ambiguity magnitude | `HeadFeatures` richness concentrated **per POS** |

**A different engine does not populate an unpopulated feature.** Every D1/D4 correction above is a
fact about how FLEx grammars are authored, not about how the FST is built, so the shortened backoff
ladder and the per-POS rung-3 rule survive the rewrite intact.

**3. The axiom makes the ambiguity problem worse, not better — this is the counterintuitive part.**
Sena sits at 49% coverage with ambiguity mean 4.61 / p90 9 / max 78. The missing 51% is **not free**.
Those words are disproportionately the *hard* ones — longer, more affixes, more complex morphotactics
(12.4% were step-capped, i.e. the engine gave up on complexity rather than rejecting the word). When
a multi-FST topology reaches them, they arrive carrying **more analyses than average, not fewer**.

So assuming high coverage is not a reprieve for the ranking layer, it is an **increase in its load**:

- D4's lattice-marginalization burden grows.
- D9 tier-2 cost grows.
- D8's `p`-upper-bound problem gets harder — bigger subtrees to bound.
- D8a's 33 ms Keyman correction budget gets tighter.

**Everything sized against ambiguity should be sized against post-rewrite ambiguity, which is
strictly worse than the numbers measured today.**

**4. Guessed parses become more load-bearing, not less.** High coverage will partly be *achieved
via* the guess branch — unknown roots exist at any coverage level (names, borrowings, neologisms).
D13's rule that guessed analyses are flagged and never counted as lexical evidence therefore matters
more after the rewrite. Report 13 **never exercised the guess branch at all**, so this is an
untested path that the coverage axiom leans on.

**5. Keep the measurement as a tripwire.** The axiom is a planning assumption, not a proof.
`rust/crates/pg-cli/examples/spellcheck_measure.rs` is committed and cheap to re-run; that is now its
main value. Re-run it against the new topology when it lands. **If coverage is still ~50%, that is a
project-level signal to surface loudly, not to absorb into the speller plan.**

**6. Carry the `p`-bound requirement *into* the rewrite.** D8's sharpest open question — that Keyman's
`traverseFromRoot` needs `p` to be a true upper bound over an unbounded generative subtree, or its
WAND-style search silently loses recall — is a constraint the multi-FST topology should know about
**while it is being designed**, not discover afterwards. A decomposed proposer may make a per-prefix
maximum tractable per-FST, or may make it harder to compute across the composition. That is a
question for the topology work, and it is cheaper to answer now than to retrofit.

**The ambiguity number is the one to keep.** Sena's mean 4.61 / p90 9 / max 78 over a real 6,973-word
interlinear corpus is the first hard evidence that D4's decision to *marginalize* over the analysis
lattice rather than disambiguate it has real work to do — a p90 of 9 is a wide lattice. The other
three grammars' near-flat ambiguity (means 1.0–1.5) is almost certainly a corpus-size artifact at
121–673 words, not a language property, and should not be quoted as one.

### Still open: how D4 is actually estimated

D4 decides the model and never decides how to fit it. Estimation must run over ambiguous parser
output, and the naive route is circular — counting the parser's 1-best requires a ranker, which is
what D4 *is*. Options: EM over the lattice; fractional counts uniform over each word's analyses;
bootstrapping from the 760 gold plus report 09's synthetic generation. **Recommendation: fractional
counts first (cannot fail), measure, add EM only if measurement demands it.**

**But the prior move is measurement, not choice.** ~~`recall@k` of the candidate generator needs no
LM, no corpus, and no gold set; report 09 named it evaluation-apparatus item 1 and D5 calls it
"buildable now, before any ranker exists."~~ **Attempted 2026-07-25 and found NOT buildable — this
was wrong, and it is worth recording why.** Report 13 searched `pg-foma`, `pg-fst`, `pg-parse` and
`pg-cli` and found **no prefix-completion or error-tolerant-generation API anywhere in the
codebase**. `recall@k` from a typed prefix requires D9's tier 1/2 generation, which is decided but
unbuilt. D5's "buildable now" claim was made about a capability that does not exist. The agent
declined to fake it, which was correct.

So the cheapest design-invalidating measurement is **not** available, and the honest substitute is
what report 13 did measure: rung cardinality and ambiguity. The zero-measurement era is over —
report 13 is the first — but the specific number that would tell us whether ranking has a solvable
problem still requires building tier-1 generation first.

---

## D14 — A warm cache ships in the language pack; runtime generation for uncached words is shelved

**Decided 2026-07-25** (John): *"Caching words is essential — I expect the user to be caching ~10k
or more words, all the high frequency ones. We can ship a warm cache with the language pack.
Everything for those is super-cheap."* And: *"I assume that word guessing will be 90% words that are
being correctly typed and are already cached, 9% words that are incorrectly typed but cached, and 1%
words that are not cached — and then only if it is a super easy language and the word is fairly
full. If we miss the 1% — no one is sad. Let's shelf it for now completely and have it as a 'helpful
future enhancement — maybe'."*

This is a traffic model, and it re-sizes the whole design around where the traffic actually is.

| Bucket | Share | Machinery | Status |
|---|---|---|---|
| Correctly typed, already cached | 90% | Tier-0 hash/trie lookup over a finite word list | cheap, ships |
| Mistyped, but the intended word is cached | 9% | Error-tolerant search **over the finite cache** + Keyman key-adjacency | cheap, ships |
| Not cached at all | 1% | Runtime grammar generation (tiers 1-2) | **shelved** |

> ### ⚠ THE 1% IS THE MOST LOAD-BEARING NUMBER IN THIS PLAN AND THE LITERATURE CONTRADICTS IT
>
> **Challenged 2026-07-25 by report 20, verified at primary source by the parent session.** The
> published OOV curves for the language families this product targets put the uncached bucket **one
> to two orders of magnitude above 1%**. The strongest single datum is verbatim-confirmed:
>
> > *"With a vocabulary of **1.3 million words** derived from proceedings and stories, held-out
> > stories have **more than 60% of words out-of-vocabulary**."*
> > — Gupta & Boulianne, *Automatic Transcription Challenges for Inuktitut, a Low-Resource
> > Polysynthetic Language*, LREC 2020, pp. 2521-2527 (`aclanthology.org/2020.lrec-1.307/`) `[A]`
>
> Corroborating, same direction: Turkish ~15% OOV at a 64k lexicon and still >5% at 500k
> `[S — UNVERIFIED. Report 20 reached this via secondary summary; report 24 then traced the figures
> to **Çarki, Geutner & Schultz, ICASSP 2000**, not to the Arısoy/Dutağacı/Arslan 2006 paper
> originally named. Both are paywalled and neither could be read at primary source. Do not promote
> to [A] without the primary text, and do not lean on it — Inuktitut alone carries this argument.]`;
> Finnish 20% word-level OOV at **40M** training tokens
> `[A for the OOV direction and the 56%→32% WER result; the **40M token count is in doubt** — report
> 24 found an independent source giving 96.4M words for the same corpus. Hirsimäki et al. 2006.]`;
> Inuktitut type-token ratio published
> between 0.144 and 0.1938 depending on corpus version, i.e. **~1.5M distinct types in ~11M tokens**
> `[A, partially verified — the 0.1938 figure was independently corroborated, the exact 0.144 was
> not; both exceed what a 10k cache can serve]`.
>
> **These are token-level miss rates against top-frequency lexicons** — the same sampling a warm
> cache uses — so Zipf skew does not rescue the estimate. It is already priced in.
>
> **The sharpest statement of the problem, which neither the plan nor report 20 makes:** an
> analyzing FST exists *precisely to solve the OOV problem* — it accepts and analyzes wordforms
> nobody has ever seen, which is the entire reason this project can serve agglutinating languages at
> all. **D14 shelves that capability at runtime and replaces it with a finite list, thereby
> reintroducing exactly the OOV problem the FST was built to eliminate.** For a polysynthetic
> language the finite list is not an optimization of the generative path; it is a downgrade to the
> architecture every other speller already fails with (D4 § "Why this handles unseen wordforms").
>
> **Disposition under D17.** The 90/9/1 split is **John's product assumption, and it remains his
> call** — but it is now reclassified from a load-bearing premise to an **unvalidated placeholder
> pending measurement**, and D16's exemption of it (D16 § "What this does and does not invalidate")
> does not survive: the
> exemption rested on query-autocompletion corroboration, and QAC's hyper-recurrent head is a
> property of *web search traffic*, not of word-level typing in a polysynthetic language.
>
> **What this does NOT do:** it does not un-decide D14. The warm cache still ships, and the 90% and
> 9% buckets are still real and still cheap. What changes is that **"shelve tiers 1-2 completely"
> becomes one calibrated operating point rather than the architecture** — D10 already owns
> per-grammar operating-point selection, and it must be empowered to pick generation *on* for a
> grammar that measures a high uncached rate, from day one, rather than that being a later
> "un-shelving" that has to fight the architecture. Live candidates and the deciding measurement are
> in § "Candidate ledger".
>
> **Coupled to the flagging gap — do not fix these separately.** See D18: with tiers 1-2 shelved,
> the only mechanism left for "is this a word" is cache membership, and at a 20-60% uncached rate
> that flags correctly-typed complex words en masse.

### Measured 2026-07-30 (report 27): the shelved bucket, measured for the first time

Everything above about the uncached bucket was argued from published OOV curves. **Report 27
measured the shelved capability itself** — not the traffic model, the *machinery*. The mechanism it
measured is not report 17's parked one: it walks the compiled proposer network's arc table,
constrained by the letters already typed, and ranks completions from the tags each path already
carries, so nothing has to be predicted before generating and no new engine is required.

| | Indonesian (1,189 states) | Sena (39,286 states) |
|---|---|---|
| True word reachable from a 4-char prefix (analysable words) | **100%** | 15% (45% at prefix 6) |
| Rank of the true word after ranking | **median 1** | median 62-98 |
| Confirms paid to fill a confirmed top-3 | **median 3** | 25 (budget cap), 0% filled |
| End-to-end per keystroke | **~13ms** | 142-788ms in the walk alone |

`[M]`, 2026-07-30, `rust/crates/pg-foma/examples/predict_census.rs`. Held-out fifth of each corpus;
containment measured against the **analysable** subset, since the FST cannot contain a word whose
stem the lexicon lacks. Sample sizes are 20-40 words per grammar per prefix length — the *direction*
is large and reproducible, the percentages are not to be quoted.

**Three things this changes, and one it must not.**

1. **The expensive half was misidentified — by this plan and by the framing that commissioned the
   measurement.** Per-confirm cost is **0.3-1.2ms** `[M]`. Every latency argument for shelving tiers
   1-2, here and in report 17 and in the parked plan, assumed the HC/`confirm` call was the thing
   nobody could afford on a keystroke. It is the cheap half. The **completion search** is where
   4-788ms goes. Any future un-shelving work should be aimed at the search, not at avoiding confirm.
2. **The capability is available where it is least needed, and absent where it is most needed.**
   Compose this table with the ⚠ box above: the uncached-token rate rises with morphological
   productivity, so the grammars that most need runtime generation are the agglutinative and
   polysynthetic ones — and those are exactly the ones where the walk does not currently fit a
   keystroke. The mildly-affixing grammar, where the walk is essentially free, is also the one whose
   warm cache would have served best anyway. **This is the same shape as R-3 in `OVERVIEW.md`: a
   favourable number arriving from the easy end of the distribution.** It is not a reason to
   disbelieve the Indonesian result; it is a reason not to generalise it.
3. **C4 candidate (c) — per-grammar, chosen by D10's calibration — gains its first measured
   support**, and the measurement is cheap to repeat per grammar. What D10 needs is a pre-flight
   number (walk p90 and containment@top-k on that grammar), which is exactly what the harness
   already prints. Note this mirrors the repo's standing per-grammar strategy-selection posture: the
   answer is measured per grammar, never guessed from language type.

**What it must not do: un-shelve anything.** D16 governs — two sample grammars, one favourable, one
not, is research signal about two samples. It may motivate the work; it may not narrow the design,
set a default, or retire the shelf. The deciding evidence is still R1's uncached-token rate on a
complete grammar, and the search-cost question now has its own ledger row (**C14**).

**Two corrections to items already in this section.** The negative cache described in D8b/D14 terms
("FST yes, HC no" verdicts) is **correct and cheap but aimed at the wrong cost**: measured, it saves
0.1-0.7 confirms per keystroke at ~0.5ms each, i.e. well under a millisecond, because it caches the
half that was already cheap. Its value, if any, is caching **walk results per prefix**. And one
correctness rule that any implementation must keep: a candidate surface abandoned at a budget cap is
**never** cached as refuted — only a proven refutation (every candidate tried, none confirmed) is —
or a budget artifact becomes a permanent wrong answer.

> **Do not read the 0.3-1.2ms confirm figure as retiring R-1 or N8.** It is a *median over
> candidates the walk proposed*, on two small grammars, in the generation direction. N8 asks for the
> **tail** of `confirm` on **one word the user actually typed** — a different question, and report
> 13 measured ~10% timeouts and ~12% step-capped on the same corpora. R-1 stands.

### The generated cache is ranked by a model biased against what generation is for

**Added 2026-07-25** (review-campaign finding P0-3). Assembled from three statements that are each
individually in this document and were never composed. The composition is the finding.

1. **D14 item 1** — a *generated* entry has no observed frequency, so ranking within the warm cache
   must come from D4's class model. Nothing else is available.
2. **D15 § "Coverage does not merely gate this layer — it biases it"** — a token with no analysis
   contributes no class and is silently dropped from class-LM training, and the dropped portion is
   *systematically the morphologically complex portion*, because that is what an incomplete grammar
   fails on.
3. **D14's purpose** — generation exists to supply forms the corpus does not contain, which are
   disproportionately the morphologically complex ones.

Composed: **the ranker that decides which generated forms are worth shipping is trained on a corpus
whose complex half was systematically dropped, and is then asked to rank precisely the complex
forms.** The bias does not cancel. It compounds, and it compounds in the direction that makes the
warm cache look adequate: the forms it is worst at scoring are the forms it will therefore not
ship, which are the forms whose absence produces the uncached-token rate the ⚠ box is about.

D14 argues the build-time/query-time separation is "not circular". That is **correct about
circularity and silent about bias** — the two are different failure modes and only one of them was
addressed.

**Disposition.** This is a *deferred* item under D17, not an elimination, and it does not un-decide
anything. But it is the kind of bias that is invisible in evaluation, because any held-out set
drawn from the same corpus inherits the same drop. **It can only be detected against text the
grammar cannot fully analyze** — which makes it one of the few questions on this list that a
synthetic sweep genuinely can attack, by generating a corpus, deleting the analyses of its complex
tail, training on the remainder, and measuring the ranking loss on the deleted portion. That is an
elimination-shaped experiment in D16's corrected sense, and it is listed in the research programme
below as **N4**.

### Scoping the shelf precisely — this is the part that can be mis-implemented

"Shelve the 1%" must **not** be read as "shelve error tolerance." The 9% bucket is error tolerance;
it is the second-largest bucket and it ships. The distinction is the search space, not the feature:

- **Error-tolerant search over a finite word list** (~10k entries) — a Levenshtein automaton against
  a DAWG/trie, or Keyman's own correction search. Bounded, milliseconds, **keep**.
- **Error-tolerant traversal of a generative FST** (D9 tier 2, Oflazer-style, unbounded subtree) —
  **shelved**.

### The assumption this rests on, flagged for correction

John's two statements are in tension on one point, and the reading below is an assumption, not a
quotation. "~10k cached words, all the high-frequency ones" and "shelve uncached-word generation
completely" are only jointly satisfiable if **the cache is generated, not merely observed**:

> **Assumed:** grammar-driven generation moves from keystroke time to **pack-build time**, where it
> populates the warm cache offline. Runtime never generates; the pack ships the output of
> generation.

The alternative reading — warm cache = corpus-observed wordforms only, no generation anywhere — has
two consequences that argue against it, and if it *is* what was meant, D4 must be rewritten:

1. **It cannot reach 10k.** Report 13's corpora are 6,973 wordforms (Sena 3), 673 (Amharic), 121
   (Indonesian). Three of four grammars have nowhere near 10k observed types.
2. **It collapses D4 to a surface n-gram.** D9's whole argument for the intra-word `P(w | class)`
   term was that tiers 1-2 emit zero-count forms that nothing else can rank. Remove generation
   entirely and the morphology becomes decoration for prediction — the precise risk D9 was written
   to close.

Under the assumed reading, **D4 survives intact but relocates**: the intra-word term earns its keep
at cache-build time (ranking which generated forms are worth shipping) instead of at keystroke time.
The grammar stays load-bearing; it just runs offline.

### Why build-time generation is safe here, when runtime enumeration is not

The standing repo rule is that materializing the wordform inventory does not terminate — 10^4-10^8
forms per stem, the reason the original delete-table plan died. Build-time cache generation is the
one place it is safe, and the reasons are structural, not optimistic:

- **Bounded by construction** — top-*N* stems x selected paradigm slots, with *N* and the slot set
  chosen to hit a stated entry budget. It is a *budgeted sample* of the inventory, never the
  inventory.
- **Offline** — no latency contract, no anytime budget, no `ComposeBudget` pressure at query time.
- **Falsifiable** — a generated cache can be measured against a held-out corpus before it ships.

This is a different activity from D9 tier 1 and should not reuse its name.

### What this de-risks — three open problems shrink

1. **The `p`-upper-bound problem largely dissolves** (D8's sharpest open engineering question, and
   the one carried into the multi-FST topology work). It was hard *only* because the subtree below a
   prefix is unbounded under runtime generation. Back `traverseFromRoot` with the **finite** warm +
   seen cache and `p` becomes an exact max over a finite subtree — the classic `trie-1.0`
   computation, precomputable at pack-build time. The available shape is now:
   - `traverseFromRoot` exposes the finite cache -> Keyman's WAND-style correction search is exactly
     safe, inside its 33 ms budget, with no recall loss;
   - anything generative, if it ever returns, lives in our own `predict()` outside that search.

   **The requirement to carry into the topology design therefore weakens to a conditional:** a true
   `p` upper bound is needed only if generative forms are ever exposed through the traversal. Worth
   still telling the topology work about — un-shelving later should not be gated on an unsolved
   problem — but it is no longer load-bearing.
2. **D8b's `file:`-origin IndexedDB risk downgrades** from "the one thing that would invalidate D8b"
   to "the cache stops learning." Pack-shipped data is read-only and always available; only the
   accumulated seen-word delta needs IndexedDB.
3. **D8a's Keyman coordination item downgrades again.** #12124 (user dictionaries) was called "the
   single highest-value coordination item" because tier 0 had no implementation on either side. A
   shipped warm cache means tier 0 exists at install time with **zero** Keyman dependency. #12124
   narrows to what D8b already scoped it to: durability for user-*authored* words.

### What it partly hedges — the coverage axiom

D13's rewrite note treats high coverage as an existential planning axiom. A shipped warm cache
**decouples supply from parse coverage for the head of the distribution**: a cached wordform can be
offered whether or not the grammar parses it. It does **not** decouple *ranking* — a cached form with
no confirmed analysis has no POS and no `syn_fs`, so D4's class model cannot place it, and it falls
back to a bare surface count. So the coverage tripwire stays; its blast radius on spellcheck
specifically is smaller than stated.

### What this opens — five items, none blocking

1. **Frequency for a generated entry is not observed.** Ranking within the warm cache needs a
   frequency estimate, and for generated forms the only available estimator is D4's class model —
   the model the cache was partly meant to relieve. Not circular (build time vs. query time), but it
   means D4 must exist before a good warm cache can be built, and cache quality is bounded by D4's.
2. **D9's binary rule needs a third rung.** "Seen vs. unseen, large fixed penalty between them" now
   has three populations: typed-by-this-user > shipped-warm-cache > generated-on-demand. The warm
   cache is words *someone else* has seen. The large fixed penalty belongs between rungs 2 and 3;
   whether rungs 1 and 2 need a smaller separation is open (recommendation: yes, a small one — a
   user's own history should outrank a shipped prior).
3. ~~**The pack format needs a third payload, and it is being written right now.**~~
   **Retracted 2026-07-25 by D15** — the warm cache is an *add-on artifact*, not a pack payload, so
   no `CONTAINER_VERSION` bump is wanted. The verified facts stand and now argue the other way:
   `.pgpack` container v1 frames exactly two payloads and
   `pg_pack::format::fingerprint_hex(runtime_payload, foma_payload)`
   (`rust/crates/pg-pack/src/format.rs:146-151`) hashes exactly those two, so putting the cache
   inside would couple a user-features artifact to the analyzer's container version. See D15 for the
   binding question that replaces this ask.
4. **Entry budget is unstated but small.** 10k entries at ~20 bytes plus a count is ~200-300 KB per
   language uncompressed — negligible against the FST payloads. But D11 keeps every accepting
   language resident, so the budget is per-language times the resident set. State a number rather
   than inheriting one.
5. **"Super easy language and the word is fairly full" is a D10 calibration statement**, not a tier.
   It is the per-grammar gate D10 already specifies, now with a stated default: **off unless
   measured cheap**. When the 1% is un-shelved, that is the shape it returns in.

### Consequences for D10 and report 11

D10's calibration scope narrows sharply: with tiers 1-2 shelved at runtime, there is no tier-2
invocation threshold to calibrate and no anytime refinement to schedule. Report 11's "tier-2
invocation must be a value-of-continuing estimate" finding **parks with tier 2** — correct, unused,
and the first thing to reread on un-shelving. What survives from report 11 is the p90 single-stream
latency metric and the reference-device question, which now apply to a much cheaper pipeline and
should be easy to clear.

The anytime contract itself survives and gets simpler, not weaker: tier 0 answers immediately, and
there is nothing slower behind it to degrade into.

---

## D15 — Layer boundary: this is an add-on trained on text, not part of the analyzer pack

**Decided 2026-07-25** (John): *"The warm cache and the precomputed n-gram require a significant
amount of text that can be fully parsed (10k sentences?), again relying upon the double n-gram — one
that looks at the POS and morpho parsing, not just the words. It is truly an add-on to the pack —
and what we are investigating here. What is being built now is just 'is this a word — analyze it' —
we are talking about user features IRL."*

This draws a boundary the plan had been blurring, and it changes what this plan may ask of work in
flight.

| | **Layer 1 — the analyzer** | **Layer 2 — user features** |
|---|---|---|
| Question answered | "Is this a word? What is it?" | "What did you mean? What comes next?" |
| Status | **being built now** (multi-FST rewrite, `pg-pack`, `pg-foma`) | **being investigated here** |
| Artifact | `.pgpack` — grammar + FST payloads | **an add-on**, separately versioned |
| Inputs | a FLEx project | Layer 1 + **a text corpus** |
| Contents | morphotactics, lexicon, rules | warm cache (D14) + D4's two n-grams |

**The add-on is not a pack payload.** D14's item 3 recommended reserving a third framed payload in
`.pgpack` container v1; that is retracted. Coupling a user-features artifact to the analyzer's
container version would make every n-gram retrain a container concern and vice versa. The two have
genuinely different lifecycles: Layer 1 rebuilds when the grammar changes, Layer 2 rebuilds when the
corpus or the ranking design changes.

### What replaces the pack ask: a binding problem, not a format problem

An add-on generated against grammar *G* is meaningless against a different grammar. So it needs a
binding, and the obvious candidate is wrong:

- **`package_fingerprint` is too tight.** It is a SHA-256 over the *payload bytes*
  (`format.rs:146-151`), so it changes on every recompile — including recompiles that change nothing
  the add-on depends on. Bind to it and a routine rebuild silently invalidates a corpus-trained
  artifact.
- **`grammar_id` alone is too loose** — it is documented freeform, with no registry
  (`manifest.rs:41-43`), and it does not change when the lexicon does.

**What the add-on actually depends on is the analyzer's *output vocabulary*:** the POS inventory,
the `syn_fs` feature space, and the morpheme inventory — because those are literally D4's classes. A
grammar edit that leaves those intact leaves the add-on valid; one that renames a POS or drops a
feature invalidates it. So the right binding is a **content digest over the class-defining
inventories**, plus a stated staleness policy (refuse, or warn-and-degrade to a coarser rung).
Whether Layer 1 should export such a digest is the only real Layer-1 ask in this plan, and it is
small.

### The corpus requirement, stated per-component

John's *"significant amount of text that can be fully parsed (10k sentences?)"* is the binding
constraint on this whole layer. It is not one requirement — the three components need very different
amounts, and saying so changes what can be built first:

| Component | What it needs | How much text | Robustness at small size |
|---|---|---|---|
| **Intra-word term** `P(w \| class)` — morpheme n-gram | morpheme sequences | **least** | **best** — morphemes recur where wordforms do not (report 04: Finnish OOV 20% at word level -> 0% at morph level) |
| **Inter-word term** `P(class \| context)` — class trigram | class sequences | moderate, and **rung-dependent** | degrades by rung, not uniformly |
| **Warm cache** (D14) — 10k forms + frequencies | wordform counts | **most** | poor — frequency ranking is exactly what needs volume |

Three consequences follow, and the first two are the useful ones:

1. **Corpus size selects the backoff rung, per grammar.** This is the missing link between report
   13's cardinality measurements and D1's ladder. Rung 1 (decomposition+`syn_fs`) measured 93.5-100%
   singleton classes — unestimable at *any* corpus size we will ever have, which is why D4's ladder
   already shortened to four rungs. The rungs below it become estimable as text accumulates. So "how
   much text do we need" has no single answer: it is "enough for rung *k*," and *k* is a per-grammar
   measurement, exactly as D10 treats tier thresholds.
2. **Generation and ordering are separable, which lowers the cache's bar.** D14 assumes the 10k
   entries are *generated* at build time. Generation needs no corpus at all — only the grammar. The
   corpus is needed to *order* them. So a thin corpus yields a complete-but-poorly-ranked warm cache
   rather than no warm cache, and the add-on degrades gracefully instead of failing to build.
3. **Gold annotation is not required to train, only to measure.** D4 already marginalizes over the
   analysis lattice with fractional counts rather than requiring hard disambiguation, so the
   training input is **raw vernacular text plus the analyzer** — not annotated text. The small gold
   set (FLEx interlinear; ~760 tokens) is the evaluation apparatus, not the training set. This is a
   much lower bar than "10k parsed sentences" suggests and should not be lost.

### Coverage does not merely gate this layer — it biases it

The sharper version of the dependency, and a genuinely new point: a token with no analysis
contributes no class, so it is silently dropped from class-LM training. At report 13's measured
coverage (Sena 49%, Amharic 24%, Indonesian 85%, Aweti 49%) roughly half the corpus disappears — and
per D13's rewrite note, **the dropped half is systematically the morphologically complex half**
(12.4% of Sena's were step-capped, i.e. the engine gave up on complexity rather than rejecting the
word).

So training on a partially-covered corpus does not just yield a smaller class LM, it yields one
**biased toward simple morphology** — the opposite of what the design is for. Effective training size
is raw size x coverage, and the loss is not random. This is a second, independent reason the add-on
sits downstream of the coverage axiom, and it is a reason that survives even if a thin corpus is
somehow enlarged.

### Where the text comes from — partly answered 2026-07-25

> **Status change.** When this section was written the answer was "nowhere, and that is the top
> unknown." Report 18 measured it: **~31.7k word tokens of running text exist inside `Sena 3.fwdata`
> today**, unextracted. That is not the 10^5 this layer wants, but it is enough to *start* the model
> comparison on real data rather than only synthetic, and it retires "we have literally nothing" as
> the framing. The bottleneck moves from *finding any text* to *finding enough text, for more than
> one grammar*.

Candidate sources, licence and domain fit still unchecked except where noted:

- **FLEx interlinear texts** — **measured `[M]`, and the estimate below was wrong.** Not 6,973
  wordforms: `Sena 3.fwdata` carries **31,682 word tokens** across 4,487 sentence-segmented
  `Segment` records (the 6,973 figure was the *type* list, a different quantity). `pg-fwdata`
  extracts none of it (`src/xml.rs:1-6`), so this is a bounded extraction task, not a sourcing
  problem. Sharply asymmetric across projects, though: Aweti has only 555 word tokens, and the
  other reference grammars are untested. **What it is not** is a gold set — see D3's correction.
- **Scripture / Paratext translations** — the only source likely to reach 10^5 tokens for a typical
  target language, and it exists across much of the same organisational footprint. **Stated cost:**
  heavy domain skew. A Scripture-trained class LM predicts Scripture-flavoured text; phone typing is
  not that. Whether the *class*-scale n-gram is more domain-robust than a surface n-gram would be
  (plausible — syntax travels further than vocabulary) is measurable and unmeasured.
- **Dictionary example sentences, community-collected text** — small, but in-domain.

**This is now the top unknown for Layer 2**, ahead of any remaining ranking-design question.

### The one constraint to place on the rewrite — do not collapse ambiguity

The analogue of D8's `p`-bound item: cheap to honour now, expensive to retrofit.

D4 scores over the **analysis lattice**, and its estimation uses fractional counts over competing
analyses. Both require the analyzer to return **all** analyses of a token, weightable — not a single
best one. It does today: `pg_parse::morpher` returns `Vec<WordAnalysis>`
(`rust/crates/pg-parse/src/morpher.rs:137`), and report 13 measured the resulting ambiguity directly
(Sena mean 4.61 / p90 9 / max 78).

If the multi-FST topology ever adds an internal disambiguation or best-analysis step — a natural
thing to want when chasing coverage and latency — **Layer 2 breaks silently**: fractional-count
training would collapse to hard counts and the class LM would be trained on the analyzer's guesses
about its own output. Worth telling the topology work now, alongside the `p`-bound note.

Batch analysis over a corpus is the other Layer-1 need, and it already exists in example form
(`rust/crates/pg-cli/examples/spellcheck_measure.rs`, built by report 13).

---

## D16 — The reference grammars are unrepresentative samples; this is research and plans, never calibration

**Decided 2026-07-25** (John): *"The four grammars we have (and the minimal texts) ARE NOT
REPRESENTATIVE AND SHOULD NOT BE DRIVING DESIGNS. We will have much more data — these are super
small incomplete projects and examples. We want to plan and design for larger sets of data, even
determining what is needed. We don't have any complete grammars and lexicons now, so everything is
just RESEARCH not actual calibration or true implementation. I want only research and plans once we
get the data."*

### This is not new guidance — it is a standing repo rule this plan drifted away from

`docs/fst-plan/synthetic-stress-grammar-plan.md:26-28` already states it for the FST work: *"The
reference grammars are small and unrepresentative (memory: build-for-full-scale — target is
10^4–10^5 entries, every construct, dozens of real stress grammars incoming)."*
`docs/superpowers/specs/2026-07-16-candidate-prefilter-plan.md:110` applies the same rule the other
way round — *"Reference grammars have zero instances (build-for-full-scale rule: implement
anyway...)"* — explicitly refusing to let absence-in-the-samples justify absence-in-the-design.

The spellcheck plan never adopted it. Report 13 arrived as the series' first real measurements, and
their novelty was allowed to carry more weight than their provenance. That was the error, and it is
mine: the correct reading of report 13 was always *"here is what four small sample projects look
like"*, not *"here is what grammars are like."*

### The rule, stated so it can be applied mechanically

1. **A measurement over the current grammars may motivate research. It may never narrow a design,
   set a default, fix a threshold, or retire a capability.**
2. **Absence in the samples is not evidence of absence.** A construct or feature that never fires in
   these four is not thereby rare, dead, or safe to drop.
3. **Presence in the samples is not evidence of typicality either.** This is the symmetric half, and
   the one that is easier to forget. A distribution observed once is a sample of one.
4. **Every design must be sized for full-scale data** — 10^4–10^5 lexical entries, every construct,
   corpora orders of magnitude past what is on disk now.
5. **When a real measurement is needed, generate the shape synthetically and sweep it** rather than
   reading a value off a sample. This is the `research/` harness's actual purpose, and it is why
   its synthetic profiles are named by statistical shape rather than after a project.
   **AMENDED 2026-07-25 (report 22, code-verified):** a synthetic sweep **may eliminate a candidate
   and may never validate one** — the generator's morphology is cleaner and more regular than any
   real language, so failure transfers and success does not. See § "D16 point 5, corrected". Do not
   read this rule as a licence to call a question answered.
6. **Calibration is a later phase, gated on real data.** Nothing here is calibrated. Where this plan
   says "measured", read "observed once, on a sample, pending replication at scale."

### What this does and does not invalidate

It does **not** invalidate the design decisions D1-D15. Those rest on published literature, on
architecture, and on John's product calls — none of which came from the four samples.

> ~~**D14 in particular is untouched**: the traffic model is a statement about how people type,
> corroborated independently by query-autocompletion measurements, not derived from these
> grammars.~~
>
> **WITHDRAWN 2026-07-25 — this exemption was false in both of its halves, and it was D16's own
> violation of D16.** Found independently by the parent session (P0-2) and by report 21 (finding
> #1), which is why it is recorded here rather than only at D14.
>
> 1. **The corroboration half fails.** Query-autocompletion's hyper-recurrent head is a property
>    of *web-search traffic*, not of word-level typing in a polysynthetic language. It is not
>    independent corroboration of this traffic model; it is a different phenomenon.
> 2. **The "not derived from these grammars" half is simply untrue.** D14 selects between the two
>    readings of its own cache design with *"It cannot reach 10k. Report 13's corpora are 6,973
>    wordforms (Sena 3), 673 (Amharic), 121 (Indonesian)"* — a sample number narrowing a design,
>    which is exactly what D16 rule 1 forbids. See the Provisional narrowings table below, where
>    it is now listed.
>
> The substantive disposition of D14 is in D14's own ⚠ box and in ledger row **C4**. The rule this
> episode establishes is in § "Amendments are written at the amended site" below: **a
> meta-decision must never grant a blanket exemption to an object-level decision it has not
> audited line by line.** D16 asserted D14's evidentiary cleanliness without checking D14's text,
> and the assertion was wrong in the direction that made the already-decided design look safer.

What it invalidates is a specific class of **narrowings** that report 13 was allowed to make. Each
is now marked provisional in place; the list is in § "Provisional narrowings" below.

### Provisional narrowings — every place a sample was allowed to shrink the design

| Narrowing | Where | Now |
|---|---|---|
| D4's backoff ladder cut from six rungs to four | D1 § "Backoff ladder", D4 | **PROVISIONAL.** Rung 1's 93.5-100% singleton rate is a property of four small projects. Build the full ladder; let per-grammar measurement pick the operating rung at *install* time, per D10's existing pattern. |
| `mpr` rung called "reliably empty, not reliably dense" | D13 § report 13 findings | **PROVISIONAL.** Nonempty in one of four samples. A complete grammar authored by a linguist who uses MPR features would populate it. Keep the rung. |
| Rung-3 feature-subset selection declared per-POS | D1, D4 | **RETAINED as a hypothesis, not a requirement.** Per-POS concentration is a plausible and useful idea; one observation does not make it the rule. Design must permit per-POS *and* per-grammar selection. |
| Ambiguity sized against Sena's mean 4.61 / p90 9 / max 78 | D4, D13 | **PROVISIONAL, and probably an underestimate.** D13's own rewrite note already argues post-rewrite ambiguity is strictly worse. Size for the worse case; do not tune to 4.61. |
| Coverage arithmetic (24-85%) driving D13's admission bar | D13 | Already superseded by the multi-FST rewrite note. Reaffirmed here. |
| D4's gold-set premise revised down to ~147 contextual tokens | D4, round 2 | **PROVISIONAL and low-stakes.** A real project with real interlinearization has orders of magnitude more. The useful output is not the number but the *requirement*: see § "What data we need". |
| Corpus size ~31.7k treated as the scale to design for | D15, round 2 | **NO.** It is what one sample happens to hold. Design for 10^5-10^6 and state what each model needs. |
| **D14's "it cannot reach 10k"** — 6,973 / 673 / 121 observed wordforms used to choose between the *generated* and the *observed-only* readings of the warm cache | D14 § "The assumption this rests on, flagged for correction" | **ADDED 2026-07-25** (P0-2; report 21 finding #1 and § 4). **PROVISIONAL, and the most consequential entry in this table.** A real project with real text may well hold 10k observed types, so this argument does not establish what it was used to establish. The generative reading may still be right — D14's *other* arguments for it are untouched — but this one is void. Note the direction of the error: it made the generative reading look more necessary than the evidence supports, and the generative reading is what keeps D4 load-bearing. |
| **Raw word-edge phonology ruled out** on 44² = 1,936 and 417² = 173,889 edge pairs against 15,804 / 184 confirmed analyses — "test a *natural-class* edge factor or nothing" | Round 2, finding 4 | **ADDED 2026-07-25** (report 21 § 4). **PROVISIONAL.** The phoneme inventories and analysis counts are four-sample data, and the conclusion retires a capability — the strictest thing D16 rule 4 forbids. The *arithmetic* (a raw edge-pair factor grows as \|phonemes\|²) is architectural and survives; the *verdict* that it is therefore not worth testing does not. Restate as: raw edge phonology is a candidate whose cost scales quadratically, to be compared against the natural-class factor, not eliminated ahead of the comparison. |

### Amendments are written at the amended site — the convention this document now follows

**Adopted 2026-07-25**, on the parent session's P0-4 and report 21's findings #1 and #3, which
found the same defect independently. It is a documentation rule, not a design decision, and it is
the single highest-value fix in the whole review campaign because it costs nothing and it is the
difference between a reader building the current design and building a superseded one.

The defect: **every amendment in this document was written at the amending decision and never at
the amended one.** D14 correctly recorded that it changes D9's ranking rule, D9's intra-word
consequence, and D10's scope — in D14. A reader entering at D9 or D10, as the decision table
invites, got a superseded design with no signal that it was superseded. The same shape produced
D16's false exemption of D14: a meta-decision made a claim about an object-level decision it had
not read line by line.

The rule, in three parts:

1. **A superseding decision must leave a one-line banner at the site it supersedes**, naming the
   amending decision. Not a rewrite — a banner. The original text stays, because the rationale is
   usually still good and only the mechanism changed. D8 → D8a → D8b already did this correctly
   and is the model; D9/D10/D14 did not and have now been retrofitted.
2. **State what survives, not only what changed.** Every banner added in this pass says which part
   of the original conclusion still holds. "Superseded" without that is read as "deleted", and
   three of the four amendments here changed the *mechanism* while leaving the *conclusion* intact.
3. **A meta-decision may not exempt an object-level decision it has not audited.** D16's
   "D14 in particular is untouched" is the counter-example. Blanket exemptions are how a document
   launders an unchecked assumption into a governing rule.

**Cite by section heading, not by line number.** This document accreted ~400 lines during the
review campaign alone and three internal line citations rotted in the process (report 21 rows 9-11,
all confirmed). Absolute line numbers in a file this volatile are a guaranteed future defect.

### What replaces sample-driven design

Three things, in order of when they can happen:

1. **Synthetic sweeps now.** The `research/` harness generates corpora with controllable ambiguity,
   feature richness, class cardinality, and Zipf skew. ~~Every question of the form "at what corpus
   size does rung *k* beat rung *k+1*" is answerable *today* on synthetic data across a range,
   which is strictly more informative than one point read off one project.~~
   **CORRECTED 2026-07-25 by report 22, verified at code level by the parent session — the
   struck sentence was false.** See § "D16 point 5, corrected" immediately below.

### D16 point 5, corrected — synthetic sweeps may eliminate, they may never validate

Report 22 audited this claim against the harness that exists, and the parent session verified every
code citation directly. Two independent problems, both real `[M]`:

**Problem A — the apparatus does not exist.** `research/src/spellcheck_research/models/` contains
exactly one model: `StupidBackoffNgram` (`models/ngram_baseline.py`), a plain **surface** trigram
with no notion of POS, `syn_fs`, or a rung. There is no rung-aware class n-gram in `research/` at
all. "At what corpus size does rung *k* become estimable" cannot be swept today because nothing in
the codebase has a rung *k*. This is ordinary engineering to close, and it should be named as work
rather than assumed done.

**Problem B — and this one does not close with engineering.** The generator's "feature richness" is
a single Bernoulli draw followed by a random subset of a flat three-feature pool
(`synthetic/generator.py:120-129`); its ambiguity is a Poisson draw (`generator.py:132-134`); its
wordforms are `stem_code + "p{p}" + "".join(affixes)` with affixes drawn **unordered, with
replacement**, from a flat per-class pool (`generator.py:106-113`). **There is no nested rung
hierarchy, no morphotactic slot template, no allomorphy, and no paradigm-cell consistency across
stems.** So building the rung-aware model would require first *deciding* what a rung-3 feature
subset looks like — which is the open, per-grammar, per-POS selection problem this plan has never
settled. The sweep would then recover that decision. That is the circularity, and it is structural.

Credit where due: the harness's own code says this about itself (`generator.py:36-42` states
outright that a distractor analysis reuses an unrelated class's morphemes). **The overclaim was in
this document's prose, not in the harness.** Report 18 scoped it honestly as infrastructure.

### The rule that replaces the struck sentence

The composition of D16 point 5 with D17 gives a clean asymmetry that neither report stated, and it
is the durable output of this correction:

> **A synthetic sweep may move a candidate into the *eliminated* column. It may never move one into
> the *validated* column.**

The reason is that synthetic data is generated from a model whose assumptions are *more favourable
and more regular* than real morphology — no allomorphy, no irregularity, no long-tail lexical
idiosyncrasy. Therefore:

- **A failure on synthetic data transfers.** If a model cannot separate two rungs even when the
  generator hands it clean, regular, well-behaved structure, it will not do so on real morphology.
  This is a genuine elimination and it satisfies D17's burden of proof.
- **A success on synthetic data does not transfer.** It shows the plumbing works and the arithmetic
  is favourable under the generator's assumptions. It is evidence about the generator.

This is D17's own asymmetry one level down, and it makes the harness genuinely useful rather than
decorative — **its job is to kill candidates cheaply**, not to bless them. Every sweep should be
designed as an attempted falsification, and a sweep whose only possible outcome is "it worked" is
not worth running.

### What is answerable today, honestly

Of the eight rows in § "What data we need", report 22 judged **zero** genuinely answerable
synthetically in the sense of producing a number that transfers. Several are *pseudo-answerable* (a
number emerges, but it is a property of the generator's parameterization) and several need real data
outright. That section now carries the distinction per row.

The Candidate ledger fares better, and that is the useful split: **C1** (lattice training procedure),
**C2** (smoother choice) and **C7** (class trigram vs. surface trigram) are runnable against the
existing count structure now — under the elimination-only rule above. **C3** needs the rung-aware
model first.
2. **Data requirements now.** For each open question, state the corpus size, annotation level, and
   grammar completeness that would settle it. That is § "What data we need", and it is the
   deliverable John asked for with *"even determining what is needed."*
3. **Calibration later**, when complete grammars and real corpora arrive — per-grammar, at install
   time, using D10's existing measured-pre-flight pattern. Not now, and not from these four.

---

## D17 — The deliverable is a two-column ledger; 2-3 live candidates is the goal, not a failure to decide

**Decided 2026-07-25** (John): *"It is ok having 2-3 different things to try out when we have real
data — that is the goal: 'these things I know won't work because I analyzed it — these things I want
to try out when I get more data'."*

D16 said what the evidence may be used for. D17 says what the **output** is supposed to look like,
and it corrects a bias this plan has had since D4.

### The bias being corrected

This document is written as a decision register, and a decision register creates pressure to
converge. D4 is titled *"The ranking layer **that ships**"*; D5 frames anything neural as an
ablation *against* it. That framing was reasonable when the alternative was an unbounded design
space, but it has a cost: **an option eliminated by argument is recorded identically to an option
eliminated by evidence**, and only the second kind is safe. At D16's evidence standard — no
complete grammar, no real corpus — almost nothing here can have been eliminated by evidence yet.

So the register has been quietly over-converging, and P0-2 in `REVIEW-LOG.md` shows the shape it
takes: D14's argument for the generative reading leans on sample corpus sizes it is not entitled to
use, in the direction that keeps the already-decided D4 load-bearing.

### The two columns, and the standard for each

| | **Eliminated by analysis** | **Deferred to real data** |
|---|---|---|
| What it means | We can say *now*, without new data, that this cannot work | Plausible; the deciding evidence does not exist yet |
| Standard of proof | An argument from architecture, arithmetic, or a published negative result — stated so it can be attacked | A statement of *what measurement would decide it*, per D16 point 2 |
| Cost of being wrong | **High** — a capability is gone and nobody rechecks | Low — it stays on the list |
| Right number of entries | as many as are genuinely proven | **2-3 per open question** |

The asymmetry in row 3 is the whole point. Elimination is the expensive move, so it carries the
burden of proof; deferral is cheap, so ambiguity resolves toward deferral. A candidate is retired
only when the argument against it would survive being handed to someone trying to save it.

### What this changes, concretely

1. **Every "DECIDED" in the table above is re-read as one of three things** — a *product/scope* call
   (D7, D11, D12, D13, D16, D17 itself: John's to make, not data's), an *architectural
   impossibility* (D8's `.zhfst` verdict: an exactness argument that no corpus can overturn), or a
   *leading candidate* (D4, D9, D14: currently top of a list, not the end of one). Only the first
   two are decisions in D17's sense. **The third class must carry its live alternatives.**
2. **D5 is upgraded, not weakened.** "Neural is a bounded late ablation" was already the right
   shape — a named alternative with a bar to clear. That is what D17 asks every candidate to look
   like. D5 is the model, not the exception.
3. **The reviewers' verdicts change shape.** A reviewer finding "UNSUPPORTED" is *not* producing an
   elimination — it is moving an item into the deferred column with a measurement attached. Only
   **BROKEN** — an argument from architecture or arithmetic — eliminates.
4. **The research harness gets its acceptance criterion.** Per D16 point 5 the synthetic sweeps
   exist to answer questions across a range. Under D17 their job is sharper: **discriminate between
   the 2-3 live candidates for each question**, and a sweep that cannot separate them is not worth
   running.

### The re-read, carried out — every decision classified

**Added 2026-07-25** (report 21 § 5, audited by the parent session). D17 above says every
**DECIDED** entry must be re-read as one of three things. That instruction was written and never
executed; here is the execution. It matters because **only the first two columns are decisions in
D17's sense** — the third is a list with one entry on it so far, and each of those must carry its
live alternatives.

| Kind | Decisions | What would change one |
|---|---|---|
| **Product / scope call — John's, and no data overturns them** | D3 (partly), D7, D11, D12, D13, D16, D17, D18's A-vs-B choice | John changing his mind about what the product is for |
| **Architectural impossibility — an argument from invariants, no corpus overturns them** | D1, D8 (`.zhfst` exactness), D8a (the "must ship the engine" half), D3's licensing half (GPL-3.0 vs MIT), D15's "not a pack payload" half | Finding an error in the argument itself |
| **Leading candidate — currently top of a list, not the end of one** | **D2** (ledger C5), **D4** (C1/C2/C3/C7), **D5** (already the model — a named alternative with a bar), **D8b** (C9), **D9** (C10), **D10**, **D14** (C4) | Evidence, per the R-ladder |

Three observations from doing this that were not visible before:

1. **D5 is the compliance model, not the exception**, exactly as D17 claimed — it was the only
   decision that already named its alternative and the bar that alternative must clear.
2. **Several decisions are mixed**, and the mixture is where errors hide. D3, D8a and D15 each have
   an architectural half that is genuinely settled and a scope or design half that is not; stating
   them as single **DECIDED** rows let the settled half lend credibility to the open half. D15's
   grammar-binding digest is the clearest case — proposed as "the right binding", never decided,
   and carrying no alternative.
3. **D10 is the one leading candidate with no ledger row**, and its scope was rewritten twice in a
   day (narrowed by D14, re-widened by C4). See its amendment banner.

### Where the live candidates are recorded

`§ "Candidate ledger"` below, filled by the review campaign — now rows C1-C10. Every open question
gets a row naming its live candidates and the single measurement that would separate them, and the
pointer block beneath the ledger maps each decision to its rows for readers who enter at a decision
rather than here. Where this document already eliminated something, that elimination is re-tested
against the standard above.

---

## D18 — A cache miss is never grounds to flag; flagging requires an attempted parse that failed

**Decided 2026-07-25**, on report 20's finding. The plan has always said what must *not* decide
flagging and never what does.

### The gap, exactly

D9 § "Tiers govern supply, never flagging" establishes a genuinely correct principle — *"A tier is a statement about where
candidates came from and what they cost — **never** about whether anything is an error"* — and rules
out LM-threshold detection with a good argument: in a data-starved language correct-but-rare text is
the norm, so a probability threshold flags correct text constantly. **But ruling out one wrong
mechanism is not supplying a right one**, and no decision anywhere in this document supplies one.
D12 governs whether an orthography is stable enough to *have* a norm, which is a precondition, not a
mechanism.

### Why the gap is dangerous rather than merely untidy

It composes with D14's challenged traffic model into an active harm. Once tiers 1-2 are shelved at
runtime, **the only signal left standing is cache membership**, so the default an implementer
reaches for — *not found ⇒ flag it* — becomes the design by omission. At the uncached rates the
literature actually reports for these language families (D14's warning box: 20-60%+), that flags
correctly-typed, morphologically complex words **en masse**. The words it flags hardest are the long,
richly-inflected ones — precisely the forms that motivated building a morphological speller instead
of a word list.

That is the documented way spellcheckers lose their users: *"a high rate of false positives would be
expected to undermine confidence in a spelling corrector and to be frustratingly distracting"*
`[A, translatehouse.org spellchecker-evaluation guide, via report 20]`.

### The decision

**Absence from the cache is never, by itself, sufficient grounds to flag a word.** Flagging requires
one of:

1. **An attempted parse that failed** — `confirm` was actually run for this specific word and
   returned an empty analysis set. A *skipped* parse is not a failed parse.
2. **An exhausted generative search** — tier 2 ran to completion and came back empty. Shelved is not
   exhausted.

Anything else is silence. **The system may decline to offer a suggestion; it may not assert an
error.** Supply and diagnosis stay separate, which is what D9 wanted and did not enforce.

### ⚠ The rule as stated above still permits the harm it was written to prevent

**Added 2026-07-25 by cross-check B, verified by the parent session.** D18 was written by the parent
session in response to report 20, and nothing reviewed it until now. This is the hole.

**A failed parse does not mean the word is wrong. It means the grammar could not analyze it.** Those
are the same event only if the grammar is complete, and no grammar is complete. So mechanism 1 —
*"`confirm` was actually run for this specific word and returned an empty analysis set"* — fires
identically for:

- a genuinely misspelled word, and
- **a correctly-spelled word that falls in a coverage gap.**

D18 flags both. And the coverage gaps of a morphological grammar are not randomly distributed: they
are concentrated in the rare, the irregular, the recently-coined, and the morphologically elaborate
— **the same population that F8's uncached tokens and F18's dropped training tokens live in.** So
D18, as written, flags correctly-spelled complex words, which is *precisely* the harm the whole
D14/D18 thread exists to prevent. It closed the cache-miss route to a false accusation and left the
coverage-gap route open.

**Why this was missed.** D18's argument was built against the *cache*, which has no opinion about
whether a word is well-formed — so "attempt a real parse" felt like the rigorous alternative. It is
more rigorous. It is not sufficient, because it silently treats *grammar coverage* as ground truth
about *the language*, which is the exact error D16 exists to police, appearing here in a new place.

**D13 is not the answer.** A coverage bar makes the failure rarer; it cannot make it safe. At 95%
analysis-level recall, 1 word in 20 is a candidate false accusation, concentrated in the words
users are least sure about and most likely to defer to the machine on. The residual is what matters
and D13 does not bound it.

**Live candidates — ledger row C13, and this is a real open question, not a fix to write in now:**

| Candidate | Shape | Cost |
|---|---|---|
| **(a) Flag only on high-confidence failure** | Require more than an empty analysis set: the word must also be *unreachable* under error-tolerant search within a small edit distance from something the grammar does accept. "Not a word, and not near one" is a much stronger signal than "not a word" | Needs tier-2-style search at flagging time — the cost D14 shelved. **This is the honest version of option A and it is expensive.** |
| **(b) Never flag; suggest only** | D18's option B | Free, and gives up spell-checking |
| **(c) Flag with calibrated hedging** | Distinguish "I don't recognise this" from "this is wrong" in the *UI*, not just internally | A product/UX call, not a technical one — and it may be the actually-right answer, since it is honest about what the system knows |

**The deciding measurement is the false-accusation rate on correctly-spelled words that fall in
coverage gaps** — which needs a complete-enough grammar and real text (**R1**), and cannot be
estimated from the four samples per D16. Until it exists, **(b) is the only option with a bounded
downside**, which strengthens the § "The research programme" recommendation to ship suggest-only
first rather than weakening it.

### The consequence that makes this expensive, and must not be evaded

D18 and D14 **cannot be fixed independently** — this is the coupling report 20 identified and it is
correct. Honouring D18 means "is this a word" requires running the analyzer at flagging time. D14
shelved that path to protect the latency budget. So the honest options are:

| Option | Flagging | Cost |
|---|---|---|
| **A. Flag only on a completed parse** | correct | the analyzer runs at flagging time — D14's budget question returns |
| **B. Never flag; suggest only** | trivially correct, zero false alarms | gives up spell-*checking*; this is a word-prediction product |
| **C. Flag on cache miss** | **prohibited by this decision** | cheap and wrong |

**B is a real product and should not be dismissed** — a keyboard that only ever offers and never
accuses is a coherent, shippable, and honest first release for a language whose orthography is still
settling, and it interacts well with D12. Under D17, A and B are both live; C is eliminated by
analysis. **This is John's call, and it is a product call, not a technical one.**

### ⚠ Option A is not currently implementable, and the A/B choice is therefore not yet real

**Added 2026-07-25 by report 23 (T1), with the mechanism corrected by the parent session.** This is
the sharpest finding of pass 3 and it invalidates the sentence immediately above.

**Report 23's version, and why it is nearly right.** It argued that A silently degrades into B: F8
says most tokens in the target language family miss the cache; D18 says a skipped parse is not a
failed parse and shelved is not exhausted; D14 shelved the runtime path; therefore no parse is
attempted, no flag can fire, and B is what ships no matter what John chooses. The conclusion is
correct. **The mechanism is not** — and the difference changes the fix.

**What D14 actually shelved.** Tier 1 is *prefix-constrained generation*; tier 2 is *error-tolerant
traversal of a generative FST*. Both are **candidate supply** — producing wordforms the user has not
finished typing. D14 shelves those. **Analysing a wordform the user has already typed is a different
operation**, and it is not tier 0 (a cache lookup), not tier 1, and not tier 2. It is `confirm` run
on one concrete string.

**So the real finding is an absence, not a shelving:** D18 mechanism 1 requires a runtime operation
that **appears nowhere in D9's tier architecture**. D14 did not turn it off; nothing ever turned it
on. D9 enumerates supply and D18 requires diagnosis, and the two were written five decisions apart
without anyone noticing that diagnosis has no home in the architecture.

Three consequences, and the third is good news:

1. **Until an analysis path is added, the product ships as B whatever the table says.** The A/B/C
   table presents a product choice that is not currently available. That framing must not stand.
2. **The fix is an addition, not an un-shelving** — a "tier A" diagnostic path with its own budget
   and its own invocation policy, parallel to the supply tiers rather than inside them. Un-shelving
   tiers 1-2 would not deliver D18; it would deliver *suggestions*, which is a different product.
3. **Option A's cost is a *different* question from D14's, not obviously a smaller one.**
   ~~Option A is cheaper than this document's own cost column claims.~~ **Softened 2026-07-25 by
   cross-check B, which was right to push back.** The cost column says "the analyzer runs at
   flagging time — **D14's budget question returns**." What is defensible is that it does not
   return *in the same form*: D14's budget concerned *error-tolerant traversal of an unbounded
   generative subtree*, whereas analysing one typed string is propose+confirm on a **bounded**
   input. Those are different cost structures and one number should not govern both.
   **What is not defensible is concluding "therefore cheaper."** Bounded input does not imply a
   bounded tail, and the tail is the whole problem: report 13 measured 9.81% timeout (Amharic) and
   12.42% step-capped (Sena) **on the confirm path itself**. The original wording also leaned on an
   uncited claim that this is "the most heavily optimised path in the repo", which is not evidence.
   **The honest statement: the cost of diagnosis is unmeasured, it is structurally different from
   the cost D14 shelved, and N8 exists to characterise it.** Note that candidate (a) in the
   coverage-gap section below — "not a word, *and not near one*" — would reintroduce error-tolerant
   search and with it much of D14's original budget problem, so the two questions are less separable
   than this paragraph originally implied.

**The caveat that keeps this honest — see T4b.** "Cheaper" is not "free", and `confirm` has a
measured heavy tail: report 13 recorded 9.81% timeout on Amharic and 12.42% step-capped on Sena
(research signal only, per D16 — the *shape* is the point, not the values). D18 mechanism 1 puts
that tail inside a synchronous `predict()` call with **no host-enforced timeout** (report 12). A
diagnostic path therefore needs a circuit breaker from its first line of code, and **a word whose
analysis was cut off by the breaker is a *skipped* parse, not a failed one — so it must produce
silence, not a flag.** That is D18's own rule applied to its own implementation, and it is the thing
most likely to be got wrong under deadline.

Recorded as ledger row **C11**. This does not un-decide D18 — the rule (a cache miss never flags) is
untouched and correct. What changes is that **A requires building something, and the plan must say
so** rather than presenting it as a choice already available.

---

## D19 — Prefix-constrained completion, if ever built, is a separate top-k entry point; never a mode of the proposer

**Decided by invariant 2026-07-30** (report 27). This is a *constraint on* a future build, not
authorization to start one. It is recorded as a decision because it is derivable from a repo
invariant rather than from any measurement, so no data will overturn it — and because it is cheap to
state now and expensive to discover after someone has wired a beam search into `propose`.

`CONTEXT.md:311` forbids beam pruning and top-*k* shortcuts **in the proposer**, confining them to
`confirm`/ranking. The reason is that the proposer's contract is over-approximation: it may only
apply language-preserving operations, which is precisely why every wordform the grammar licenses is
reachable and why "we can predict words nobody has typed" is true rather than aspirational (D4
§ "Why this handles unseen wordforms"). A prefix-constrained best-first walk is a top-*k* beam **by
construction** — it exists to return the best few completions and to stop.

Both things are fine simultaneously, but only if the boundary is explicit:

- **It ships as its own entry point** with a stated *"top-k, no recall claim"* contract. Never as a
  `propose` mode, never behind a flag on the proposer, never sharing the proposer's recall test.
- **A measurement of the walk is therefore not a proposer measurement.** Without this boundary, a
  future containment number like report 27's Sena 15% reads as a catastrophic recall regression in
  the analyzer, which it is not — it is a beam-width report about a different component.
- **The confirm gate on what is displayed stays absolute.** The walk changes *how many* candidates
  are paid for, never *whether* a candidate shown to a user was confirmed. Report 27's descent
  ("confirm down the ranked list until k survive") is a budget policy, not a weakening of the gate.
- **The negative cache obeys the same discipline**: a surface abandoned at a budget cap is unproven,
  not refuted, and must not be cached as refuted (see D14 § "Measured 2026-07-30").

**Naming.** Call it *completion* or *prediction*, not *proposing*. D14 § "Why build-time generation
is safe here" already had to make this distinction once for build-time cache generation ("a different
activity from D9 tier 1, and should not reuse its name"); this is the third activity that generates
wordforms, and the three now need three names.

---

## Candidate ledger

Promised by D17. One row per open question: the live candidates, and the single measurement that
would separate them. **A question with one candidate is a warning sign, not an achievement.**

| # | Question | Live candidates | The deciding measurement |
|---|---|---|---|
| C1 | How is the class LM trained over an ambiguous analysis lattice? | (a) uniform 1/*k* fractional weighting; (b) EM/Baum-Welch with capped iterations; (c) silver 1-best seeding, EM off | Tagging accuracy vs. a held-out gold set at several seed sizes. **Merialdo/Elworthy say the sign of EM's effect is not guaranteed positive** — so (a) and (c) are not merely cheaper fallbacks, they may win. |
| C2 | Which smoother, given fractional counts? | (a) expected-count KN (Zhang & Chiang, `P14-1072`); (b) hierarchical Pitman-Yor (HPYLM); (c) plain MKN on rounded counts | Perplexity and acc@1 at 10^3/10^4/10^5 tokens. HPYLM is the entry D4 never had — it is *designed* for small data and uncertainty-aware backoff. |
| C3 | Does the intra-word term earn its place? | (a) `P(morphemes\|class)` at rung 2; (b) unconditioned `P(morphemes)`; (c) weights on the morphotactic FST arcs instead of a separate n-gram | Head-to-head at equal corpus size. If (a) ≈ (b), the class-conditioning framing is dropped. **Currently unmeasured at the rung D4 actually uses.** |
| C4 | Is runtime generation shelved? | (a) shelved (D14 as written); (b) always on; (c) **per-grammar, chosen by D10's calibration** | Measured uncached-token rate per grammar. The literature says (a) is wrong for polysynthetic languages; (c) is the shape that survives both answers. **First measured support for (c), 2026-07-30 (report 27):** the same code and budget delivers rank-1 completion in ~13ms on one grammar and does not fit a keystroke on another, so the answer is a per-grammar number and the pre-flight measurement that produces it already exists. Note the direction — it works on the grammar that needed it least (D14 § "Measured 2026-07-30", point 2). |
| C5 | What fits the error model? | (a) grammar-derived synthetic corruption; (b) generic weighted Levenshtein + key adjacency; (c) cross-language transfer; **(d) logged real corrections from a deployed suggest-only stage 1** (added 2026-07-25) | recall@k on real typos, once any exist. **(b) is the floor (a) must beat.** **(d) is noisy and biased** — see the R4 discussion — so the live question is whether logged pairs are *training data* or only a *validation set* that tells us whether the synthetic distribution in (a) was right. |
| C6 | When is a word flagged? | (a) only on a completed failed parse; (b) never flag — suggest only | Product call (D18). False-alarm rate on correctly-typed complex words is the number that decides it. |
| C7 | What is the inter-word unit? | (a) class trigram (D4); (b) surface trigram (the permanent diagnostic floor); (c) lemma/stem term (round-2 proposal 1); (d) phrase table | acc@1 and KSR at matched corpus size. (b) is the floor everything must beat, per round-2 finding 2. |
| **C8** | **What is D13's admission bar, now that the gate it pointed at is gone?** (report 21 #2) | (a) analysis-level corpus recall above a stated threshold, owned by this plan; (b) the multi-FST topology's own conformance posture, once it exists; (c) no coverage bar — ship, and let D18 option B (suggest-only) absorb the incompleteness | Per-grammar recall against real running text, and the false-alarm rate that incompleteness produces at flagging time. **(c) is not a cop-out** — it is the honest answer if D18 lands on suggest-only, because then incomplete coverage costs suggestions rather than false accusations. |
| **C9** | **What happens to D8b if the `file:`-origin IndexedDB spike fails?** (report 21 §5) | (a) IndexedDB in-worker as designed; (b) hand the durable store to Keyman's user-dictionary epic (**unbuilt** — this is a dependency, not a fallback); (c) in-memory only, cache does not persist across sessions | The spike itself: does IndexedDB open from a `file:`-origin worker on each target platform. **Cheap, isolated, and currently the only unresolved item blocking D8b from being a real decision.** |
| **C10** | **Is strict tier priority — "a generated form never outranks a typed one" — the right supply architecture?** (report 21 §5: D9 is named by D17 as a leading candidate and had **no** live alternative anywhere) | (a) hard tier ordering with a fixed penalty (D9/D14 as written); (b) a single unified score where provenance is one *feature* among D4's terms, not a gate; (c) hard ordering for correction, unified scoring for prediction — the two products may not want the same rule | acc@1 and KSR with provenance as a feature vs. as a gate, on the same candidate set. **(a) is chosen for a stated reason — a learned penalty would be estimated from the starved data it exists to correct for — and that reason weakens as corpus size grows, so this row's answer is corpus-size-dependent by construction.** |
| **C11** | **What supplies D18's "attempted parse", and what stops it?** (report 23 T1/T4b, mechanism corrected) — diagnosis has no home in D9's supply tiers | (a) a **"tier A" diagnostic path**: `confirm` on the typed string, own budget, own circuit breaker, breaker-trip ⇒ silence; (b) **batch/idle diagnosis** — never on the keystroke path, flag on a pause or at document scope, which sidesteps the latency question entirely; (c) **no analysis path** ⇒ D18 option B is the product, stated as a decision rather than arrived at by omission | The `confirm` latency distribution on one typed wordform for a complete grammar — **the tail, not the mean** (report 13's shape: ~10% timeout / ~12% step-capped). **(b) is the underrated entry**: spell-*checking* has never had to be keystroke-synchronous, and every desktop spellchecker flags on a delay. |
| **C12** | **Can Keyman deliver enough left context for D4's inter-word term?** (report 23 T2, verified at primary source) | (a) host context alone; (b) host context + a **rolling in-session buffer** we maintain, which fixes continuous typing and not cold start; (c) inter-word term degrades to unigram when context is short — stated and measured rather than silent | Fraction of real predict() calls with less than one full preceding word of context, and acc@1 loss on those. **Keyman's own worked example for polysynthetic languages requests `leftContextCodePoints: 16` `[A]` — one Inuktitut-scale word can exceed that on its own.** |
| **C13** | **How do we avoid flagging a correctly-spelled word that merely falls in a grammar coverage gap?** (cross-check B — D18 closed the cache-miss route to a false accusation and left this one open) | (a) **"not a word, and not near one"** — require an empty analysis set *and* unreachability under a small-edit-distance search from something the grammar accepts; (b) never flag (D18 option B); (c) **calibrated hedging in the UI** — "I don't recognise this" is a different statement from "this is wrong", and the system can honestly make the first | False-accusation rate on correctly-spelled words that fall in coverage gaps, at a known coverage level. **Needs R1** — unestimable from the four samples per D16. **(a) reintroduces error-tolerant search and with it much of D14's budget problem** (see D18); **(c) is a product/UX call and may be the actually-right answer**, since it is honest about what the system knows. |
| **C14** | **How is the completion set bounded on a grammar whose prefix-extension set is large?** (report 27 — the measured obstacle to keystroke-time completion is the *search*, not confirm) | (a) an **admissible A\* heuristic** — uniform-cost search systematically prefers short completions, which is exactly backwards for an agglutinative language, and this is the cheapest thing untried; (b) a **much better stem prior** — report 27's ranked 47-118 stems trained from 132-421 confirmed analyses, thin enough that the rank figures may be model starvation rather than a ceiling; (c) **bound the free tail by slot depth rather than by byte length**, so the budget is expressed in morphology instead of orthography; (d) **per-grammar off** (D10), i.e. C4 candidate (c) answered "no" for this grammar | Containment@top-*k* and **walk p90** on a *complete* agglutinative grammar — needs **R1**. All four are cheap to try against the existing harness today, and (a)-(c) are elimination-shaped: a heuristic that cannot fix a 39k-state sample will not fix a real grammar. |
| **C15** | **When a candidate surface has many analyses, is its score the sum over them or its single best?** (report 27; distinct from marginalising over *context* analyses, which is settled — see the warning in D4) | (a) **best analysis (max)**; (b) marginalised sum over analyses; (c) sum with a multiplicity penalty or length normalisation — the middle position, and the one nobody has tried | acc@1 on real text at **R1**. Report 27 measured (b) losing catastrophically to (a) on one grammar (rank 114 → 1, 0% → 100% top-3) `[M]` — an elimination-shaped result on one sample, so it motivates preferring (a) and does not license it as a default (D16). |

**Pointer for readers entering at a decision rather than here** (added 2026-07-25, report 21 §5 —
D17 requires a leading candidate to carry its alternatives *at its own site*): **D4**'s live
alternatives are rows **C1, C2, C3, C7** and, for candidate scoring, **C15**; **D9**'s are **C10**
and, for the shelving question, **C4**; **D13**'s is **C8**; **D14**'s are **C4** and, for the
search-cost half measured in 2026-07-30, **C14**; **D18**'s is **C6**; **D2**'s is **C5** and is
also written out in D2's own section; **D8b**'s is **C9**. **D19** carries no ledger row on purpose —
it is a boundary derived from an invariant, and the *build* question it constrains is C4/C14.
**D10** and **D15** are the two
decisions still carrying open design questions with no ledger row — D10's post-D14 scope is stated
in its amendment banner, and D15's grammar-binding digest is proposed with no stated alternative.
Both are recorded here as known gaps rather than silently left out.

---

## The research programme — what runs now, and what each arrival of data unlocks

**Written 2026-07-25**, closing the review campaign. D16 says what evidence may be used for; D17
says the output is a ledger of live candidates; D16-as-corrected says a synthetic sweep may
eliminate but never validate. This section is the operational consequence of all three: **the
order of work, and the trigger for each piece of it.**

It is deliberately organised by *what we have*, not by *what we want to know*, because the binding
constraint is data arrival and everything else is schedulable around it.

### First: these are three products, not one, and they should not ship together

The plan has treated "spelling correction and word suggestion" as one system with one set of
open questions. It is three, and separating them reorders the whole programme, because **their
data hunger and their cost of failure rank in the same order.**

| | **Prediction** (next word, completion) | **Correction** (rank candidates for a word already known to be wrong) | **Checking / flagging** (assert that a word *is* wrong) |
|---|---|---|---|
| What failure looks like | A suggestion the user ignores | The right word is not in the top *k* | A correctly-typed word is marked as an error |
| Cost of failure | **Low** — the user's text is unaffected | **Medium** — the user rejects the list and types on | **High** — the system is wrong *about the user's own language*, in public, and the documented way spellcheckers lose their users `[A, via report 20]` |
| Needs an error model (D2)? | No | **Yes** | Yes, to be useful |
| Needs a settled orthography (D12)? | **Not to be correct**, but see the caveat below | Weakly — a norm to correct *toward* | **Yes, strictly.** Without a norm there is no such thing as an error |
| Needs grammar coverage (D13)? | Degrades gracefully — a gap costs a suggestion | Degrades | **Fails dangerously** — a gap becomes a false accusation, and **D18 does not prevent this**; see D18 § "the rule still permits the harm it was written to prevent" and ledger row **C13** |
| Decidable from text alone? | **Yes** | Partly | No — needs a norm and a real false-alarm rate |
| Governing ledger rows | C1, C2, C3, C7, C10 | C5, C10 | C4, C6, C8 |

**The consequence, and it is the strongest recommendation in this document:** ship them in that
order. Prediction first, correction second, flagging last and only on evidence. This is not
caution for its own sake — it is the only sequence in which **the product earns the data that the
next stage needs** (see the R4 problem below). D18's option B ("never flag; suggest only") has
been recorded as a live product option; read this table as the argument that it is not a fallback
but the correct **stage one**.

> **Caveat on the orthography row, added 2026-07-25 (cross-check B).** "Prediction needs no settled
> orthography" is true of *correctness* — a predictor cannot be wrong about a norm it never asserts
> — and false of *effect*. A predictor trained on inconsistently-spelled text learns the variants,
> then offers them, and F5's finding is that **suggestions bias what people write** (Arnold,
> Chauncey & Gajos, IUI 2020 `[A]`). So a prediction-only product deployed into an unsettled
> orthography **quietly becomes a standardising force**, pushing usage toward whichever variant the
> training corpus happened to favour. That is a real effect on a living language, it is invisible to
> every metric in this plan, and D12 — which governs *scope* — does not address it. It is not a
> reason to hold stage 1 back. It **is** a reason that stage 1 is not the ethically neutral option
> it appears to be, and it belongs in the D7/D12 conversation rather than being discovered later.

### Track N — what runs now, with no *representative* data

*(Heading corrected 2026-07-25, cross-check B: the original said "with no real data at all", which
was wrong — N7, N8 and N9 use real artifacts. The four grammars and the repo are real; what they
are not is **representative**, which is D16's actual claim and the one that binds.)*

Every item is **elimination-shaped**: it is worth running only because a negative result transfers
(D16 point 5, corrected). Each declares the hypothesis it would falsify. **A sweep that cannot
produce a negative is not on this list, and must not be added to it.**

| # | Experiment | Falsifies | Blocked on | Ledger rows |
|---|---|---|---|---|
| **N1** | **Build the rung-aware class n-gram in `research/`.** Today `models/` holds one model, a plain surface trigram (report 22, code-verified). | nothing — it is the apparatus, not an experiment | nothing; ordinary engineering | prerequisite for N2, N3, N5 |
| **N2** | Class trigram vs. **surface trigram** at matched corpus size, swept 10^3→10^6 | "the class model beats the surface floor" — if it cannot win on the generator's *clean, regular* morphology, it will not win on real morphology | N1 | **C7** |
| **N3** | **Weight recovery, not weight stability** (report 22 / F13): generate data from known interpolation weights, then check whether the grid search recovers them, as a function of gold-set size | "a grid search over a small gold set finds the right weights" — a stable wrong optimum is the expected failure and this is the test that sees it | N1 | C1, C2, and D4 § interpolation |
| **N4** | **The compounding-bias probe** (F18): generate a corpus, delete the analyses of its morphologically complex tail to mimic coverage loss, train the class LM on the remainder, measure ranking loss **on the deleted portion** | "build-time generation and query-time ranking are independent enough that coverage bias does not compound" | N1 | C3, C4, and D14 § compounding bias |
| **N5** | Provenance as a **hard gate** vs. provenance as a **feature** in a unified score | "strict tier priority is the right supply architecture at every corpus size" — D9's own rationale is corpus-size-dependent by construction | N1 | **C10** |
| **N6** | **Cache-adequacy falsification**: sweep morphological productivity and type-token ratio and ask **one yes/no question** — *is there any setting of the generator at which a 10k cache reaches 99% token coverage?* (Restated 2026-07-25, cross-check B: the original "measure what cache size is needed" framing was validation-shaped — a required-size number is a property of the generator's parameterisation and transfers to nothing.) | "a ~10k warm cache serves 99% of tokens" — and note the asymmetry: if it fails on the generator's *regular* morphology, it certainly fails on real morphology. This is the cheapest available attack on the number F8 already challenged | nothing — runnable on the current generator | **C4** |
| **N7** | Literature settlement of the items the campaign left open: HPYLM/Pitman-Yor as a live C2 entry, CRF/MaxEnt as a class predictor, copy/pointer mechanisms (F7) | — reading, not sweeping | nothing | C2, C10 |
| **N8** | **The `confirm`-on-one-typed-word latency distribution — the tail, not the mean.** Runnable today on the existing grammars via `pg-cli`. **NOT a deciding measurement** — corrected 2026-07-25 (cross-check B), which caught the parent session committing the exact D16 rule-1 violation this campaign already caught once at D14: the four grammars are unrepresentative, so this **motivates and finds the shape**, and R1 decides. Its legitimate use is as an *elimination*: if the tail is unacceptable on grammars this small, it will not improve on complete ones | "a per-word diagnostic parse fits inside a keystroke budget" — and if it does not, C11 candidate (b), batch/idle diagnosis, becomes the leading entry rather than the fallback | nothing — the pipeline exists and is already instrumented | **C11** |
| **N9** | **Ask Keyman** whether `leftContextCodePoints` above 16 is granted, and what the host ceiling actually is | "the host can feed D4's inter-word term one full preceding word in a polysynthetic language" | nothing — it is a question to a team, not an experiment | **C12** |
| **N10** | **An admissible A\* heuristic in the completion walk** (report 27's own recommended next lever). Uniform-cost best-first prefers short completions, which is backwards for an agglutinative grammar; add an admissible lower bound on the cost-to-accept and re-measure containment@top-3 and walk p90 | "the completion set can be bounded by better search alone" — if an admissible heuristic cannot bring a 39,286-state *sample* grammar inside a keystroke budget, it will not do so for a complete one | nothing — `rust/crates/pg-foma/examples/predict_census.rs` exists and already prints both metrics | **C14** |
| **N11** | **Error tolerance on the typed prefix.** Report 27 matched prefixes exactly, so the "+ some spelling correction" half of the idea is unmeasured. foma-rs already exposes a per-symbol-pair cost matrix (`cmatrix_set_cost`, `cmatrix_default_substitute\|insert\|delete`) — the keyboard-aware edit-cost hook report 03 wanted — but `apply_med` matches whole words, so the tolerance itself has to fold into the walk | "prefix error tolerance is nearly free once the walk exists" — the cost multiplier on an already-marginal search is the whole question, and a bad answer here transfers | N10 is not required but makes the result interpretable | **C14**, and C5's floor candidate (b) |
| **N12** | **Sum-vs-max candidate scoring on every runnable grammar.** Report 27 ran this A/B on one grammar only, where the marginalised sum lost catastrophically. The harness takes `--score sum\|max`, so this is a re-run, not a build | "marginalising a candidate's score over its own analyses helps ranking" — a second grammar showing the same collapse makes (b) eliminable rather than merely suspect | nothing | **C15** |

**N6 is still the one to run first.** It needs no new apparatus, and it attacks the number that the
most architecture rests on. **N10 and N12 are the cheapest items on the list** — both are re-runs of
an existing harness against an existing corpus, and N12 needs only a flag change.

> **N8 is NOT satisfied by report 27's 0.3-1.2ms per-confirm figure** (added 2026-07-30 — the two are
> easy to confuse, and confusing them would retire R-1 on the strength of the wrong measurement).
> That figure is a **median** over candidates a *generative walk* proposed. N8 asks for the **tail**
> of `confirm` on **one wordform the user typed**. Report 13's ~10% timeout / ~12% step-capped on the
> same corpora is the shape N8 exists to characterise, and nothing since has characterised it.

### The rule every sweep must follow

Stated once, because report 22's finding was that the plan had drifted into treating the harness as
a validator: **every sweep declares, before it runs, the sentence it would let us strike.** If the
only possible outcome is "it worked", it is not an experiment, it is a demo. Results go into the
ledger's *eliminated* column or nowhere.

### Track R — the data ladder, and what each rung unlocks

Rungs are defined by **what arrives**, and they are not strictly ordered in time — R3 could arrive
before R1. Each row states what becomes decidable *the day that data exists*, so nothing has to be
re-derived under time pressure later.

| Rung | What arrives | What it decides | What it still cannot decide |
|---|---|---|---|
| **R0** | Running text, no annotation, **incomplete** grammar — this is today (~31.7k Sena tokens, coverage 24-85%) | Almost nothing about model quality. It *does* let us shake down the instrumentation contract below, and stand up the surface-trigram floor on real text. **Ambiguity and coverage measured here measure grammar incompleteness, not the language** (D16). | everything in C1-C10 |
| **R1** | **10^5+ tokens of running text in one language with a *complete* grammar** | The programme's keystone. Decides **C1** (lattice training), **C2** (smoother), **C3** (intra-word term at rung 2), **C7** (inter-word unit), **C10** (gate vs. feature) — and yields the first honest **uncached-token rate**, which is the single number **C4** turns on. | C5, C6 (no errors, no norm-violation data), C8 |
| **R2** | A **second** complete grammar + text | **D11's open bet**: are class-LM scores comparable across languages, or does every grammar need its own calibration? Also the first evidence on whether per-POS rung selection generalises. | as R1 |
| **R3** | A **token-level gold set** (10^3-10^4 tokens) | Evaluation itself — every acc@1/KSR number above becomes checkable rather than self-reported. Unblocks the parked constrained-generation work. N3 tells us how big this set must be *before* we ask anyone to build it. | C5, C6 |
| **R4** | **Real typing telemetry**: keystrokes, corrections, offered-vs-accepted suggestions | The only rung that decides **C5** (what fits the error model — real typos are the only test) and **C6** (when to flag — a real false-alarm rate on correctly-typed complex words). | — |

#### The R4 problem, stated plainly — it is the programme's one genuine circularity

**R4 data can only be produced by a shipped product.** There is no corpus of real spelling errors
for a language with a few hundred speakers, there will not be one, and D2 exists precisely because
we accepted that (MAGEC and Zarma are the evidence that a synthesised error model is a credible
substitute — pending report 24's audit of those numbers). But the *flagging* decision, C6, is the
one that most needs R4 and is also the riskiest thing to ship.

So the circularity is: **the highest-risk feature needs data that only shipping can produce.**

The best available resolution is the shipping order from the table above. Ship prediction — which
needs no error model, no norm, and degrades gracefully — and instrument it, so that the deployment
begins producing the only evidence that can ever settle C5 and C6. **Any plan that ships flagging
first is spending its one irreplaceable asset — user trust — to buy data it could have gathered
more cheaply.**

> **Do not overstate what the log gives you.** The parent session originally wrote that the
> correction log *"is the error corpus"*. **Cross-check B was right that this is too strong**, in
> three ways that compound:
>
> 1. **The "corrected" side has no verified ground truth.** A user who rejects the suggestions and
>    types something else may be fixing a typo — or changing their mind, switching register, or
>    correcting toward a variant spelling that is not the norm. Nothing distinguishes these, so the
>    pairs are **noisily labelled**, not gold.
> 2. **It inherits the same bias, for the third time.** Suggestions come from the warm cache and the
>    class model, both of which under-serve complex forms (F8, F18), so the events the log captures
>    are skewed toward the words the system was already good at. **Errors on the hardest words are
>    the least likely to produce a logged correction event**, because no plausible suggestion was
>    offered to reject.
> 3. **Stage 1 may not generate the events at all.** With tier 2 shelved there is no error-tolerant
>    correction flow, so the classic "system offered the fix, user took it" signal is largely absent
>    by construction. The signal that *does* survive is coarser: **backspace-and-retype** — a user
>    deleting back into a word and typing a different one. That is observable from context alone
>    (D8b's mechanism) and it is the most likely real source of (wrong → intended) pairs in stage 1.
>    **It requires deliberate instrumentation** and is now item 6 of the contract below.
>
> **What survives.** The log is a **lead source, not a labelled corpus** — a stream of weak,
> biased, cheaply-collected hypotheses about real errors in a language that has none recorded.
> Against a baseline of *zero* real error data that is still transformative, and it is still the
> argument for shipping prediction first. But **D2's synthetic corruption does not become
> unnecessary when the log arrives.** The two are complementary: synthetic pairs are clean and
> unrepresentative, logged pairs are representative and noisy, and the interesting question — now
> ledger row **C5** candidate (d) — is whether logged pairs are best used as *training data* or
> merely as a *validation set* that tells us whether the synthetic distribution was right.

### The instrumentation contract — build this now, because retrofitting it is expensive

D15's own test is *"cheap to honour now, expensive to retrofit."* These five items cost almost
nothing at build time and are unrecoverable afterwards: data not logged in the first release is
data that does not exist when the question is finally asked. All of it is subject to D7 — local
first, opt-in, and nothing leaves the device without consent.

> **Correction 2026-07-25 (cross-check B): items 1 and 4 are not free, because the hook they assume
> does not exist.** D8a establishes, verified against the interface, that **there is no
> learn/persist/accept hook anywhere on Keyman's `LexicalModel`** — the model is never told that a
> suggestion was accepted. The parent session wrote items 1 and 4 as though it were. They are still
> worth doing, but they are **inferences, not observations**, and the inference is D8b's own
> mechanism: we know what we offered on the previous call, and we see the resulting context on the
> next one, so acceptance can be *inferred* by matching our prior suggestions against what
> subsequently appeared. That inference is lossy — a user may type the same word we happened to
> suggest — so what gets recorded is a **confidence, not a bit**, and *how good that inference is*
> is itself an open question. Items 2, 3, 5 are unaffected and remain straightforwardly ours.

1. **Provenance on every accumulated wordform: `typed` vs `accepted-suggestion`** (F5). Without it, a
   wrong suggestion accepted once is indistinguishable from a word the user chose, so it is
   reinforced and offered again. Arnold, Chauncey & Gajos (IUI 2020) establishes the precondition —
   suggestions bias output toward the model's own predictions. **Inferred, not observed** (see the
   correction above); record the inference and its confidence, and never let an inferred-accepted
   word carry the same weight as a typed one.
2. **An uncached-token counter.** Tokens seen ÷ tokens found in the warm cache. This is the number
   **C4** turns on, F8 says the plan's estimate for it is wrong by one to two orders of magnitude,
   and it costs one increment per lookup.
3. **A three-way parse-outcome counter: `parsed` / `failed` / `skipped`.** D18 turns on the
   distinction between a parse that *failed* and one that was never *run*. If the runtime cannot
   report which happened, D18 is unenforceable — it becomes a comment rather than a mechanism.
4. **A suggestion-outcome record: offered → accepted / ignored / rejected-and-here-is-what-was-typed-instead.**
   The third case is the error corpus. This is the highest-value item on the list and the one most
   likely to be dropped as "analytics we don't need yet".
5. **A per-grammar record of which operating point D10 selected, and on what measurement.** Without
   it, a per-grammar calibration is an unreproducible accident.
6. **Backspace-and-retype events** (added 2026-07-25, cross-check B). A user deleting back into a
   completed word and typing a different one is **the most likely genuine source of (wrong →
   intended) pairs in a suggest-only stage 1**, and unlike items 1 and 4 it needs no accept hook —
   it is visible in context alone. It is also the item nobody would think to add later, because its
   value is invisible until you go looking for an error corpus and find the log has none.

### What this programme cannot do, said explicitly

- It cannot **validate** anything before R1. Under D16-as-corrected, everything in track N can only
  strike candidates.
- It cannot decide **C5** or **C6** before R4, and R4 requires shipping. Those two rows will remain
  open through the first release **by construction**, not by neglect. Any document that later shows
  them as settled without an R4 arrival is wrong.
- It cannot fix **incomplete grammar coverage**, which remains the top blocker (below) and is not
  this plan's to solve. Several questions, run today, would measure incompleteness and report it as
  a language property.

---

## What data we need — the requirement, stated per open question

Written per D16 point 2. This is a **specification of inputs**, not a wish list: each row says what
would have to exist for the question to be answerable, so that when data arrives we know
immediately what it unlocks. Sizes are order-of-magnitude research estimates `[S]` unless a cited
source pins them.

### Corpus requirements

The **last two columns were promised by the D16 amendment and are added here 2026-07-25**, closing
the gap report 22 opened: "answerable synthetically?" is answered per row, in the corrected sense
(*eliminate only, never validate*), and each row is tied to the data rung that actually settles it.

| Question to settle | Text needed | Annotation needed | Grammar needed | Answerable synthetically? | Settled at |
|---|---|---|---|---|---|
| Does the intra-word morpheme n-gram beat a surface n-gram, and by how much? | 10^4-10^5 tokens | none — analyzer output suffices (D15) | one complete grammar | **Elimination only** — a loss on clean generated morphology transfers; a win does not (N2) | **R1** |
| At what corpus size does each backoff rung become estimable? | a **sweep**: 10^3, 10^4, 10^5, 10^6 tokens of the same language | none | one complete grammar, full feature inventory | **No — pseudo-answerable.** The generator has no nested rung hierarchy, so a sweep would first have to *assume* the rung-3 encoding it is meant to discover (report 22, Problem B). The number it returns is a property of that assumption | **R1** |
| Is a lemma/stem term worth its weight (round-2 proposal 1)? | 10^4-10^5 tokens | none | complete lexicon — this is the one that most needs *lexical* completeness | **No.** The generator has no lexical semantics and no realistic stem-frequency structure; a lemma term's value is exactly the thing it does not model | **R1** |
| Does a phrase table beat a general n-gram on the same data? | 10^4-10^5 tokens, **in-domain** | none | none — surface-level mining works without a grammar | **No.** Collocation is a fact about real language use; the generator draws affixes independently and has no collocational structure to find | **R0-R1** — the one row that needs no grammar, so real text alone moves it |
| Do cross-word phonological effects help at all? | 10^5 tokens | none | grammar with a full phoneme inventory and natural classes | **Elimination only**, and only for the *state-space* half: the quadratic growth argument is architectural and already made | **R1** |
| Can tag bundles be predicted from context well enough for constrained generation? | 10^4+ tokens | **gold or high-confidence silver, token-level** — this is the annotation-hungry one | complete grammar | **No** — needs annotation the generator cannot make meaningful | **R3** |
| Are class-LM scores comparable across languages (D11's open bet)? | two languages, 10^4+ tokens each | none | two complete grammars | **Elimination only** — if scores are incomparable across two *synthetic* profiles they will not be comparable across two real languages | **R2** |
| What is the real ambiguity distribution? | any running text | none | **complete** grammar — ambiguity on an incomplete grammar measures incompleteness, not ambiguity | **No.** Ambiguity is a Poisson draw in the generator (`generator.py:132-134`) — a sweep recovers the parameter it was given | **R1** |
| **What fraction of typed tokens miss the warm cache?** (added — this is C4, and it was not in this table) | 10^5+ tokens, ideally real typing rather than edited prose | none | one complete grammar | **Elimination only, and worth running now (N6)** — if a 10k cache cannot cover *X*% of the generator's regular morphology, it will not cover *X*% of real morphology | **R1** for text; **R4** for real typing |
| **What is the false-alarm rate on correctly-typed complex words?** (added — this is C6, the number D18 turns on) | — | — | — | **No, in principle.** A false alarm is a disagreement with a human's judgement of their own language. Nothing synthetic contains that | **R4 only** |

### The three inputs we do not have, in priority order

1. **A complete grammar with a complete lexicon.** Nothing above is fully answerable without one,
   and several questions measure grammar incompleteness if run today. This is the top blocker and
   it is not ours to solve here.
2. **10^5 tokens of running text in one language with a complete grammar.** The pairing matters:
   text without its grammar, or a grammar without its text, unlocks nothing.
3. **A token-level annotated evaluation set.** Needed only for evaluation and for the parked
   constrained-generation work, not for training (D15). Size requirement is genuinely unknown and
   is itself a research question the synthetic harness can bound — measure how large a gold set has
   to be before grid-searched weights stop moving, on synthetic data, and use that as the ask.

### What we can do meanwhile, without any of it

**Superseded 2026-07-25 by § "The research programme" above** — this list said "synthetic sweeps
for every question marked sweep", which report 22 showed was an overclaim. The corrected,
per-experiment list is **track N**. What remains true here:

- Literature work, which is what reports 01-24 are.
- Design and plans, marked provisional, sized for full-scale data.
- **Building the extraction path** (round-2 proposal 3) so that when real projects arrive their text
  is reachable. That is bounded engineering whose value does not depend on the current samples —
  and per the R-ladder it is what converts an arriving project into **R1** on day one instead of
  month three.
- **The instrumentation contract** (§ "The research programme"). Five items, all cheap now and
  unrecoverable later. This is the highest-leverage work available today, because it is the only
  work that changes *what data will exist* rather than what we know about the data that does not.

---

# Research round 2 — reports 14-18 (2026-07-25)

Commissioned by John: *"Send sonnet subagents to research n-grams for this — we will need some
research code here (python — separate root folder) to see how well different types of systems work
— and it may even be language dependent... We will likely ship multiple at the same time, ideally
self-updating based upon what people type."* Five agents; reports `14`-`18`, the parked plan
`parked-constrained-generation-plan.md`, and the `research/` harness.

**Nothing below is a decision.** These are findings and proposals; the decisions John needs to make
are listed at the end.

## What changed in the existing plan

| Existing claim | Status after round 2 |
|---|---|
| D3: FLEx interlinear is "gold annotation, the scarcest resource" | **Half-refuted `[M]`.** Real corpus (31,682 Sena word tokens), but only 0.46% token-level annotated. Corpus yes, gold set no. |
| D4: interpolation weights grid-searched on "~760 gold tokens" | **Corrected `[M]`.** ~147 contextual gold tokens. Silver-set projection is the cheap fix. |
| D15: "the training corpus is the top unknown" | **Downgraded.** Text exists and is unextracted; the unknown is now *enough* text, for more than one grammar. |
| D9's tier-0 seen-word cache | **Renamed, not changed.** It is a cache language model in the Kuhn & De Mori (1990) sense. Naming it that way makes existing literature applicable. |

## Findings worth carrying

1. **A finite lookup beats a learned model on the majority case, and that is measured** — query
   autocompletion's MPC baseline: MRR .570 on seen queries against a neural model's .427, while
   scoring .000 on unseen `[A, arXiv:1909.00599]`. Independent corroboration of D14's traffic model
   from a different domain. Note the *overall* scores tie; the neural model buys tail coverage by
   giving back head accuracy, which is exactly the trade D14 declines.
2. **The surface n-gram is a permanent diagnostic, never the ranking layer.** Finnish morph-LM work
   trained on 40M words still carried 20% word-level OOV `[A for the OOV figure; the 40M token count
   is disputed — see D14's ⚠ box, report 24]` — three to four orders of magnitude
   past our ceiling. Keep it in the harness forever as a floor: any model that cannot beat it is
   broken.
3. **A phrase/collocation table is a second D14-shaped artifact worth shipping**, mined with
   Dunning log-likelihood (not PMI) over D4's analysis stream rather than a third pipeline. It is a
   finite list, so it ships in the pack and searches exactly — same architecture as the warm cache.
4. **Two clean negatives.** Raw word-edge phonology blows the state space (44² = 1,936 edge pairs
   for one grammar, 417² = 173,889 for another, against 15,804 and 184 confirmed analyses) — test a
   *natural-class* edge factor or nothing. And an LM must **not** be fused into the proposing FST:
   compose/minimize cannot honour D10's anytime contract, and `ComposeBudget` was calibrated for
   compile-time nets. Keep the LM a downstream lattice-rescoring term.
5. **The evaluation trap is measured, not hypothetical** — suggestion selection time scales
   +610 ms per suggestion (R²=.98) with no net composition-speed gain `[A, arXiv:2101.09157]`. Any
   harness reporting keystroke savings alone can show a win while the user gets slower.
6. **Constrained generation: the literature validates the reframe.** CoNLL-SIGMORPHON 2018 Task 2
   Track 2 (predict tags from context *and* generate) at low resource: best system 38.60% against a
   copy-the-lemma baseline of 36.62%, neural baseline 2.19% `[A]`. The half that fails at our scale
   is exactly the half PanGloss skips, because we own the generator. Conformal prediction is the
   instrument for John's "90% confidence" — prediction *sets* with a coverage guarantee, whose size
   bounds generation cost directly. Parked; see `parked-constrained-generation-plan.md`.

## Proposals awaiting John's decision

1. **Add a lemma/stem term to D4** as a fourth additive log-space factor with its own backoff —
   *not* a fifth rung in D1's ladder. Report 04 quotes Bilmes & Kirchhoff's four canonical factored-LM
   factors as word/stem/class/POS; D4 implements two and silently drops stem. Caveat: no controlled
   lemma-vs-POS-vs-surface comparison exists, and attested-lemma cardinality is unmeasured. Note
   `resolve_morpheme` (`pg-parse/src/morpher.rs:1103`) is private and a linear scan — it needs an
   index before it can run at LM-scoring frequency.
2. **Ship the phrase table** as a second cache artifact alongside the warm cache (finding 3).
3. **Open an OpenSpec change to extract interlinear text in `pg-fwdata`.** This is the unlock for
   everything in Layer 2 and it is bounded engineering, not research — the records are already in
   the raw stream and discarded by class name. Sibling to `import-writing-system-data`.
4. **Run the silver-set projection measurement** (D4's corrected premise): project single-analysis
   wordform types onto their token occurrences and report how much usable evaluation data results.
   Free, and it sizes the evaluation problem.

## Verification note

Every report was reviewed against source before commit. Corrections applied: report 16's
"measured lemma:wordform ratios" retagged `[S]` (numerator and denominator came from different
populations); report 14's headline MPC figures retagged from `[M]` to `[A]` (they are external);
report 18's token counts corrected for punctuation (~31.7k word tokens, not ~40k), its
"previously undocumented" second Sena project identified as a near-duplicate, and the type-level
vs token-level annotation distinction added. Report 17's code claims and report 16's
`resolve_morpheme` citation were verified and hold. **Not independently verified:** the
SIGMORPHON digits in finding 6 (no PDF rendering available here; the table is internally
consistent and the direction is corroborated by report 08).

---

# Research round 3 — report 27 (2026-07-30)

Commissioned by John: *"We CAN do word prediction for words we've never seen before! We just start
the FST and constrain it with the letters already typed (+ some spelling correction), and then 'let it
run' to get X number of words at the end — but don't run HC to prune them (key point). Any that come
up as real candidates, then we can run HC — only if it makes it in the top 5 or so. Since we know the
morphemes of the FST candidates, we should be able to have the same 'you are the right class — you are
a common stem — you are the right POS'."* Then: *"Measure it, but constrain to top 3, bring in total
'stem' probability, and cache known 'FST yes, HC no' words."*

One report, `27-prefix-constrained-fst-prediction.md`, and a dev-only measurement binary
(`rust/crates/pg-foma/examples/predict_census.rs`). **Unlike rounds 1 and 2 this round is measurement,
not literature** — every number in report 27 is `[M]`.

## What changed in the existing plan

| Existing claim | Status after round 3 |
|---|---|
| D14/report 17/parked plan: keystroke-time generation is unaffordable *because of the analyzer confirm cost* | **Refuted `[M]`.** Confirm is 0.3-1.2ms — the cheap half. The completion **search** is the 4-788ms half. The shelving conclusion may survive; its stated reason does not. |
| Report 17 §6: a lazy prefix-aware enumeration engine is the blocking prerequisite | **True of report 17's mechanism, irrelevant to this one `[M]`.** foma-rs exposes the compiled arc table (`Fsm::states`, CSR blocks) and the analysis tape carries only tag symbols, so one walk yields the surface string *and* its morpheme decomposition. No new engine, no upstream change. |
| "Bring in total stem probability" (the commissioning instruction) | **Contradicted by measurement `[M]`.** Marginalising over a candidate's own analyses rewards multiplicity; best-path scoring moved the true word from rank 114 to 1. Amended at D4; ledger row **C15**. |
| "Cache known 'FST yes, HC no' words" (the commissioning instruction) | **Correct and cheap, aimed at the wrong cost `[M]`.** Saves 0.1-0.7 confirms/keystroke at ~0.5ms each. The cacheable expensive thing is the *walk result per prefix*. |
| D9/D14: unseen wordforms are reachable in principle | **Demonstrated end to end on one grammar `[M]`** — 100% containment from a 4-char prefix, rank 1, ~13ms. Not demonstrated on the other. |

## Findings worth carrying

1. **The recall guarantee is structural and free, and it is the reason this is not just autocomplete.**
   The proposer over-approximates and admits only language-preserving operations
   (`CONTEXT.md:271,311`), so a superset of the relation is a superset of its surface projection: every
   wordform the *grammar* licenses is in the walk's search space, corpus or no corpus `[S]`. No cache
   and no n-gram tier can offer that. It is bounded by the lexicon, though — unseen **wordforms**, not
   unseen **stems**, so a borrowing or a proper name is still unreachable.
2. **The ranking signal needs no predictor.** Report 17's expensive half — predict the tag bundle,
   then calibrate the prediction conformally — existed to *guess* the analysis before generating.
   Walking the network hands you each candidate's morpheme sequence, so "right class / common stem /
   right POS" is a table lookup on symbols the walk already emitted, ranked after the fact `[M]`.
3. **The split is by grammar, not by language family, and it must be measured per grammar.** Same
   code, same budgets: rank 1 at 13ms on one, rank ~98 at 142-788ms on the other. This is the
   spellcheck instance of the repo's standing rule that strategy is chosen by per-grammar pre-flight
   measurement rather than guessed from language type.
4. **A measurement instrument produced plausible numbers while broken, twice, in opposite
   directions.** A depth-first walk (truncating an arbitrary branch, not the ranking tail) manufactured
   a false "20-50ms per confirm"; a missing candidate dedupe made the confirm descent spend its whole
   budget inside rank 1 and report 0% acceptance everywhere. Both produced *believable* output. What
   caught them was a self-check that runs production `FomaProposer::propose` + `confirm_all` against
   the harness's own walk + `confirm_all` on the same words every run and prints agreement — and its
   agreement rate independently reproduced report 13's Sena coverage figure. **Any future harness in
   this series should carry an equivalent check.**
5. **Report 27's own honest limits.** 20-40 held-out words per grammar per prefix length; stem priors
   trained on 132-421 confirmed analyses (47-118 distinct stems), thin enough that the rank figures
   may be model starvation rather than a ceiling; Amharic and Aweti not run at all (report 13
   measured 9.81% and 6.73% confirm timeouts and the harness has no per-word timeout); prefixes
   matched exactly, so the error-tolerance half of the idea is unmeasured.

## Proposals awaiting John's decision

1. **Run N10 (A\* heuristic) and N12 (sum-vs-max on the other grammars) before anything else here.**
   Both are re-runs of the existing harness — N12 is a flag change — and together they decide whether
   C14 has a cheap answer or needs R1.
2. **Decide whether the completion walk is worth a real implementation for the grammars where it
   already works**, or waits for C14. D19 constrains *how* it would ship if so (a separate top-k entry
   point, never a proposer mode); it does not answer whether.
3. **Fold N11 (prefix error tolerance) in early if the walk is built at all.** The cost matrix that
   makes it keyboard-aware already exists in foma-rs, and the tolerance changes the search's cost
   profile — retrofitting it onto a tuned walk means re-tuning the walk.
4. **Leave report 17's parked plan parked.** Report 27 measured a *different* mechanism; the parked
   plan's un-park trigger concerns prerequisites (a lazy enumeration engine, a tag-bundle predictor,
   conformal calibration, R3's gold set) that none of these numbers touch.

## Verification note

Report 27's code claims were re-read at source before commit (`foma-0.4.2/src/line_table.rs`,
`pg-foma/src/tags.rs`, `pg-foma/src/analyzer.rs`, `CONTEXT.md:271,311`). Every number is `[M]` from a
harness that self-checks against the production path on each run; the two instrument bugs above were
found and fixed *before* any number in the report was recorded, and the results were reproduced after
the fixes. **Corrected in place during the session and worth stating because the wrong figure was
reported to John before the right one:** per-confirm cost is 0.3-1.2ms, not the 20-50ms first read off
the depth-first version. **Not independently verified:** nothing — but see finding 5 for what was not
*measured*, which is the larger caveat.
