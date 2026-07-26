# First measurements — analysis ambiguity, D1/D4 backoff-rung cardinality, syn_fs/mpr population

Report 13 in the spell-checking research series. Scope: **real numbers**, measured on the four
languages named in `PLAN.md` § D13 (`openspec/changes/certify-four-language-matrix/`), driving
the existing `pg-fwdata` + `pg-grammar` + `pg-parse` surface — no new engine code, no LM, no
reranker. Eleven prior reports and thirteen decisions produced zero measurements on a real
PanGloss grammar; this is the first. Series convention: `[M]` = measured/observed directly in
this session, `[S]` = my own derivation shown in full, `[A]` = carried from an earlier report's
secondary-source citation (used sparingly here, mostly to compare against a prior claim).

**Everything below is measured on real grammars in this environment on 2026-07-25.** Nothing is
committed by this report; the one artifact added is a dev-only example binary (see § Methodology).

---

## Headline numbers

| | Sena 3 | Amharic | Indonesian | Aweti |
|---|---|---|---|---|
| Corpus (unique wordforms) | 6,973 | 673 | 121 | 208 |
| Corpus source | real `.fwdata` interlinear-text wordform inventory | real `.fwdata` wordform inventory (thin project) | pre-existing test corpus (Rust-port parity infra) | pre-existing test corpus (Rust-port parity infra) |
| Coverage (≥1 confirmed analysis) | 49.20% | 24.37% | 85.12% | 48.56% |
| `invalid_shape` (unsegmentable) | 19.00% | 0.59% | 0.83% | 5.77% |
| `timed_out` (10s/8s cap) | 0.00% | 9.81% | 0.00% | 6.73% |
| step-capped (200k steps) | 12.42% | 0.00% | 0.00% | 40.87% |
| Total confirmed analyses | 15,804 | 184 | 106 | 148 |
| Ambiguity: mean / median / p90 / max | 4.61 / 4 / 9 / 78 | 1.12 / 1 / 2 / 2 | 1.03 / 1 / 1 / 2 | 1.47 / 1 / 2 / 4 |
| `syn_fs` beyond bare POS | 30.99% | 85.33% | **0.00%** | 45.27% |
| `mpr` nonempty | 0.00% | 0.00% | 0.00% | 38.51% |
| `mpr_names` declared | 3 | 6 | 4 | **9** |
| Rung 1 (decomp+full `syn_fs`) singleton-class rate | 94.62% | **100.00%** | 98.08% | 93.53% |
| Rung 2/3 (POS+`syn_fs` / POS+head) distinct classes | 47 | 38 | **3** | 41 |
| Rung 4 vs. Rung 5 (POS+`mpr` vs. POS alone) | **identical** (24=24) | **identical** (6=6) | **identical** (3=3) | **differ** (18≠16) |

**Headline finding 1 — rung 1 has zero statistical power on every grammar measured.** 93.5%–100%
of rung-1 classes (full morpheme decomposition + full `syn_fs`) are singletons. This is
universal across all four grammars, independent of corpus size. `[M]`

**Headline finding 2 — D1's stated collapse risk is real but not universal, and it depends on
which POS the syn_fs richness is concentrated on vs. which POS dominates the corpus, not on the
language as a whole.** Only Indonesian shows the total collapse D1 warned about (rungs 2–5 are
byte-identical, because zero confirmed analyses in that corpus carry any feature beyond POS).
Sena, Amharic, and Aweti all retain a meaningfully finer rung 2/3 than rung 5. See § What this
means for D1 and D4. `[M]`

**Headline finding 3 — `mpr` is not the reliably dense rung D1 describes.** In 3 of 4 grammars
measured, not one confirmed analysis carries a nonempty `mpr` value, so rung 4 (POS+`mpr`) is
byte-identical to rung 5 (POS alone) — the ladder effectively loses a rung. Aweti is the
exception (38.5% nonempty, and rung 4 ≠ rung 5). Aweti also declares **9** `mpr_names`, exceeding
`PLAN.md` D1's "≤6 members across the reference grammars" claim. `[M]`

---

## Methodology

**Pipeline used, and why.** All four runs used `pg_parse::Morpher` (via `pg_parse::hc_parse_batch`)
with `ParseOptions::default()` — CONTEXT.md's **"Rust HermitCrab-only"** named analysis pipeline
("also supported for engine integration, parity, and detailed parse-failure diagnostics"), **not**
the "normal deployable" FST-propose + HermitCrab-confirm pipeline (`--engine=foma` /
`pg_foma::composite::FomaAnalyzer`). Reasons, stated up front because this is a real methodology
choice with consequences:
- It needs no FST network compilation, so it carries none of the FST-emission blow-up risk
  `docs/fst-plan/morphotactic-composite-pruning.md` documents for Aweti (that blow-up is
  specifically inside `pg_foma::emit()`/`build_composites`, a different code path from the one
  used here — confirmed by this session: Aweti compiled and parsed successfully through
  `pg_grammar::compile_project` + `Morpher`, see § Aweti).
- It is a named, supported, citable pipeline in `CONTEXT.md`, not an ad hoc substitute.
- `--engine=foma` was tried once, on Indonesian only, as a bonus cross-check (see § What I could
  not measure). Sena and Amharic were never attempted under `--engine=foma`.

**`guess_root` was OFF (`ParseOptions::default()`).** `pg-parse/src/morpher.rs:187` shows
`guess_root: bool` defaults to `false`; `hc_parse_batch` (`pg-parse/src/batch.rs:112`) calls
`morpher.parse_word()`, which uses that default. Every `guessed` count in this report is 0 for
exactly this reason — the guess branch was never exercised. See caveat in § What I could not
measure.

**Harness added (dev-only, not production).** `rust/crates/pg-cli/examples/spellcheck_measure.rs`
(new file, ~530 lines). An `examples/` binary — not part of `pangloss`, not invoked by any shipped
tooling, not on any production path. It dispatches on grammar-path extension exactly like
`pg-cli`'s own `load_grammar` (`.fwdata` → `pg_fwdata::import_file` + `pg_grammar::compile_project`;
`.json` → `pg_snapshot::Snapshot` + `pg_grammar::compile_project`; `.xml` → `pg_grammar::load`),
then runs `pg_parse::hc_parse_batch` over a wordform list and aggregates the census/rung tables
directly from each returned `WordAnalysis`. No production file was changed; `git status` shows only
this new file added. Built with:
```
cd rust && cargo build -p pg-cli --release --example spellcheck_measure
```

**Corpora and exact commands, per grammar:**

- **Sena 3** — `C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects\Sena 3\Sena 3.fwdata`
  (the same file `rust/crates/pg-fwdata/tests/real_projects.rs::sena3_imports_with_expected_counts`
  uses). The 6,973-wordform corpus is every `<rt class="WfiWordform">` record's
  `<Form><AUni ws="seh">` text, extracted directly from the `.fwdata` XML with:
  ```
  awk '
  /<rt class="WfiWordform"/ { inrec=1; next }
  inrec && /<AUni ws="seh">/ {
    line=$0; sub(/.*<AUni ws="seh">/,"",line); sub(/<\/AUni>.*/,"",line); print line; inrec=0
  }' "Sena 3.fwdata" > sena3_wordforms.txt
  ```
  Count matches `PLAN.md` § D13's already-recorded 6,973 exactly, and matches
  `real_projects.rs`'s independently-asserted `LexEntry`/phoneme counts on load. All 6,973 forms
  are unique (FieldWorks's own wordform table is already deduplicated). Run:
  ```
  ./target/release/examples/spellcheck_measure.exe "Sena 3.fwdata" sena3_wordforms.txt \
    --threads 8 --step-cap 200000 --word-timeout-ms 10000
  ```
  Wall time: 320s.

- **Amharic** — `C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects\Amharic\Amharic.fwdata`
  (same file `real_projects.rs::amharic_imports_with_expected_counts_and_adhoc_warning` uses).
  673 wordforms extracted identically (`ws="am"`). Same command shape. Wall time: 184s.

- **Indonesian** — `samples/data/indonesian-hc.xml` + `samples/data/indonesian-words.txt`
  (121 words). **Not extracted by me** — these are pre-existing files in the main PanGloss
  checkout, gitignored per `/samples/data/*-hc.xml` / `*.json` in `.gitignore` (same
  dev-machine-local convention as the FieldWorks sample projects), and already the standard
  Indonesian reference corpus used throughout the Rust port's own parity-test infrastructure
  (e.g. `docs/fst-plan/FST_FAST_PATH_PLAN.md:138`, `docs/history/rust-optimizations-phase2.md:1162`
  cite the identical file). They are not present in this worktree (`.claude/worktrees/spellcheck`)
  because gitignored files are not shared across worktrees; I referenced them by absolute path
  from the main checkout, `C:\Users\johnm\Documents\repos\PanGloss\samples\data\`. Wall time: 0.1s.

- **Aweti** — `samples/data/aweti.json` + `samples/data/aweti-words.txt` (208 words), same
  provenance/caveat as Indonesian's files. `aweti.json` is a `pg_snapshot::Snapshot`, loaded via
  `pg_grammar::compile_project` (not `.fwdata`). `--word-timeout-ms 8000` (slightly tighter, given
  Aweti's documented pathology). Wall time: 64.5s — **no explosion, no OOM**; see § Aweti.

All runs: `--threads 8` (this machine's `available_parallelism()`), rayon-parallel dispatch via
`hc_parse_batch` (longest-surface-first order, results reindexed to original order — unaffected by
dispatch order).

**Denominator, precisely.** "Coverage" here is *"this specific wordform TYPE received ≥1
confirmed analysis under `ParseOptions::default()`, within the stated step/time budget"* — the
raw unique-wordform-type inventory as denominator, not a "declared corpus" in the
`certify-four-language-matrix` sense (that change's own `tasks.md` is entirely unchecked as of
this session — there is no existing certified number for any of the four languages to compare
against; see § What I could not measure).

---

## Per-grammar detail

### Sena 3

`[M]` 6,973 wordforms → 1,325 (19.00%) don't even segment against the surface char-def table
(`invalid_shape`); 2,217 (31.79%) segment but confirm zero analyses; 3,431 (49.20%) confirm ≥1.
866 words (12.42%) hit the 200,000-step cap — their recorded analysis counts are a **lower bound**
on true ambiguity (capping only stops search early; it never removes an already-confirmed
analysis). 0 words timed out at 10s.

The 19% `invalid_shape` rate is itself a real finding, corroborated by the grammar's own compile
warnings (dozens of `cannot segment "X": no character definition matches` entries for allomorphs
like `sábadu`, `Jorge`, `báulu`, `bulángeti` — diacritics/foreign letters/loanwords the
char-def table doesn't cover). This is exactly D12's concern surfacing empirically: real project
text carries orthographic material the grammar's own segmentation rules don't recognize.

**Ambiguity** (n=3,431 words with ≥1 analysis): mean **4.61**, median **4**, p90 **9**, p99 **18**,
max **78**. Full histogram in the raw run output; not dominated by a long thin tail — the p50→p90
range (4→9) is itself wide. This is the "single most useful number nobody had" the task named: a
real, non-trivial lattice width that D4's marginalization approach has genuine work to do over.

**syn_fs population**, over 15,804 total confirmed analyses: 30.99% carry a feature beyond bare
POS. But this aggregate hides a sharp POS split — the corpus is verb-dominant (V = 11,747/15,804 =
74.3% of all confirmed analyses) and verbs carry `syn_fs` beyond POS only **16.9%** of the time,
while nouns (N = 3,217 analyses, 20.4% of the corpus) carry it **88.8%** of the time. The
richness lives in `genro` (Bantu noun class/gender, 20 symbols — matches the pre-existing code
comment `pg-featstruct/src/bitvec.rs:246`, "Sena's widest feature (20 symbols)", independently
corroborated) and `NounAgr` (agreement), both nominal-domain features. `mpr` is declared (3
names: "Irregularly Inflected Form", "Plural", "Past") but **never fires**: 0/15,804 nonempty.

**Rungs**: rung 1 = 14,913 classes/15,804 analyses, 94.62% singleton (no power). Rung 2 = rung 3
= 47 classes (my rung-3 "head-only" proxy is numerically identical to rung 2 here — see caveat
below), mean class size 336, only 4.26% singleton — a genuinely useful, dense mid-level rung.
Rung 4 = rung 5 = 24 classes exactly (mpr contributes zero information). Rung 6 = 3 classes: open
15,554 (98.4%), closed 235 (1.5%), unknown-POS 15.

### Amharic

`[M]` 673 wordforms, from a much thinner reference project (130 `LexEntry` vs. Sena's 1,462) but
richer phonology (417 phonemes vs. Sena's 44). Only 4 (0.59%) fail to segment, but 439 (65.23%)
segment and confirm **zero** analyses — the worst coverage of the four. 66 (9.81%) time out at
10s — the highest timeout rate of any grammar measured, despite the smallest word count, which is
consistent with per-word cost tracking phonological complexity rather than lexicon size.

**Ambiguity** (n=164): mean 1.12, median 1, p90 2, max 2 — essentially flat. Given only 184 total
confirmed analyses, this is a thin sample; treat as directional, not a stable estimate (this
project is a FieldWorks demo, not a scaled interlinear corpus the way Sena 3 is).

**syn_fs population**: 85.33% beyond bare POS — the opposite pattern from Sena. Here the corpus is
also verb-dominant (v.ipfv + v.pfv + v.conv + cop = 63+62+17+6 = 148/184 = 80.4%), and **verbs are
exactly the POS category carrying rich features** (100% of v.ipfv/v.pfv/v.conv/cop analyses carry
`syn_fs` beyond POS — person/aspect/gender/number/subject agreement), while nouns carry it only
28.1% of the time. `mpr` (6 names declared, including "i-stem", "Perfective", "Imperfective") again
**never fires**: 0/184 nonempty.

**Rungs**: rung 1 = 184/184, **100% singleton** — literally zero statistical power, every analysis
in its own class. Rung 2 = rung 3 = 38 classes, mean 4.84, 31.58% singleton — real discriminative
power retained, not collapsed. Rung 4 = rung 5 = 6 classes exactly (again, mpr adds nothing). Rung
6 = 2 classes: open 174 (94.6%), closed 10 (5.4%).

### Indonesian

`[M]` 121 words, a small pre-existing test corpus over a correspondingly small grammar (66
`LexEntry`; only 2 syntactic features declared at all: bare POS and one complex `head` feature).
1 word (0.83%) fails to segment; 17 (14.05%) confirm zero analyses; 103 (85.12%) confirm ≥1 — the
best coverage of the four, consistent with this being the smallest, simplest grammar.

**Ambiguity** (n=103): mean 1.03, median 1, p90 1, max 2 — near-flat, consistent with the
grammar's own small size (79 morphemes total).

**syn_fs population: 0.00%.** Across all 106 confirmed analyses, not one carries any feature
beyond bare POS, despite the grammar declaring a `head` complex feature. This is the clean,
unambiguous case of D1's stated risk: **rungs 2, 3, 4, and 5 are byte-identical** (3 classes each,
same class sizes, same members) — the backoff ladder has genuinely collapsed to POS alone for
this grammar/corpus. `mpr` (4 names declared) also never fires.

**Rungs**: rung 1 = 104/106, 98.08% singleton. Rungs 2=3=4=5 = 3 classes, mean size 35.3, 0%
singleton. Rung 6 = 1 class — every confirmed analysis fell into an "open" POS (v/n/adj) in this
corpus, so even the floor rung is trivial here.

**Bonus, `--engine=foma` (the actual deployable propose+confirm pipeline), Indonesian only**: ran
`pangloss batch --engine=foma` with `HC_FOMA_STATS=1`. Over 120 attempted words: mean candidates
generated by the FST proposer = 1.24/word, mean confirmed = 0.88/word, max generated = 9,
27/120 words (22.5%) saw the proposer generate more candidates than HC confirmed (overgeneration
correctly pruned) — a direct, cheap empirical confirmation of the propose-and-confirm invariant
(`CONTEXT.md`: "may safely overapproximate... must not omit a valid analysis") operating as
designed on a real grammar. Not extended to Sena/Amharic/Aweti — see § What I could not measure.

### Aweti

`[M]` 208 words, the grammar `docs/fst-plan/morphotactic-composite-pruning.md` documents exploding
past 4.9GB RSS inside `pg_foma::emit()`'s `build_composites`. **This measurement did not hit that
path** — `pg_grammar::compile_project` + `pg_parse::Morpher` (no FST emission at all) compiled and
parsed the full 208-word corpus in 64.5s with no memory issue. This is a real, useful scoping
finding: the documented Aweti pathology is specific to FST *emission*, not to grammar compilation
or the Rust-HermitCrab-only search path. It does **not** mean Aweti is cheap: 85/208 words (40.87%,
by far the highest of the four) hit the 200,000-step cap, and 14/208 (6.73%) timed out at 8s —
Aweti is still visibly the most expensive grammar per word among the four, just not explosively so
on this path.

12 words (5.77%) fail to segment; 81 (38.94%) confirm zero analyses; 101 (48.56%) confirm ≥1.

**Ambiguity** (n=101): mean 1.47, median 1, p90 2, max 4 — again a small-corpus number, but the
richest distribution of the three small grammars.

**syn_fs population**: 45.27% beyond bare POS, concentrated in verbal subtypes — INTV
(intransitive verb) and TRV (transitive verb) both carry `syn_fs` beyond POS **100%** of the time
(mood/person/aspect/number/subject/absolutive agreement), while nominal categories (N, PROPN,
PPRON, DEM) carry it 0%.

**`mpr` is the one grammar where it actually fires**: 38.51% of confirmed analyses carry a
nonempty `mpr`, using 2 of the grammar's 9 declared `mpr_names`. **9 declared `mpr_names`
directly exceeds `PLAN.md` D1's citation of "≤6 members across the reference grammars"** —
Sena has 3, Amharic 6, Indonesian 4, Aweti 9. That specific factual claim in the ratified D1
section needs revisiting against this measurement (not edited here, per instructions).

**Rungs**: rung 1 = 139/148, 93.53% singleton. Rung 2 = rung 3 = 41 classes, mean 3.61, 43.90%
singleton — the sparsest of the "dense" rungs measured, but still meaningfully finer than rung 5.
Rung 4 = **18** classes vs. rung 5 = **16** classes — the *only* grammar of the four where `mpr`
adds any discriminative power beyond POS alone. Rung 6 = 2 classes: open 72 (48.6%), closed 76
(51.4%) — the most balanced open/closed split of the four (Sena/Amharic/Indonesian are all
85–100% open, reflecting their verb/noun-dominant corpora; Aweti's tagset draws more of its
confirmed analyses from closed categories like PRT, PROPN, ADV, PPRON, DEM).

---

## What this means for D1 and D4

**1. Rung 1 is dead weight on every grammar measured — not a hypothesis, a fact now.** 93.5–100%
singleton-class rates across all four means rung 1 (full decomposition + full `syn_fs`) has
essentially zero statistical power on any of the four certified languages, at any corpus scale
measured (6,973 words down to 121). D4's backoff ladder should not expect rung 1 to ever
contribute meaningfully; it exists to *fail fast* into rung 2, not to be estimated from directly.
This was D4's own predicted failure mode ("if rung 1 assigns a near-unique class to almost every
wordform it has no statistical power") and it is now confirmed, universally, not conditionally.

**2. D1's stated collapse risk ("if real grammars carry thin syn_fs, rungs 1-3 collapse toward
rung 5") is real, but it is not a property of a *language* — it is a property of the intersection
between (a) which POS category a grammar's authors happened to populate `HeadFeatures` for, and
(b) which POS category dominates the specific corpus sampled.** Indonesian shows the total
collapse D1 feared. Sena and Amharic do not collapse — rung 2/3 stays meaningfully denser than
rung 5 in both — but for opposite reasons: Sena's richness sits on nouns while its corpus is
verb-dominant (so the *aggregate* population number, 31%, looks thinner than the *per-POS*
reality); Amharic's richness sits on verbs and its corpus is also verb-dominant (so its aggregate
number, 85%, looks richer, for what is structurally the same "richness lives on one POS" pattern
as Sena, just luckier alignment with the corpus). **A single grammar-wide "% of analyses with
syn_fs beyond POS" statistic is not a reliable predictor of any given word's backoff behavior —
D4's rung-3 feature-subset selection (already flagged in `PLAN.md` as needing to be chosen
per-grammar) should be chosen **per-POS within a grammar**, not once per grammar.** This is a
sharper, more actionable version of the open item `PLAN.md` already carries.

**3. `mpr` is not the reliably-dense rung D1 describes; treat it as a live per-grammar decision,
not a floor to lean on.** In 3 of 4 grammars, `mpr` is populated in **zero** confirmed analyses
despite being declared with 3–6 members, so rung 4 collapses into rung 5 exactly. Aweti is the
sole counterexample, and even there only 2 of 9 declared features ever fire. D1's rationale for
`mpr` ("≤6 members... therefore dense even on tiny corpora") conflates *cardinality* with
*usage frequency* — a low-cardinality feature that is never assigned contributes nothing, no
matter how low its cardinality is. The backoff ladder should treat "does this grammar's corpus
actually use `mpr`?" as a measured per-grammar gate, not an assumption, and should be prepared to
skip straight from rung 3 to rung 5 when it doesn't.

**4. The Aweti `mpr_names` count (9) contradicts `PLAN.md` D1's specific "≤6 members across the
reference grammars" claim.** This is a concrete, falsifiable correction this measurement produced
— worth revisiting in D1 directly (not done here, per instructions not to edit `PLAN.md`).

**5. Coverage, not ranking, is the bottleneck on every corpus measured here.** None of the four
runs come close to D13's "near 100%" bar: Sena 49.2%, Amharic 24.4%, Indonesian 85.1%, Aweti
48.6% (all under non-guessing defaults, over the raw wordform-type inventory — see caveats above
and below). The governing question this report was commissioned to test — "does ranking have a
solvable problem on real grammars" — has a prior, more basic answer for these four
corpora-as-measured: for roughly half to three-quarters of the words in Sena/Amharic/Aweti, and
15% of Indonesian, there is no confirmed analysis at all yet under this pipeline/options, so no
amount of ranking sophistication touches those words. This does **not** mean D4's design is wrong
— lattice marginalization over what *does* parse is still the right approach for the words that do
— but it does mean the coverage gate D13 already names as the admission criterion is doing real,
necessary work, and none of the four languages would currently be admitted under it as measured
here (with the strong caveat in the next section that this is not the certified number).

---

## What I could not measure, and why

1. **Item 4 — recall@k of prediction from a typed prefix (a "does ranking have a solvable
   problem" analogue for completion, not just correction) is NOT reachable with existing
   surface, and I did not fake it.** I searched `pg-foma`, `pg-fst`, `pg-parse`, and `pg-cli` for
   any prefix-completion or error-tolerant-generation API (`prefix`, `complete`, etc.) and found
   none — only morphological rule-role naming (`Role::Prefix`/`Role::Suffix`, about affix
   position, unrelated). `Morpher::generate_words`/`generate_words_from_analysis` (what
   `pangloss generate` wraps) run the *opposite* direction: they synthesize a surface form from an
   explicit, already-known morpheme sequence — they do not enumerate completions of a partial
   orthographic string. This is exactly D9's Tier 1/2 generative candidate supply, which `PLAN.md`
   already records as **decided but unbuilt** ("the calibration itself is unbuilt"). Building it
   would be new engine code, explicitly out of scope for this measurement task.

2. **09's item 1 ("recall@k of the candidate generator alone")** is a related but distinct
   number — given a *full* (possibly misspelled) surface word, does the correct analysis survive
   into the FST proposer's candidate set before HC confirm/prune. This *is* reachable, via
   `--engine=foma` + the existing `HC_FOMA_STATS` diagnostic. I ran it once, on Indonesian only
   (§ Indonesian bonus). I did not extend it to Sena, Amharic, or Aweti: Sena/Amharic would each
   require a fresh, untested-in-this-session foma-network compile of a much larger grammar, and
   Aweti specifically carries the documented FST-emission explosion risk this report's own primary
   pipeline choice was designed to sidestep. Judged not worth the time/risk budget given items 1–3
   were the stated priority.

3. **No certified/official corpus-recall number exists to compare against.**
   `openspec/changes/certify-four-language-matrix/tasks.md` is entirely unchecked (`[ ]`) as of
   this session — the certification work itself has not been run. My coverage numbers use a
   different denominator (raw unique wordform-type inventory, not a "declared corpus"), a
   different pipeline in general (Rust-HermitCrab-only, not necessarily FST-propose+confirm), and
   generic rather than per-grammar-calibrated step/time budgets. Treat every coverage percentage
   in this report as a first, honest signal, not a certification result.

4. **Indonesian and Aweti were measured on small (121- and 208-word) pre-existing test corpora**
   inherited from the Rust port's own parity-test infrastructure, not from a real interlinear-text
   corpus the way Sena/Amharic's `WfiWordform` inventories are. Their ambiguity and rung numbers
   are small-n and directional; I did not have, and did not find within this session's budget, a
   larger real corpus for either language in this environment.

5. **I did not attempt `--engine=foma` on Sena or Amharic at all.** Everything reported for those
   two grammars used the Rust-HermitCrab-only pipeline exclusively.

6. **The guess branch (`guess_root=true`) was never exercised.** `ParseOptions::default()` sets
   `guess_root: false`, confirmed directly in `pg-parse/src/morpher.rs:187` and in
   `hc_parse_batch`'s call site (`pg-parse/src/batch.rs:112`). Every `guessed` count reported here
   is 0 for exactly this reason, not because guessing never fires. D13's own stated mitigation
   ("guessed parses are partly usable... coverage holes become partial credit rather than
   zeros") is therefore untested by this report. This is a natural, cheap follow-up: rerun with
   `ParseOptions::default().with_guess_root(true)` and see how much of each grammar's
   `zero_analyses` bucket converts to a guessed-but-informative analysis. My harness already
   tracks a `guessed` counter and threads `AnalysisProvenance` through, so this needs a one-line
   change, not new plumbing.

7. **Sena's step-capped words (12.42%) carry a censored ambiguity count.** The step cap stops
   search early; it never discards an already-confirmed analysis, so the true ambiguity for those
   866 words is `≥` what I recorded, never less. I did not re-run at a higher cap to see where
   this stabilizes.

8. **Rung 3 ("POS + a selected feature subset") is an approximation, not a real per-language
   selection.** `PLAN.md` names choosing this subset as a language-specific open item. Lacking
   that selection, I approximated it as "POS + the `head` complex feature only, excluding `foot`."
   In all four grammars measured, this was numerically identical to rung 2 (full `syn_fs`),
   because none of the four declares a separate top-level `foot` feature or any other top-level
   syntactic feature outside `{pos, head}` — so my proxy never actually demonstrated a coarsening
   step between rung 2 and rung 3. A real per-language subset selection could behave very
   differently; this is untested.

9. **Rung 6 (open/closed) is my own post-hoc heuristic, not derived from any authoritative field
   in the grammar data** — no such field exists in `pg_grammar::Grammar` or `pg_snapshot::Snapshot`.
   I built it by inspecting each grammar's actual POS abbreviation convention after the fact, and
   revised it once mid-session (Sena/Amharic/Indonesian mark verb subtypes with a *leading* v/V —
   `Vaux`, `v.pfv`; Aweti marks them with a *trailing* V — `INTV`, `TRV` — my first pass caught only
   the former and badly misclassified Aweti until I noticed and fixed it). A fifth grammar with a
   different convention could defeat this silently. Treat rung 6 as the least trustworthy number in
   this report.

10. **No neural or statistical component of any kind was built or trained.** Out of scope by the
    task brief — this report measures whether D4 has raw material to work with, not D4 itself.

---

## Artifacts

- `rust/crates/pg-cli/examples/spellcheck_measure.rs` — the dev-only measurement harness (new
  file; not wired into `pangloss` or any shipped surface; only file added to the repo by this
  report — confirmed via `git status`).
- Raw per-run console output and per-word TSV dumps are in the session scratchpad, not committed
  (this report's own text is the durable record of the numbers).
