# Hunspell — system profile

Rubric profile for the cross-system comparison table (`docs/research/spellcheck/00-synthesis.md`
and siblings). Labels: **measured** = read/quoted directly from a primary source; **asserted** =
primary source states it without a derivation/benchmark I could verify; **synthesis** = my
conclusion combining sourced facts with PanGloss's own architecture (already documented in
`00-synthesis.md`, `01-lexical-distance.md`, `02-phonological-distance.md`, `03-keyboard-keyman.md`,
`06-personalization-and-privacy.md`). Unfetchable sources are flagged inline.

Primary sources used: `hunspell/hunspell` man page (`man/hunspell.5`, fetched directly), the
project README (fetched directly), `license.hunspell` (fetched directly), Spylls (a from-source,
tested reimplementation used across the sibling reports as a primary-adjacent description of
Hunspell's actual C++ suggestion algorithm — the original C++ has no separate algorithm-design
document), the `TrnsltLife/HunspellXML` wiki (a third-party dictionary-authoring tool, used only as
evidence of authoring effort), and the Apache Lucene JIRA issue `LUCENE-5468` (a measured memory
report against Lucene's own Java Hunspell port, flagged as such — not the original C++ library).

---

- **ARCH:** A C++ library that checks whether a surface wordform is generable from a `.dic` wordlist
  by applying a bounded set of hand-authored `.aff` prefix/suffix rules (optionally two-deep,
  "twofold stripping"), then ranks correction candidates through a fixed, ordered cascade of
  independent structural-edit generators (KEY-adjacent substitution, TRY-alphabet scan, REP-table
  replacement, n-gram-over-whole-dictionary, optional PHONE fallback) — **measured** (man page;
  Spylls `algo_suggest.html`, cited in `03-keyboard-keyman.md` §1 and `02-phonological-distance.md`
  §6).

- **LEXICON:** Affix-compressed wordlist: a flat `.dic` word list (one stem per line, `word[/flags]`)
  plus a separate `.aff` file of `PFX`/`SFX` rules keyed by flag, with an `AF` table that further
  compresses repeated flag-set combinations to ordinal aliases ("Hunspell can substitute affix flag
  sets with ordinal numbers in affix rules (alias compression)") — **measured** (man page). Not an
  FST and not a generative grammar; it is a stem list with a rule table applied at lookup time.

- **MORPHOLOGY:** Affix-flag rules only, not full generative morphology, and **cannot** accept an
  unbounded set of inflected forms without enumerating the affix rules that produce them — every
  inflected/derived form must trace to an explicit `PFX`/`SFX` rule attached (via a flag) to a
  specific dictionary stem. "Twofold suffix stripping" lets one stem cover both an inflectional and
  a derivational suffix layer ("requires only the square root of the number of suffix rules compared
  with a one-level implementation") for agglutinative languages (Azeri, Basque, Estonian, Finnish,
  Hungarian, Turkish named explicitly) — **measured** (man page) — but stripping depth is fixed at
  two levels, not recursive/unbounded concatenation the way HermitCrab/foma morphotactics is. There
  is no representation of an open-ended template or circumfix chain beyond what a `.aff` file's
  author hand-writes rule-by-rule.

- **ERRORMODEL:** A cascade of independently-authored, incomparable-score mechanisms, not one
  weighted model: `KEY` (single-character keyboard-adjacent substitution, same-row strings only, no
  2D geometry), `TRY` (frequency-ordered alphabet for brute-force single-char edits — Hunspell's
  actual primary fallback), `REP` (hand-authored common-mistake replacement table), an n-gram
  distance pass over the whole dictionary, and an optional `PHONE` table borrowed from Aspell's
  phonetic algorithm — **measured**, all five directives confirmed directly from the man page; the
  cascade ordering and score-incomparability ("if any good edits found, ngram suggestions wouldn't be
  used") is from Spylls' from-source description, corroborated independently by reports 02 §6 and 03
  §1. No edit-distance/phonetic/keyboard cost is ever combined into one score; each stage runs to
  exhaustion or is skipped by an ad hoc gate.

- **DETECTION:** Correction-only in the sense that matters here — Hunspell has **no** representation
  of sentence context, so it cannot flag **real-word errors** (a validly-spelled word used
  incorrectly for its grammatical/semantic slot). Its "detection" is pure non-membership: a word
  fails only if no stem+affix-flag combination in the loaded `.dic`/`.aff` produces it — **synthesis**
  from the architecture description above (no primary source claims real-word detection; the man
  page describes only membership-checking and suggestion generation). This matches `00-synthesis.md`'s
  standing finding that CG-style disambiguation, not Hunspell-style checking, is needed for real-word
  detection.

- **CONTEXT:** None. No sentence-level, n-gram, or grammar/agreement signal anywhere in the
  documented architecture — every check and every suggestion-cascade stage operates on one isolated
  wordform at a time — **measured** (absence confirmed across the full man page read).

- **SEMANTICS_POS:** Confirmed no, with one narrow caveat. The `.dic`/`.aff` morphological-description
  fields include an optional `po:` (part-of-speech) tag and related `st:`/`al:`/`ds:`/`is:` fields —
  **measured** (man page) — but these are inert annotation-only fields for downstream stemming/
  lemmatization consumers (e.g. returning "this stem is a noun" to a caller); they are **not** used by
  the spell-checking or suggestion algorithm itself to gate or rank anything. There is no semantic-
  domain concept anywhere in the format. Net: no POS- or semantic-category-driven behavior in the
  speller itself — **synthesis**, confirming the expectation stated in the task brief.

- **DATA_REQ:** Minimum to stand up a new-language speller is a `.dic` word list (can be as small as
  hand-authored, no minimum enforced by the format — even a single-stem file with an empty `.aff`
  "works," per the `oxygenxml` custom-dictionary doc found via search) plus a `.aff` file that must
  be hand-authored per language: flag scheme, character `SET`/encoding, and every `PFX`/`SFX` rule
  needed to cover the language's inflection/derivation — **measured** structurally (man page), with
  authoring-effort evidence from the third-party `TrnsltLife/HunspellXML` tool's own tutorial, which
  needs multiple conditional rules just for English noun pluralization (`cats` vs `foxes` vs
  `bunnies` vs irregular `geese`/`mice`) and states plainly "other languages have much richer
  morphology" — i.e. non-trivial linguistic engineering, not a data-volume threshold; there is no
  corpus or training-pair requirement at all (unlike a statistical/neural approach) since the model
  is entirely rule- and wordlist-based. For a hyper-minority language this trades "no corpus needed"
  for "someone must hand-write and test every affix rule," with zero help from any generative grammar
  already authored (a separate rule set from whatever HermitCrab/LibLCM already encodes).

- **PERSONALIZATION:** A personal dictionary is a flat, unversioned word-list file (no leading
  word-count line, unlike the main `.dic`), one entry per line, with exactly two extra primitives: a
  leading `*` marks an entry **forbidden**, and a `word/model` form lets a personal entry inherit an
  existing word's affix class by reference — **measured** (man page, cross-checked against
  `manpages.ubuntu.com`). Default location is locale-derived (`$HOME/.hunspell_default` or
  `$HOME/.hunspell_<dicname>`). No versioning, no conflict/merge handling beyond line
  presence/absence, no incremental confusion-model or LM adaptation of any kind — confirmed in
  `06-personalization-and-privacy.md` §3, which also notes PanGloss's own `SuppliedRootOverlay` +
  revisioned `LexiconSnapshot` (`rust/crates/pg-parse/src/overlay.rs`, `rust/crates/pg-lexicon/src/runtime.rs`)
  is already a strict superset of this mechanism (trie-keyed, feature-lane-aware, CAS-versioned vs.
  Hunspell's flat unversioned file).

- **INTEGRATION:** LibreOffice, OpenOffice.org, Mozilla Firefox/Thunderbird, Google Chrome, macOS, and
  proprietary tools including InDesign, memoQ, Opera, and SDL Trados all consume Hunspell — **measured**
  (README, cross-checked via search-result secondary summary for the proprietary-app list). It is
  the de facto standard `.dic`/`.aff` dictionary format across the open-source desktop/browser
  ecosystem, with dictionaries distributed as `.oxt`/browser-extension packages. **Could not confirm
  whether Paratext consumes Hunspell** — searched directly, found no primary or secondary source
  confirming or denying Paratext's spell-checker backend; flagged as an open gap, not assumed either
  way.

- **LICENSE:** Tri-licensed MPL 1.1 / GPL 2.0 / LGPL 2.1 — user's choice of any one — **measured**,
  read directly from `license.hunspell`. Fully compatible with embedding in a permissively-licensed
  project (choose the LGPL 2.1 or MPL 1.1 term).

- **FOOTPRINT:** No primary-source memory/runtime benchmark exists in Hunspell's own README or man
  page (checked directly — absent). One **measured** third-party data point, flagged for its
  provenance: Apache Lucene's own Java port of Hunspell dictionary-loading (`LUCENE-5468`) reported
  that loading a 4.5MB Polish `.dic`/`.aff` pair required close to 2GB of heap before Lucene's own
  fix, roughly 8x a comparable Java stemmer (Stempel) at ~250MB for the same data — this is a bug
  report about a specific **Java reimplementation's** loading strategy, not a measurement of the
  original C++ library's runtime footprint, and should not be read as "Hunspell inherently needs 2GB."
  **WASM feasibility: real evidence, not hypothetical.** Multiple independent WASM ports exist and
  are used in production-adjacent contexts: `hunspell-asm` (Emscripten-compiled Hunspell C++ to
  WebAssembly, isomorphic JS bindings), `hunspell-wasm` (WebAssembly port with a TypeScript wrapper),
  and LibreOffice's own full WASM/Emscripten build (`LibreOffice/core static/README.wasm.md`) which
  ships Hunspell as part of the whole office suite running client-side in-browser — **measured**
  (existence and purpose of each, via direct search/fetch), though none of these sources publish a
  specific WASM binary size or memory-ceiling number for Hunspell in isolation.

- **RUST_C:** Native library is C++ (the reference `hunspell/hunspell` implementation). Rust options,
  all found via direct search of crates.io/lib.rs/GitHub: (a) FFI bindings wrapping the C++ library —
  `hunspell-sys` (raw bindings to the Hunspell C API) and `hunspell-rs`/`hunspell` (higher-level
  wrappers built on `hunspell-sys`; `hunspell-rs`'s own docs note "not all Hunspell features are
  supported yet," and "not possible to change the dictionary at runtime" — **measured**, from the
  crate's own description); (b) a **native Rust reimplementation**, `zspell` (`pluots/zspell`),
  which maintains `.dic`/`.aff` format compatibility with Hunspell dictionaries without linking any
  C library, explicitly states WASM as a design goal ("the goal of being usable via WASM, though
  official WASM bindings will be added at some point"), and claims to outperform other spellcheckers
  by holding compiled word lists (reported ~20MiB for a full dictionary) entirely in memory —
  **measured** (crate's own README/description), **asserted** for the specific performance claim (no
  independent benchmark verified by me). Per PanGloss's stated build philosophy (port/build over
  wrap-a-C-lib unless trivially usable), `zspell` is the more aligned option if any Hunspell-format
  compatibility were ever wanted, over FFI-wrapping the C++ original.

- **MINORITY_VERDICT:** Poor fit, concretely and specifically. (1) The lexicon model requires a
  wordlist — for a hyper-minority language with no existing corpus, that wordlist must be manually
  compiled from scratch, and every inflected surface form not covered by a hand-written `PFX`/`SFX`
  rule is simply invisible to the speller (unbounded morphology is structurally impossible to
  represent, per MORPHOLOGY above) — this is the same "dictionary of surface forms" ceiling
  `01-lexical-distance.md` §4 identifies as fundamentally mismatched to a propose→confirm generative
  parser. (2) Authoring the `.aff` file duplicates linguistic work a HermitCrab/LibLCM grammar has
  usually already done, in a completely separate, non-reusable rule language — there is no path to
  derive a `.aff` file mechanically from an existing PanGloss grammar without essentially
  re-encoding its morphotactics by hand. (3) The error/suggestion model (`KEY`/`TRY`/`REP`/n-gram/
  `PHONE`) is a fixed English/European-typing-oriented cascade with documented score-incomparability
  failure modes (per 02 §6, 03 §1) and no keyboard-geometry story that matches a Keyman-typed,
  dead-key-heavy orthography (per 03 §1: `KEY` is same-row-only, no diagonal/2D, no dead-key
  representation at all). (4) No detection of real-word errors, no context/grammar use, no
  personalization beyond a flat forbid/inherit word list — every axis PanGloss's target use case
  actually needs (unbounded morphology, detection, personalization with versioning) is either absent
  or requires bypassing Hunspell's own model entirely. **What doesn't break**: the file *format* is
  simple enough to hand-author for a small closed-class vocabulary (e.g., a fixed list of function
  words) and the tri-license/WASM-portability story is genuinely good if PanGloss ever needed to
  interoperate with a host (LibreOffice, Firefox) that only speaks `.dic`/`.aff` — but standing up a
  Hunspell speller as the **actual spelling-correction engine** for a new orthography with few
  hundred–few thousand speakers means either enumerating a wordlist that can never be complete or
  hand-writing affix rules that duplicate the grammar's own morphology in a weaker, non-generative
  form.

- **HEADLINE:**
  - Strengths: (1) Universally deployed, de facto standard format — LibreOffice, Firefox/Thunderbird,
    Chrome, macOS, and multiple proprietary tools all consume `.dic`/`.aff` directly, so it is the
    single most interoperable target if a host integration is ever required. (2) Genuinely
    lightweight to stand up for a small, closed-vocabulary use case — no corpus, no training pairs,
    just a wordlist and (optionally trivial) affix file. (3) Real, working WASM ports already exist
    (`hunspell-asm`, `hunspell-wasm`, LibreOffice's own WASM build) — the deployment-target question
    is already answered by prior art, not something PanGloss would be first to prove.
  - Weaknesses vs. a morphology-aware approach: (1) Morphology is a fixed, hand-enumerated affix-flag
    rule set, not a generative grammar — cannot represent unbounded inflection/derivation the way
    HermitCrab/foma does, and duplicates whatever morphological work the grammar author already did
    in a separate, non-reusable rule language. (2) The error/suggestion model is a cascade of
    independently-authored, score-incomparable stages (Hunspell's/Spylls' own documented weakness),
    not the unified weighted-FST composition PanGloss's own research (reports 01-03) already
    converges on. (3) Zero context/grammar/POS/semantic-domain use anywhere — cannot detect
    real-word errors, which `00-synthesis.md` identifies as the harder half of the detection problem.
