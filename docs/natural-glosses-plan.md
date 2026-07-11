# Natural Glosses via Grammatical Framework — Plan Assessment & Revised Plan

**Status:** assessment of a proposed design (2026-07-11). Not scheduled; candidate post-parity work (after F9).

The proposal: use PanGloss as the "decompiler" (LRL surface word → structural feature bundle)
and a Grammatical Framework (GF) grammar compiled to PGF as the "compiler" (feature bundle →
fluent LWC phrase, e.g. `evlerimde` → `house+PL+1SG+LOC` → *"in my houses"* / *"मेरे घरों में"*).

## Verdict

**The architecture is sound and well-matched to GF — this is literally GF's classic
"application grammar" use case — but the plan as written is not implementable.** Four of its
load-bearing specifics are wrong (a nonexistent Rust crate, a PanGloss output format that
doesn't exist, GF code that won't compile, and a "single portable PGF asset" claim that the
PGF runtime model doesn't support). The famous "90% shared / 10% custom" split is roughly
inverted: the shared GF/RGL machinery is real, but the per-grammar mapping layer the plan
hand-waves in Step 1 is most of the work.

Feasibility: **yes, as a re-architected, phased effort** (revised plan below). The riskiest
unknown is the Rust PGF runtime story; a 2–3 day spike settles it before anything else is
built.

---

## 1. What the plan gets right

- **The pipeline shape.** Analysis → language-neutral interlingua → GF linearization is the
  textbook GF embedding pattern. GF's Resource Grammar Library (RGL) genuinely handles the
  hard LWC-side morphosyntax (English plural + possessive ordering, Hindi postpositions and
  gender agreement) so the application layer stays small *per LWC*.
- **PGF as a data asset.** Compiling grammars to a `.pgf` binary on a dev machine and shipping
  it as a static asset (`include_bytes!`) is the intended GF deployment model. End devices
  never need the Haskell GF compiler.
- **Modularity claim.** Adding an LWC really is "write one concrete syntax module, recompile
  the PGF, drop in the asset" — *for the feature/construction layer*. (Not for the lexicon;
  see §2.4.)
- **Determinism and footprint.** PGF linearization is a table lookup — microseconds, no ML,
  fully offline. That fits the project's field-deployment aspirations (plan §12 follow-ups
  #6/#7: offline/on-device delivery).

## 2. Where the plan breaks

### 2.1 PanGloss does not emit `house+PL+1SG+LOC` — and its output string is frozen

The plan's Step 1 says "configure your dictionary entries and morpheme slots to emit exactly
these unified tags." Reality:

- The parser's output is **morpheme-ID sequences plus a phonological surface**, not gloss
  tags. Per analysis it is `join("+", MorphemeId strings) + "|" + surface`
  (`rust/crates/hc-parse/src/lib.rs`, `result_signature`), e.g. Sena `++|[mn]+?bal+?i`.
  The `<MorphemeId>` element is *empty* for every Indonesian morpheme, so the left half is
  often blank.
- Leipzig-style glosses **do exist**, but as free-form per-grammar strings on
  `MorphemeInfo.gloss` (`hc-grammar/src/model.rs:478`) sourced from `<Gloss>` in the
  HermitCrab XML — Indonesian uses `Caus`, `NMLZR`, `LOC`, `APPL`, `AV`, `pl`, `RECIP`,
  plus lexical glosses like `read`, `sell`, `use`. They are display data, **not part of the
  parity output**.
- The batch signature is **frozen against the C# conformance oracle** (byte-identical golden
  TSVs, F0). It cannot be repurposed as the GF input format. Any natural-gloss output must be
  a **new, additive layer** that reads `WordAnalysis.morpheme_ids` →
  `Grammar::morphemes[i].gloss`, leaving the parity string untouched.
- There is no `PanGloss::load_from_config("cherokee_rules.json")` and no JSON grammar config
  anywhere. Grammars are HermitCrab XML (`samples/data/*-hc.xml`) loaded by `hc_grammar::load`;
  the parse API is `hc_parse::Morpher::parse_word`.

**Consequence:** Step 1 is not "establish a token template" — it is *build a mapping layer*
from per-grammar free-form gloss strings to a controlled concept/feature vocabulary. Glosses
are whatever the FLEx grammar author typed (`pl` vs `PL`, `his` vs `3SG.POSS`). This mapping
table is per-grammar configuration, it is the real Step 1, and it is most of the ongoing
linguist-facing work.

### 2.2 `pgf-runtime = "0.1"` does not exist

There is no such crate. The actual Rust options, none of which match the plan's description
of "thin, zero-overhead bindings to the ultra-light pgf-c runtime":

| Option | What it is | Risk |
|---|---|---|
| [`gf-core`](https://crates.io/crates/gf-core) (crates.io, v0.3.x) | Pure-Rust PGF runtime (loads binary `.pgf`, converts to JSON internally, then interprets — in the lineage of the JS/TS runtime) | **Immature**: ~1 star, no GitHub releases, correctness on complex RGL-compiled grammars unproven. Must be validated in a spike; may need patches or a fork. |
| C runtime (`libpgf` from GF's `runtime/c`) via bindgen | The "real" embeddable runtime the plan imagined | No published Rust bindings — we'd write and maintain them. C build on the project's MSVC toolchain is untested and the C runtime itself is lightly maintained. Breaks the workspace's current pure-Rust, no-`build.rs` property. |
| Haskell `gf` as an offline tool only | Use the GF shell at *build/authoring time* to pre-generate outputs; ship no runtime | Zero runtime risk, but only works if the realization space can be finitely enumerated (see §4, Alternative B). |
| GF web service (`pgf-http`/GF cloud) | Runtime over HTTP | Dead on arrival for offline field use. |

**Consequence:** Step 4 must start with a **runtime selection spike**, not a `Cargo.toml`
edit. `gf-core` first (it keeps the workspace pure Rust); C-FFI as fallback; pre-generated
tables as the escape hatch.

### 2.3 The GF grammar sketch won't compile, and its shape is wrong

`list Feature`, `PrepModifier`, `mkPrepModifier`, and `applyModifiers` are not GF/RGL
constructs. More fundamentally, a `Concept -> [Feature] -> Phrase` function is the wrong
abstraction: features are not an unordered bag you fold over — number, possessor, and case
*interact*, and the RGL exposes them as typed slots. The workable shape is a small set of
**typed constructions**, e.g. (illustrative, real RGL API):

```gf
abstract Gloss = {
  flags startcat = Gl ;
  cat Gl ; NConcept ; Poss ; Num ;
  fun
    NPhrase   : Poss -> Num -> NConcept -> Gl ;  -- "my houses"
    LocPhrase : Poss -> Num -> NConcept -> Gl ;  -- "in my houses"
    Sg, Pl : Num ;
    NoPoss, P1Sg, P2Sg, P3Sg : Poss ;
    house_N : NConcept ;                          -- generated, see §2.4
}

concrete GlossEng of Gloss = open SyntaxEng, ParadigmsEng in {
  lincat Gl = Utt ; NConcept = N ; Poss = Quant ; Num = Num ;
  lin
    Sg = sgNum ;  Pl = plNum ;
    NoPoss = a_Quant ;  P1Sg = mkQuant i_Pron ;
    house_N = mkN "house" ;
    NPhrase p n c   = mkUtt (mkNP p n c) ;
    LocPhrase p n c = mkUtt (mkAdv (mkPrep "in") (mkNP p n c)) ;
}
```

**Consequence:** the abstract syntax must enumerate the *constructions* the gloss bundles can
form (a possessed located nominal; a tensed honorific verb; …), not a generic
concept-plus-feature-list. That inventory grows with linguistic coverage (evidentiality,
aspect, valence-changing morphology like the Indonesian `Caus`/`APPL`/`RECIP` set) and is a
per-construction design task, not boilerplate. The Rust side then maps a normalized feature
bundle → one abstract-tree template; unmapped bundles fall back to plain Leipzig gloss output.

### 2.4 The lexicon breaks "one portable Bridge.pgf"

PGF grammars are **closed at compile time** — the standard runtimes cannot add lexemes at
linearization time. But every LRL project's dictionary contributes its own root concepts
(Indonesian sample alone: `read`, `sell`, `use`, `see`, …, hundreds to thousands in a real
FLEx project). Baking a fixed `house_Concept`/`teach_Concept` list into a universal asset
cannot work.

**Consequence:** the PGF is a **per-project generated asset**, not a fixed universal one:

1. The shared, hand-written layer: abstract constructions + per-LWC concrete rules (this *is*
   genuinely reusable — the plan's "10%").
2. A **lexicon generator**: a small tool that reads a project's HermitCrab XML, takes each
   root's `<Gloss>` (an English-ish citation form), and emits a GF lexicon module per LWC via
   smart paradigms (`mkN "house"` for English; for Hindi either a bilingual lookup table or a
   transliteration/fallback strategy — a real design decision the plan never mentions).
3. Recompile `Gloss<Proj>.pgf` whenever the dictionary changes — i.e. PGF compilation joins
   the *project* authoring loop, not just the developer loop. Field teams would need the GF
   compiler or a hosted compile step.

Also check before committing: **PGF size** with RGL-based concretes plus a project lexicon can
exceed the plan's 1.5–3 MB estimate (measure in the spike), and **licensing** — the GF
compiler is GPL (fine, dev-time only), the RGL is LGPL/BSD-style, `gf-core` crate license and
the status of a compiled `.pgf` as data vs. derived work should be confirmed against this
repo's MIT licensing before shipping embedded assets.

### 2.5 Repo-fit corrections

- New code goes in a **new workspace crate** (suggested: `rust/crates/hc-realize`), consumed
  by `hc-cli` (and optionally `hc-ffi`), feature-gated so the parity toolchain is unaffected.
  Core crates (`hc-parse`, `hc-grammar`) gain at most a small read-only accessor.
- Timing: the workspace is mid-hybrid-FST plan (F8 in flight, F9 next). This work is
  independent of that plan and should be sequenced **after F9** or on a parallel branch that
  touches no `hc-hybrid`/`hc-parse` internals.
- The plan's `main()` sketch (single-word demo loop) doesn't match the real entry points;
  wiring is a `hc-rs parse --natural-gloss=<lang>` flag and/or a `hc-rs gloss` subcommand on
  top of `ParseOutcome`.

---

## 3. Revised plan (phased, spike-first)

### Phase 0 — Gloss extraction layer *(valuable regardless of GF)*

Add to `hc-parse` (or the new crate) a function
`gloss_bundle(&Grammar, &WordAnalysis) -> GlossBundle` where

```rust
pub struct GlossBundle {
    pub root: String,             // gloss of morphemes[root_morpheme_index]
    pub affix_glosses: Vec<String>, // glosses of the non-root morphemes, in order
    pub pos: Option<String>,
}
```

Resolves `morpheme_ids` → `Grammar::morphemes[i].gloss`, special-casing
`MorphemeId::GUESSED`. Expose as `hc-rs parse --gloss` (Leipzig-style `read-AV-APPL` output).
This is small, additive, immediately useful for field display, and is the input contract for
everything below. **~1–2 agent-days.**

### Phase 1 — Interlingua + per-grammar mapping config

- Define the normalized IR: `Concept` + typed feature slots (number, person/possessor, case
  role, tense/aspect, …) + a construction selector.
- Define a per-grammar mapping file (TOML alongside the grammar XML) from raw gloss strings
  to IR features: `pl = "Num:Pl"`, `LOC = "Case:Loc"`, `his = "Poss:3Sg"`, with an explicit
  `unmapped` policy (pass through as bracketed Leipzig tag — the pipeline must degrade
  gracefully, never fail, on unmapped glosses).
- Document the convention linguists follow when authoring new grammars.
  **~2–3 agent-days + linguist review.**

### Phase 2 — Offline GF prototype (no Rust yet)

- Write the abstract construction inventory for an initial scope (suggested: possessed/located
  nominals + basic verb tense — enough for the `evlerimde` demo) and concrete modules for
  **English + one morphologically demanding LWC (Hindi)**, using the real RGL API.
- Write the **lexicon generator** (any language; even a script) that emits GF lexicon modules
  from a grammar XML's root glosses.
- Compile with `gf -make`, validate linearizations by hand in the GF shell against a test
  sheet a linguist signs off on. Measure the `.pgf` size.
  **~3–5 agent-days.** *This phase proves or kills the linguistic design cheaply.*

### Phase 3 — Runtime selection spike *(the gating risk)*

Load the Phase 2 `.pgf` with the `gf-core` crate on Windows MSVC; linearize the full test
sheet; diff against GF-shell output. Decision gate:

- `gf-core` output matches → adopt it (pure Rust preserved).
- Close but buggy → evaluate patching/forking (it's small) vs. C-FFI bindings to `libpgf`.
- Structurally broken → fall back to **Alternative B** (pre-generated tables, §4).
  **~2–3 agent-days. Do this before committing to Phases 4–5.**

### Phase 4 — `hc-realize` crate

- `trait Realizer { fn realize(&self, ir: &GlossIr, lang: LwcId) -> Realization; }` so the
  PGF backend is swappable (table backend, future NN backend).
- PGF backend: `include_bytes!` per-project asset or load-from-path (projects regenerate
  their own PGF, so load-from-path is the primary mode; embedding suits demo/default assets).
- Bundle→tree templating: normalized IR → abstract expression string → linearize; fallback to
  Phase 0 Leipzig output on any gap. Property test: realizer never panics and never returns
  empty for any parseable word in the three sample corpora.
  **~4–6 agent-days.**

### Phase 5 — Wiring & authoring loop

- `hc-rs parse --natural-gloss=eng,hin` and batch equivalent; optional `hc-ffi` export.
- Document the project-authoring loop: edit FLEx grammar → export XML → run lexicon generator
  → `gf -make` → drop `.pgf` next to the grammar. Decide who runs the GF compiler (field
  laptop install vs. hosted build).
  **~2–3 agent-days.**

**Total: roughly 15–22 agent-days** to a demoable end-to-end pipeline for English + Hindi on
the Indonesian sample grammar — comparable to the hybrid-FST milestone scale, with Phases 0–3
(~8–13 days) buying a firm go/no-go before the larger commitment.

---

## 4. Alternatives if GF proves too heavy

**A. Pure-Rust micro-realizer.** For English-only output, the "10%" is small enough to
hand-write (plural via a small exception table, determiner/preposition templates). Loses the
RGL exactly where it earns its keep (Hindi agreement, Mandarin classifiers), but ships in
days and fits under the same `Realizer` trait.

**B. Pre-generated linearization tables.** Keep GF strictly *offline*: enumerate the
(bounded) construction × feature-bundle space, have the GF shell linearize each with a
placeholder lexeme plus each lexeme's inflected forms, and ship a compact template table
(`Loc+Poss1Sg+Pl → "in my {N.pl}"`) the Rust side fills in. No runtime dependency at all;
viable because the feature-bundle space, unlike the lexicon, is small. This is the strongest
fallback if Phase 3 fails, and arguably a contender on its own merits.

---

## 5. Open questions

1. **Target LWC lexicon sourcing** (§2.4): where do Hindi/French translations of root glosses
   come from? Bilingual wordlist (CAWL?), human pass, or English-fallback-with-flag?
   *Partially answered in §7.4: GF WordNet is a strong source for its 15 covered languages.*
2. **Construction coverage policy**: which gloss bundles get fluent realizations vs. Leipzig
   fallback, and who decides per grammar?
3. **Ambiguity**: `ParseOutcome` routinely carries multiple analyses — realize all, rank, or
   defer to the caller?
4. **Guessed roots** (`MorphemeId::GUESSED`): realize with the raw surface form as the
   concept ("in my *mbal*-s")? *Answered in §7.8: GF's `Symb` category embeds literal strings
   in trees; this is a supported pattern, not a hack.*
5. **Licensing** of `gf-core`, RGL-derived `.pgf` assets vs. this repo's MIT license.
6. **Who compiles PGF in the field** — is a hosted compile service acceptable, given the
   offline-first goal? *Architecture B (§8) eliminates the question; A and C must answer it.*

---

## 6. A better PanGloss output contract (2026-07-11 addendum)

*Question: can PanGloss's output be changed to a better format for realization?*
**Yes — additively, and the raw material is better than §2.1 assumed.** The frozen parity
signature stays (it is the conformance oracle, not a display format), but a parallel
structured record can carry strictly more information than glosses. Three channels already
exist in the engine:

### 6.1 The unified syntactic feature structure (currently thrown away)

The morpher maintains `w.syn_fs` per in-flight analysis — the word's syntactic
`FeatureStruct`, seeded from the root lex entry (`morpher.rs:507`) and updated by applied
rules, validated against obligatory features (`morpher.rs:614`), HermitCrab's real
morphosyntax. Today only `pos_id` is extracted from it (`morpher.rs:721`); the rest is
dropped before `WordAnalysis`.

This is exactly the "controlled feature vocabulary" the plan's Step 1 wished for, and it is
**typed and grammar-defined** (`SynFeatureSystem`, `<HeadFeatures>`), not free-form text.
Where a grammar author populates head features (Sena's `genro`, 20 symbols), the gloss-string
→ feature mapping problem of §2.1 largely disappears: the mapping becomes *feature-system
symbol → IR feature*, declarative and finite. Caveat: richness is grammar-dependent —
Indonesian's `<HeadFeatures/>` is empty and its grammatical meaning lives in derivational
affix glosses (`Caus`, `APPL`, `RECIP`), so glosses remain a required second channel.

### 6.2 Morpheme properties (an unused explicit channel)

`MorphemeInfo.properties: Vec<(String, String)>` (`hc-grammar/src/model.rs:481`) is loaded
from the XML's `<Properties>` and currently unused for output. A documented convention —
e.g. a `realize` property holding an explicit IR tag (`realize = "Case:Loc"`) — lets grammar
authors opt specific morphemes into precise realization **inside the grammar file itself**,
removing the sidecar-TOML guesswork for new grammars. The sidecar mapping remains only as an
override/retrofit mechanism for grammars we don't control.

### 6.3 Proposed record

Emit per analysis (behind `hc-rs parse --analysis-json` or similar; additive, parity string
untouched):

```jsonc
{
  "surface": "evlerimde",
  "pos": "N",
  "head_features": { "num": "pl", "case": "loc" },   // from syn_fs, if authored
  "morphemes": [
    { "gloss": "house", "is_root": true,  "properties": {} },
    { "gloss": "pl",    "is_root": false, "properties": { "realize": "Num:Pl" } },
    { "gloss": "1SG",   "is_root": false, "properties": { "realize": "Poss:1Sg" } },
    { "gloss": "LOC",   "is_root": false, "properties": { "realize": "Case:Loc" } }
  ],
  "guessed": false
}
```

The realizer's IR builder then resolves features by priority: explicit `realize` property →
head-feature symbol → sidecar gloss mapping → unmapped (Leipzig fallback). This supersedes
the Phase 0/1 sketch in §3: same shape, richer sources. Longer term, once C# parity is
retired as the oracle, this record — not the signature string — becomes PanGloss's primary
machine output.

---

## 7. GF asset inventory — reuse everything that exists

What GF already provides, and where each asset slots in:

1. **RGL — ~40 languages** of syntax + morphology behind one API
   ([grammaticalframework.org](https://www.grammaticalframework.org/),
   [gf-rgl](https://github.com/GrammaticalFramework/gf-rgl)). All LWC-side morphosyntax
   (English determiner/plural ordering, Hindi postpositions + gender agreement) is inherited,
   never written.
2. **The functor / incomplete-concrete pattern.** Write the application concrete **once**,
   `open`ing only the language-independent `Syntax` interface; instantiate per LWC with a
   ~10-line functor instantiation (`GlossEng = GlossFunctor with (Syntax = SyntaxEng, LexGloss = LexGlossEng)`).
   Per-LWC cost collapses to a lexicon module plus any idiom exceptions. This is the single
   biggest reuse lever and the plan's "10%" made real.
3. **Smart paradigms.** `mkN "house"`, `mkV "read"` infer full inflection tables from
   citation forms — the lexicon generator emits one-liners, not paradigm tables.
4. **GF WordNet** ([gf-wordnet](https://cloud.grammaticalframework.org/wordnet/gf-wordnet-help.html)):
   ~100k interlingual lemmas (`apple_1_N`) with inflection tables across 15 languages
   (Bulgarian, Catalan, Chinese, Dutch, English, Estonian, Finnish, Italian, Portuguese,
   Slovenian, Spanish, Swedish, Thai, Turkish, Zulu; English fully verbalized, others
   partial). Use it as the **source database for the lexicon generator** — map root gloss →
   WordNet sense → per-language rendering — not as a shipped runtime asset. Directly attacks
   open question 1. (Hindi is *not* covered; Hindi lexicon still needs a bilingual list or
   human pass.)
5. **The GF shell as a generation engine.** `gt` (generate all trees to a depth) and `l`
   (linearize) are scriptable; this is the entire build-time machinery for Architecture B and
   the golden-test generator for all architectures.
6. **Multiple PGF runtimes** (Haskell, C `libpgf`, TS/JS, `gf-core` Rust): the TS runtime
   matters if a web-based field UI ever appears; the C runtime is Architecture A's fallback.
7. **RGL `Extend` / `Constructors` / `Structural` modules** for constructions beyond the core
   API (topicalization, existentials) before writing anything by hand.
8. **Literal/symbol categories (`Symb`, `mkSymb`, `SymbPN`)**: embed arbitrary strings as
   proper nouns/terms inside trees — the supported mechanism for `MorphemeId::GUESSED` roots
   ("in my **mbal**-s") and for OOV passthrough generally.

---

## 8. Three candidate architectures

All three keep GF; they differ in **where GF runs** and **which existing GF layer we lean
on**. All share §6's output contract, the IR, and the Leipzig fallback; A and B share their
grammar source too, so the choice between them can be deferred to the Phase 3 spike.

### Architecture A — Embedded interpreter ("GF in the box")

Own small `Gloss` abstract (typed constructions, §2.3 shape) written as a **functor over
`Syntax`** (§7.2); per-project lexicon module generated from the grammar XML (WordNet-assisted
where possible, §7.4); compiled to a per-project `.pgf`; `hc-realize` loads it at runtime via
`gf-core` (fallback: bindgen over C `libpgf`).

- **Strengths:** full generality — construction inventory can grow without redesign; handles
  ambiguity natively; bidirectional (could later *parse* LWC input back to IR for authoring
  tools); per-LWC addition is a functor instantiation + recompile.
- **Weaknesses / risks:** `gf-core` maturity is unproven (§2.2 — the Phase 3 spike);
  per-project PGF compile loop requires the GF compiler on some machine in the authoring
  workflow (open question 6); PGF size unmeasured.
- **Choose when:** this becomes a long-lived platform feature with a growing construction
  inventory, and a hosted or field-laptop compile step is acceptable.

### Architecture B — Compile-time GF, runtime tables ("GF as build tool")

Identical grammar source to A, but GF runs **only at build/authoring time**. Two generated
artifacts ship instead of a PGF:

1. **Template table:** enumerate the construction × feature-bundle space with `gt` (it is
   small and finite — tens to low hundreds of bundles per construction set), linearize each
   with placeholder lexemes: `LocPhrase P1Sg Pl → "in my {n:pl}"`.
2. **Lexicon table:** per root, the inflected forms each language's templates reference,
   generated via smart paradigms (`house → {sg: "house", pl: "houses"}`), plus any
   agreement-class key the language needs.

A ~200-line Rust filler joins them at runtime. Where a template depends on lexeme class
(Hindi gender: *mera ghar* vs. *meri kitab*), templates are keyed by class and the class is a
lexicon-table column — the RGL knows each lexeme's class at generation time, so this is
mechanical, but the table schema is a per-language-family design task.

- **Strengths:** **zero runtime dependency** — no PGF interpreter, no FFI, workspace stays
  pure Rust with no `build.rs`; field devices need nothing installed; output is a diffable
  text artifact (auditable by linguists, goldens for free); immune to the §2.2 runtime risk
  entirely.
- **Weaknesses / risks:** construction space must stay finitely enumerable (it does for the
  glossing use case, but this architecture can never grow into free realization);
  agreement-heavy languages inflate table schemas; discontinuous/clitic phenomena may not
  factor cleanly into template + slots.
- **Choose when:** offline-first is paramount and the construction inventory is a bounded
  glossing vocabulary — which is precisely this feature. **Recommended ship-first
  architecture.**

### Architecture C — Stand on the big grammar (GF WordNet / RGL-API trees)

Author (almost) no grammar. Build IR trees directly against the **existing wide-coverage
RGL API + GF WordNet lexicon**: `evlerimde` → `mkAdv in_Prep (mkNP (mkQuant i_Pron) plNum house_1_N)`.
The per-project work reduces to (a) linking each root gloss to a WordNet sense
(semi-automatic — string match on the English gloss, linguist confirms ambiguous cases) and
(b) the generic feature → RGL-API mapping, which is written once, not per project.

- **Strengths:** the lexicon problem (§2.4) largely dissolves for WordNet's 15 languages —
  no lexicon generator, no smart-paradigm guessing, human-checked inflection tables;
  construction coverage is "whatever the RGL API expresses", i.e. effectively unlimited; the
  sense link is itself a valuable lexical artifact (interlingual dictionary for the project).
- **Weaknesses / risks:** non-English verbalization coverage in GF WordNet is partial, and
  quality varies (much is automatically induced); Hindi and most SIL-relevant LWCs beyond the
  15 are absent, forcing the A/B lexicon path anyway for them; full WordNet PGFs are far too
  large to ship, so a **sense-extraction step** (compile a project-sized PGF containing only
  used senses) is required — which reintroduces A's compile loop, or a hosted service;
  sense-linking adds a disambiguation UX the other architectures don't need.
- **Choose when:** target LWCs are inside WordNet's coverage and minimizing grammar authoring
  matters more than offline purity. **Even if not chosen wholesale, its sense-linking idea
  upgrades A's and B's lexicon generator** — C degrades gracefully into "A or B with a
  WordNet-backed lexicon source."

### Recommendation

Build Phases 0–2 (§3) once — they are identical for all three. Use **B as the ship-first
target** (it removes the only fatal risk class and matches the offline field constraint),
with the grammar source written functor-style so **A remains a pure upgrade** (swap the table
generator for a PGF load) if the construction inventory outgrows tables. Adopt **C's
WordNet sense-linking as the lexicon source** inside the Phase 2 lexicon generator for
covered languages from day one. The Phase 3 spike then becomes optional rather than gating:
run it opportunistically to keep A's door open, not as a blocker to shipping.
