## Context

This change sits directly on top of the decided single-language design in
`docs/research/spellcheck/PLAN.md`:

- **D1** — load-bearing factors are exactly the fields of `WordAnalysis`
  (`rust/crates/pg-parse/src/lib.rs:25-44`): `morpheme_ids`, `root_morpheme_index`, `pos_id`,
  `syn_fs`, `mpr`, `guessed`, `provenance`/`supplied_root`. Semantic domains and authored lexical
  data (glosses, valency, etc.) are discarded, not parked (`PLAN.md:24-182`).
- **D3** — Constraint Grammar is deferred; not a prerequisite for the speller; ambiguity is
  marginalized over the analysis lattice, not resolved (`PLAN.md:330-467`).
- **D4** — the ranking layer that ships is a two-scale class n-gram: an inter-word class trigram
  (`P(class(w) | context)`) plus an intra-word morpheme n-gram (`P(w | class(w))`), composed with
  the error-model cost as additive log-space terms (`PLAN.md:185-271`).
- **D5** — anything neural is a bounded later ablation measured against D4, not the design
  (`PLAN.md:274-327`).

None of the ten research reports behind those decisions considers more than one loaded grammar. This
change's job is exactly the gap: identify which loaded language a word belongs to, keep D4 correct
when context crosses a language boundary, and define the data/session model for several resident
languages — without touching D1/D3/D4/D5 themselves.

**Runtime already supports multiple resident packs.** `CONTEXT.md:110-111` (definition of
**PanGloss Runtime**): "A process may load multiple packs as isolated immutable handles; every
request names its handle and owns independent scratch, budget, trace, and cancellation state."
`CONTEXT.md:108` lists spell checking as a capability the Runtime performs once a Language Pack is
loaded. So the base multi-pack-residency mechanism is not new; what is new is (a) the
cross-pack coordination this change designs and (b) the additive per-pack data D4 needs, which does
not exist in `.pgpack` today (`.pgpack` is currently "the proposing FST, matching Rust-HermitCrab
runtime data, configured compact diagnostic symbols, and package metadata," `CONTEXT.md:126-131`,
with no n-gram/class-LM section).

**Not staged in `openspec/changes/STAGING.md`.** That file's scope is explicitly "the active
grammar-coverage changes" (`STAGING.md:3`) — the FST-compilation/coverage track that makes a single
grammar buildable at all. This change presupposes that track's output (a loadable `.pgpack` per
language) and is a separate, downstream capability. It is not inserted into that file's stage/merge
ordering; a future implementation change consuming this design should state its own dependency on
the coverage track's completion rather than reading it out of `STAGING.md`.

## Goals / Non-Goals

**Goals:** answer, as OpenSpec requirements/scenarios, how PanGloss identifies which loaded language
a word belongs to cheaply; how D4's two-scale n-gram behaves across simultaneously loaded languages,
including at code-switch boundaries; and what the multi-language data model looks like relative to
`.pgpack`, memory budget, load/unload policy, and personal overlays.

**Non-goals:**
- Reopening D1, D3, D4, or D5. Every per-language mechanism in this change reuses them unchanged.
- Cross-word syntactic error detection of any kind — completely out of scope, per the user's
  explicit constraint; this change does not smuggle in a CG-shaped mechanism under a different name.
- Cross-user aggregation/federated learning implementation — `00-synthesis.md`'s tiered consent
  model and `06-personalization-and-privacy.md`'s small-N verdict stand unchanged; this change only
  notes that any such mechanism would also need to be per-language (see D-Data-3).
- Designing D2 (the unified weighted composition itself) — `PLAN.md`'s own table marks D2 "direction
  settled, not designed" (`PLAN.md:14`). This change's Q2 answer describes how the composition's
  n-gram *terms* behave across languages; it does not design the composition mechanism D2 owns.
- Any code, benchmark, or spike. Every quantitative claim below not sourced to this repo or a cited
  research report is marked as a design bet.

## Decisions

### D-LangID-1 — A cheap cascade decides which loaded grammar(s) to run before any FST work

Running full FST propose→confirm against every loaded grammar for every word is exactly the cost
this question asks to avoid ("cheaply enough to run one checker rather than N"). The cascade, in
order, each step only running if the previous step left more than one candidate language:

1. **Host-supplied writing-system metadata.** FLEx knows the current field's writing system,
   Paratext knows the project language, Keyman knows the active keyboard's associated language.
   When the host supplies this, it is authoritative and free — use it directly, no computation.
   This matches the repo's own stated boundary that the host, not PanGloss, owns "project history,
   baseline selection, publication policy" (`CONTEXT.md:19-21`); PanGloss consumes the signal, it
   does not infer it independently when the host already has it.
2. **Script / character-set feasibility gate.** For each loaded language's writing system(s),
   cheaply test the word's characters against that writing system's word-forming character set and
   custom-character/PUA inventory. `sil-primary-sources.md` findings 1-2 (`sil-primary-sources.md:18-51`)
   establish that FieldWorks writing systems already carry per-writing-system combining-class
   overrides, multigraph collation tailoring, and custom PUA characters — the natural source for
   this gate, grounded in real LibLCM data rather than a generic Unicode script property. This gate
   is O(word length × loaded languages) and eliminates languages whose orthography cannot contain
   this word at all. **It does nothing for the common hard case** — related languages/dialects that
   share one orthography — which is why steps 1 and 3 exist.
3. **Session/document language prior.** A running, recency-weighted record of which loaded language
   recent words were identified as (see D-NGram-2 for how a code-switch is itself detected/scored),
   plus the document's own declared writing system if the host provides one. Biases which language
   is tried first and is the primary mechanism for step 4's tie-break.
4. **Full parse, narrowed candidate set.** Only after 1-3, run FST propose→confirm against the
   remaining candidate languages — in the overwhelming majority of cases exactly one, occasionally a
   small cluster of orthography-sharing related languages.

#### Scenario: a word parses in exactly one loaded language

The analyzer accepting a word in exactly one loaded grammar (non-`guessed`) **is** the
identification — free, per the same logic `PLAN.md:76-86` already established for the single-language
`guessed` flag. The design should short-circuit as soon as one grammar accepts, rather than always
running every narrowed candidate to completion, when the cascade has already reduced the candidate
set to a size where an early accept is decisive.

#### Scenario: a word parses in two or more loaded languages

This is the real, common case for closely related languages/dialects sharing an orthography — cheap
signals (script/character-set) structurally cannot resolve it, because by construction the word is
valid text in more than one loaded grammar.

**Superseded 2026-07-24 by `PLAN.md` § D11** (John: *"Prefer all languages — one language is just
'faster' or 'better options'"*). This section originally specified an elimination order in which the
session prior and then a cross-language score comparison forced a single language, with
multi-language tagging as the last resort. D11 inverts that: **every accepting language is kept, and
the word is multi-language-tagged by default.** Narrowing to one language is an optimization
(speed, and a less diluted candidate list), never a correctness step.

The steps below therefore survive as a **ranking** order over a result set that keeps every
accepting language, not as an elimination order:
1. The language matching the session/document prior ranks first (D-LangID-1 step 3).
2. Remaining languages are ordered by the candidate's score under each language's own D4 class-LM
   (see D-NGram-3 for the normalization this needs) — reusing the ranking machinery already being
   built, not a new classifier.
3. The word is tagged with **all** accepting languages, ordered by 1-2 — see D-Data-2 for how the
   seen-word cache represents a multi-language-tagged entry.

The governing rule, per D11: **hard feasibility signals may eliminate a language; soft signals may
only rank.** Host-declared writing system (authoritative external input) and the script/character-set
gate (a hard fact about what a writing system can contain) may eliminate. The session prior and the
cross-language score comparison may not — they order.

One consequence worth stating here because it changes this design's own risk profile: D-NGram-3's
cross-language comparability bet **stops being load-bearing**. Under the elimination order a
mis-normalized comparison discarded a correct analysis; under D11 it only mis-orders a result set
that still contains it. Still a bet, still to be measured, no longer blocking.

#### Scenario: a word parses in none of the loaded languages

Route to each remaining candidate language's own guess-branch (`WordAnalysis.guessed = true`,
`PLAN.md:76-86`), in session-prior order, exactly as the single-language design already does. If no
loaded language's guess branch produces an acceptable analysis either (most likely: the word is in a
script/orthography no loaded language uses), that is a distinct outcome — "no loaded language could
plausibly account for this word" — not a silent no-op and not a forced guess under the
best-available-but-wrong language. What a host does with that outcome (prompt to load another
Language Pack, flag for review, ignore) is a host-UX decision PanGloss does not own, per the
existing FieldWorks-investigation-handoff boundary (`CONTEXT.md:231-236`); PanGloss's contract is to
return this as a distinct, typed outcome rather than folding it into an ordinary "no correction
found" result. The exact outcome shape is an open question (see Open Questions).

### D-NGram-1 — D4's two-scale n-gram stays strictly per-language; no shared vocabulary

D4's classes (`PLAN.md:99-114`) are POS/`syn_fs`/`mpr`-derived and defined per-grammar; there is no
inter-lingual tag space to unify them into, for the same reason D1 excludes authored semantic data —
inventing a cross-language class mapping is not something the parser deterministically fixes, and no
research report (04-10) proposes or measures one. Each loaded language keeps its own complete
two-scale class n-gram (inter-word class trigram + intra-word morpheme n-gram), trained on its own
corpus/synthetic data, exactly as D4 already specifies for the single-language case. There is no
union vocabulary and no shared model.

### D-NGram-2 — A cross-language context boundary degrades to the coarsest backoff rung, not a joint model

When the word immediately to the left was identified (D-LangID-1) as a **different** language than
the one the current candidate is being scored under, there is no trained joint distribution over
(language A's classes, language B's classes) — building one would need a cross-lingual factored LM
that no report evidences and D1 gives no basis for. Instead, treat a cross-language boundary as
equivalent to a context break for LM purposes: back off to D4's coarsest rung (open/closed class, or
no context at all) exactly as a sentence-initial position already does in an ordinary backoff ladder
(`PLAN.md:99-114`). This reuses an existing mechanism rather than inventing a new one for
code-switching, at a stated, explicit cost: **ranking quality degrades at every code-switch
boundary**, not just at points where it structurally must.

### D-NGram-3 — Cross-language score comparability is a stated design bet, not a solved problem

D-LangID-1's tie-break (step 2) needs to compare a candidate's score *as language A* against its
score *as language B*. Each language's class-LM score is a log-probability from a model trained on
that language's own corpus, with its own class cardinality and smoothing — not comparable by
construction. No cited research report addresses cross-language score comparability (04-10 are all
explicitly single-grammar in scope). The design position, offered as a bet requiring empirical
validation before being load-bearing: normalize each language's score against that language's own
score distribution over a calibration set before comparing (e.g. a per-language z-score), or restrict
the cross-language comparison to the error-model term alone (D1's grammar-derived cost, which is at
least dimensionally comparable — "how well-formed is this string" — across grammars in a way a
class-LM log-probability is not). This is recorded here as a bet and again in Open Questions; it must
not be treated as settled.

### D-NGram-4 — Word prediction across languages runs each loaded language's own model and merges

For the "next word" product, PanGloss does not know in advance which loaded language the next word
will be in. Each loaded language's own class-LM independently proposes its most likely continuations;
candidate lists are merged and ranked using the same per-language-normalized scoring from
D-NGram-3, with the session/document language prior (D-LangID-1 step 3) weighted heavily toward
continuing in the same language as the immediately preceding word — an explicit bias, stated as
such, reflecting that code-switches are the minority case in running text, not a measured constant.

### D-Data-1 — Per-language spell-check data is an additive `.pgpack` section; no shared/union blob

D4's per-language class-LM tables (inter-word and intra-word), the phonological substitution-cost
table derived from that grammar's own `CharDefTable`/`unif_closure` (`00-synthesis.md:92-99`), and
per-writing-system orthographic-unit/word-forming-set data (D-LangID-1 step 2's data source) are new
additive sections inside that language's own `.pgpack` — not a new shared package format, and nothing
crosses between languages' packages. This is consistent with `.pgpack` already being "data-only" with
"no WASM modules, native libraries, scripts, or executable extensions" (`CONTEXT.md:126-131`): every
addition here is more data, read by code the Runtime already ships.

### D-Data-2 — A new session-level layer holds the active language set and a cross-language seen-word cache

Two things live above any single `.pgpack`, at the session/request level, generalizing the existing
"every request names its handle" pattern (`CONTEXT.md:110-111`) to "a request may name a **set** of
handles plus a priority ordering":

1. **Active language set** — which loaded packs are in scope for identification for this
   session/document, in host-supplied priority order (mirrors FLEx's project vernacular+analysis
   writing-system list or Keyman's installed-keyboard set). This is session state, not `.pgpack`
   content.
2. **Seen-word cache** — the caching target is words *seen* (typed by this user, or present in this
   document), not enumerated wordforms, per the stated requirement. Because a code-switched document
   is naturally one mixed-language stream, the cache is modeled as **one flat, language-tagged
   set per session/document** (each entry carries the language(s) D-LangID-1 attributed it to,
   including the multi-language tag from the tied-word scenario) rather than split into N
   per-language caches at write time. This cache is ephemeral per document/session; it is explicitly
   **not** the same thing as the personal lexicon overlay (D-Data-3), which is authored and
   persistent. Promotion from "seen this session" to "in my personal overlay" reuses the
   already-designed speller→lexicon acceptance path (`00-synthesis.md:153`, "the speller→lexicon 'add
   this lexeme' path").

### D-Data-3 — Personal overlays stay per-(user, language); no new sharing mechanism

A personal wordlist, confusion/error model, and cache-LM are all inherently language-specific: a
user's OOV word belongs to one grammar's lexicon, and a user's typo pattern for one language's
phonology/keyboard does not transfer to another language's. This change makes **no** change to the
mechanism `06-personalization-and-privacy.md` already grounds by direct code reading: personal
wordlist entries are additional `SuppliedRoot`s in `SuppliedRootOverlay`
(`rust/crates/pg-parse/src/overlay.rs:15-17,55`, confirmed directly: `RootAuthority::SuppliedOverride`
at line 17 is the override primitive, `SuppliedRootOverlay` at line 55), and the whole
lexicon state is a revisioned, immutable, CAS-checked snapshot swapped behind an `Arc`
(`rust/crates/pg-lexicon/src/runtime.rs:48,77,142`, confirmed directly: `LexiconSnapshot` at line 48,
`revision: Revision` field, `RwLock<Arc<LexiconSnapshot>>`-shaped state at line 77,
`expected_revision`-based compare-and-swap checked at, e.g., line 305). Multilingual operation is
**one more overlay instance per loaded pack, per user** — the existing pattern applied N times, not a
new cross-language sharing layer. Any future cross-user aggregation (`00-synthesis.md`'s Tiers 1-2)
would likewise need to be scoped per-language; this change does not design that, only notes the
scoping constraint so a later change does not accidentally propose a cross-language aggregate.

### D-Data-4 — Memory budget scales roughly linearly in resident languages; load/unload is host-driven, no PanGloss-owned eviction policy

Because D-Data-1 keeps every language's data self-contained (no shared FST or lexicon substrate to
amortize, matching D1's per-grammar factor scoping), N resident languages cost approximately N× one
language's resource envelope — a real, stated cost, not something this design can amortize away.
Given packs are already independent immutable handles (`CONTEXT.md:110-111`), the natural v1 policy
is: the host explicitly loads and unloads packs (mirroring FLEx's project writing-system list or
Keyman's installed keyboards) and PanGloss does not implement its own eviction heuristic (LRU or
otherwise) in this design. This matches the repo's stated posture that callers own "deployment UI"
and PanGloss "never launches FieldWorks" or owns caller history (`CONTEXT.md:19-21`). Adding a
language at runtime is `load(new .pgpack)`; removing one is `unload(handle)`; neither requires
recompilation, since packs are already immutable data files. Smarter lazy-loading is deferred until
real multi-pack memory pressure is measured — consistent with the repo's stated build philosophy of
not compromising or over-building ahead of evidence (`00-synthesis.md:17-26`).

## Dependencies and Ownership

- Depends on `PLAN.md`'s D1/D3/D4/D5 remaining decided and unmodified.
- Depends on a `.pgpack`-producing, loadable-analysis-artifact pipeline existing — the
  `openspec/changes/STAGING.md` FST-coverage track's eventual output — but does not depend on any
  specific change within that track and is not sequenced against it here.
- Any later implementation change consuming this design owns: the new `.pgpack` section schema
  (D-Data-1), the session-layer active-language-set and seen-word-cache API (D-Data-2), the
  script/character-set gate's data source (D-LangID-1 step 2 — contingent on the Open Questions
  item about whether that data is already extracted), and the per-language D4 training pipeline
  extended to run once per loaded language rather than once globally.

## Risks

- **D-NGram-3's normalization bet is unvalidated.** If cross-language score normalization does not
  actually produce sensible tie-breaks in practice, D-LangID-1's step 2 tie-break has no fallback
  besides the session prior (step 1) and the multi-language tag (step 3) — both of which this design
  already treats as acceptable terminal states, so the risk is degraded tie-break quality, not a
  correctness failure.
- **The script/character-set gate (D-LangID-1 step 2) may have no data to run on today.** See Open
  Questions — if per-writing-system script/character-set data is not yet extracted into
  `pg_snapshot`, this gate cannot ship as designed until that extraction exists, which is a
  new, currently-unscoped prerequisite this change surfaces but does not itself resolve.
- **Resource envelope scoping for N resident packs is undefined.** See Open Questions — if the
  absolute resource ceiling (`CONTEXT.md:254-256`) is process-global rather than per-pack, D-Data-4's
  "roughly N×" cost model could silently exceed it well before a host expects a failure.

## Open Questions

These are gaps this change could not settle from the required reading or repo inspection. They are
recorded rather than papered over with an invented answer.

1. **Is per-writing-system script/character-set data (word-forming set, multigraphs, custom PUA
   characters, combining-class overrides) actually extracted into PanGloss's own data model today?**
   `sil-primary-sources.md` (`:18-51`) documents that FieldWorks/LibLCM has this data. But the only
   writing-system data found in the extraction pipeline by direct inspection is a plain list of
   writing-system tag strings — `pg_snapshot::Project { vernacular_writing_systems: Vec<String>,
   analysis_writing_systems: Vec<String> }`, populated from `CurVernWss`/`CurAnalysisWss` as
   space-separated tags (`rust/crates/pg-fwdata/src/extract/project.rs:33-37`). No richer per-writing-
   system script/character-set/combining-class object was found in `pg-snapshot` or `pg-grammar` by
   grepping for writing-system-related identifiers. If the richer data is not extracted yet,
   D-LangID-1 step 2 needs its own extraction work, scoped as a prerequisite change, not assumed
   available. **This needs the user's call**: confirm whether this data already exists somewhere
   this search missed, or whether it needs to be scoped as new work.
2. **Is the absolute resource ceiling (`CONTEXT.md:254-256`, "a versioned, hard-coded, deliberately
   high non-disableable limit") scoped per loaded pack, or aggregated across every pack resident in
   one process?** `CONTEXT.md` does not say, and D-Data-4's "roughly N× cost" model needs this answer
   to state a real multi-language memory budget. Unresolved by this change.
3. **Cross-language score normalization (D-NGram-3)** is a design bet with no supporting measurement
   in any of reports 04-10, all of which are explicitly single-grammar in scope. It needs its own
   validation pass (a calibration-set measurement, not a spike/implementation) before being treated
   as more than a placeholder.
4. **The exact typed outcome for "a word parses in no loaded language's normal or guess branch"**
   (D-LangID-1's last scenario) is named as a distinct outcome here but its precise shape (a new
   variant on the existing atomic-word-analysis-result contract? a separate diagnostic event?) is not
   designed. Left for the implementation change that consumes this design.
5. ~~**Whether a persistently-tied multi-language word (D-LangID-1's ambiguity scenario) should ever
   be forced to a single language by a deterministic tiebreak rule, or whether a multi-language-tagged
   terminal state is an acceptable permanent result**~~ — **ANSWERED 2026-07-24 by `PLAN.md` § D11**,
   and answered more strongly than this change proposed. A multi-language-tagged result is not merely
   acceptable, it is the **default**: all accepting languages are kept, soft signals rank rather than
   eliminate, and narrowing to a single language is an optimization for speed and candidate quality
   with no correctness standing. See D-LangID-1's ambiguity scenario, rewritten above. Dropping a
   candidate language under resource pressure is a *stated* degraded mode, never silent — the same
   rule `PLAN.md` § D10 applies to tier budgets.
6. **Sequencing against D2.** D2 (the unified weighted composition itself) is "direction settled, not
   designed" (`PLAN.md:14`). This change's cross-language n-gram answer (D-NGram-1 through 4)
   describes how D4's *terms* behave across languages, but the composition mechanism those terms feed
   into does not have its own design yet — a sequencing risk to flag, not a contradiction.
