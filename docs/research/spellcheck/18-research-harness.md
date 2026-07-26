# Research infrastructure: does running text exist, and a Python model-comparison harness

Report 18 in the spell-checking research series. Two independent deliverables: **Part 1**
settles a factual question D15 named as "the top unknown" — does running text exist for
training D4's n-grams, and what would it take to get at it. **Part 2** is a self-contained
Python research harness (`research/`, new top-level directory) that the sibling reports
14-17's n-gram model families plug into for comparison. Labeling convention (matches
report 13): `[M]` = measured/observed directly in this session, `[A]` = asserted with a
cited source, `[S]` = my own speculation/derivation, flagged as such.

## Verdict up front

**Part 1 — running text exists, in real volume, but it is not gold-annotated at the token
level, and PLAN.md's D3 characterization overstates what is actually linked per-token.**
`[M]` Three real `.fwdata` projects were inspected directly (not sampled from a secondary
source): `Sena 3.fwdata` (the same file report 13 used), a **previously undocumented**
54.6MB project `Sena_InterlinearTraining.fwdata`, and `aweti.fwdata`. All three carry real
`Text` -> `StText` -> `StTxtPara` -> `Segment` structures — genuine sentence-segmented
running text with free translations attached, not just the flat wordform-type inventory
report 13 sampled. Sena's two projects together show **~40,000+ running-text tokens each**
(4,487 and 4,558 segments respectively). But token-level analysis linkage — the thing
D3 called "gold annotation, the scarcest resource in this entire problem" — is present on
only **0.0-0.4% of tokens** in both Sena projects; the remaining ~99.6-100% of `Segment`
token references point at bare, unanalyzed `WfiWordform` records or `PunctuationForm`.
Aweti is the exception: a much smaller running-text corpus (80 segments, 666 tokens) but
**73.1%** of its tokens carry a direct link to a specific `WfiGloss`. **None of this is
extracted by `pg-fwdata` today** — confirmed directly in source, not inferred: the
extractor's own doc comment names exactly these classes as skipped
(`rust/crates/pg-fwdata/src/xml.rs:1-6`). Getting at it is a bounded, well-scoped addition
(the classes are visible in the raw stream already; the extractor discards them by class
name), not a research problem — see § What it would take.

**Part 2 — a working Python research harness exists in `research/`,** runs end-to-end on
zero real-language data, and its 26 tests pass in ~2 seconds. A surface-wordform Stupid
Backoff trigram is fit and evaluated against four named synthetic-corpus shapes (recall@k,
MRR, keystroke savings, perplexity), with real pasted output in § What runs today. The
interchange format (JSONL, one record per token, full analysis lattice never collapsed to
one "best" reading) is the load-bearing design decision and is documented with a worked
example in `research/docs/interchange-format.md`.

---

## Independently re-verified by the parent session, 2026-07-25 `[M]`

The central finding **holds**: running text exists in real volume, and it is essentially
unannotated at the token level. Segment counts reproduced exactly (4,487 / 4,558 / 80). An
independent pass over the same three files (guid→class index, then resolving every
`<Analyses>` `objsur` inside every `Segment` record) corrects three numbers and adds one
distinction that changes what the corpus is good for.

| | Sena 3 | Sena_InterlinearTraining | Aweti |
|---|---|---|---|
| `Segment` records | 4,487 | 4,558 | 80 |
| Analysis slots (incl. punctuation) | 40,255 | 40,650 | 666 |
| — of which `PunctuationForm` | 8,573 (21.30%) | 8,854 (21.78%) | 111 (16.67%) |
| **Word tokens** | **31,682** | **31,796** | **555** |
| Slots resolving to `WfiGloss` | 145 | 6 | 487 |
| Slots resolving to `WfiAnalysis` | 2 | 0 | 1 |
| **Disambiguated word tokens** | **147 (0.46%)** | **6 (0.02%)** | **488 (87.9%)** |
| `WfiWordform` types in inventory | 6,973 | 6,615 | 208 |
| `WfiAnalysis` records in inventory | 760 | 66 | 262 |
| `Text` records | 4 | 8 | 1 |

**Correction 1 — "~40,000 tokens" counts punctuation as tokens.** Roughly 21% of every
Sena segment's analysis slots are `PunctuationForm`. The usable running-text figure is
**~31.7k word tokens**, not ~40k. Still the largest corpus available to this project by a
factor of ~4.5 over the 6,973-form type list, and still the finding that matters.

**Correction 2 — `Sena_InterlinearTraining` is not a second corpus.** It is a near-duplicate
of `Sena 3`: it adds ~114 word tokens (31,796 vs 31,682) and carries **1/11th** the analysis
records (66 vs 760). Its 8 `Text` records against Sena 3's 4 suggest overlapping rather than
additional content. Treat the two as one corpus needing de-duplication, not as ~63k tokens.
`Sena 3` is the better-annotated of the pair and remains the right file to work from.

**Correction 3, and the one that bites — type-level analyses are not token-level gold.**
Sena 3's 760 `WfiAnalysis` records (the number report 13 quotes, and the number D4's
weight-tuning premise rests on) are analyses **owned by wordform types**, not anchored at
text positions. Only **147 of 31,682 word tokens (0.46%)** carry a disambiguated reading in
context. So for any *contextual* evaluation — tuning D4's interpolation weights, measuring
tag-prediction accuracy for the parked constrained-generation plan — the usable gold set is
roughly **147 tokens, not 760**. That is five times smaller than assumed and is the binding
constraint on evaluation, distinct from the training-data constraint.

**The mitigation, which is real:** a wordform type carrying exactly one analysis is
unambiguous wherever it occurs, so type-level analyses can be projected onto their token
occurrences to bootstrap a larger silver set. Sena's measured ambiguity (mean 4.61, median
4) says most types will not qualify, but the singleton-analysis subset is free and unmeasured.
**This is the cheapest next measurement available.**

**Aweti's 73.1%** is the share of all slots including punctuation; against word tokens alone
it is **87.9%** — densely glossed, and still only 555 tokens.

---

## Part 1 — Does any running text exist?

### Method

Rather than trust `PLAN.md`'s existing characterization (`docs/fwdata-import-plan.md:81`,
quoted there as: the snapshot carries "only parser-relevant data (no texts, wordform
analyses...)"), I inspected the raw XML of three real `.fwdata` files directly with a small
Python script (not committed — session-scratchpad only, per instructions; reproducible in
a few lines, shown below) that:

1. Indexes every `<rt class="X" guid="G" ...>` record by class, so I could count classes
   present (`Text`, `StText`, `StTxtPara`, `Segment`, `WfiWordform`, `WfiAnalysis`,
   `WfiGloss`, `WfiMorphBundle`, `PunctuationForm`, ...) directly, not from documentation.
2. For every `Segment` record, extracts its `<Analyses><objsur guid="..." /></Analyses>`
   list (LibLCM's per-token occurrence pointer) and resolves each referenced guid's class
   from the same index — this answers "what does a running-text token actually point at":
   a specific chosen analysis (`WfiGloss`/`WfiAnalysis`), or just the bare wordform type
   (`WfiWordform`), or punctuation (`PunctuationForm`).

```python
class_re = re.compile(r'<rt class="(\w+)" guid="([0-9a-fA-F-]+)"')
seg_re = re.compile(r'<rt class="Segment"[^>]*>(.*?)</rt>', re.DOTALL)
analyses_re = re.compile(r'<Analyses>(.*?)</Analyses>', re.DOTALL)
objsur_re = re.compile(r'<objsur guid="([0-9a-fA-F-]+)"')
# build guid -> class map, then for every Segment resolve each Analyses objsur's target class
```

Three files, all in the local (gitignored) sample/demo-project set already named
elsewhere in this research series:

- `C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects\Sena 3\Sena 3.fwdata` — the
  exact file report 13 used (55.9MB).
- `C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects\Sena_InterlinearTraining\Sena_InterlinearTraining.fwdata`
  — **54.6MB, a real FieldWorks project, present on this machine, and not mentioned
  anywhere in this repository before this report** (`grep -r InterlinearTraining` over the
  whole worktree: zero hits, checked directly).
- `C:\Users\johnm\Documents\repos\PanGloss\samples\data\aweti.fwdata` — the same Aweti
  project used throughout this series (11.5MB).

### What a real `.fwdata` project actually contains

`[M]` Class counts, direct from each file:

| Class | Sena 3 | Sena_InterlinearTraining | Aweti |
|---|---|---|---|
| `Text` | (not separately counted; see `StText` below) | 8 | 1 |
| `StText` | 9,955 | 9,958 | 20 |
| `StTxtPara` | 9,673 | 9,753 | 172 |
| `Segment` | 4,487 | 4,558 | 80 |
| `WfiWordform` | 6,973 | 6,615 | 208 |
| `WfiAnalysis` | 760 | 66 | 262 |
| `WfiGloss` | 520 | 66 | 155 |
| `WfiMorphBundle` | 1,932 | 75 | 590 |
| `PunctuationForm` | 206 | — | 260 |

(`StText` is a generic multi-paragraph-text container class LibLCM reuses well beyond the
one field a `Text` owns for its main content — e.g. lexicon example sentences, notes — so
its count is much larger than the `Text` count; it is not evidence of 9,958 independent
running texts. `Text`/`ScrBookRef`/`ScrSection` classes indicate this is Scripture
(Bible-translation-checking) material — `ChkRef`/`ChkTerm`/`CmDomainQ` are Paratext-style
consistency-checking record classes, consistent with `Sena_InterlinearTraining` being a
translation-checking export, not a hand-curated interlinear-glossing exercise.)

**Segment-level token census** — every `Segment.Analyses` occurrence, resolved to its
target class `[M]`:

| | Sena 3 | Sena_InterlinearTraining | Aweti |
|---|---|---|---|
| Segments (≈ sentences/clauses) | 4,487 | 4,558 | 80 |
| Total token occurrences | 40,255 | 40,650 | 666 |
| Mean tokens/segment | 8.97 | 8.92 | 8.32 |
| → resolves to bare `WfiWordform` (unanalyzed) | 31,535 (78.3%) | 31,790 (78.2%) | 67 (10.1%) |
| → resolves to `PunctuationForm` | 8,573 (21.3%) | 8,854 (21.8%) | 111 (16.7%) |
| → resolves to `WfiGloss` (a chosen, glossed reading) | 145 (0.4%) | 6 (0.015%) | 487 (**73.1%**) |
| → resolves to `WfiAnalysis` directly | 2 (0.0%) | 0 (0.0%) | 1 (0.2%) |

A sample `Segment` record, shown because the structure is worth seeing directly rather than
taking my classification on faith (`Sena_InterlinearTraining.fwdata`, byte offset ~212):

```xml
<rt class="Segment" guid="0008755c-..." ownerguid="2dbf074b-...">
  <Analyses>
    <objsur guid="42ee9047-..." t="r" />   <!-- resolves elsewhere to <rt class="WfiWordform"> -->
    <objsur guid="843ede05-..." t="r" />   <!-- resolves elsewhere to <rt class="WfiWordform"> -->
    ...
  </Analyses>
  <BeginOffset val="1" />
  <FreeTranslation>
    <AStr ws="pt"><Run ws="pt">Alegrem-se os limpos de coração,</Run></AStr>
  </FreeTranslation>
</rt>
```

...and confirming what one of those referenced `WfiWordform` records actually holds — no
morph-bundle, no analysis, just a bare surface form:

```xml
<rt class="WfiWordform" guid="42ee9047-...">
  <Checksum val="0" />
  <Form><AUni ws="seh">Mbatsanzaye</AUni></Form>
  <SpellingStatus val="0" />
</rt>
```

### What this means, precisely

1. **Running text, with real sentence/clause boundaries and free translations, exists in
   both Sena projects at a scale D15 asked for** (John: "a significant amount of text that
   can be fully parsed (10k sentences?)") — ~4,500 segments each, ~40,000 tokens each.
   Combined across the two Sena projects (they may or may not overlap in content — not
   checked; both are plausibly the same underlying Scripture text at different revision
   stages, so treat 40k+40k as a **ceiling on distinct volume, not a sum**, until dedup is
   checked) this is in the right order of magnitude for D15's "10k sentences" framing.
2. **D3's "gold-annotated corpus... the scarcest resource in this entire problem" is the
   wrong characterization for the Sena material, and correct only for Aweti.** In both Sena
   projects, 99.6-100% of running-text token occurrences are bare, unanalyzed `WfiWordform`
   pointers or punctuation — there is no chosen analysis to import for almost any of them.
   The gold annotation D3 was excited about (66-760 `WfiAnalysis`/`WfiGloss` records exist
   in these files) is real but tiny relative to the running text itself, and much of it is
   not even linked to a specific segment occurrence (only 2-6 of the ~40,000+ token
   occurrences point directly at one). Aweti inverts this: a small corpus, but 73.1% of its
   tokens are properly glossed.
3. **This does not weaken D15's actual design — it validates the framing D15 already
   chose.** D15 already said: *"Gold annotation is not required to train, only to measure
   ... the training input is raw vernacular text plus the analyzer — not annotated text."*
   This measurement confirms that framing was necessary, not just cautious: if training
   depended on existing gold links, Sena would supply approximately zero usable training
   signal (a few hundred tokens out of 40,000+) despite having abundant raw text. The
   correct move — already decided — is to re-parse the raw token sequence with PanGloss's
   own analyzer and train on the resulting (ambiguous, un-disambiguated) analysis lattice,
   exactly as D4/D15 already specify. What this report adds is that D3's "found asset"
   description needs the annotation-density caveat above; the *existence* of the text asset
   is confirmed, its *annotation density* was overstated.
4. **`Sena_InterlinearTraining.fwdata` is a new, previously undocumented find.** It does not
   appear anywhere in this repository before this report (`grep -r InterlinearTraining`
   returns zero hits across docs, code, and plans). It is a second, independent
   ~40,000-token Sena corpus alongside `Sena 3`, and its very name suggests it may have been
   assembled specifically for training/interlinearization purposes — worth the parent
   session following up on with whoever placed it on this machine, to learn its provenance
   and relationship to `Sena 3` before relying on it.

### Is `pg-fwdata` extracting any of this today? No — confirmed at file:line, not inferred

`[M]` `rust/crates/pg-fwdata/src/xml.rs:1-6` (the streaming reader's own module doc
comment, quoted verbatim):

> "Streaming `.fwdata` reader: walks the flat sequence of `<rt class="..." guid="..."
> [ownerguid="..."]>` records one at a time ... and builds a `RawGraph` keyed by GUID,
> containing only records whose `class` is one this crate's extractor understands. **Every
> other class (the bulk of a real project — `ChkRef`, `WfiWordform`, `StText`, Scripture
> data, ...) is skipped without ever being parsed into a `crate::node::Node`.**"

That sentence names `WfiWordform` and `StText` explicitly as skipped classes, in the
codebase's own words, before this report ever ran. `rust/crates/pg-fwdata/src/extract/mod.rs`
confirms this structurally: the extractor is composed of exactly five sections —
`project`, `features`, `phonology`, `morphology`, `lexicon` (`mod.rs:22-26`) — there is no
`text`/`corpus` section, and none of `lexicon.rs`, `morphology.rs`, `phonology.rs`,
`features.rs`, or `project.rs` reference `Text`, `StText`, `StTxtPara`, `Segment`,
`WfiAnalysis`, or `WfiGloss` anywhere (checked directly, grep across all five files: zero
matches outside one unrelated false-positive string, `"InsertSegments"`, a rule-mapping
variant name unrelated to interlinear `Segment`).

The output format mirrors this exactly: `pg_snapshot::Snapshot` (what `pg-fwdata` produces
and what `aweti.json` — a serialized snapshot of the same Aweti project — actually
contains) has exactly these five top-level keys, confirmed by loading the real file:

```
>>> json.load(open("aweti.json")).keys()
['format', 'version', 'project', 'featureSystems', 'phonology', 'morphology', 'lexicon']
```

No `text`, no `corpus`, no `segments` key exists anywhere in the format. This is not a bug
or an oversight to patch — `docs/research/spellcheck/PLAN.md` D15 already retracted the
idea of putting a corpus-trained artifact inside the analyzer pack at all ("the add-on is
not a pack payload... the two have genuinely different lifecycles"). It does mean a
**separate** importer (or a separate extraction mode of `pg-fwdata`, or a wholly separate
research-side tool) is needed before any real corpus can reach Layer 2 — nothing existing
today produces it.

Also checked and confirmed absent: `LexExampleSentence` (1,284-1,297 records in the two
Sena projects — dictionary example sentences, a smaller and differently-sourced kind of
"text") is likewise never referenced anywhere in `pg-fwdata/src`. Not a token-sequence
corpus at the same scale, but a candidate small supplementary source, also currently
unextracted.

### What it would take to get at it

This is bounded, ordinary engineering, not a research question:

1. **Walk the owning chain** `Text.ContentsOA -> StText.ParagraphsOS -> StTxtPara.SegmentsOS
   (via analysis) -> Segment.Analyses`. All of these are visible in the raw XML stream
   already — `RawGraph`/`Record` (`pg-fwdata/src/xml.rs`) already parses every
   `<rt class=... guid=... ownerguid=...>` generically; the extractor simply discards
   anything whose class isn't on its allow-list today. Adding these classes to that
   allow-list and writing a sixth extraction section (mirroring `extract/lexicon.rs`'s
   shape) is the concrete unit of work.
2. **Resolve each `Segment.Analyses` `objsur` guid** to either a `WfiWordform.Form` (the
   bare-surface case, ~78-100% of tokens per this measurement), a `WfiGloss`/`WfiAnalysis`
   (the rare gold-linked case), or a `PunctuationForm` (~17-22% of tokens) — three
   resolution branches, all already visible in the sample records above.
3. **Preserve segment boundaries as sentence boundaries** and carry `FreeTranslation`
   through if wanted (present on at least some segments, in Portuguese in the samples
   inspected) — directly maps onto this project's own `Sentence.free_translation` field
   (§ Part 2).
4. **Keep it out of the analyzer snapshot**, per D15's explicit ruling — this is a new,
   separate artifact/tool for Layer 2 corpus ingestion, not a `pg-fwdata`/`pg_snapshot`
   schema change.
5. **The corpus itself stays local/gitignored**, same as every other real-language input in
   this repo — nothing about extracting it changes the standing rule that real text is
   design-only local input, never committed. The synthetic generator in Part 2 is what lets
   development and testing proceed without it in the meantime.

None of the above was built in this session — it is scoped, not implemented, matching the
task's framing of this as a factual-settlement item, not a build item.

---

## Part 2 — The Python research harness

### Layout

```
research/                              # new top-level directory, sibling of rust/, docs/, openspec/
  pyproject.toml
  README.md
  .gitignore                            # build-artifact ignores local to this subproject
  src/spellcheck_research/
    interchange.py                      # the format (see below)
    models/
      base.py                           # SpellcheckModel abstract interface
      ngram_baseline.py                 # Stupid Backoff surface trigram (the shipped baseline)
    synthetic/
      profiles.py                       # 4 named shapes, by statistics, never by language
      generator.py                      # zero-real-data corpus generator
    eval/
      metrics.py                        # recall@k, MRR, keystroke savings, perplexity
      harness.py                        # split_corpus + evaluate()
    cli.py                              # `spellcheck-research run`
  docs/
    interchange-format.md               # format spec + worked example
  data/local/                            # gitignored real-data adapter path
    README.md
  tests/                                 # 26 tests, ~2s
```

### Setup — verified directly in this session `[M]`

```sh
cd research
uv venv
uv pip install -e ".[dev]"
```

(A plain `python -m venv .venv && pip install -e ".[dev]"` works identically — both
documented in `research/README.md`.) Dependencies: `numpy` plus `pytest` for `dev` — no
torch, no transformers, per D5. Python `>=3.11` required; this session ran on `3.14.0`
(via `uv venv`, which selected the newest available interpreter) with no compatibility
issues.

### The interchange format — the load-bearing decision

Line-delimited JSON, one JSON object per **token** (not per sentence — see
`research/docs/interchange-format.md` for the streaming/greppability rationale), carrying
surface form, explicit sentence/document boundary markers, and **the full list of
analyses**, never a single "best" one. Every `Analysis` field is a direct projection of the
real `WordAnalysis` struct (`rust/crates/pg-parse/src/lib.rs:25-44`) per D1's load-bearing
criterion — `pos`, `features` (the `syn_fs` bundle minus POS), `morphemes`
(`morpheme_ids`), `stem` (the root morpheme), `guessed`, and a relative `score` (explicitly
documented as *not* a calibrated probability).

This is not a stylistic choice: D4 (`PLAN.md`) "scores over the analysis lattice ... rather
than requiring a hard disambiguation pass first," so a format that stored one analysis per
token would silently break the whole design by forcing disambiguation at export time.
`research/tests/test_interchange.py` specifically asserts an ambiguous token's full
analysis list (2 analyses, different POS, different features, one marked `guessed`)
survives a write/read round trip byte-for-byte. An **empty** analysis list is also a
first-class, tested state (the `zero_analyses` case report 13 measured dominates real
coverage) — never conflated with "not yet processed."

Full worked example (a real generated corpus, run live in this session) is in
`research/docs/interchange-format.md`.

### The model interface

`spellcheck_research.models.base.SpellcheckModel` — `fit(corpus)`, `score(candidate,
context)`, `predict_next(context, k)`, optional `update(token)` (no-op default, for a
future online-adaptation experiment). Kept deliberately small per the brief; the one
shipped implementation (`StupidBackoffNgram`) is ~90 lines.

### The synthetic corpus generator — zero real-language data

`spellcheck_research.synthetic` generates token sequences with controllable:

- **class cardinality** (open vs. closed classes, stems per class — closed classes
  deliberately small, matching real closed-class behavior),
- **Zipf skew** (per-class stem-frequency weighting),
- **morphological richness** (Poisson-distributed affix counts per wordform, drawn from a
  small per-class affix pool so **affixes recur across many stems even though whole
  wordforms do not** — the exact property report 04/D4 depend on),
- **ambiguity rate** (`1 + Poisson(mean-1)` analyses per token, with distractor analyses
  drawn from other classes at decaying relative score),
- **feature richness** (probability an analysis carries a feature beyond bare POS),
- **cross-word structure** (a Markov chain over classes, transition rows drawn from a
  Dirichlet whose concentration controls how peaked/informative the class sequence is).

Four named profiles target the four corners of report 13's measured (ambiguity ×
richness) space, named by shape, never by language:

| Profile name | Target mean/p90 analyses | Target feature richness |
|---|---|---|
| `high_ambiguity_moderate_richness` | 4.61 / 9 | 0.35 |
| `low_ambiguity_zero_richness` | 1.03 / 1 | 0.0 |
| `low_ambiguity_high_richness` | 1.12 / 2 | 0.85 |
| `moderate_ambiguity_mixed_richness` | 1.47 / 2 | 0.45 |

No profile is a stand-in for any specific real grammar — each reproduces one *combination
of numbers* report 13 happened to observe occupied, nothing more specific. Every generated
surface form and morpheme is a placeholder code (e.g. `st6_002p1fx6_0`), never a real word.
`tests/test_synthetic_generator.py` checks determinism-given-seed, that the zero-richness
profile really does attach zero features across a 200-sentence sample, and that the
achieved mean ambiguity lands within a documented tolerance of the target.

### The evaluation harness

`eval.harness.evaluate` computes, per D4/D9/D10's stated needs: next-word recall@k and MRR,
correction recall@k and MRR (against a small candidate set including a cheap synthetic
corruption of the true word — a stated stand-in for a real error corpus, none exists per
`00-synthesis.md`'s open question), keystroke savings rate (standard word-completion
definition, simulated one prefix-length at a time), and perplexity (explicitly
renormalized over the evaluation vocabulary at every position, since Stupid Backoff scores
are not calibrated probabilities — documented in `models/ngram_baseline.py` and
`eval/metrics.py`).

Three traps are **enforced in code**, not just documented:

1. `split_corpus` shuffles and slices whole `Sentence` objects — a sentence can never
   straddle train/dev/test.
2. `evaluate(model, train, test)` raises `ValueError` if any `(doc_id, sentence_id)`
   appears in both `train` and `test`, checked before a single metric is computed.
3. `oov_rate` is a first-class field of every result, computed and reported alongside
   every other metric — never silently folded into an average that a reader would assume
   covers every token.

Results are emitted as an aligned plain-text table and as machine-readable JSON
(`EvaluationResult.to_dict()` / `write_results_json`), so runs are comparable over time.

### The baseline model

`StupidBackoffNgram` — a surface-wordform trigram with Stupid Backoff smoothing (Brants et
al. 2007), chosen over Kneser-Ney for this first drop specifically because it needs no
discount-mass estimation and is easy to hand-verify (the brief allows either; a modified-KN
implementation is real nontrivial work, left as a stated follow-up, not rushed). It is
deliberately the weakest model family in this research programme — report 04's own finding
that a plain surface-word n-gram is "the textbook worst case" for morphologically rich
languages — included specifically to prove the pipeline end-to-end, not to be competitive.
The class/factored n-gram D4 actually specifies plugs into the same `SpellcheckModel`
interface as future work.

### What runs today — real output, pasted, not summarized

Full test suite, this session, this machine:

```
$ uv run --python .venv pytest -q
..........................                                               [100%]
26 passed in 2.07s
```

End-to-end demo — synthetic generation, fit, split, evaluate, table + JSON, across all four
profiles, 3,000 sentences each, this session:

```
$ uv run --python .venv spellcheck-research run --n-sentences 3000 --k 5 --seed 0 --out-dir runs

model                        | source                            | k | n_tok | oov% | nw_recall@k | nw_mrr | corr_recall@k | corr_mrr | ksr   | ppl
-----------------------------+-----------------------------------+---+-------+------+-------------+--------+---------------+----------+-------+-------
stupid-backoff-3gram-surface | high_ambiguity_moderate_richness  | 5 | 2358  | 1.0  | 0.243       | 0.113  | 0.972         | 0.830    | 0.417 | 192.20
stupid-backoff-3gram-surface | low_ambiguity_high_richness       | 5 | 2441  | 1.3  | 0.200       | 0.090  | 0.965         | 0.823    | 0.471 | 220.57
stupid-backoff-3gram-surface | low_ambiguity_zero_richness       | 5 | 2380  | 1.6  | 0.194       | 0.082  | 0.963         | 0.833    | 0.419 | 200.10
stupid-backoff-3gram-surface | moderate_ambiguity_mixed_richness | 5 | 2289  | 1.1  | 0.246       | 0.113  | 0.973         | 0.843    | 0.436 | 175.85

wrote runs\results.json
```

Sanity reading of these numbers (this is a deliberately weak baseline, so weak numbers are
expected and are not a bug): next-word recall@5 sits around 0.19-0.25 against a vocabulary
in the hundreds-to-low-thousands of distinct synthetic wordforms, and perplexity is high
(176-221) — both consistent with report 04's prediction that a bare surface n-gram
struggles on morphologically generated data even when it is synthetic. Correction
recall@5/MRR are high (0.96+/0.82+) largely because the correction-candidate set in this
harness always includes the true word among a small pool (§ eval harness) — this measures
*ranking* quality given a small candidate set, not candidate-generator recall, and is
documented as such in `eval/harness.py`.

### What is deliberately left unbuilt

- **A modified-Kneser-Ney implementation.** Stupid Backoff satisfies the brief ("stupid
  backoff and/or Kneser-Ney"); KN's discount-mass/continuation-count machinery is real work
  better done once a second model family is actually being compared against the baseline.
- **The class/factored n-gram (D4) itself.** This harness is the infrastructure the sibling
  reports' model families plug into; it deliberately ships only the weakest baseline to
  prove the pipeline, not D4's actual ranking layer.
- **A real error corpus / grammar-informed synthetic error generation.** The correction
  metric's corruption function is a cheap single-edit stand-in (`harness._corrupt`),
  explicitly flagged in its own docstring as not a substitute for D5's synthetic
  error-generation programme.
- **Candidate-generator recall@k** (whether the true word survives into a correction
  candidate set at all, as opposed to ranking well once present) — named in report 13's own
  "What I could not measure" list and not addressed here either; the harness always injects
  the true word into its candidate set, which sidesteps rather than measures this question.
- **The FLEx running-text importer scoped in Part 1.** Explicitly out of scope for this
  session (factual settlement, not a build task) — see § What it would take.
- **Provenance check on `Sena_InterlinearTraining.fwdata`.** A new find, not yet understood
  — see Part 1 finding 4.
