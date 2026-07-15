# Final analysis: FST-only, off-the-shelf, no HermitCrab pruning — the three questions answered

Author: Fable (main session), synthesizing reports 01–04, 06a, 06b (subagent evidence) with the
adversarial review (05) and independent measurements. Evidence citations point into those reports;
each of them cites code/URLs/measurements directly.

**Decision context (user-stated):** if any FST toolchain, in any language, can handle the full HC
complexity without HC verification, the plan is to abandon this Rust repo and rebuild the FST
compiler in C#. Output must preserve full morphological analysis + glossing. Budgets: <5 s build,
<10 MB deployed, <1 ms/word.

---

## Verdict

**Analysis finds no construct in the HermitCrab grammar space that is inexpressible in a classical
FST with the standard mechanisms (flag diacritics, compile-replace, α-expansion), and no
super-linear state cost that lacks a standard, shipped mitigation.** The propose/verify hybrid is
not necessitated by the problem — it is necessitated by the incumbent encoding (unification arcs,
no determinization/minimization). The empirical PoC (report 04) confirms the semantics side on the
hardest named cases (Indonesian 121/121 exact parity including reduplication×phonology opacity;
multi-analysis survives determinize/minimize). One verification step remains genuinely open —
real `lexicon ∘ rules` composition sizes on our grammars with a native toolkit — and it is exactly
the "compile it and see" step deferred per instruction.

---

## Q1 — What FST features we need

Derived construct-by-construct from the full inventory (06a, 18 construct families):

| # | Feature | Driven by (HC construct) | Status of need |
|---|---|---|---|
| 1 | Two-tape FST, compiled in synthesis direction, inverted for analysis | whole architecture | absolute |
| 2 | Multichar symbols: ~30–420 segments + tags at **(morpheme ID, allomorph ID)** granularity + root index | 06a (c): analysis identity = (morpheme IDs, allomorph IDs, root index) under default config; Sena `mbali` 9× free-fluctuation multiplicity is combinatorial and must be enumerable | absolute |
| 3 | Replace-rule calculus: directional (L2R/R2L), iterative & simultaneous modes, obligatory, deletion/epenthesis, composed in stratum order | rewrite rules; feeding/bleeding/opacity (06a (a): both modes exist and differ in feed/bleed) | absolute |
| 4 | Flag diacritics **evaluated at lookup time** | MPR features, co-occurrence rules (long-distance, 06a (d)), stem names, RequiredHeadFeatures/OutputHeadFeatures (report 04's dominant Sena mismatch class), circumfix pairing | absolute — the linear-vs-exponential lever |
| 5 | compile-replace (or `_eq`) over the finite lexicon | whole-stem reduplication — the **only** mathematically non-regular construct in the inventory (06a's final section), regular once the lexicon is finite; proven mechanically in report 04 §5 | absolute for redup grammars |
| 6 | All-paths lookup (not 1-best) | multi-analysis semantics incl. free-fluctuation multiplicity | absolute |
| 7 | Determinize/minimize over concrete symbols | size + <1 ms lookup | absolute |
| 8 | Compact serialization + small, permissively-licensed runtime (C# target) | <10 MB, MIT product | absolute |
| 9 | *(insurance)* lazy runtime composition | only if eager lexicon∘rules is oversized on some grammar | contingent |
| 10 | *(optional)* guesser lexicon (phonotactic root patterns) | HC's OOV root guesser (`guess.rs`); compiled FSTs are closed-vocabulary | product decision |

Not needed: weights/semirings; unification at runtime (06a: features resolve to concrete
symbol sets + flags); capture groups (tags carry identity).

## Q2 — Does any toolkit have every needed feature?

From the verified capability matrix (06b):

- **HFST family: yes on 1–8.** Full xfst calculus (`hfst-xfst`/`hfst-twolc`/`hfst-lexc`),
  compile-replace shipped and maintained (NEWS-dated), flags **verified evaluated at lookup time in
  source** (`transducer.cc` `try_epsilon_transitions`), all-paths default, published `.hfstol`
  format (FSMNLP 2009) with 119k–408k words/sec measured, and — decisive for the C# plan — HFST's
  own maintainers license *lookup-only* readers **Apache-2.0** (Java reader: 1,500 lines/13 files)
  separate from the GPLv3 compiler, which is offline-only in this architecture.
- **foma: yes except compile-replace** (only `_eq()` — 06b corrected this premise) — still viable
  (Apache-2.0, ~500-line `flookup.c`), with `_eq()` or per-root expansion for redup.
- **Bare OpenFst / rustfst / lttoolbox: no** — no rule calculus and/or no flags. Pynini/Thrax add
  the calculus over OpenFst but have no flag-evaluating runtime (◐).
- **Known seams, none fatal:** (i) feature 9 (lazy composition) exists only in OpenFst/rustfst,
  which lack the calculus — the toolkit with rules and the toolkit with laziness are different
  tools, so the eager artifact must fit the budget (mitigation: per-language scoping; our lexicons
  are 10²–10³ entries vs the 10⁵–10⁶ precedents); (ii) `.hfstol` has a 16-bit symbol ceiling
  (~65,536) — comfortable for ~500 segments + thousands of allomorph tags at our scale, but a
  60k-entry FLEx lexicon would crowd it (mitigation: factored tags or foma format); (iii) **no C#
  runtime exists today** — bounded build, two permissive precedents (1,500-line Java, divvunspell
  Rust), with the Java reader's staleness-vs-current-format warning to heed.

**Answer: yes — the HFST family covers the full required feature set; the shipped C# side is a
~1.5–2k-line format reader away.**

## Q3 — To drop the HC pruning pass entirely: what must be compiled in, and does it explode?

Let E = entries, A = affix allomorphs, Σ = segments, v = values per feature (per equivalence class
a rule distinguishes), k = α-variables per rule, m = cross-morpheme constraints, R = rules, c =
context length, F = free-fluctuating alternatives per slot.

| What verify does today | Classical encoding | State/arc cost | Explodes? |
|---|---|---|---|
| Allomorph environments (left/right) | conditions on allomorph continuation arcs | O(A·c·Σ) | No — linear (Sena census: 72/1,702) |
| MPR / co-occurrence / stem names / HeadFeatures | flag diacritics | O(m) symbols; runtime check | **No with flags**; O(2^m) without — flags are mandatory, and runtimes evaluate them (06b, verified) |
| α-variables | expand per feature-equivalence class | O(v^k) sub-rules **per rule** | The one true exponential cell. Schema caps k ≤ 24 (06a, DTD-verified); real grammars use 1–2 (Indonesian nasal assimilation, Amharic CV-merger), v ≈ 2–8 → ≤ dozens of sub-rules. Local to one rule; never multiplies across rules. Flag as a compiler lint: warn at k ≥ 4. |
| Obligatory application, feed/bleed, opacity | replace-rule semantics + stratum-order composition | see next row | No — proven on the real meN- cascade (04 §5, mechanical) |
| Rule cascade | `lexicon ∘ R₁ ∘ … ∘ Rₙ` with minimize-between-steps | worst case ∏ᵢ|Rᵢ|; practice ≈ O(lexicon) | **The one empirically open cell.** Bounded by precedent (06b §4: Finnish 555k states/29 MB at 400× our lexicon; Hebrew 2M states; Arabic templatic at 5k words/sec) and by the fact that the repo's own "exploded" attempt was *without minimization over unification arcs* — an encoding artifact. Sena needs zero composition (0 phon rules). Verify by compiling — the deferred final step. |
| Whole-stem reduplication | compile-replace over finite lexicon | ≤ ×2 the redup-eligible sublexicon (+ interactions) | No — 04 §5: all 7 corpus words exact, incl. suffix-outside-copy and copy-reflects-assimilation |
| Free-fluctuation multiplicity | distinct allomorph-tag paths | output paths ∝ ∏ F per word (9× on `mbali`) | No — same multiplicity the engine emits; enumeration cost is per-analysis at lookup, µs each |
| Templates/slots, clitics, compounding (≤2 roots, config-cap) | continuation classes; bounded loop | O(A) | No — additive; lexc's home turf |
| Metathesis / truncation / process morphs | bounded-window rewrites | small constants | No |
| Category/feature output | tags + post-lookup join | O(1) per analysis | No — pure function of the tag key under default config (06a (c), verified against `WordAnalysis.Equals`); the purity gap exists only under non-default `Unordered`+parallel, which no reference grammar uses — an FST is arguably *more* deterministic there than the engine |
| OOV guessing | guesser sublexicon (standard) or omit | linear in pattern inventory | No, but a product decision — closed vocabulary is structural |

**Explosion summary:** two theoretical cells (α-expansion O(v^k) per rule; cascade-composition
intermediates), both bounded in practice — one by schema+usage (k≈1–2), one by 40 years of
precedent + per-step minimization + per-language scoping. Everything else is linear or additive
*given flag diacritics*. Without flags, the analysis flips: constraint products are exponential —
which is precisely why "does the toolkit evaluate flags at lookup time" was verified at source
level (06b §3.3).

**The Amharic caveat, honestly:** the one precedent that abandoned classical FSTs for Semitic
(HornMorpho, feature-structure-weighted FSTs) solved a *harder* problem — lexicon-free
root-and-pattern guessing. With HC's closed lexicon, templatic Amharic is in AraComLex/HAMSAH
territory (both classical, both shipped). Amharic remains the mandatory stress test in the
verification compile.

---

## What this means, and the single remaining step

1. **The hybrid propose/verify architecture is not required by the problem.** Every function the
   verify pass performs maps to a standard mechanism with linear cost (flags) or a bounded local
   expansion (α), and the PoC already reproduced the engine exactly on the grammar exercising the
   hardest interactions.
2. **The off-the-shelf answer exists**: compile offline with HFST (GPL, offline-only) — lexc from
   HC XML (generated by our compiler, not hand-authored), twolc/xfst rules from HC rewrite rules,
   flags for gating, compile-replace for redup — emit `.hfstol`, ship it with a from-scratch C#
   reader (Apache-2.0 precedent, ~1.5–2k lines). foma is the fallback if compile-replace
   licensing/patent caution (06b §5.3) bites; per-root expansion also works at our lexicon sizes.
3. **The one open cell is empirical, not analytical**: real composition sizes on Indonesian
   (5 rules), Amharic (7 rules, 417 segments, α-rule), Sena (0 rules — lexc only). That is the
   final verification compile. Expected outcomes based on this analysis: Sena small and fast
   (its 10-second engine words become table lookups); Indonesian small; Amharic is the one to
   watch (α-expansion over feature-quotiented classes + huge alphabet).
4. **Parity discipline transfers**: the golden-TSV oracle machinery this repo already has is the
   right acceptance test for the C# compiler; the PoC demonstrated trace-diffing localizes
   semantic bugs (boundary transparency, α idioms) quickly.

### Budget outlook (analysis, to be confirmed by the verification compile)
- **<10 MB**: expected yes with room — our lexicons are 66–1,371 entries vs. 555k-lexeme
  precedents at 29 MB; determinized+minimized, gzipped `.hfstol` for grammars this size should be
  well under 1 MB. (The PoC's 12 MB number is a no-sharing enumeration artifact — see 04 §4.1.)
- **<1 ms/word**: expected yes by 2–3 orders of magnitude (measured format throughput
  119k–408k words/sec; 06b §3.3), including the current 100 ms redup words and Sena's 10 s words.
- **<5 s build**: for the *product* build, compilation happens offline per grammar; the shipped
  build compiles only the C# reader. Grammar-compile time itself: precedents suggest seconds to
  minutes per grammar (HAMSAH's 27 min is a 2M-state outlier 1,000× our lexicon size).
