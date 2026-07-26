# System profile: Divvun / Giellatekno (divvunspell + hfst-ospell + Constraint Grammar)

Profiled against the fixed rubric for the PanGloss spell-checking comparison table. Divvun is
PanGloss's closest peer: FST-based morphology for minority/indigenous languages (chiefly the Sámi
languages), open source, with a production spelling **and** grammar-checking pipeline shipped to
real hosts. This report builds on `00-synthesis.md` and reports 02-05, which already established:
the ERRORSOURCE⊗LEXICON composition precedent (01/00), the CG-over-analysis detection architecture
and `divvun-cgspell` unknown-word path (04, 05), and the open question about the
normalization-vs-correction boundary (00, flagged unconfirmed by 02/05 — still unconfirmed here,
see DETECTION). It does not repeat those findings; it adds source-verified detail (licenses,
package/file formats, exact composition formula, integration surface, footprint/WASM evidence) and
fills the fields those reports didn't need.

**Labeling convention:** `[M]` = measured from primary source (repo contents, docs I fetched and
read myself), `[A]` = asserted by a primary source (a claim in a README/paper/site I read but
couldn't independently verify), `[S]` = my synthesis/inference across sources, `[UNFETCHED]` =
source exists but I could not extract usable content from it this pass.

Primary sources actually fetched this pass: `github.com/divvun/divvunspell` (README, root
`Cargo.toml`, `ffi/README.md`, `support/accuracy-viewer/Cargo.toml`, directory listings) `[M]`;
`github.com/hfst/hfst-ospell` README `[M]`; `github.com/divvun/libdivvun` README `[M]`;
`github.com/hfst/hfst` repo metadata `[M]`; `divvun.no` `[M]`; `divvun.no/future/proofing/lexfile-spec.html`
(ZHFST spec) `[M]`; `giellalt.uit.no/proof/spelling/hfst/HowToWriteSpellerCorrectionsForHfst.html`
`[M]`; GitHub repo metadata for `vislcg3` (via search, not `gh api` — org is not `divvun`) `[M]`;
Divvun/Giellatekno host listings (LibreOffice extension, MS/Google Docs add-in, Divvun Manager,
Divvun Keyboards app, WoodWing plugin) via GitHub repo search `[M]`. **Unfetchable this pass:**
`aclanthology.org/2023.nodalida-cgmta.pdf` (South Sámi grammar-checker eval paper — PDF downloaded
but not text-extractable in this environment, no `pdftoppm`/poppler installed) `[UNFETCHED]`;
Pirinen & Lindén 2010 SaLTMiL paper full text (found via search, abstract-level only) `[UNFETCHED]`;
GiellaLT syntactic-tag/valency documentation pages beyond the search-engine summary `[UNFETCHED]`.

---

## ARCH

`[M]` **Weighted acceptor ∘ weighted error-model FST, composed at query time; Constraint Grammar
runs as a separate stage above the analyzer, not inside the FST composition.** Verbatim from the
`hfst-ospell` README: "Pass (weighted!) Transducer pointers to the Speller constructor... Run a
composition of ERRORSOURCE and LEXICON on standard input and print corrected output." `divvunspell`
(Rust) is an explicit "reimplementation and extension of hfst-ospell" `[A, divvunspell README]`
adding parallel suggestion generation, memory-mapped transducers, tokenization, and case handling.
CG (`libdivvun`, C++, wraps VISL `vislcg3`) sits **downstream** of the analyzer as a distinct
pipeline stage (`tokenisation/morphology | multiword handling | disambiguation | error rules |
generation`, per report 05's reading of `libdivvun`) — it is not fused into the FST search the way
the error model is. So the full stack is two architecturally different mechanisms stacked, not one
unified weighted search: (1) FST-level weighted composition for surface-form
spelling-correction, (2) CG-level rule disambiguation for morphosyntactic/real-word error
detection. This confirms and sharpens 00's "unified weighted composition" finding — that unification
is real but scoped to the speller half only; the grammar-checker half is a separate rule engine.

## LEXICON

`[M]` The lexicon is a **compiled finite-state acceptor** (`acceptor.default.hfst` /
`acceptor.DESCR.hfst` inside the ZHFST/BHFST package), built from the same source lexc/twolc/xfst
grammar files as the language's morphological analyzer — report 05 already cited the giellalt
`lang-sme` repo showing analyzer and speller share source files. The acceptor can be either a
**plain single-level string acceptor** (bare spelling-correct/incorrect) or a **full two-level
analyzing transducer that also returns morphological analyses** `[M, lexfile-spec.html: "Acceptors
can be single-level (default) or two-level analyzing transducers"]` — confirmed directly in the
`divvunspell` CLI's `-A`/analyze mode, which "requires that the spell checker's acceptor (lexicon)
is a full morphological analyzer" `[M, divvunspell README]`. So the lexicon model is exactly
PanGloss's: a compiled generative grammar (stems × morphotactics), not a flat wordlist — this is
the strongest point of architectural overlap between the two systems.

## MORPHOLOGY

`[M/S]` Yes — full generative FST morphology, unbounded inflection without enumeration, same as
PanGloss's HermitCrab/foma path. The acceptor is compiled from the language's full lexc
morphotactics + twolc/xfst phonology (giellalt `lang-*` repos), so agglutinating/inflection-heavy
Sámi word forms are accepted by traversal, not lookup against a stored list. The divergence from
PanGloss is not "does morphology generate" (both do) but **what else rides on that morphology**:
PanGloss's HermitCrab source also carries POS + full inflectional feature bundles + (in LibLCM)
semantic domains as first-class exportable data (per CONTEXT.md, `docs/grammar-json-export-plan.md`),
whereas Divvun's lexc-only grammars encode POS/inflection class in the FST tags but have no
LibLCM-equivalent structured feature-structure layer sitting above the FST — the tags **are** the
data model, not a projection of a richer external one. `[S]`

## ERRORMODEL

`[M]` A second weighted FST, `errmodel.DESCR.hfst` (reserved default name
`errmodel.default.hfst`), composed with the lexicon acceptor at correction time — exactly
`ERRORSOURCE ⊗ LEXICON` per the `hfst-ospell` usage docs. Concretely, per search results from
`giellalt.uit.no`'s HFST speller-corrections how-to `[M]`: **"The error model itself is... a
traditional Levenshtein edit-distance model, where the distance can be specified (default is 1),
and one can also add swaps. The error model produced by the makefile is using an edit distance of
2, including swaps."** This is machine-generated by an HFST-project Python script (not
hand-composed per language) and is then hand-tunable via two mechanisms in
`default-error-model.txt`: (1) symbol exclusions (`~`-marked) to shrink the model, and (2)
**hand-authored weighted replacement pairs** in an `@@`-prefixed section, format
`input<TAB>replacement<TAB>weight`, e.g. `a	á	0.5` — i.e. a human encodes phonetic/orthographic
confusability (accented-vowel confusion) as an explicit weighted rule, not a derived feature
distance. This is architecturally **less principled than PanGloss's proposed grammar-derived
natural-class cost matrix** (report 02): Divvun's phonetic/diacritic weighting is a short,
hand-authored table per language, not derived from the grammar's own phoneme inventory or feature
system. No keyboard-geometry error model was found in any primary source fetched this pass — the
error model is pure edit-distance-with-swaps plus a short hand-tuned confusion list; no evidence of
Keyman/keyboard-layout-derived weighting. `[M for the base model + hand-tuning mechanism; S for the
"less principled than PanGloss's design" comparison]`

**Scoring formula** — `[M]`, directly from the `divvunspell` README's `--verbose` accuracy-viewer
output: **`total = lexicon_weight + mutator_weight + reweighting`**, where `lex` = the acceptor's
own weight for that analysis (frequency/tag weights, from a `tags.reweight` file), `mut` = the
error-model ("mutator") weight for the edit applied, and `rew` = a **positional** penalty
(configurable `start-penalty`/`mid-penalty`/`end-penalty`, defaults 10.0/5.0/10.0) added on top —
edits near word edges are penalized more than mid-word edits. `divvunspell` additionally supports
n-best truncation, a `max-weight` cutoff, and an optional **beam search** (max weight-distance
between best and worst kept suggestion) — a genuine algorithmic extension over `hfst-ospell`, which
has no beam parameter in its own README-documented API.

## DETECTION

`[M, corroborating 04/05]` Detection and correction are architecturally separate, confirmed at the
tool level, not just the pipeline-diagram level: `libdivvun` ships **`divvun-checker`** (grammar/error
detection over a full pipeline XML spec — runs CG error rules), **`divvun-suggest`** (turns CG
error-tag readings into human-readable correction messages, "meant to be used as a late stage"),
and **`divvun-cgspell`** (spells *unknown* word forms specifically, adding them as new CG readings)
as three distinct executables/modules, not three modes of one binary. Real-word errors (a valid
word used wrongly — agreement/case mistakes) are exactly what the CG error-rule layer inside
`divvun-checker`'s pipeline targets, per report 05's finding that CG rules run "after morphological
analysis and disambiguation" and can mark "readings explicitly marked for suggestion," with
`co&`-prefixed co-error tags preventing contradictory simultaneous fixes. **The
normalization-vs-correction boundary remains unconfirmed from a primary source** — I looked
specifically at the ZHFST spec and the speller-corrections how-to page and neither addresses it;
this open question from `00-synthesis.md` stands. `[UNFETCHED for that specific sub-question]`

## CONTEXT

`[M]` Constraint Grammar (`vislcg3`, GPL-3.0-or-later, C++, implements Pasi Tapanainen's CG-2
formalism, backward-compatible with CG-2/VISLCG) is the sentence-context mechanism, wrapped by
`libdivvun`'s pipeline. Rules are organized per-language in `lang-xxx/src/cg3/*.cg3` files,
including a dedicated **`disambiguation.cg3`** (resolves morphological homonymy using sentence
context — the classic CG task) and a separate **`valency.cg3`** (subcategorization-frame tagging,
e.g. transitive/intransitive marking) `[M, search result summarizing giellalt.github.io grammar
tag docs]`, plus the grammar-*checking* rules proper (agreement/case/error rules) documented at
`giellalt.uit.no/tools/docu-vislcg3.html`. This is genuinely hand-written, per-language linguistic
engineering, not learned or statistical — matching 04/05's characterization that CG "does not
degrade as corpus size shrinks."

## SEMANTICS_POS

`[M/S]` **Confirmed precisely: CG operates over morphosyntactic tags (POS + inflectional
features) plus shallow lexical-semantic/valency tags (subcategorization frames — transitive vs.
intransitive, case government) authored directly in the lexc grammar and CG rule set** — the
existence of a dedicated `valency.cg3` file `[M]` is direct confirmation that valency (a
semantic-adjacent, argument-structure property) is in scope for CG, not just morphosyntax. There is
**no evidence in any source fetched this pass of a broader selectional-restriction or semantic-domain
apparatus** — nothing resembling FLEx's ~1,800-category semantic-domain list (report 04, §4) appears
anywhere in the Divvun/GiellaLT toolchain description, the ZHFST spec, or the CG documentation
summaries found. The tag inventory is: POS, inflectional features, and per-lexeme
valency/subcategorization — a materially narrower semantic layer than PanGloss's LibLCM data model,
which carries semantic domains as a distinct FLEx field independent of valency. `[S: the comparison
claim; M: the absence-of-evidence for semantic domains specifically, within what was fetched]`

## DATA_REQ

`[M/S]` Minimum viable data to stand up a new-language Divvun-style speller, reconstructed from
primary sources:
1. **A full generative FST morphology** (lexc stems + morphotactics + twolc/xfst phonology) — this
   is the same authoring burden as any lexc-based analyzer project; no shortcut documented.
2. **An error model** — this part is the cheap one: `[M]` "We can get a fairly decent error model
   by using a tool (a python script) made in the HFST project" — an off-the-shelf Levenshtein+swap
   model is generated automatically from the acceptor's own alphabet with **zero hand-authoring
   required to get a working baseline**; hand-tuning (the `@@` weighted-pairs section) is
   optional polish, not a gate to shipping.
3. **CG rules are needed only for the grammar-checker / real-word-error tier, not for the basic
   speller.** A ZHFST/BHFST package needs only an acceptor + optional error model to function as a
   spelling suggester (`divvunspell`/`hfst-ospell` alone) — CG (`libdivvun`/`vislcg3`) is a
   separate, additional investment layered on top for grammar-checking and is a materially bigger
   authoring burden (hand-written disambiguation + error rules, unquantified in any source I could
   read — the one paper that might quantify rule count for a real deployment,
   the South Sámi CG-MTA 2023 paper, was `[UNFETCHED]`).
4. **Net**: getting a basic Divvun-style *speller* running for a new language needs "only" a
   complete FST morphology — the error model is close to free (auto-generated). The CG
   *grammar-checker* is the expensive, hand-authored tier, and is optional/additive, not a
   prerequisite for spelling suggestions. This is a materially lower floor than the prompt's framing
   ("need a full FST morphology + hand-written CG rules + error-model weights") suggests for the
   *speller* alone — CG is real but decoupled, and error-model weights are auto-derived by default.
   `[S, synthesizing 1-3]`

## PERSONALIZATION

`[UNFETCHED, corroborating 06]` No primary source found describing a Divvun/Giellatekno
personal-dictionary, on-device adaptation, or incremental-learning mechanism — report 06 already
flagged this exact gap ("searched for specifically, not found... Divvun's runtime behavior being
under-documented relative to its architecture papers"). I searched again this pass (ZHFST spec,
`hfst-ospell`/`divvunspell` READMEs, `divvun.no`) and found nothing addressing personal wordlists or
user-adapted weights; the `divvunspell` CLI/library API surface documented in its README has no
visible "add word"/"personal overlay" concept — suggestions come only from `SpellerArchive` +
`SpellerConfig` against the shipped ZHFST/BHFST. Absence of evidence, not confirmed absence of
feature — flagged, not asserted.

## INTEGRATION

`[M]` Broad, real, shipped host integration — this is Divvun's strongest area and the one PanGloss
should study hardest:
- **Word processors**: `libreoffice-divvun` (LibreOffice extension, language checker + hyphenator);
  Divvun Grammar Checker add-ins for **MS Word (incl. web)** and **Google Docs** (both listed on
  their respective official marketplaces).
- **OS-wide spelling**: `divvun.org/proofing/oswide.html` documents system-level spell-checking
  integration on macOS/Windows via **Divvun Manager**, a package manager built on the **Páhkat**
  repository format (`divvun-manager-macos`, `divvun-manager-windows` — separate native apps per
  OS, communicating with a `pahkatd` background service over a local socket) — i.e. install once,
  get spellers/keyboards/TTS voices for a chosen language kept up to date automatically.
- **Mobile keyboards**: **Divvun Keyboards** (iOS + Android app, on the Apple App Store), covering
  North/Julev(Lule)/South/Pite/Inari/Skolt Sámi keyboards with built-in spell checkers for
  North/Lule/South/Inari; `giellakbd-ios` is an open-source keyboard-extension reimplementation.
- **Enterprise/publishing**: `woodwing-divvungc`, a speller/grammar-checker plugin for WoodWing
  ContentStation (a commercial publishing CMS) — evidence Divvun integrates outside the
  desktop-office/mobile-keyboard categories the rubric names.
- **Artifact format**: `.zhfst` (zip archive; `index.xml` metadata + `acceptor.*.hfst` +
  optional `errmodel.*.hfst`) is the distribution format; `.bhfst` (via `thfst-tools`) is a
  byte-aligned, memory-mappable reformatting of the same content inside a `box` container — explicitly
  documented as "required for ARM processors" and recommended for distribution, i.e. a
  load-performance/mobile-target optimization of the same logical package, not a different format.
- **FFI surface**: the `divvunspell` repo ships bindings for **C/C++ (cffi-based), Java/JNI,
  Python, Swift, and Deno** (`ffi/{c,java-jni,python,swift,deno}`) — all native-library FFI, not
  WASM (see FOOTPRINT).

## LICENSE

`[M]` Layered, and the layering matters for reuse planning:
- **`divvunspell` library** (Rust): dual **MIT OR Apache-2.0** — fully permissive, per the repo's
  own `Cargo.toml` (`license = "MIT OR Apache-2.0"`) and README license section.
- **`divvunspell` CLI + `thfst-tools`** (the binaries): **GPL-3.0** (separate `LICENSE-GPL`) —
  the library is permissive but the shipped command-line tools are copyleft; embedding the library
  directly (not shelling out to the CLI) avoids the GPL.
- **`hfst-ospell`** (C++, the library divvunspell reimplements/extends): **Apache-2.0**, per its
  own README ("The library is licenced under Apache licence version 2, other licences can be
  obtained from University of Helsinki") — also permissive, and notably the copyright holder
  offers alternate licensing on request.
- **`hfst` (libhfst core toolkit — the compiler/algorithm library used to *build* FSTs)**:
  **GPL-3.0**, confirmed via repo metadata. This is a real asymmetry: **consuming** a compiled
  ZHFST/BHFST at runtime (`hfst-ospell`/`divvunspell`) is permissively licensed, but the **toolchain
  that compiles** the lexc/twolc grammar into the acceptor FST in the first place is GPL.
- **`vislcg3`** (CG engine): **GPL-3.0-or-later**.
- **`libdivvun`** (the CG pipeline glue/grammar-checker library): **GPL-3.0**.
- **Net**: the *speller half* (analyzer/error-model FST + lookup) is cleanly reusable
  (Apache/MIT) at the runtime-library level; the *grammar-checker half* (CG engine + pipeline) is
  GPL end-to-end, and the FST-compilation toolchain itself is GPL too (relevant only if PanGloss
  wanted to reuse Divvun's *compiler*, not just consume its output format).

## FOOTPRINT

`[M/S]` No wasm32 build of the core `divvunspell` engine was found. The only WASM artifact in the
repo is `support/accuracy-viewer` — its own `Cargo.toml` (fetched directly) depends on `dioxus`
(web feature), `wasm-bindgen`, and `web-sys`, but **does not depend on the `divvunspell` crate at
all** — it's a standalone JSON accuracy-report *viewer* (reads a JSON report a native `accuracy`
CLI run produced), explicitly isolated into its own Cargo workspace "so it never resolves against
the native divvunspell workspace" per that file's own comment. So: **no confirmed evidence the FST
lookup/composition engine itself has ever been compiled to wasm32** — this is a gap, not a
confirmed capability, and should not be assumed. `[M, from directly reading the Cargo.toml]`
Runtime footprint characteristics that *are* documented: **memory-mapped transducers**
(`memmap2`/`mmap-io` are direct dependencies) mean the compiled FST is not fully loaded into heap —
pages are faulted in on access, which is favorable for constrained environments generally (this is
architecturally WASM-friendly in principle, since it avoids large heap allocation, but WASM's
lack of real `mmap` means this specific mechanism would need a fallback path, not a straight port)
`[S]`. THFST/BHFST format is explicitly justified as "required for ARM processors" `[M]`, i.e.
Divvun already treats byte-alignment/endianness portability as a real constraint for
lower-power/mobile targets — a relevant precedent for PanGloss's own resource-envelope work, even
without a WASM build to point to directly.

## RUST_C

`[M]` `divvunspell` **is** Rust, dual MIT/Apache-2.0 at the library level (see LICENSE) — directly
reusable as a dependency or as an algorithmic reference implementation without copyleft
encumbrance, for the **speller** half specifically. Concretely reusable/study-worthy pieces,
confirmed from the fetched `Cargo.toml` and README: the `n-best`/`max-weight`/`beam` search
parameterization, the three-component weight formula (`lex + mut + rew`), the positional
start/mid/end reweighting design, and the ZHFST/BHFST parsing + memory-mapping code. **Not**
reusable without GPL exposure: the CG engine (`vislcg3`) and the grammar-checking pipeline
(`libdivvun`), both GPL-3.0, both C++ — if PanGloss wants Divvun's CG-disambiguation *architecture*
it must be reimplemented (matching the repo's "build philosophy" note in `00-synthesis.md`: "if an
engine doesn't exist in Rust... we port it"), not linked. No Rust CG-3 engine implementation was
found in this pass (not specifically searched for exhaustively — flag as an open question for the
"engines needing a Rust port" inventory task in `00-synthesis.md`'s followups list). `[S for the
final point]`

## MINORITY_VERDICT

`[S, grounded in M above]` Divvun's own deployment targets are hyper-minority by world standards
(Skolt Sámi has on the order of a few hundred speakers) and it demonstrates the architecture works
at that end of the population spectrum for *languages with a mature, decades-funded linguistic
description* (UiT/Giellatekno's Sámi grammars are among the most thoroughly FST-formalized
minority-language grammars in existence). What it assumes that a **new, young-orthography,
no-existing-corpus** PanGloss target language may lack:
1. **A complete lexc/twolc/xfst morphology already exists or is fundable** — Divvun did not have
   to solve "how do you build the FST in the first place for a language with no prior formal
   description"; Giellatekno's decades of linguistic fieldwork did that. PanGloss's actual bet
   (HermitCrab/LibLCM authoring from a field linguist's own analysis, generatively) is the harder,
   earlier-stage problem Divvun's architecture presupposes is already solved.
2. **The error model is nearly free** (auto-generated Levenshtein+swap, per DATA_REQ) — this part
   of the prompt's assumed authoring burden is **not** actually a barrier, corrected from what the
   prompt implied.
3. **CG rules are a real, hand-authored, decades-of-linguist-hours investment** (disambiguation +
   valency + error rules per language) **and are optional** — a new-language deployment can ship a
   working speller (FST + auto error model) with zero CG investment; it only loses the
   real-word-error/grammar-checking tier, not spelling suggestions entirely. This significantly
   softens the "must have hand-written CG rules" assumption in the prompt for the *speller* case,
   while confirming it fully for the *grammar-checker* case.
4. **No corpus-dependency in the core design** — neither the FST acceptor, the error model, nor CG
   rules require a training corpus (report 04 already noted CG "does not degrade as corpus size
   shrinks"); this is actually a **good fit** for languages with no corpus, better than a
   statistical alternative would be.
5. **What Divvun's architecture has no answer for**: personalization/incremental learning
   (PERSONALIZATION, above — genuinely undocumented, possibly absent), and semantic-domain-level
   disambiguation (SEMANTICS_POS) — neither is a strength to borrow.

Net: for a hyper-minority language that **already has, or is actively building, a complete
generative FST morphology** (i.e., exactly PanGloss's own core deliverable), the Divvun speller
architecture is directly achievable with a nearly-free error model and no corpus requirement — the
floor is lower than the prompt assumed. The CG grammar-checker floor is genuinely high (hand-written
linguistic engineering, unquantified in hours/rules from sources available this pass) and should be
treated as a separate, deferrable investment, matching PanGloss's own likely rollout order (speller
before grammar-checker).

## HEADLINE

**Strengths:**
- **Proven production integration breadth**: LibreOffice, MS Word/Google Docs, OS-wide via a
  dedicated package manager (Windows + macOS), iOS/Android keyboards, even a commercial publishing
  CMS plugin — no other system in this comparison set has this many shipped host integrations for
  minority languages specifically. `[M]`
- **Auto-generated baseline error model**: a full working speller needs no hand-authored error
  weights to start (script-derived Levenshtein+swap from the acceptor's own alphabet) — the
  authoring floor for the *speller* is lower than assumed. `[M]`
- **CG-based real-word-error detection is a mature, working answer** to the hardest part of
  spell-checking (detection, not correction) that a statistics-starved minority language can never
  get from an n-gram model, and it's been running in production for multiple Sámi languages for
  years. `[M/A]`

**Weaknesses:**
- **CG is GPL-3.0, C++, and requires substantial hand-authored per-language linguistic engineering**
  (disambiguation + valency + error rules) with no quantified per-language cost found in any
  source fetched this pass, and no Rust implementation exists to reuse or study directly. `[M/S]`
- **No confirmed WASM build of the core engine** — the only WASM artifact found is an unrelated
  report-viewer tool; PanGloss would be establishing WASM feasibility for this class of engine
  itself, not confirming an existing result. `[M]`
- **No documented personalization/user-adaptation mechanism** — a real gap relative to PanGloss's
  own personalization design work (report 06), and Divvun's own docs are silent on it despite
  otherwise-thorough documentation elsewhere. `[UNFETCHED/absence]`

**What PanGloss has that Divvun does not**, confirmed against what was actually found in Divvun's
own sources this pass:
- **A structured feature-system/natural-class layer above the raw FST alphabet**
  (`CharDefTable::unif_closure`/`feature_lanes`, report 02) that a phonological error-cost model can
  be *derived* from — Divvun's phonetic/diacritic weighting is a short hand-authored table
  (`a	á	0.5`-style pairs), not a derived distance over an authored feature inventory.
- **Semantic domains as first-class LibLCM/FLEx data** (even though not yet in PanGloss's own
  parser-export schema per report 04 — but present in the data model Divvun's toolchain has no
  equivalent of at all; Divvun's tag inventory tops out at POS + inflection + valency).
- **A design intent to synthesize error-training data from the grammar itself** (report 00/05's
  "synthetic error generation from our own generative grammar" line) — Divvun's error model is
  either a mechanical edit-distance-with-swaps generator or short hand-authored pairs; nothing in
  any Divvun source describes sampling the grammar's own generative capacity to synthesize a richer
  error-training signal.
- **One engine, explicit deployment-mode separation** (inference/WASM vs. native-build vs.
  reference-validation, per `CONTEXT.md`) as a first-class architectural concern from day one,
  whereas Divvun's WASM story is (per this research) simply unestablished rather than designed for
  or against.
