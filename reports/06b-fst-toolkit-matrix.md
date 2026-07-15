# Capability matrix: established FST toolkits, for an offline-compile + tiny-runtime architecture

**Scope and how this differs from `reports/02-established-fst-libraries.md`.** Report 02 asks "should
`hc-fst`/`hc-hybrid` be replaced by an established Rust FST crate" and answers no, because `hc-fst` is a
unification-over-partial-feature-structures matcher with capture groups, not a classical discrete-alphabet
FST, and no established library (rustfst, `fst`, OpenFst, foma, HFST, SFST) implements that primitive. **This
report does not re-litigate that.** It answers a different, narrower architecture question, stated in the
brief: *if* HermitCrab grammars were compiled **offline** (compiler in any language, any license) into a
**classical symbol-alphabet FST**, and only a **small runtime** (target: C#) plus the **compiled artifact**
shipped — under `<10MB` artifact / `<1ms`-per-word lookup / MIT-compatible-license constraints on the shipped
side only — which toolkit(s) could do the offline compilation, and what would the shipped runtime look like?
This requires resolving hc-fst's feature-structure constraints into a discrete symbol alphabet before
compilation (an orthogonal, non-trivial step this report does not attempt to size); everything below assumes
that step has already happened and asks what the classical-FST world can do with the result.

Every claim below is cited to a URL (official docs, source code, or a published paper). Claims that could not
be pinned to a primary source are explicitly marked **[UNVERIFIED]**. Two facts were re-verified directly in
this session (not just taken from sub-agent research) because they are load-bearing for the recommendation:
the HFST optimized-lookup flag-diacritic evaluation code, and the license of an independent `.hfstol` reader
implementation (§3.3, §7).

---

## 1. Executive summary

- **Yes, a full replace-rule calculus (context rules, directional modes, optional/obligatory, epenthesis/
  deletion, cascading) exists and is mature** in the xfst-descended family: original Xerox xfst, its two open
  reimplementations foma and HFST's `hfst-xfst`/`hfst-twolc`, and — for the OpenFst world — Google's Thrax and
  Pynini (`cdrewrite`, with explicit `LEFT_TO_RIGHT`/`RIGHT_TO_LEFT`/`SIMULTANEOUS` and `OBLIGATORY`/`OPTIONAL`
  parameters). Bare OpenFst and rustfst have **no rule calculus at all** — only automaton algebra primitives;
  lttoolbox has **no rewrite-rule calculus either**, only lexicon/paradigm compilation (§3.7).
- **Flag diacritics are supported at compile time by the whole xfst-descended family, and — critically —
  HFST's `hfst-optimized-lookup` runtime genuinely *evaluates* them per-path at lookup time**, pruning
  disallowed continuations rather than passing flags through as literal symbols. This is verified directly
  against source (`libhfst/src/HfstFlagDiacritics.h`, `libhfst/src/implementations/optimized-lookup/
  transducer.cc`, §3.3) and independently corroborated in `foma`'s `apply` engine (`obey-flags`, on by
  default) and in `kfst`'s pure-Python `transducer.py`. This is the mechanism that keeps morphotactic
  constraint combination linear rather than requiring an exponential pre-expanded product automaton — it is
  real, shipped, and load-bearing evidence for the offline-compile architecture.
- **Reduplication via compile-replace (`^[ ... ^]`) is real, dates to Xerox xfst (Beesley & Karttunen 2000),
  and is supported today in HFST's `hfst-xfst`** (dated in HFST's own changelog, v3.8.2/v3.9.1). **Current
  foma does *not* ship compile-replace** — only the weaker `_eq()` predicate (added foma v0.9.12alpha, 2009)
  — this corrects the task brief's premise for foma specifically. Compile-replace was historically described
  as patent-encumbered (a matching-titled Xerox US patent, 7,010,476, was located, but its claims could not be
  read from the OCR-only USPTO PDF this session — **[UNVERIFIED]** as to whether it actually covers
  compile-replace or is still in force; if filed in Xerox's ~2000–2003 FST-patent era it would likely have
  already expired under the standard 20-year utility-patent term, but this is inference, not confirmation).
  Reduplication is a **compile-time/artifact-size concern, not a runtime one** — once compiled, the shipped
  runtime just traverses a (larger) finite automaton with no special-case code; see §6.
- **Large, real production morphologies exist that exercise big alphabets, big lexicons, and — separately —
  reduplication and templatic morphology**, but *not all in the same project*: OMorFi (Finnish, HFST,
  concatenative, 555k+ lexemes, published states/arcs/disk-size table), AraComLex (Arabic, foma/xfst/HFST,
  templatic root-and-pattern, 30,587 lemmas generating 12.9M word forms at ~5,000 words/sec — note the
  paper's 11MB/340MB figures describe a gzipped *enumerated word-form list*, not the compiled transducer,
  whose size is not published), Hebrew
  HAMSAH (xfst, ~2M states/2.2M arcs), Zulu/Xhosa (xfst/foma, productive reduplication explicitly
  documented), Skolt/North Sámi (HFST, huge paradigm counts, no reduplication). The one language that pushed
  hardest on templatic + long-distance morphotactics — **Amharic/Tigrinya (Gasser's HornMorpho) — did *not*
  stay inside a classical FST formalism**; it extended to **weighted FSTs where the "weight" is an
  accumulated feature structure**, a genuine formalism extension beyond what HFST/foma/OpenFst ship (§4,
  §5.5). This is the most consequential finding for feasibility of "any templatic morphology fits in a
  classical FST."
- **Lazy/on-the-fly runtime composition exists in OpenFst (`ComposeFst`, delayed, doxygen-documented) and
  rustfst (`lazy` module, `ComposeFst`/`LazyFst`) — but not in HFST's shipped runtime.** HFST's pipeline is
  always compile lexicon + rules separately, merge ahead-of-time with `hfst-compose-intersect`, convert to
  `.hfstol`, then traverse one pre-composed network; no lazy per-word lexicon∘rules composition is exposed
  through `hfst-optimized-lookup`, `hfst-lookup`, or the C++ API (§3.4). If PanGloss ever needs true
  runtime-lazy composition, that is an OpenFst/rustfst-shaped feature, not an HFST-shaped one.
- **Multi-analysis (all paths, not 1-best) is the universal default** across every runtime checked — foma
  `apply up`, HFST `hfst-lookup`/`hfst-optimized-lookup`, lttoolbox `lt-proc`, rustfst's path iteration,
  pyfoma's `analyze()` generator — all confirmed by primary source (§3, per-toolkit).
- **For the C#-shipped runtime specifically: no C# implementation of any of these formats exists today**
  (verified: no HFST/foma/OpenFst bindings or format readers found on NuGet/GitHub; SIL's own
  `SIL.Machine.FiniteState` namespace, checked directly in this worktree's `machine/` submodule, is the
  unification-matcher family report 02 already covers, not a classical-FST/flag/serialization engine — grep
  for `hfst|openfst|foma|FlagDiacritic` across `machine/` returns nothing). **The closest sizeable precedent
  is `hfst-optimized-lookup-java`** — a from-scratch, Apache-2.0-licensed Java reader for the exact `.hfstol`
  format, including a dedicated `FlagDiacriticOperation.java`, measured in this session at **1,500 lines
  across 13 files** (§7) — a genuinely bounded reference for what a C# port would need to reproduce. A newer,
  actively-maintained, larger precedent also exists: `divvunspell` (Rust, Apache-2.0, ~484KB of Rust source),
  described by its own README as a reimplementation/extension of the C `hfst-ospell` runtime.
- **License, precisely, per pipeline stage** (not a blanket verdict — the offline/artifact/runtime stages
  have *different* license profiles, and the task's premise that offline GPL tooling is fine changes the
  calculus from report 02's static-linking question):
  - *Offline compiler*: HFST core is **GPLv3** (verified: repo `COPYING` is the full GPLv3 text, contradicting
    HFST's own wiki License page which claims LGPLv3 — the `COPYING` file is authoritative). foma is
    **Apache-2.0** (verified: license headers in current source; no separate top-level `LICENSE` file, which
    is why GitHub's license-detection API reports `null`). Both are fine to use **offline only**, per the
    task's own premise.
  - *Compiled artifact (the `.hfstol`/`.bin`/`.fst` file itself)*: data, not linked code — not GPL-encumbered
    by virtue of being produced by a GPL tool (compiling with GCC does not GPL your binary; compiling with
    HFST does not GPL your transducer file). HFST's own maintainers implicitly treat the format this way (see
    next point).
  - *Shipped runtime*: **HFST's own project explicitly splits this out.** The HFST wiki License page states:
    "The interfaces for fast lookup from transducers and spell checking are distributed separately by the
    HFST project... licensed under the Apache license" — confirmed empirically: `hfst-ospell` and its Rust
    reimplementation `divvunspell` are both Apache-2.0 (verified via GitHub license API), and the standalone
    `hfst-optimized-lookup-java`/`-python` readers in `hfst/hfst-optimized-lookup` are **also Apache-2.0**
    (verified directly, this session, by fetching `COPYING` in that repo). No source found makes the precise
    legal claim "a from-scratch `.hfstol` reader is not a GPL derivative work" in so many words, but the
    circumstantial case is strong: an independent, peer-reviewed format specification exists (Lindén,
    Silfverberg & Pirinen, FSMNLP 2009), and HFST's own maintainers ship multiple from-scratch lookup-only
    implementations under Apache-2.0 rather than GPLv3. A from-scratch C# reader following the published
    format spec, written without copying GPL code, sits in the same bucket.
  - lttoolbox is **GPLv2** (verified: `COPYING`, "Version 2, June 1991"); SFST is **GPLv2 or later**; `kfst`
    is **LGPL-3.0**; rustfst, OpenFst, Thrax, Pynini, pyfoma are all **Apache-2.0/MIT-class** permissive.
    Xerox xfst itself was **proprietary, never open-sourced**, and appears effectively discontinued (only
    known distribution was a CD bundled with the 2003 Beesley & Karttunen book).

---

## 2. Capability matrix

Legend: ✓ = confirmed present/supported; ✗ = confirmed absent; ◐ = partial/qualified; — = not applicable to
this toolkit's layer; **?** = no evidence found either way (treat as unverified, not as "no").

| Toolkit | 1. Rewrite-rule calculus | 2. Flag diacritics (compile → runtime) | 3. Reduplication (compile-replace/`_eq`) | 4. Alphabet/scale precedent | 5. Lazy runtime compose | 6. Multi-analysis (all paths) | 7. Serialization + minimal runtime | 8. License |
|---|---|---|---|---|---|---|---|---|
| **foma** | ✓ full xfst-compatible calculus (`->`,`@->`,`(->)`,`||`,`,,`) | ✓ → ✓ (`obey-flags` default on, evaluated in `apply`) | ◐ `_eq()` only; **no compile-replace** in current foma | Benchmarked to ~100k-word lexica; North Sámi lexc 14.2s compile | ✗ eager determinize/minimize between ops | ✓ `apply up`/`down` return all paths | `.bin` gzip binary (byte spec **?**); `flookup.c` ~550-line minimal reader | **Apache-2.0** |
| **HFST compiler** (`hfst-xfst`/`hfst-twolc`/`hfst-lexc`) | ✓ full calculus + weighted variants; twolc weighted rules "NOT FULLY IMPLEMENTED" (own docs) | ✓ compile-time `-x`/`-F`/`-M` flag-diacritic controls | ✓ compile-replace shipped (NEWS v3.8.2, bugfixed v3.9.1) | AraComLex (12.9M forms), Skolt Sámi (30k lemmas/2.3M forms), OMorFi-scale | ✗ `hfst-compose-intersect` is ahead-of-time only | — (compiler, not runtime) | HFST3 container format (`HFST30` signature) | **GPLv3** (repo `COPYING`, verified) |
| **hfst-optimized-lookup** (runtime, `.hfstol`) | — | ✓ **evaluates** flags per-path at traversal (`HfstFlagDiacritics.h`+`transducer.cc`, verified) | — (runtime only traverses; reduplication is already baked into the automaton) | Same corpora as above; format uses 16-bit symbol IDs (~65,536 ceiling) | ✗ traverses one pre-composed network | ✓ `-n N` caps count; default is all analyses | Published format spec (Lindén et al. 2009, FSMNLP); Java reader 1,500 LOC; Rust reader (divvunspell) ~484KB | **Apache-2.0** (explicit carve-out from core GPLv3, verified) |
| **xfst/lexc (Xerox, historical)** | ✓ (this is where the calculus originated) | ✓ (this is where flags originated, Beesley 1998) | ✓ (compile-replace originated here, Beesley & Karttunen 2000) | HAMSAH Hebrew: ~2M states/2.2M arcs, 27m41s compile, 70 words/sec | — (precompiles; no lazy runtime documented) | ✓ (documented default since 2000 paper) | Proprietary `.fst` binary, not documented in public detail | **Proprietary**, effectively discontinued |
| **OpenFst (core)** | ✗ no rule compiler at all (that's Thrax/Pynini's job) | — | — | No numeric symbol-count cap documented | ✓ `ComposeFst`, doxygen-documented delayed FST | — (library, not an analyzer) | OpenFst binary `.fst`/text AT&T format | **Apache-2.0** |
| **Thrax / Pynini** (on OpenFst) | ✓ `CDRewrite`/`cdrewrite()`, directional + obligatory/optional params | ◐ inherits OpenFst's discrete alphabet; flags expressible as symbols, no dedicated flag-eval runtime found | **?** not investigated (out of scope: Python/offline tool, no C# runtime story) | Used at Google production NLP scale (no PanGloss-relevant figures found) | ✓ (inherits OpenFst `ComposeFst`) | ✓ (`paths()` enumerates; `shortestpath()` for 1-best) | Same as OpenFst | **Apache-2.0** |
| **rustfst** | ✗ automaton primitives only, confirmed absent | ✗ confirmed absent | — | No documented cap; `SymbolTable` unbounded in principle | ✓ dedicated `lazy` module + `ComposeFst` | ✓ path iteration vs. `shortest_path`/N-best distinguished | OpenFst-file-compatible (`SerializableFst`); no minimal single-file runtime | **MIT OR Apache-2.0** |
| **lttoolbox (Apertium)** | ✗ "no facility for rule based transformations" (Apertium's own GSoC proposal) | ✗ no production mechanism (an experimental Java-only feature was "never brought into use") | ◐ none native; ad hoc via companion tool `lexd` | apertium-eng: 59,533 stems/391 paradigms/4.2MB source; 58,823 words/sec lookup | ✗ `lt-comp` offline, `lt-proc` lookup-only; `lt-compose` also offline | ✓ `lt-proc -a` returns all ambiguous readings by default | `.bin` format, binary-compatible Java port (`lttoolbox-java`) exists | **GPLv2** (verified `COPYING`) |
| **SFST** | ✓ 4 replace-operator variants, modeled on Karttunen 1995 | ✗ no flag diacritics; uses a distinct, more limited "agreement variables" mechanism instead | **?** none found | Not sized in available docs | **?** not investigated | **?** not investigated | AT&T-adjacent; dormant project | **GPLv2 or later** |
| **kfst** (`fergusq/fst-python`) | ✗ pure runtime library, no rule compiler | ✓ full P/R/D/C/U syntax in `symbols.py`; runtime semantics in `transducer.py` | **?** none found | Small/early-stage project, no scale precedent | **?** not investigated | **?** not investigated | Reads AT&T format or own binary format; pure Python | **LGPL-3.0** |
| **pyfoma** | ✓ `$^rewrite(...)` with leftmost/longest/shortest/optional/weighted params | ✓ "feature calculus" (`[[$X=y]]` etc.), explicitly modeled on xfst flags | ✗ none found | No numeric limits documented | ✗ eager determinize/minimize/trim always | ✓ `analyze()` returns a generator of all analyses | No dedicated binary format found (pure Python) | **Apache-2.0** |

---

## 3. Per-toolkit notes with citations

### 3.1 foma

- **Rewrite rules**: full xfst-compatible replace calculus — `->` (unconditional), `@->` (left-to-right
  longest match), `@>` (left-to-right shortest match), `(@->)` (optional longest-match), context operators
  `||`/`\\`/`//`/`\/`, parallel-rule cascading via `,,`. [regexreference.html](https://fomafst.github.io/regexreference.html),
  [simpleintro.md](https://github.com/mhulden/foma/blob/master/foma/docs/simpleintro.md), Hulden's own
  EACL 2009 demo paper Table 1 ([PDF](https://dingo.sbs.arizona.edu/~mhulden/hulden_foma_2009.pdf)).
- **Flag diacritics**: full `@U/@P/@N/@R/@D/@C/@E@` set documented at
  [morphtut.html](https://fomafst.github.io/morphtut.html). Runtime evaluation confirmed in
  [`foma/foma/iface.c`](https://github.com/mhulden/foma/blob/master/foma/iface.c): `apply_set_obey_flags`,
  with help text "obey flag diacritics in `apply'" — **on by default**, meaning flags actively prune
  `apply up`/`apply down`/`flookup` traversal rather than passing through as literal symbols.
- **Reduplication**: `_eq()` added foma v0.9.12alpha (2009-10-25), per
  [`foma/foma/CHANGELOG`](https://github.com/mhulden/foma/blob/master/foma/CHANGELOG); "Filters from the
  output side of X all those strings where some substrings occurring between the delimiters L and R are
  different" ([regexreference.html](https://fomafst.github.io/regexreference.html)). **Current foma's
  CHANGELOG has zero mentions of `compile-replace`** — it is not a foma feature, contrary to a premise in the
  task brief; that mechanism lives in Xerox xfst and HFST's `hfst-xfst` (§3.3). Live user reports of `_eq()`
  reduplication workarounds: [mhulden/foma#61](https://github.com/mhulden/foma/issues/61),
  [#60](https://github.com/mhulden/foma/issues/60).
- **Scale**: Hulden 2009 benchmark table: 38,418-entry LEXC English dictionary compiles in 1.224s; the North
  Sámi lexc lexicon in 14.23s; note "Foma seems to perform comparably with e.g. the Xerox/PARC toolkit,
  perhaps with the exception of certain types of very large lexicon descriptions (>100,000 words)."
- **Lazy composition**: none — "by default, for efficiency reasons, Foma determinizes and minimizes automata
  between nearly every incremental operation" (same paper). This is the opposite of lazy.
- **Multi-analysis**: `apply up` on "runs" returns both `run+V+3P+Sg` and `run+N+Pl` simultaneously
  ([morphtut.html](https://fomafst.github.io/morphtut.html)).
- **Serialization + minimal runtime**: `save stack file.bin` (gzip-wrapped binary; exact byte layout not
  found in primary foma docs — **[UNVERIFIED]** at the byte level, though universally described as gzip). A
  genuinely small standalone lookup tool exists,
  [`foma/foma/flookup.c`](https://raw.githubusercontent.com/mhulden/foma/master/foma/flookup.c), measured
  this session at **~450–500 lines**, separate from the full interactive compiler (`foma.c`).
- **License**: current source carries an Apache-2.0 header (`foma/foma/foma.c`, copyright 2008–2021,
  verified directly this session by fetching the raw file). The original 2009 paper says foma was GPL at the
  time ("Foma is licensed under the GNU general public license") — **it was later relicensed to Apache-2.0**;
  GitHub's repo-metadata API reports `license: null` only because there is no separate top-level `LICENSE`
  file, not because the code isn't under a license (the in-source headers are authoritative, and Homebrew's
  formula and Wikipedia both independently describe foma as Apache-2.0).
- **C# port**: none found, for the binary format or the apply engine.

### 3.2 HFST compiler layer (`hfst-xfst`, `hfst-twolc`, `hfst-lexc`)

- **Rewrite rules**: `hfst-xfst` implements the full Xerox replace calculus (`->`, `<-`, `<->`, `@->`, `@>`,
  weighted variants, `||`/`//`/`\\` context-restriction operators, parenthesized-optional `(->)`) —
  [HfstXfst wiki](https://github.com/hfst/hfst/wiki/HfstXfst). `hfst-twolc` implements classic two-level rule
  types `=>`, `<=`, `<=>`, `/<=` plus variables/`except` — [HfstTwolc wiki](https://github.com/hfst/hfst/wiki/HfstTwolc)
  — with an explicit caveat on that same page: "weighted rule compilation is NOT FULLY IMPLEMENTED YET," and
  regexp-center rules are excluded from automatic conflict detection.
- **Cascading**: `hfst-compose-intersect` composes a lexicon with one or more rule transducers
  ([manpage](https://manpages.ubuntu.com/manpages/xenial/man1/hfst-compose-intersect.1.html)) — ahead-of-time
  only (§3.4).
- **Reduplication**: HFST's own [NEWS](https://github.com/hfst/hfst/blob/master/NEWS) dates compile-replace
  support directly: v3.8.2 "Merge and compile-replace operations supported in hfst-xfst"; v3.9.1 "A bugfix in
  compile-replace in hfst-xfst" (evidence of real, maintained use, not a stub). A general precedent
  (non-HFST-specific) for compile-replace on a real reduplicating language: Singha, Singha & Purkaystha, "A
  Morphological Analyzer for Reduplicated Manipuri Adjectives and Adverbs: Applying Compile-Replace,"
  *IJITCS* 8(2), 2016 ([mecs-press.org](https://www.mecs-press.org/ijitcs/ijitcs-v8-n2/v8n2-4.html)).
- **License**: `COPYING` at repo root of [hfst/hfst](https://github.com/hfst/hfst) is the full GPLv3 text
  (verified directly this session). This contradicts [HFST's own wiki License page](https://github.com/hfst/hfst/wiki/License),
  which claims LGPLv3 for the core — the actual `COPYING` file is authoritative and says plain GPLv3, not
  LGPL. PyPI's `hfst` Python bindings package also classifies itself GPLv3.

### 3.3 hfst-optimized-lookup (the runtime — the piece that would actually ship)

This is deliberately broken out as its own matrix row, separate from the HFST compiler, because its
capability profile *and* its license are both different from the compiler's.

- **Flag diacritics evaluated at lookup time — the single most load-bearing finding in this report,
  verified directly against source this session.** [`libhfst/src/HfstFlagDiacritics.h`](https://github.com/hfst/hfst/blob/master/libhfst/src/HfstFlagDiacritics.h)
  defines `enum FdOperator {Pop, Nop, Rop, Dop, Cop, Uop}` (positive-set / negative-set / require / disallow /
  clear / unify) and a class `FdState` holding a live vector of feature-values with `apply_operation()`
  implementing P/N/R/D/C/U semantics against that running state. The actual traversal-time call site is in
  [`libhfst/src/implementations/optimized-lookup/transducer.cc`](https://github.com/hfst/hfst/blob/master/libhfst/src/implementations/optimized-lookup/transducer.cc),
  method `try_epsilon_transitions`: when the current input symbol is a flag diacritic, the code calls
  `flag_state.apply_operation(...)` and only recurses into `get_analyses(...)` (continues that path) if it
  returns true — **the path is pruned if the flag check fails.** This was independently found by both an
  initial WebSearch (locating the equivalent logic in the standalone CLI tool `tools/src/hfst-optimized-lookup.cc`,
  class `TransducerFd::PushState`) and a dedicated research pass citing the library-internal call site — two
  independent code paths converge on the same conclusion. **This is real runtime flag evaluation, not
  compile-time-only sugar**, and it is exactly the mechanism that keeps morphotactic-constraint combination
  linear instead of exponential.
- **Compile-time flag controls that feed this**: `hfst-lexc -x/--xerox-composition=VALUE`, documented as
  "whether flag diacritics are treated as ordinary symbols in composition (default is true)"
  ([HfstLexc wiki](https://github.com/hfst/hfst/wiki/HfstLexc)) — i.e., flags are deliberately preserved as
  literal symbols through composition specifically so the runtime above can evaluate them live, rather than
  being resolved away at compile time.
- **Multi-analysis**: [HfstLookUp wiki](https://github.com/hfst/hfst/wiki/HfstLookUp) worked example — a
  transducer gives two results for "cactus" (`cacti`, `cactuses`) by default; `-n N`/`--analyses=N` caps the
  count, confirming "all analyses" is the unflagged default.
- **Serialization — a published, independent format spec exists**: Silfverberg & Lindén, "HFST runtime
  format: A compacted transducer format allowing for fast lookup," *FSMNLP 2009*
  ([Semantic Scholar](https://www.semanticscholar.org/paper/HFST-runtime-format:-A-compacted-transducer-format-Silfverberg-Lindén/3b6573352391038c438b3e081c890d79f8584e9a),
  summarized at [OptimizedLookupFormat wiki](https://github.com/hfst/hfst/wiki/OptimizedLookupFormat)):
  **16-bit symbol IDs, confirmed directly from source this session** (not just the wiki paraphrase) — the
  from-scratch Java reader's transition-table reader states "each transition entry is two unsigned shorts, an
  unsigned int and a float" (input symbol, output symbol, target index, weight) and reads them with
  `ByteArray.getUShort()`
  ([`WeightedTransducer.java:153-159`](https://raw.githubusercontent.com/hfst/hfst-optimized-lookup/master/hfst-optimized-lookup-java/src/net/sf/hfst/WeightedTransducer.java),
  header symbol counts likewise via `getUShort()` in
  [`TransducerHeader.java:51-52`](https://raw.githubusercontent.com/hfst/hfst-optimized-lookup/master/hfst-optimized-lookup-java/src/net/sf/hfst/TransducerHeader.java)).
  This gives a firm ~65,536-symbol practical ceiling — comfortably above the ~500 segment + few thousand tag
  symbols PanGloss needs — plus a header/alphabet/index-table/transition-table layout, and a
  **measured throughput of 119,000–408,000 words/second** for the fastest (random-access, Liang-compacted)
  representation, tested on Morphalou French and the Divvun North Sámi lexicon — both far beyond the `<1ms`
  per-word target (1ms/word implies only 1,000 words/sec would be needed).
- **Independent reader implementations, sized** (§7 has the full breakdown): a from-scratch Java reader
  (`hfst-optimized-lookup-java`, 1,500 lines, Apache-2.0, verified this session) including its own
  `FlagDiacriticOperation.java`; a from-scratch Python reader (same repo, Apache-2.0); a modern, actively
  maintained Rust reimplementation, `divvunspell` (Apache-2.0). **No C# reader found.**
- **License — the explicit carve-out**: [HFST's wiki License page](https://github.com/hfst/hfst/wiki/License)
  states verbatim: *"The interfaces for fast lookup from transducers and spell checking are distributed
  separately by the HFST project. They have been coded in the HFST project and contain no external code. All
  rights belong to the University of Helsinki. They are licensed under the Apache license."* Confirmed
  empirically: `hfst-ospell` and `divvunspell` are both Apache-2.0 (GitHub license API); the standalone
  `hfst-optimized-lookup-java`/`-python` readers in [`hfst/hfst-optimized-lookup`](https://github.com/hfst/hfst-optimized-lookup)
  are **also Apache-2.0** — verified directly this session by fetching `COPYING` from that repo (opens
  "Apache License / Version 2.0, January 2004"). **This is HFST's own maintainers choosing to license
  lookup-only code permissively, separate from the GPLv3 compiler core** — the strongest available evidence
  (short of an explicit legal opinion) that a from-scratch `.hfstol` reader, including a C# one built from the
  published format spec rather than by copying GPL source, is not treated as a GPL derivative even by the
  rightsholder.
- **No lazy runtime composition** — see §3.4.

### 3.4 Lazy/on-the-fly runtime composition — cross-toolkit comparison

- **OpenFst: yes, and this is the reference implementation of the concept.** Direct doxygen source-comment
  quote from `fst/compose.h`: *"Computes the composition of two transducers. This version is a delayed FST...
  ComposeFst does not trim its output (since it is a delayed operation)"* — complexity given as
  O(v1·v2·d1·(log d2 + m2)) where v1/v2 are *states visited*, not total states, i.e. the product is computed
  on demand. [compose_8h_source.html](https://www.openfst.org/doxygen/fst/html/compose_8h_source.html),
  [classfst_1_1ComposeFst.html](https://openfst.org/doxygen/fst/html/classfst_1_1ComposeFst.html).
- **rustfst: yes, a direct architectural analogue.** A dedicated `lazy` module ("Module providing the
  necessary functions to implement a new Delayed Fst") exposes `LazyFst`, `LazyFst2`,
  `FstOp`/`FstOp2` traits; the `compose` module exposes `ComposeFst`/`ComposeFstOp`/`ComposeFstOpOptions`
  directly analogous to OpenFst's. [docs.rs/rustfst/.../algorithms/lazy/](https://docs.rs/rustfst/latest/rustfst/algorithms/lazy/index.html),
  [docs.rs/rustfst/.../algorithms/compose/](https://docs.rs/rustfst/latest/rustfst/algorithms/compose/index.html)
  (independently reconfirmed by direct WebFetch this session).
- **HFST: no evidence of lazy runtime composition anywhere in the shipped pipeline.** `hfst-compose-intersect`
  is explicitly an ahead-of-time tool ("Compose a lexicon with one or more rule transducers," with a `--fast`
  *compile-time* memory/speed tradeoff flag); the standard pipeline compiles lexicon and rules separately,
  merges them once via `hfst-compose-intersect`, converts the single merged result to `.hfstol`, and
  `hfst-optimized-lookup` traverses that one pre-composed network. No lazy per-word lexicon∘rules composition
  is exposed through any HFST runtime tool or the `HfstTransducer` C++ API. **If the architecture ever needs
  "lexicon∘rules product too big, compose lazily per word at runtime," HFST is not the toolkit that provides
  it — that capability lives in OpenFst/rustfst, not the HFST family.**
- **foma, lttoolbox, SFST, kfst, pyfoma**: no lazy composition found in any of them; several (foma, pyfoma,
  lttoolbox) are explicitly eager-only by design (determinize/minimize/trim after every op, or ahead-of-time
  `lt-comp`/`lt-proc`/`lt-compose` split).

### 3.5 xfst/lexc (Xerox) — historical reference only

- Full rewrite-rule calculus, flag diacritics, and compile-replace **all originate here** — Beesley &
  Karttunen, *Finite State Morphology* (CSLI, 2003); Beesley & Karttunen, "Finite-State Non-Concatenative
  Morphotactics," *SIGPHON-2000* ([arXiv:cs/0006044](https://arxiv.org/abs/cs/0006044)) — the compile-replace
  paper itself, demonstrated on **Malay full-stem reduplication** (`bagi` → `bagibagi` via `^[{bagi}^2^]`) and
  **Arabic stem interdigitation** (a `.m>./.<m.` merge operator for templatic morphology), with concrete
  figures: a Malay prototype of ~1,000 roots/1,500 entries, and Arabic compile time reduced "from hours to
  minutes" for ~90,000 stems.
- Multi-analysis confirmed as the 2000-era default: applying upward to French *suis* "produces the four
  related lexical strings... such ambiguity of surface strings is very common" (same paper, Fig. 5).
- Scale precedent: Cohen-Sygal & Wintner, "XFST2FSA" (ACL 2005, [aclanthology.org/W05-1108.pdf](https://aclanthology.org/W05-1108.pdf))
  report the HAMSAH Hebrew analyzer at **~2 million states / 2.2 million arcs** total (adjectives subnetwork
  ~100k states/120k arcs; nouns ~700k states/950k arcs), compiled in 27m41s using ~3GB memory, running at 70
  words/sec — and note XFST "prints its networks only in text format... for small networks only," i.e. the
  binary format was not itself publicly documented in detail.
- License: proprietary, never open-sourced. Cohen-Sygal & Wintner (2005): "XFST is proprietary." Only known
  distribution was a CD bundled with the 2003 Beesley & Karttunen book; a 2018 university course page directs
  students to foma/hfst-xfst instead of xfst — consistent with effective discontinuation. No `_eq()`
  equivalent was found documented for xfst itself (that appears to be foma's own later addition).

### 3.6 OpenFst core + Thrax/Pynini

- OpenFst core has **no rule compiler** — bare automaton algebra (compose/union/concat/determinize/minimize
  etc.) via `fst/*.h`. That gap is exactly what Thrax and Pynini exist to fill.
- **Thrax**: "a toolkit for compiling grammars based on regular expressions and context-dependent rewrite
  rules into weighted finite-state transducers... `CDRewrite` rewrites portions of the input in a
  context-dependent fashion," compiling `.grm` sources into `.far` archives —
  [OpenGrm Thrax Quick Tour](https://www.openfst.org/twiki/bin/view/GRM/ThraxQuickTour).
- **Pynini**: `cdrewrite(transducer("A","B"), "C", "D", sigma_star)` implements A→B / C__D, with explicit
  `LEFT_TO_RIGHT`/`RIGHT_TO_LEFT`/`SIMULTANEOUS` direction and `OBLIGATORY`/`OPTIONAL` mode parameters —
  [pynini cdrewrite.h](https://github.com/mjansche/pynini/blob/master/src/cdrewrite.h),
  [OpenGrm Pynini wiki](https://www.opengrm.org/twiki/bin/view/GRM/Pynini) (confirmed directly this session).
  License: Apache-2.0 ([raw LICENSE](https://raw.githubusercontent.com/kylebgorman/pynini/master/LICENSE),
  confirmed).
- License, OpenFst core: Apache-2.0, confirmed directly
  ([raw LICENSE](https://raw.githubusercontent.com/google-research/openfst/master/LICENSE)).

### 3.7 rustfst

Already the subject of report 02's build/size/WASM analysis; this report adds the rule-calculus/flag/lazy
angle. Confirmed absent: rewrite-rule compiler, flag diacritics (neither found anywhere in
[docs.rs/rustfst](https://docs.rs/rustfst/latest/rustfst/algorithms/index.html) or its issue tracker).
Confirmed present: lazy composition (§3.4) and a path-iteration/`shortest_path`/N-best distinction for
multi-analysis vs. 1-best. License MIT OR Apache-2.0 ([crates.io/crates/rustfst](https://crates.io/crates/rustfst)).

### 3.8 lttoolbox (Apertium)

- **No rewrite-rule calculus** — Apertium's own GSoC proposal states plainly: "There is no facility for rule
  based transformations in lttoolbox... Currently lttoolbox doesn't support any kind of rule based character
  alterations" ([wiki.apertium.org/wiki/User:Techievena/Proposal](https://wiki.apertium.org/wiki/User:Techievena/Proposal)).
  A "Twol rules in lttoolbox" design page has sat at status "In Progress" and never shipped
  ([wiki](https://wiki.apertium.org/wiki/Twol_rules_in_lttoolbox)). lttoolbox is fundamentally a lexc-like
  paradigm/dictionary compiler: `.dix` XML → `lt-comp` → "augmented letter transducers"
  ([lt-comp manpage](https://manpages.debian.org/experimental/lttoolbox-dev/lt-comp.1.en.html)). Apertium's
  separate transfer-rule stage (`apertium-transfer`) is a different, XML-based structural-transfer mechanism,
  not an FST rewrite calculus.
- **No flag-diacritic equivalent in production** — an experimental Java-only feature from 2010 was "never
  brought into use" ([wiki](https://wiki.apertium.org/wiki/Lttoolbox-java/Flag_diacritics)).
- **No native reduplication** — the companion tool `lexd` supports repeated lexicon references for
  reduplication patterns, compiling to AT&T format fed into `lt-comp`/`lt-proc`, but this is not native
  lttoolbox ([lexd Usage.md](https://github.com/apertium/lexd/blob/main/Usage.md)).
- **Scale**: apertium-eng — 59,533 stems, 391 paradigms, 4,245,652-byte `.dix` source
  ([apertium-en/stats](https://wiki.apertium.org/wiki/Apertium-en/stats)); 1,000,000 words processed by
  `lt-proc` in 17.6s with `en-ca.automorf.bin` (~58,823 words/sec) — [Lttoolbox wiki](https://wiki.apertium.org/wiki/Lttoolbox).
- **Lazy composition**: none — `lt-comp` compiles offline, `lt-proc` looks up only, `lt-compose` is a
  separate *offline* combining utility, not a runtime lazy-compose feature.
- **Multi-analysis**: `lt-proc -a` returns all ambiguous readings by default
  ([Ambiguity wiki](https://wiki.apertium.org/wiki/Ambiguity); [lt-proc manpage](https://manpages.ubuntu.com/manpages/focal/en/man1/lt-proc.1.html)
  — `-N`/`-L` exist to *restrict* output, confirming unrestricted-all is the default).
- **Serialization**: no public byte-level format spec found; a binary-compatible Java port,
  [lttoolbox-java](https://github.com/apertium/lttoolbox-java), exists and "generates exactly the same
  output" as the C++ tool — another cross-language portability precedent, though no C#/JS port was found.
- **License**: GPLv2, confirmed directly this session (`COPYING`, "Version 2, June 1991").

### 3.9 SFST and kfst (minor/dormant, brief)

- **SFST**: full 4-variant replace calculus (`^->`,`_->`,`/->`,`\->`) modeled on Karttunen 1995, per the
  primary [SFST Manual](https://www.cis.lmu.de/~schmid/tools/SFST/data/SFST-Manual.pdf) — but **no flag
  diacritics**; HFST's own docs note "agreement variables must be used instead of flag diacritics" when
  compiling SFST-format files ([HFST symbols wiki](https://github.com/hfst/python/wiki/Symbols)). Effectively
  dormant (last substantive engine work ~2015; a 2026-dated fork shows only build-system churn, not
  capability changes). License GPLv2-or-later.
- **kfst** (`fergusq/fst-python` — the `kfst` GitHub org itself has no public repos, so "kfst" in practice
  means this project): pure-Python runtime library with **no rule compiler of its own**, but **does**
  implement the full Xerox/HFST-style flag-diacritic syntax and P/R/D/C runtime semantics directly in source
  (`kfst/kfst_py/symbols.py`'s `FlagDiacriticSymbol`, `transducer.py` lines ~344–390) — modestly active (last
  push 2026-04-23) but no formal releases. License LGPL-3.0.

### 3.10 pyfoma

Independent pure-Python reimplementation (explicitly "inherits none of the code in foma"), re-implementing
Hulden's own rewrite-rule algorithms via a function-style syntax: `$^rewrite(a:b / c _ d, leftmost=True,
longest=True, shortest=True)`, including weighted rewrites — Table 2 of the [ACL 2024 System Demos
paper](https://aclanthology.org/2024.acl-demos.24.pdf). Flag-diacritic-style "feature calculus"
(`[[$X=y]]`/`[[$X?=y]]`/etc.) explicitly modeled on xfst flags, same paper §3.5. `analyze()` returns a
generator of all analyses (confirmed all-paths). No reduplication mechanism, no documented alphabet-scale
limit, no lazy composition (eager determinize/minimize/coaccessible-trim always), no dedicated binary
serialization format found. License Apache-2.0, confirmed ([github.com/mhulden/pyfoma](https://github.com/mhulden/pyfoma)).

---

## 4. Precedent morphologies — the empirical answer to "does it explode on real complexity"

| Language | Toolkit | Constructs exercised | Size | Speed | Citation |
|---|---|---|---|---|---|
| **Finnish** (OMorFi) | HFST (`hfst-lexc`/`hfst-twolc`) | Large-scale concatenative agglutination (inflection/derivation/compounding); not templatic, not reduplicative | 567,540 lexemes; 555/542/231/145 paradigm classes (proper nouns/nouns/verbs/adjectives); compiled automata up to **555,144 states / 1,238,250 arcs / 29MB** (`generate`), **540,768 states/1,249,753 arcs/30MB** (`describe`) | Not published | Pirinen, "Omorfi—Free and open source morphological lexical database for Finnish," *NODALIDA 2015* ([ACL Anthology](https://aclanthology.org/W15-1844.pdf)); [project stats page](http://flammie.github.io/omorfi/statistics.html) |
| **Arabic** (AraComLex) | foma/xfst-compilable, HFST-compatible | **Templatic (root-and-pattern), non-concatenative** — the key precedent for "does classical FST handle non-concatenative morphology" | 30,587 lemmas → **12,951,042 generated word forms**; the paper's "340MB flat file, 11MB compressed" figures describe an *enumerated word-form list*, not the compiled transducer — **compiled-transducer size is not published**, do not read 11MB as an artifact-size precedent | **~5,000 words/sec** analysis | Attia, Pecina, Tounsi, Toral & van Genabith, "A corpus-based finite-state morphological toolkit for contemporary Arabic," *J. Logic and Computation* 24(2), 2014 (abstract-level figures; full-text paywalled, treat exact numbers as moderately- not primary-verified) |
| **Hebrew** (HAMSAH) | xfst | Concatenative + templatic mix, very deep morphotactics | **~2,000,000 states / 2,200,000 arcs** total (adjectives ~100k/120k; nouns ~700k/950k) | 70 words/sec; 27m41s compile, ~3GB memory | Cohen-Sygal & Wintner, "XFST2FSA," *ACL 2005* ([aclanthology.org/W05-1108.pdf](https://aclanthology.org/W05-1108.pdf)) |
| **Amharic/Tigrinya/Oromo** (HornMorpho, Gasser) | **Not a classical FST** — weighted FST where weights are accumulated feature structures | Templatic Semitic verb morphology + long-distance Tigrinya morphotactic dependencies | Not published in states/arcs terms (different formalism, not directly comparable) | Not published | Gasser, "Semitic Morphological Analysis and Generation Using Finite State Transducers with Feature Structures," *EACL 2009* ([aclanthology.org/E09-1036](https://aclanthology.org/E09-1036/)); confirmed by [HornMorpho README](https://github.com/hltdi/HornMorpho): "implemented in the form of finite-state transducers weighted with feature structures" |
| **North/Skolt Sámi** (Giella/GiellaLT) | HFST (`hfst-lexc`/`hfst-twolc`) | Deep concatenative + consonant gradation (a two-level phenomenon); **no reduplication/templatic mechanism** | Skolt Sámi: 30,000+ lemmas, 2.3M+ inflectional forms, 148 inflectional paradigms, nominal stem types 56→308, verbal stems 30→115 | Not published | Rueter & Hämäläinen, "FST Morphology for the Endangered Skolt Sami Language," *SLTU-CCURL 2020* ([arXiv:2004.04803](https://arxiv.org/abs/2004.04803)); Trosterud & Uibo (Koskenniemi Festschrift, 2005) explicitly define states/arcs as a comparison metric for Sámi/Estonian gradation (numeric table not extracted this session) |
| **Zulu, Xhosa** (ZulMorph et al.) | xfst, also foma-compilable | Bantu noun-class agglutination + agreement; **reduplication explicitly confirmed as productive** | Not published | Not published | Pretorius & Bosch, "Finite-State Computational Morphology: An Analyzer Prototype for Zulu," *Machine Translation* 18(3), 2004; Pretorius & Bosch, "Exploiting cross-linguistic similarities in Zulu and Xhosa computational morphology," *AfLaT 2009* ([aclanthology.org/W09-0714.pdf](https://aclanthology.org/W09-0714.pdf)) — explicitly states "reduplication occurs productively in both languages," handled via xfst transducers requiring "careful rule formulation... to avoid excessive computational complexity" |
| **Swahili** (SALAMA / XSMA) | Originally two-level FST, later a mixed pattern-matching + post-processing pipeline | Bantu noun-class agglutination | Not published | Not published | [Technical Reports in Language Technology, Report No. 9, 2010](https://tuhat.helsinki.fi/ws/portalfiles/portal/282613691/language_learning3.pdf); note the production system moved *away* from pure two-level FST for accuracy reasons — a soft data point that pure classical FST alone was found insufficient for this language in practice |
| **Malay** | Xerox xfst compile-replace | Full-stem reduplication (`bagi`→`bagibagi`) — the founding example of compile-replace | ~1,000 roots / 1,500 entries (prototype scale) | Not published | Beesley & Karttunen, "Finite-State Non-Concatenative Morphotactics," *SIGPHON-2000* ([arXiv:cs/0006044](https://arxiv.org/abs/cs/0006044)) |
| **Manipuri** | XFST+LEXC compile-replace | Restricted and complete reduplication of adjectives/adverbs — a real, non-toy production case | Not published | Not published | Singha, Singha & Purkaystha, *IJITCS* 8(2), 2016 ([mecs-press.org](https://www.mecs-press.org/ijitcs/ijitcs-v8-n2/v8n2-4.html)) |
| **English** (Apertium apertium-eng) | lttoolbox | Concatenative only | 59,533 stems, 391 paradigms, 4.2MB `.dix` source | **~58,823 words/sec** (`lt-proc`, 1M words in 17.6s) | [Apertium-en/stats wiki](https://wiki.apertium.org/wiki/Apertium-en/stats); [Lttoolbox wiki](https://wiki.apertium.org/wiki/Lttoolbox) |
| **French (Morphalou) + North Sámi (Divvun lexicon)** | HFST runtime format (random-access, Liang-compacted) | Benchmark of the lookup format itself, not a specific language's full morphology | N/A (format benchmark) | **119,000–408,000 words/sec** | Silfverberg & Lindén, "HFST runtime format: A compacted transducer format allowing for fast lookup," *FSMNLP 2009* |

**Reading across this table**: large concatenative morphologies (Finnish, Sámi) are unambiguously solved,
with published state/arc/size numbers in the hundreds-of-thousands to low-millions and file sizes in the
tens-of-MB range for the *whole* generation/description network (a subset scoped to what PanGloss would
actually ship — one target language's grammar, not a multi-million-word generator — would be dramatically
smaller). Templatic (Arabic) and productively-reduplicative (Zulu/Xhosa, Malay, Manipuri) morphologies are
*also* solved, but every citable case needed the xfst-family's compile-replace/flag-diacritic machinery
specifically — the toolkits that lack it (bare OpenFst, rustfst, lttoolbox) have no citable large templatic or
reduplicative precedent at all. The one case that pushed hardest on combined templatic + long-distance
constraints (Amharic/Tigrinya) is the one case that **left the classical-FST formalism** for a
feature-structure-weighted extension — worth treating as a ceiling-of-applicability data point, not just
another success story, if PanGloss's own grammars ever need comparably deep non-concatenative machinery (they
likely will for at least some target Bantu/Semitic-adjacent languages, given the sample grammars already in
this repo include Sena and Amharic — the same `docs/fst-plan/HYBRID_FST_FEASIBILITY.md` cited in report 01).

---

## 5. Gaps no toolkit covers

1. **Feature-structure unification with capture groups (`hc-fst`'s actual primitive) is out of scope for this
   report by design** — report 02 already establishes that no classical-alphabet toolkit here has it. This
   report's whole premise (offline-compile to a *classical* FST) requires that gap to be closed by a separate,
   unaddressed step: resolving HermitCrab's partial feature structures into a discrete symbol alphabet before
   any of the toolkits above can even begin. That resolution step's cost/feasibility is not sized here.
2. **Reduplication is a compile-time/artifact-size tradeoff, not a runtime capability gap.** Once
   compile-replace (or `_eq()`) has run, the shipped runtime needs no special-case reduplication code — it
   just traverses a (larger) finite automaton, and every toolkit's runtime here (`hfst-optimized-lookup`,
   foma's `flookup`, lttoolbox's `lt-proc`) does that traversal identically whether or not reduplication was
   involved in producing the automaton. The actual risk is that **unbounded/full-copy reduplication is not a
   regular relation** — compile-replace and `_eq()` both only support a *finite, bounded* number of
   reduplication copies (baked in at compile time), and baking in more copies/more reduplication patterns
   inflates the compiled artifact's state/arc count, which is a real tension against the `<10MB` shipped
   budget (§4's Finnish/Sámi/Hebrew examples show whole-generation-network sizes already reaching tens of MB
   at full lexicon scale — a scoped single-language artifact would need to stay well below that, or be
   pruned/compressed aggressively).
3. **Patent status of compile-replace is unresolved.** A matching-titled Xerox US patent, 7,010,476 ("Method
   and apparatus for constructing finite-state networks modeling non-concatenative processes"), was located,
   but its full claims text is only available as an OCR-blocked scanned PDF from USPTO and could not be read
   this session. **[UNVERIFIED]** whether it covers HFST's/foma's specific implementations, and whether it is
   still in force (if filed in Xerox's ~2000–2003 FST-patent-filing era, it would likely already be expired
   under the standard 20-year utility term by 2026, but this is inference from the era, not the actual filing
   date on the document). Given the task's premise that the *offline* compiler can be any license/any
   provenance, and the *artifact* it produces is just data, this is a lower-priority risk than the license
   questions in §1 — but it should be resolved with a lawyer or a readable copy of the patent before actually
   depending on `hfst-xfst compile-replace` or Xerox-derived reduplication techniques in a shipped product's
   toolchain, out of an abundance of caution.
4. **No toolkit's runtime does true per-word lazy lexicon∘rules composition except OpenFst/rustfst, and
   neither of those has a rule calculus** (§3.4, §3.6) — the toolkit with the rule calculus and the toolkit
   with lazy composition are not the same toolkit. If eager pre-composition of lexicon+rules turns out to
   produce too large an artifact for the `<10MB` budget, closing that gap means either (a) accepting HFST's
   ahead-of-time-only model and controlling size via a **smaller, per-language** artifact (not a
   multi-million-word generator, unlike several §4 precedents), or (b) hand-building an OpenFst/rustfst-style
   lazy composition layer on top of a rule-less discrete-alphabet automaton — a nontrivial, unaddressed
   engineering task, not a reuse-an-existing-tool task.
5. **No production-grade C# runtime for any of these formats exists anywhere, in any state of maturity.**
   Verified: no NuGet package, no GitHub C# port, and SIL's own `SIL.Machine` codebase (checked directly, this
   worktree's `machine/` submodule) has no HFST/OpenFst/foma bindings and no flag-diacritic-evaluating
   transducer — its `FiniteState/Fst.cs` is the same unification-matcher family `hc-fst` was ported from
   (report 02), architecturally unrelated to a classical `.hfstol`/`.fst` reader. The nearest sizeable,
   permissively-licensed precedents to port from are `hfst-optimized-lookup-java` (1,500 lines, Apache-2.0,
   but self-flagged by its own maintainers as stale against the current binary format —
   [HfstOptimizedLookupJava wiki](https://github.com/hfst/hfst/wiki/HfstOptimizedLookupJava): "not current...
   there have been bugfixes and changes to the binary transducer format not reflected") and `divvunspell`
   (Rust, Apache-2.0, actively maintained, larger and more current, ~484KB of Rust source across the core
   plus per-platform binding surfaces including TypeScript). **A C# port is a bounded, scoped engineering
   task with two concrete size precedents to estimate against — but it does not exist today, and would need
   to be validated against the *current* `.hfstol` format (the Java reader's staleness warning is a real
   caution, not just a formality) rather than the format as of whichever HFST version the Java reader last
   tracked.**

---

## 6. What this means for the offline-compile + C# runtime architecture, briefly

The capability profile that best matches the stated constraints is: **compile offline with HFST's
`hfst-xfst`/`hfst-twolc`/`hfst-lexc` (GPLv3, offline-only — acceptable per the task's own premise) →
`hfst-compose-intersect` to merge lexicon+rules ahead of time → `hfst-fst2fst -O` to produce a `.hfstol`
artifact → ship that artifact (data, not GPL-encumbered) plus a from-scratch C# reader built against the
published Lindén/Silfverberg/Pirinen (FSMNLP 2009) format spec and licensed however PanGloss likes (Apache-2.0
precedent already exists for exactly this kind of from-scratch reader, both in Java and Rust).** This gets:
a real replace-rule calculus with directional/optional/obligatory modes and compile-replace for reduplication
(§3.2); flags genuinely evaluated at lookup time, which is the mechanism that keeps the compiled artifact from
needing an exponential morphotactic product (§3.3); an alphabet ceiling (~65,536 16-bit symbol IDs) far above
PanGloss's stated ~500-segment-plus-thousands-of-tags need; measured lookup throughput two to three orders of
magnitude faster than the `<1ms`/word target (§3.3, §4); and all-analyses-by-default lookup semantics matching
what a morphological analyzer needs (§3, per-toolkit). The two real open risks are **artifact size at full
lexicon scale** (§5.2 — needs a per-language-scoped build, not a Finnish/Arabic-scale multi-million-word
generator, to comfortably fit `<10MB`) and **the absence of any existing C# reader** (§5.5 — a real but bounded
build task, not a research gap, given two sub-2,000-line-class precedents to work from).
