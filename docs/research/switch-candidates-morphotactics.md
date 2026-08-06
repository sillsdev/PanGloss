# Candidate FST construction switches — the morphotactic half

Read-only research. No code was edited, no builds run, no git commands run. Scope: morphotactics and
word structure only (position classes, derivational depth and branching, compounding/incorporation,
circumfix/infix/affix ordering, clitics, lexicon scale and allomorphy shape, morphotactic gating).
The phonological/junction half is out of scope.

## The evidence rule this document is held to

Every candidate below carries hard evidence of exactly one of two kinds:

- **(a)** a real grammar in `samples/data/` that demonstrably needs it, cited to a specific construct
  count or measurement — re-derived here by direct inspection of the grammar files, not taken on
  trust from a prior report;
- **(b)** a published source (title, author, URL) describing a language family with the
  morphological characteristic that would require it.

Candidates with neither are in §4, "Speculative — no evidence found." That section is an output, not
a failure. Two candidates that a plausible-sounding catalogue would have included were moved there
after the grammars were actually checked and found not to exercise them.

**Where a candidate is already implemented in the shipped mainline, that is said first and counted as
the strongest evidence available** — an existing construction with a measured payoff outranks a
citation. `mainline-selection-audit.md` §A2/§A3 is the map, and this document cross-references its
S-numbers. Eleven of the seventeen (a)-backed candidates turn out to be shipping already, several
under a different name; six have a live trigger in a real grammar and no construction at all.

**Fixture rule.** Every proposed conformance grammar is **synthetic only** — no real-language data,
and no fixture named for a language or family. Family names appear below in prose, as motivation,
never as a fixture name. Every proposed name uses construct adjectives.

---

## 1. The measured evidence base

Everything in this table was read directly out of `samples/data/` for this document
(`amharic-hc.xml`, `indonesian-hc.xml`, `sena-hc.xml` parsed as XML; `aweti.fwdata`/`aweti.json`
scanned for FieldWorks class names). It is the (a)-evidence pool the catalogue draws on, and several
rows correct or sharpen a figure carried in an earlier report.

| Property | Amharic | Indonesian | Sena | Aweti |
|---|---|---|---|---|
| Lexical entries / root allomorphs | 76 / 77 | 66 / 66 | **1,371 / 1,463** | 1,022 / 995 stem allomorphs |
| Entries with >1 root allomorph | 1 | 0 | **86** (max 5) | — |
| `AffixTemplate` / `Slot` | 15 / 24 | **0 / 0** | **24 / 75** | 14 / 19 |
| Distinct template `requiredPartsOfSpeech` keys | 8 (of 15 templates) | — | 9 (of 24) | — |
| Morphological rules / subrules | 87 / 93 | 13 / 13 | 132 / 239 | 83 affix processes |
| Subrules with >1 subrule per rule | 5 rules | 0 | **69 rules** (61×2, 3×3, 5×4, 5×6) | — |
| Subrules carrying `RequiredEnvironments` | 0 / 93 | 0 / 13 | **5 / 239** | — |
| Multi-part `MorphologicalInput` subrules | **8** (5×2-part, 3×4-part) | 1 (2-part) | **0** (all 239 single-part) | — |
| Output shapes that interleave insert between copies | **4** | 4 | 0 | — |
| Circumfix-shaped rules (wrap both sides) | 0 | **3** (`Insert…Copy…Insert`) | 0 | — |
| Reduplication rules | 0 | **3** (one of them also circumfix-shaped) | 0 (hyphen artefacts only) | — |
| `partial="true"` morphological rules | 0 | **4 of 13** | 0 | — |
| `CompoundingRule` | 1 | 2 | **8** | 0 |
| Compounding rules whose output PoS re-enters their own head PoS | **1 (self-feeding)** | n/a (unrestricted PoS) | **3 self-feeding + 1 mutually-recursive pair** | — |
| Derivational PoS graph: self-loop rules / PoS on a cycle | 1 / 1 | **2 / 2** | 0 / 0 | — |
| Strata, and their declared rule order | 3, **all `unordered`** | 3, **all `unordered`** | 3, **all `unordered`** | 3 |
| Largest `unordered` stratum's loose-rule count | 22 | 15 | **25** | — |
| `requiredPartsOfSpeech` attributes | 95 | 12 | **151** | — |
| Stem names declared / referenced by an allomorph | 0 / 0 | 0 / 0 | 0 / 0 | **8 / 69** |
| MPR features / groups declared | 0 / 0 | 0 / 0 | 3 / 1 — **referenced by zero rules** | 0 exception features |
| `excludedMPRFeatures` on a subrule | 0 | 1 | 0 | — |
| Entries with a bound single-allomorph root | **36** | 0 | 1 | — |
| Boundary definitions | 1 table | 1 table | **3 kinds**, one a null-morph family `{^0,*0,&0,∅}` | 2 boundary markers |
| Affix allomorphs whose whole shape is boundary characters | 0 | 0 | **7** (all `^0+`) | — |
| Phonological rules | 7 | 5 | **0** | 18 |

Two corrections this pass produced, both worth carrying forward:

1. **`handspun-technique-audit.md` §2.7 records the bound-root compile-time discharge as "measured as
   a no-op on every real fixture… likely zero", because the search was run over
   `machine/conformance/` and `conformance-staging/` only and the Sena corpus was absent from that
   worktree.** In this worktree the corpora are present, and **Amharic declares 36 entries with a
   bound allomorph, every one of them single-allomorph** — i.e. exactly `never_valid_bare`'s
   trigger, 36 times, on a real reference grammar. Sena declares 1. The technique's real-world
   payoff on the four named grammars is **not** zero and can now be measured.
2. **Sena declares three MPR features and one `matchType="all"` MPR group, and no rule anywhere in
   the grammar references any of them.** Dead declaration — the same shape
   `mpr-overwrite-encoding-research.md` found for Overwrite groups. Any switch keyed on MPR gating
   therefore has **no** (a) evidence from the four grammars, only the declaration. Recorded honestly
   in §3, not promoted into the main catalogue.

---

## 2. The catalogue

### M1. Position-class slot chains vs. a flat continuation chain

1. **Construction difference.** With templates: one lexc slot-chain per `<AffixTemplate>`, each slot
   appearing exactly once in its chain (an optional slot contributes an epsilon skip), prefix slots
   reversed into surface order. Without: a single depth-bounded derivation layer over the loose rules
   with no positional constraint at all. These are different automata over the same affix inventory —
   the templated one forbids orders the flat one admits.
2. **Trigger.** `!grammar.templates.is_empty()` — `GrammarSemantics::declared_templates()`,
   **cheap, O(1)** on a loaded grammar. `template_count()` gives the magnitude.
3. **Hard evidence — (a), and already implemented.** Sena 24 templates / 75 slots; Amharic 15 / 24;
   Aweti 14 / 19; Indonesian **0 / 0**. The mainline builds this today
   (`emit.rs:1652-1746`), and the module doc records the measurement that forced it: the depth-N
   "bag" design it replaced "overgenerated by six orders of magnitude on real Sena words — 2.5M
   candidates for `mbali` vs the engine's 8" (`emit.rs:17-23`).
   `recipe_registry.rs`'s `complete-template` family declares the identical trigger
   (`Applicability::HasTemplates`).
4. **Families.** Bantu verbal extension ordering is the canonical templatic case — Hyman shows Bantu
   suffix order follows a Pan-Bantu default template (Causative-Applicative-Reciprocal-Passive) and
   is "largely templatic", explicitly *not* driven by semantic compositionality (Larry M. Hyman,
   "Suffix ordering in Bantu: a morphocentric approach", *Yearbook of Morphology 2002*,
   http://roa.rutgers.edu/files/506-0302/506-0302-HYMAN-0-0.PDF). Athabaskan verb templates are the
   other standard case, and are the one Rice argues *against* templates for, which is itself evidence
   that the templatic/compositional split is a real axis rather than a notational choice (Keren Rice,
   *Morpheme Order and Semantic Scope: Word Formation in the Athapaskan Verb*, Cambridge University
   Press 2000, https://www.cambridge.org/us/universitypress/subjects/languages-linguistics/semantics-and-pragmatics/morpheme-order-and-semantic-scope-word-formation-athapaskan-verb).
   The cross-domain typology of templates as fixed linear position structures is Jeff Good, "The
   Typology of Templates", *Language and Linguistics Compass* 5(10):731–747, 2011,
   https://compass.onlinelibrary.wiley.com/doi/abs/10.1111/j.1749-818X.2011.00306.x.
5. **Fixture.** `position-class-template-and-loose-affixation` — one synthetic grammar with two
   parts of speech: one whose affixes are declared in an ordered 4-slot template, one whose affixes
   are the same count of loose stratum rules with no template. The `words.yaml` must include, for the
   templated PoS, a **negative control** whose morphemes are all legal but in an order the template
   forbids. Nothing in the current corpus contrasts the two constructions inside one grammar.

---

### M2. Template grouping by shared required category

1. **Construction difference.** Templates whose `required_syn_fs` is the identical interned `FsId`
   share one emitted root section, and control joins a union of the group's suffix-slot chains after
   it; ungrouped, the root wiring is replicated once per template. The grouped form is an explicit
   *upward* approximation — a word can take group-mate template A's prefix slots with template B's
   suffix slots ("more paths than trie, never fewer", `emit.rs:56-58`) — traded against lexc size.
2. **Trigger.** Count of distinct `required_syn_fs` ids across `grammar.templates` vs. the template
   count — **cheap**, one pass over the template list (`emit.rs:2844`).
3. **Hard evidence — (a), already implemented.** Sena's 24 templates collapse to **9** groups
   (`emit.rs:53`). Independently re-derived here: Sena has 9 distinct template
   `requiredPartsOfSpeech` strings across 24 templates, one of which lists ten parts of speech at
   once. Amharic: 15 templates over **8** distinct keys (one key carries 6 templates, another 4).
   Indonesian is inert (zero templates). So the ratio is 2.7:1 on Sena and 1.9:1 on Amharic — the
   grouping is load-bearing on two of the four, not one.
4. **Families.** Bantu, where a single noun-class/PoS licenses many alternative extension templates
   (Hyman 2003, above); the general phenomenon of one category admitting several competing templates
   is Good 2011's "template as a fixed linear structure" applied per-category rather than per-word.
5. **Fixture.** `many-templates-one-category-sharing` — 6 templates, 2 categories (4 + 2), with the
   4-template group deliberately containing one template whose prefix slots and another whose suffix
   slots are individually legal but whose *combination* is not licensed by either template. The
   `words.yaml` then pins the approximation as a documented over-generation with an `expect_fail`
   confirm-side witness, rather than leaving it undescribed. `conformance-staging`'s existing
   `template-category-sharing` covers the 2-template case only.

---

### M3. Derivation-layer depth taken from the loose-rule count, not a constant

1. **Construction difference.** The standalone (non-template) derivation chain is unrolled to
   `max(rules_routed_to_this_side, DERIV_DEPTH_MIN=2)` levels rather than a fixed 2. Every extra
   level is a real lexc continuation class, so this is a linear-in-rule-count size change that buys
   deeper legal stacks.
2. **Trigger.** Per-stratum, per-side standalone rule count — **cheap**. The *soundness* of using
   rule count as the bound is contingent on `multipleApplication = 1` and is a human judgement, not a
   read (`emit.rs:24-32`); see M4 and the `max_apps` defect below.
3. **Hard evidence — (a), already implemented, and discovered by a failing recall gate.** Sena's
   corpus word `kubulukira` stacks three derivational suffixes and depth 2 silently loses it
   (`emit.rs:24-32`). Verified here against the grammar: Sena stratum 1 holds 17 loose derivational
   rules including `mrule11` (`-ul`, REV), `mrule17` (`-uk`, "separado") and `mrule18` (`-er`,
   APPLIC) — the exact three the note names — plus CAU, NEU, PAS, REC, AGN and two nominalizers. A
   17-rule stratum with a depth-2 chain is the failure.
4. **Families.** Bantu verbal extension stacking is the attested case and the source of the CARP
   ordering literature (Hyman 2003, above). Rice 2000 (above) is the standard treatment of how deep
   and how freely derivational morphemes stack in a polysynthetic verb.
5. **Fixture.** `derivational-stacking-beyond-default-depth` — one stratum with 5 loose derivational
   suffix rules and a `words.yaml` containing a 4-suffix stack. A grammar authored to fail under a
   hardcoded depth of 2 or 3 and pass only when the depth is derived from the rule count.

---

### M4. Cyclic vs. acyclic derivational category graph

1. **Construction difference.** Where the derivational rules' `requiredPartsOfSpeech →
   outputPartOfSpeech` relation is acyclic, the chain can be unrolled as a **layered DAG** whose
   depth is the graph's longest path — often far shorter than the rule count, and exact. Where it
   contains a cycle (including a self-loop), no finite longest path exists and the construction must
   fall back to a **budgeted unrolling** with an honest refusal above it. Today the mainline uses the
   rule count for both cases, so it over-builds the acyclic case and under-justifies the cyclic one.
2. **Trigger.** Cycle detection over the PoS graph induced by the morphological rules — **cheap**,
   O(rules + categories), and computed from data already loaded. Not currently computed anywhere.
3. **Hard evidence — (a), and NOT implemented.** Re-derived from the grammars: Indonesian's rule
   graph has **2 self-loop rules and 2 categories on a cycle** (a rule from category X to Y and
   another from Y back to X, plus a rule from X to itself). Amharic has 1 self-loop and 1 category on
   a cycle. **Sena's derivational graph is acyclic** (10 edges, 0 cycles) despite having by far the
   most rules — so on the largest grammar the rule-count bound (17 on one side) is being used where a
   longest-path bound would be much smaller. `mainline-selection-audit.md` §A6 defect 2 records the
   adjacent hazard: `preexpand.rs:570` and `emit.rs:2232` both enforce "a rule cannot appear twice in
   one chain" while *asserting* `multipleApplication = 1` without reading the field, which is
   recall-losing for a self-feeding rule that legitimately reapplies.
4. **Families.** Recursive derivation — a derivational affix whose output category re-enters its own
   input category — is the morphological analogue of the compounding recursion in M6 and is what
   makes polysynthetic derivational morphology formally unbounded (Michael Fortescue, Marianne
   Mithun and Nicholas Evans, eds., *The Oxford Handbook of Polysynthesis*, Oxford University Press
   2017, https://academic.oup.com/edited-volume/40413 — Fortescue's rule-of-thumb definition turns on
   the productive combination of "semantically heavy" derivational morphemes besides the root).
5. **Fixture.** `cyclic-derivational-category-graph` and its sibling
   `acyclic-derivational-category-layering` — two grammars with the *same* five derivational rules
   and the same lexicon, differing only in whether one rule's `outputPartOfSpeech` re-enters its own
   `requiredPartsOfSpeech`. The pair makes the two constructions' size and refusal behaviour directly
   comparable, which a single grammar cannot.

---

### M5. Outer (post-template) layer for an enclitic-like outer stratum

1. **Construction difference.** Every template path is additionally wired through `OuterPfx`/
   `OuterSfx` chains carrying the same rule sets as the inner derivation layers, so both
   `[root, ADD, IND]` and `[root, IND, ADD]` orders exist. Without it, a later-stratum rule can only
   attach *inside* the template, and a word whose true analysis puts it outside the template's final
   slot is never proposed.
2. **Trigger.** A stratum later than the template-bearing stratum that still carries loose
   morphological rules — **cheap** structural read of `grammar.strata`. Whether such a rule *must* be
   able to attach outside is a semantic fact about HermitCrab's own stratum ordering, not readable
   from the rule alone.
3. **Hard evidence — (a), already implemented, discovered by a recall gate.** Verified against the
   grammar: **Sena stratum 2 contains exactly two rules, `mrule139` named `=mbo` (gloss ADD) and
   `mrule140` named `=di` (gloss EVID)** — both named with the leading `=` that is the standard
   orthographic convention for an enclitic, and both isolated into their own stratum above the
   template-bearing stratum. `emit.rs:38-44` records that the C# trie placed all strata's standalone
   rules in the inner layer only, and that Sena's `=mbo` lands *after* the template's final-vowel
   suffix slot. Amharic and Indonesian have no rule-bearing stratum above their templates.
4. **Families.** The clitic/affix boundary is the classic diagnostic problem, and the criteria that
   make it decidable — degree of host selection, arbitrary gaps, morphological and semantic
   idiosyncrasy, and crucially "clitics can attach to material already containing clitics, but
   affixes cannot" — are Arnold M. Zwicky and Geoffrey K. Pullum, "Cliticization vs. Inflection:
   English N'T", *Language* 59(3):502–513, 1983, https://web.stanford.edu/~zwicky/ZPCliticsInfl.pdf.
   Criterion F is exactly the construction difference: an outer layer that may stack, versus a slot
   that may not.
5. **Fixture.** `outer-enclitic-stratum-after-template` — a templated grammar plus a second stratum
   with two clitic-like rules, and a `words.yaml` requiring **both** relative orders of the clitic
   and the template's final slot, plus one word carrying both clitics stacked (Zwicky & Pullum's
   criterion F) to force the outer layer to be re-entrant rather than one-shot.

---

### M6. Recursive and mutually-recursive compounding, with a computed depth cap

1. **Construction difference.** A bounded compound loop emits one `{base}{k}Roots` continuation class
   per level, restricted to the MPR/PoS-licensed non-head subset, for `max_depth - 1` levels —
   linear in emitted text, combinatorial in accepted language. The depth is computed
   (`1 + max_apps(r) + Σ max_apps(ancestors)`), not guessed, and refuses above
   `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET = 200` before any lexc is written.
2. **Trigger.** Presence of any `CompoundingRuleDef` — **cheap**, boolean. Recursion-*possibility* is
   a **cheap** rule-graph reachability test (does the rule's output PoS re-enter its own head/non-head
   input set). The depth bound is closed-form arithmetic on `multipleApplication`.
3. **Hard evidence — (a), and the mutually-recursive case is a genuine gap.** Re-derived here:
   **Amharic `mrule1` declares `headPartsOfSpeech`, `nonHeadPartsOfSpeech` and `outputPartOfSpeech`
   as the same single category** — a textbook directly self-feeding N+N→N rule. **Sena declares 8
   compounding rules, of which `mrule2`, `mrule4` and `mrule6` are directly self-feeding, and
   `mrule1`/`mrule2` form a mutually recursive pair** (mrule1's output category is mrule2's head
   category and vice versa). Indonesian declares 2 with no PoS restriction at all, which is
   recursion-possible vacuously. So **all four self-feeding configurations occur in real reference
   grammars**, and `compounding_max_depth`'s per-rule `1 + max_apps + Σ ancestors` sum does not model
   a *cycle across two rules* — Sena's shape — at all. `recursive-endocentric-compounding` (staged)
   covers the single-rule case only.
4. **Families.** The formal claim (an `N → N N` rule is "recursively applicable without limit") is
   Mark Lauer, *Designing Statistical Language Learners: Experiments on Noun Compounds*, PhD
   dissertation, Macquarie University, 1995, §2.2, https://arxiv.org/abs/cmp-lg/9609008. The attested
   depth distribution is sharply skewed but with long tails: Sanskrit is 94.1% at ≤3 members with a
   maximum of 16 (Amba Kulkarni and Anil Kumar, "Statistical Constituency Parser for Sanskrit
   Compounds", *ICON-2011*,
   https://sanskrit.uohyd.ac.in/faculty/amba/PUBLICATIONS/papers/samaasa_const_parser_icon2011.pdf;
   Jivnesh Sandhan et al., "DepNeCTI: Dependency-based Nested Compound Type Identification for
   Sanskrit", *Findings of the ACL: EMNLP 2023*, https://aclanthology.org/2023.findings-emnlp.914/).
   Germanic and Sinitic N-N stacking are the other commonly cited families.
5. **Fixture.** `mutually-recursive-compounding-cycle` — two compounding rules whose output
   categories feed each other's head slot, neither self-feeding on its own, each with
   `multipleApplication="1"`. Under the current per-rule sum each computes a depth of 2, while the
   pair can build arbitrarily deep. The `words.yaml` needs a 4-root word that only the cycle admits.

---

### M7. Multi-part morphological input (interdigitation) pre-expansion

1. **Construction difference.** A rule whose `MorphologicalInput` is split into several parts, with
   inserted material interleaved between copies of the parts, has no two-entry (root, then-continue)
   lexc encoding — there is no cuttable boundary. The construction instead replays the *real* engine
   (`pg_rules::morph::synthesize` + `probe_synthesize`) per (root, rule) pair and emits **one** lexc
   entry carrying both tags in the engine's own computed morph order. This is the difference between
   a concatenative encoding and an enumerated composite encoding.
2. **Trigger.** Any subrule whose `MorphologicalInput` has more than one part, or any allomorph
   classified `Role::Infix` — **cheap** structural read. Whether a given (root, rule) pair actually
   produces a non-literal surface is **expensive**: it requires actually running the real engine, and
   is not predictable from the rule's declaration (`preexpand.rs`, and `mainline-selection-audit.md`
   S4).
3. **Hard evidence — (a), already implemented.** Re-derived here: **Amharic has 8 subrules with a
   multi-part input — 5 with 2 parts and 3 with 4 parts** — and 4 output shapes that interleave an
   insert between copied parts, including `Copy Copy Insert Copy Copy` and
   `Copy Copy Insert Copy Insert Copy`. **Indonesian has 1** two-part-input subrule. **Sena has zero
   — all 239 of its subrules are single-part** and every output is a plain prefix or suffix. Measured
   payoff: Amharic emits 2,930 interdigitation composites; Indonesian probes 457 pairs and emits
   zero; Sena's `should_run` short-circuits before touching an entry (`preexpand.rs:53-60`). The
   trigger is therefore not merely present but *discriminating* across the four.
4. **Families.** Semitic root-and-pattern morphology is the canonical case; the general survey of
   interior insertion, its placement pivots and its typological distribution is Alan C. L. Yu, *A
   Natural History of Infixation* (Oxford Studies in Theoretical Linguistics 15), Oxford University
   Press 2007, https://global.oup.com/academic/product/a-natural-history-of-infixation-9780199279388
   — 154 infixation patterns from over a hundred languages.
5. **Fixture.** `multipart-input-interdigitation-depth` — a synthetic grammar with a 3-part
   consonantal-skeleton input and two vowel-melody rules that must interleave with it, plus a third
   ordinary suffix rule, so that a word requires an interdigitating rule *chained under* a
   concatenative one. Depth-1 interdigitation is already exercised; a chained case is not.

---

### M8. Circumfix as one indivisible unit

1. **Construction difference.** A circumfix's two halves are emitted as a single lexc entry with one
   tag and both text pieces bound to one path, so that neither half can be selected without the
   other. The alternative — two independent affix entries — admits half-circumfixed words the engine
   never produces.
2. **Trigger.** An allomorph RHS with both a leading and a trailing insert around a copy —
   `classify_affix`'s `CircumfixPrefix` test, **cheap**, O(|RHS|) per allomorph. The precedence of
   that test against competing classifications is M9.
3. **Hard evidence — (a), already implemented.** Re-derived here: **Indonesian declares 3
   circumfixing rules** — `mrule5` (`ke- -an2`), `mrule6` (`ke- -an1`) and `mrule8` (`peN- -an`) —
   and one subrule with the literal output shape `Insert Copy Insert`. Amharic and Sena declare none.
   `CircumfixOutputAction` is a `ConfigPredicate` characteristic in its own right
   (`capability.rs:131-135`, keyed on `allomorph_drops_lhs_material`), and
   `docs/conformance/circumfix-structural-composite-census.md` records that the census found the
   mechanism allomorph-complete and every gap in candidate *selection*.
4. **Families.** Circumfixation is contested as a primitive precisely because the alternative
   analysis (independent prefix + suffix) is always available, which is what makes the obligatory
   co-occurrence the load-bearing fact: Franc Lanko Marušič, "Circumfixation", *Wiley Blackwell
   Companion to Morphology*, https://www2.ung.si/~fmarusic/pub/marusic_2021_circumfixation_MorphCom.pdf.
   Austronesian (Malay/Indonesian `ke-…-an`, `peN-…-an`), Germanic (`ge-…-t`) and Nguni are the
   commonly cited families.
5. **Fixture.** `circumfix-halves-in-separate-template-slots` — a circumfix whose prefix half is
   declared in template slot 1 and whose suffix half is declared in slot 5, with three optional slots
   between them. This is the configuration where a slot-chain construction is most tempted to let the
   two halves vary independently, and no current fixture places a circumfix inside a template at all.

---

### M9. Circumfix × reduplication routing precedence

1. **Construction difference.** An allomorph RHS that is *both* circumfix-shaped and
   reduplication-shaped can be routed either to the runtime `ReduplicationPeeler` (four one-sided
   `O(word length)` scans) or to the `O(roots × rules^depth)` structural-composite enumeration. The
   peel cannot recall a wrap-both-sides shape — each of its four scans is one-sided — so the routing
   is a recall decision, not a cost decision.
2. **Trigger.** The order in which `classify_affix` tests `CircumfixPrefix` against
   reduplication-shape — **cheap**, but the *correct* order was found only by a conformance fixture.
   `mainline-selection-audit.md` records this as **S7**, one of only seven genuine strategy choices
   in the whole shipped compiler, and notes the first choice was wrong.
3. **Hard evidence — (a), already implemented.** Re-derived here: **Indonesian's `mrule15` is named
   `REDUP-meN` (gloss RECIP), carries `partial="true"`, and has the output shape
   `Copy Insert Insert Copy Insert Copy`** — a rule that is genuinely both reduplicating and
   wrapping, in a real reference grammar. `handspun-technique-audit.md` §2.19 records that the
   misroute for this shape was a real recall gap (unlike the sibling circumfix-infix case, where
   both builders call the same resynthesis and recall was never lost), closed by
   `circumfix-reduplication-precedence`.
4. **Families.** Austronesian reduplication interacting with affixation is the standard case; the
   boundedness split that decides whether copying is compilable at all is stated as a cross-language
   constraint in this repo's own harvest and rests on Yu 2007 (above) for the placement half.
5. **Fixture.** Already exists — `circumfix-reduplication-precedence` (staged). The remaining gap
   worth a new fixture is `circumfix-reduplication-under-template`: the same combined shape declared
   in a template slot rather than as a loose rule, since the routing decision and the slot-chain
   construction have never been exercised together.

---

### M10. Partial-rule gating ("this stem requires more derivation")

1. **Construction difference.** A rule marked `partial="true"` produces a stem that is not yet a legal
   word; a template must refuse to admit it, and the derivation chain must require at least one
   further rule before the accept state. The alternative construction lets the partial stem exit to
   `#`, admitting words the engine never accepts.
2. **Trigger.** `MorphologicalRule@partial` — **cheap**, a boolean attribute. `morphotactics.rs:359,590`
   already reads the entry-level analogue (`root_is_partial`) to decide that a root "can never enter
   any template, for the chain's whole life" (`mainline-selection-audit.md` §A3).
3. **Hard evidence — (a), partially implemented.** Re-derived here: **Indonesian marks 4 of its 13
   morphological rules `partial="true"`** — `mrule12` (`-nya`), `mrule13` (`-Pl`), `mrule14` (`meN`,
   the grammar's headline prefix) and `mrule15` (`REDUP-meN`). That is nearly a third of the rule
   inventory. Amharic and Sena mark none. The morphotactic index consumes the *entry*-level flag;
   whether the *rule*-level flag drives the derivation chain's accept states is a distinct question
   this document could not settle from the audits and should be checked before the switch is
   promoted.
4. **Families.** The inflection→derivation→inflection layering in which an inner layer must be
   incomplete "in a controlled way" before an outer layer may apply is the Quechuan case (David John
   Weber, *A Grammar of Huallaga (Huánuco) Quechua*, University of California Publications in
   Linguistics 112, 1989, https://escholarship.org/uc/item/85m6h0jn). The general point that affix
   order is constrained by morphological rather than semantic or phonological factors — one of the
   eight distinct approaches they enumerate — is Stela Manova and Mark Aronoff, "Modeling affix
   order", *Morphology* 20:109–131, 2010, https://link.springer.com/article/10.1007/s11525-010-9153-6.
5. **Fixture.** `partial-stem-requires-further-derivation` — a grammar where rule A is `partial`,
   rule B is not, and a `words.yaml` containing (i) root+A+B, which must parse, and (ii) root+A
   alone, an `expect_fail` negative control. Without (ii) the fixture proves nothing, because a
   construction that ignores the flag also passes (i).

---

### M11. Category-filtered root eligibility, and the arm that compounding kills

1. **Construction difference.** A template group's root section either admits only roots whose
   syntactic features unify with the group's `required_syn_fs`, or admits every root in the lexicon.
   On a large lexicon that is the difference between a partitioned root section and a single
   undifferentiated one.
2. **Trigger.** Today: `has_compounding_rules || permissive[gi] || key_fs.is_empty() ||
   is_unifiable(...)` (`emit.rs:3423-3426`) — **cheap**. `mainline-selection-audit.md` records this
   as **S2**, hardcoded, with `SurfaceRootScopePolicy` (`emit.rs:2555-2557`) sitting there as an
   already-threaded, single-variant enum that is literally the parameter for it.
3. **Hard evidence — (a), already implemented, and the tight arm is provably dead.** The comment at
   `emit.rs:3404-3422` justifies the broad arm by Sena's `musandilesera` (8 of 10 analyses recovered)
   and states "Every reference grammar has at least one compounding rule, so all three are
   broadened." Re-derived and confirmed here across all four: **Amharic 1, Indonesian 2, Sena 8
   compounding rules; Aweti 0.** So the tight arm is unreachable on the three HC grammars and
   reachable only on the one grammar the enumeration path cannot compile anyway. Meanwhile the filter
   it disables is not marginal: **Sena carries 151 `requiredPartsOfSpeech` attributes, Amharic 95,
   Indonesian 12**, over lexicons of 1,371 / 76 / 66 entries respectively.
4. **Families.** Lexically-selected inflection/declension classes gating which stems a given
   morphotactic slot may host — Zapotecan class hierarchies and Latin declensions are the standard
   cases; the general treatment of stems as lexically indexed objects selected by paradigm cell is
   Olivier Bonami and Gilles Boyé's stem-space work (Olivier Bonami, "Stem spaces and predictability
   in verbal inflection", http://www.llf.cnrs.fr/sites/llf.cnrs.fr/files/biblio/WSPisa_def.pdf).
5. **Fixture.** `category-filtered-roots-without-compounding` — a templated grammar with
   PoS-restricted templates, a lexicon spanning three categories, and **zero compounding rules**.
   This is the first artifact that would make S2's tight arm reachable and measurable at all; today
   no grammar in the corpus can select it. Pair it with
   `category-filtered-roots-with-compounding` (the same grammar plus one compounding rule) so the
   recall delta between the two arms is a single-variable measurement.

---

### M12. Unordered vs. linear stratum rule order, and the multiplicity budget

1. **Construction difference.** An `Unordered` stratum's loose rules may combine in any order, so the
   chain admits up to `n!`-shaped (or `n^d` under multi-application) orderings and must be unrolled
   or budgeted; a `Linear` stratum fixes document order and needs a single ordered chain. These are
   different automata, and the budget (`DEFAULT_ORDERING_MULTIPLICITY_BUDGET = 100`) exists only for
   the first.
2. **Trigger.** `StratumDef.morphologicalRuleOrder` plus that stratum's loose-rule count — **cheap**,
   both direct reads.
3. **Hard evidence — (a), already implemented, and the *other* arm is completely untested.**
   Re-derived here: **every rule-bearing stratum in all three HC grammars declares
   `morphologicalRuleOrder="unordered"` — Amharic 2 strata (14 and 22 rules), Indonesian 2 (15 and
   0), Sena 2 (25 and 2). Not one `linear` stratum exists in the reference corpus.** Sena's 25-rule
   stratum is named in `compose_budget.rs:283-295` as "the real ceiling" the budget of 100 was
   calibrated against, with ~4× headroom. So the budget is real-grammar-calibrated, and the
   `OrderedMorphRuleApplication` construction — which `capability.rs` grades **Proven** — has zero
   real-grammar exercise.
4. **Families.** That fixed and free affix order are two genuinely different regimes, requiring
   different explanations, is the framing question of the affix-ordering literature: Manova and
   Aronoff 2010 (above) enumerate eight competing approaches precisely because no single one accounts
   for both. Rice 2000 (above) argues that fixed order reflects a unilateral semantic dependency and
   variable order its absence — i.e. the two arms correspond to a real linguistic distinction, not a
   notational one.
5. **Fixture.** `linear-ordered-stratum-and-unordered-stratum` — one grammar with two strata over
   the same five rules, one declared `linear` and one `unordered`, and a `words.yaml` containing a
   morpheme sequence that is legal only under the unordered reading, as an `expect_fail` against the
   linear stratum. Without the negative witness a construction that ignores the attribute passes.

---

### M13. Lexicon scale × root-allomorph multiplicity

1. **Construction difference.** At reference scale a single shared root lexicon is fine; at 10³–10⁴
   entries the same construction's interaction with self-looping continuation classes, per-level
   compound root sections and `{name}Stripped` sibling lexicons multiplies the emitted text and, worse,
   the proposal count. The switch is between one root section reused everywhere and per-level /
   per-partition root sections sized against the entry count.
2. **Trigger.** `entry_count` and the root-allomorph histogram — **cheap** counts. What the counts
   *imply* for proposal volume is **not** cheap and is not predictable from them: it needed a real
   propose run to find.
3. **Hard evidence — (a).** Re-derived here: **Sena 1,371 entries / 1,463 root allomorphs, of which
   86 entries carry 2–5 allomorphs; Aweti 1,022 entries / 995 stem allomorphs; Amharic 76 / 77;
   Indonesian 66 / 66.** Sena is 18× Amharic and 21× Indonesian on entries. The measured consequence
   is on record: `large-lexicon-proposal-explosion.md` records a **425×** proposal blow-up on a
   5-word Sena slice (127 → 53,992 proposals; `mbali` alone 104 → 53,720, i.e. 516×), reduced to
   ~575 after the fix — a failure that exists *because* the lexicon is large enough for a
   zero-width self-loop to multiply against it. Neither Amharic nor Indonesian could have exposed it.
4. **Families.** Not a typological property; this is a scale property of any broad-coverage lexicon,
   and the repo's own standing rule (`build-for-full-scale-grammars`) targets 10⁴–10⁵ entries. The
   relevant published grounding is the lexc continuation-class model itself and its intended use for
   large agglutinative lexicons (Kenneth R. Beesley and Lauri Karttunen, *Finite State Morphology*,
   CSLI Publications 2003, https://www.bibliovault.org/BV.book.epl?ISBN=9781575864341).
5. **Fixture.** `large-lexicon-shared-continuation-scale` — a generated synthetic grammar with 10⁴
   roots over a small affix inventory, held to a proposal-count ceiling rather than only to recall.
   `pg-grammar-gen`'s `ScaleKnobs` already exists to produce exactly this and no conformance fixture
   currently uses it at scale. The fixture's assertion must be a *count*, not a pass/fail parse:
   the 425× bug passed recall throughout.

---

### M14. Stem-name-conditioned root/slot selection

1. **Construction difference.** A stem name partitions a lexeme's allomorphs into named stems, and a
   morphological rule may require a specific one. The construction is a per-stem-name root lexicon
   partition with slot chains that enter only the matching partition. Today there is **no admission
   filter at all** — `StemName` is `ConfirmOnly, permanent` (`capability.rs:314-321`) and
   `mainline-selection-audit.md` classifies it **INERT**: never observed by `characterize`. So the
   proposer emits every stem for every slot and confirm alone prunes.
2. **Trigger.** Any allomorph carrying a `stem_name` — **cheap**, a field read.
3. **Hard evidence — (a), and NOT implemented.** Re-derived here: **Aweti declares 8 `MoStemName`
   objects and 69 allomorph references to them**, across 995 stem allomorphs — the names divide verb
   stems from noun stems and regular from relational stems, and separately mark inflected vs.
   uninflected. **Amharic, Indonesian and Sena declare zero.** So the one grammar in the corpus that
   uses stem names is also the one grammar the enumeration path cannot compile, and the construction
   that would gate its 995 stem allomorphs down to the ~69 stem-name-relevant ones does not exist.
   `strategy_coverage.rs`'s mainline row nonetheless asserts `Represents` for `StemName` via a single
   blanket match arm covering all 22 kinds — a claim `capability.rs:170-175`'s own doc contradicts
   (`mainline-selection-audit.md` §C5 finding 2).
4. **Families.** Lexically indexed stem inventories selected by paradigm cell, independent of
   phonology and of any single morphosyntactic feature, are the "stem space" / morphomic-stem
   phenomenon: Bonami, "Stem spaces and predictability in verbal inflection"
   (http://www.llf.cnrs.fr/sites/llf.cnrs.fr/files/biblio/WSPisa_def.pdf), and the extension of a
   verbal stem space to lexeme formation (Olivier Bonami and Gilles Boyé, "Verbal stem space and verb
   to noun conversion in French", *Word Structure*, https://shs.hal.science/halshs-00746304/document).
   Romance conjugation is the best-documented family.
5. **Fixture.** `stem-name-selected-slot-entry` — a grammar with three stem names over a 40-root
   lexicon and two template slots, one of which requires a specific stem name. The `words.yaml` must
   include an `expect_fail` word combining the wrong stem with that slot; without it, an
   over-generating proposer plus a correct confirm passes and the switch is untestable at propose
   time — which is exactly the situation today.

---

### M15. Unconditioned vs. environment-conditioned affix allomorph sets

1. **Construction difference.** An affix rule with several subrules that differ only in inserted text
   is either (i) a genuine conditioned alternation, where each alternant's `RequiredEnvironments`
   restrict which roots it may follow and the lexc entry can be emitted per-environment, or (ii) an
   unconditioned set, where every alternant must be emitted unconditionally and the fan-out is the
   full product. These are different constructions and different proposal volumes.
2. **Trigger.** Per-rule subrule count, and whether each subrule carries a `RequiredEnvironments` —
   **cheap**, both structural.
3. **Hard evidence — (a), NOT implemented as a distinguishing switch.** Re-derived here: **Sena has
   69 morphological rules with more than one subrule — 61 with exactly 2, 3 with 3, 5 with 4 and 5
   with 6 — and only 5 of its 239 subrules carry any `RequiredEnvironments` at all.** The alternant
   pairs are vowel-height alternations (a mid-vowel and a high-vowel variant of the same suffix), and
   **Sena declares zero phonological rules**, so the alternation lives entirely in the lexicon as an
   unconditioned allomorph set. Amharic: 5 multi-subrule rules, 0 of 93 subrules environment-gated.
   Indonesian: 0 multi-subrule rules. This is the concrete grammar-side explanation for the finding
   the `dead-end-census` skill states as its motivating measurement — "Sena has 72 env constraints
   and zero rewrite rules, yet d1 was <2% and d5 dominated": the cost is unconditioned morphotactic
   fan-out, not phonology.
4. **Families.** That allomorph choice can be conditioned by phonological subcategorization rather
   than by a phonological rule — and that the two must not be conflated — is the subject of Mary E.
   Paster, *Phonological Conditions on Affixation*, PhD dissertation, UC Berkeley 2006,
   https://escholarship.org/uc/item/7tc6m7jw. Bantu extension harmony (the Sena shape) is one of her
   central case types.
5. **Fixture.** `unconditioned-allomorph-set-fanout` paired with
   `environment-conditioned-allomorph-set` — the same six-alternant suffix rule, once with and once
   without `RequiredEnvironments` on each alternant, over the same 30-root lexicon. The paired form
   is what makes the fan-out difference a single-variable measurement; a single fixture only shows
   that both parse.

---

### M16. Root suppletion — multi-allomorph lexical entries

1. **Construction difference.** A lexeme with several phonologically unrelated root allomorphs either
   gets one lexc entry per allomorph all sharing a tag (the disjunctive-recheck shape, where confirm
   must re-verify which allomorph was legal), or a partitioned entry keyed on whatever selects the
   allomorph. Today it is the former: `FreeFluctuation` is `ConfirmOnly, permanent`
   (`capability.rs:322-328`) and, per `mainline-selection-audit.md`, **INERT** — never observed.
2. **Trigger.** `LexEntryDef.allomorphs.len() > 1` — **cheap**, a count.
3. **Hard evidence — (a), no propose-side construction.** Re-derived here: **Sena has 86 entries with
   more than one root allomorph (83 with 2, one each with 3, 4 and 5)**; Amharic has 1; Indonesian 0;
   Aweti's 1,022 entries carry 995 stem allomorphs of which 69 are stem-name-distinguished (M14) and
   the remainder are plain alternants. Sena's 86 is small relative to its 1,371 entries but is the
   only real multi-allomorph population in the corpus, and it is the population the 425× proposal
   blow-up (M13) multiplied against.
4. **Families.** Suppletion is far more widespread than its reputation — a study of thirty languages
   found it resistant to paradigmatic levelling, preserved by frequency, inflectional category and
   the shape of the stem distribution: Andrew Hippisley, Marina Chumakina, Greville G. Corbett and
   Dunstan Brown, "Suppletion: frequency, categories and distribution of stems", *Studies in
   Language* 28(2), 2004, https://benjamins.com/catalog/sl.28.2.05hip; the underlying typological
   database is the Surrey Suppletion Database,
   https://www.smg.surrey.ac.uk/projects/suppletion/.
5. **Fixture.** `suppletive-root-allomorph-partition` — 20 lexemes each with 3 unrelated root
   allomorphs, where a template slot requires one specific allomorph per lexeme, so a construction
   that emits all three unconditionally over-proposes 3× and the count is the assertion. A fixture
   asserting only recall cannot distinguish the two constructions.

---

### M17. Zero-exponent (null-morph) affix handling

1. **Construction difference.** An affix allomorph whose entire underlying shape is boundary
   characters realizes a morpheme with no surface exponent. On a self-looping continuation class this
   degenerates to a zero-width, epsilon-tagged entry that may be taken arbitrarily many times without
   consuming input; the fix routes such a line off the self-loop onto a one-shot, non-reentrant
   successor while duplicating every *ordinary* line into a parallel `*NoNull` continuation so real
   affixes can still stack on both sides of the at-most-once null marker.
2. **Trigger.** An allomorph whose lower-tape text is entirely `Boundary`-kind characters — **cheap**,
   a scan of each emitted line against the char table's boundary definitions. The mainline avoids the
   hazard differently, by never putting boundary tokens on the queryable tape at all
   (`emit.rs:575`); the hazard is specific to constructions that do.
3. **Hard evidence — (a), implemented on one path with a named open scope gap.** Re-derived here:
   **Sena's character table declares three boundary kinds — an ordinary separator `+`, a null-morph
   family `{^0, *0, &0, ∅}`, and `.` — and exactly 7 affix allomorphs in the grammar have a phonetic
   shape composed entirely of boundary characters, all of them the identical string `^0+`.** Amharic
   and Indonesian have neither the null-morph family nor any such allomorph. The measured cost is the
   425×/516× blow-up of M13, and `build.rs:270-287` records that the shipped guard is name-based and
   therefore blind to the later-added compound-loop lexicons that recreate the identical hazard —
   "a name-based guard cannot defend a lexicon that did not exist when the guard was written."
4. **Families.** Zero exponence as a legitimate paradigm cell realizer, and the multiple-realization
   phenomena around it, are surveyed across more than 200 languages in Alice C. Harris, *Multiple
   Exponence*, Oxford University Press 2017, https://academic.oup.com/book/26649. Bantu noun-class
   prefixation — where a class marker may be segmentally null — is the family Sena's own analysis
   names for the trigger.
5. **Fixture.** `zero-exponent-affix-on-stacking-chain` — a grammar with one boundary-only affix
   allomorph plus two ordinary stackable affixes, and a `words.yaml` asserting an exact proposal
   *multiplicity* for a word in which the null morpheme and a real prefix can legitimately occur in
   either order. That multiplicity assertion is what caught the first, too-narrow version of the fix
   (`MultiplicityMismatch { word: "ps", expected: 3, actual: 2 }`, `build.rs:228-244`); a
   pass/fail parse assertion would not have.

---

## 3. Candidates with (b) evidence only — no grammar we have exercises them

These are real constructions with real published motivation, and nothing in `samples/data/` needs
them. They are listed separately so that nobody reads the catalogue above as the whole space.

### B1. Noun incorporation as a compounding configuration

**Construction difference.** An incorporating compound differs from an ordinary endocentric one in
that the non-head is drawn from an open lexical class and the output is a *verb*, so the non-head
root lexicon of the compound chain must be the full nominal lexicon rather than a small licensed
subset — an `N × V` cross-product rather than a bounded partition.
**Trigger.** A `CompoundingRuleDef` whose `nonHeadPartsOfSpeech` is a large open class and whose
`outputPartOfSpeech` differs from it — **cheap**, but the "large open class" half needs the lexicon
count, so it is a joint rule/lexicon read.
**Why (b) and not (a).** Verified: Aweti declares **zero** compounding rules; Amharic's single rule is
same-category N+N; Sena's eight are noun/pronoun combinations; Indonesian's two carry no category
restriction at all. None is an incorporation configuration.
**Families and source.** Marianne Mithun, "The Evolution of Noun Incorporation", *Language*
60(4):847–894, 1984, https://www.semanticscholar.org/paper/e3c06a38294283769d209a47c571293537cef818 —
the four-type implicational typology (lexical compounding, case manipulation, discourse manipulation,
classificatory incorporation), attested across Iroquoian, Eskimo-Aleut, Caddoan and Oceanic. Type IV
(classificatory) is the configuration that most stresses a compound-chain construction, because the
incorporated generic and the external specific co-occur.
**Fixture.** `classificatory-incorporating-compound` — a compounding rule taking a 200-entry open
nominal class as non-head into a 10-entry verbal head class, with a `words.yaml` word in which the
incorporated generic root and a separate external modifier both appear.

### B2. Prosodically pivoted infixation

**Construction difference.** An infix whose position is defined relative to a *prosodic* pivot (after
the first consonant, before the final syllable) rather than relative to a declared multi-part input
cannot be pre-expanded per (root, rule) pair the way M7's interdigitation is, because the insertion
point is a function of the root's own shape, not of the rule's declared parts. It needs either a
per-root probe or a pivot-computing relation.
**Trigger.** A single-part-input rule with an interior insert — distinguishable from M7's multi-part
case by `MorphologicalInput` part count, **cheap**.
**Why (b) and not (a).** Amharic's infixation is declared as multi-part input (5 two-part and 3
four-part subrules, verified above), i.e. the template supplies the pivot. No grammar in
`samples/data/` declares a single-part input with an interior insert.
**Families and source.** Alan C. L. Yu, *A Natural History of Infixation*, Oxford University Press
2007, https://global.oup.com/academic/product/a-natural-history-of-infixation-9780199279388 — the
Pivot Theory chapter argues infix positions are drawn from a small set of phonologically defined
edge/prominence pivots; Austronesian (Tagalog `-um-`) and Austroasiatic are the most cited families.
**Fixture.** `prosodic-pivot-infixation` — an infix rule whose environment places it after the first
consonant of a single-part root, over a lexicon whose roots have three different onset shapes.

### B3. Non-adjacent slot co-occurrence constraints

**Construction difference.** A dependency between two non-adjacent template slots (slot 1's filler
requires a particular slot 5 filler) cannot be expressed by a slot chain at all — the chain has no
memory between them. It needs either a split into complete template alternatives, or a co-occurrence
filter composed onto the chain.
**Trigger.** `MorphemeCoOccurrenceRuleDef` / `AllomorphCoOccurrenceRuleDef` presence — **cheap**,
boolean.
**Why (b) and not (a).** Verified: **zero** `MorphemeCoOccurrenceRule` and zero
`AllomorphCoOccurrenceRule` in Amharic, Indonesian and Sena; Aweti's `adhocProhibitions` list is
empty. `CoOccurrenceConstraint` is `ConfirmOnly, permanent`, and there is nothing in the corpus to
test it against.
**Families and source.** That morphological (as opposed to semantic, syntactic or phonological)
co-occurrence restrictions are a distinct and necessary mechanism in affix ordering is one of the
eight approaches enumerated by Stela Manova and Mark Aronoff, "Modeling affix order", *Morphology*
20:109–131, 2010, https://link.springer.com/article/10.1007/s11525-010-9153-6; Good 2011 (above)
treats the same problem as the limit of what a strictly linear template can express. Uto-Aztecan
number marking (an ambiguous person prefix disambiguated only by an obligatory non-adjacent plural
suffix) is the case this repo's own harvest names.
**Fixture.** `discontinuous-slot-cooccurrence-requirement` — a 5-slot template where a specific slot-1
filler requires a specific slot-5 filler, with an `expect_fail` word supplying the first without the
second. `prefixal-discontinuous-slot-dependency` covers the *dependency* but not the
`MorphemeCoOccurrenceRule` mechanism.

### B4. MPR-feature gating of compounding member eligibility

**Construction difference.** `compound_license` computes head/non-head eligible lexicon subsets by
bitset overlap against each compounding rule's MPR gates, producing partitioned per-level root
sections instead of one. Where the gate is vacuous the partition is the whole lexicon.
**Trigger.** A compounding rule referencing MPR features — **cheap**.
**Why (b) and not (a).** Verified: **Sena declares three MPR features and one `matchType="all"` group
and no rule in the grammar references any of them** — dead declaration. Amharic, Indonesian and Aweti
declare none at all. Indonesian's one `excludedMPRFeatures` is on a *phonological* subrule, not a
compounding rule, so it belongs to the phonological half of this survey. The mechanism is
implemented (`emit.rs:1158-1200`) and has no real trigger anywhere in the corpus.
**Families and source.** Compound-member exception features have OR-like semantics unlike the AND-like
semantics of affix exception features, so one generic Boolean gate is wrong — this is the point
Manova and Aronoff 2010 (above) make generally about morphological conditioning being its own axis,
and that Paster 2006 (above) makes about lexically diacritic conditioning being distinct from
phonological conditioning.
**Fixture.** `mpr-gated-compound-member-eligibility` — two compounding rules over one lexicon,
distinguished only by which MPR feature they require of the non-head, with a `words.yaml` asserting
the *count* of licensed non-heads rather than a single parse.

### B5. Deep binary-nested compounding beyond the default stem count

**Construction difference.** The compound chain unrolls a flat sequence of non-head levels. A deeply
nested compound is analysed in the literature as a *binary tree*, and the number of groupings of an
n-member compound is the Catalan number — so a flat chain that admits only the left-nested reading
under-generates, and one that admits all groupings explodes.
**Trigger.** The computed `compounding_max_depth` exceeding some threshold — **cheap** arithmetic.
**Why (b) and not (a).** None of the four grammars sets `multipleApplication` on a compounding rule,
so every computed depth is the default; and the reference engine's own `Morpher.MaxStemCount`
defaults to 2. The deep case is real in the world and absent here.
**Families and source.** Sanskrit attests up to 16 members with 94.1% at ≤3 (Kulkarni and Kumar 2011,
https://sanskrit.uohyd.ac.in/faculty/amba/PUBLICATIONS/papers/samaasa_const_parser_icon2011.pdf;
Sandhan et al. 2023, https://aclanthology.org/2023.findings-emnlp.914/); English three-noun compounds
branch 67% left / 33% right, i.e. genuinely branching rather than a single chain shape (David Vadas
and James R. Curran, "Parsing Noun Phrases in the Penn Treebank", *Computational Linguistics*
37(4):753–806, 2011, https://aclanthology.org/J11-4006/, which also reports that 23,129 of 60,959
ambiguous Penn Treebank NPs required brackets to be inserted when the originally flat annotation was
undone).
**Fixture.** `binary-nested-compounding-depth-four` — a self-feeding compounding rule with
`multipleApplication="3"` and a `words.yaml` containing a 4-root word with *two distinct* correct
bracketings, so the fixture measures whether both groupings are proposed rather than only the
left-nested one. Note the honest scoping: HermitCrab's own analysis output may not distinguish the
two bracketings, in which case this fixture demotes to a depth test and the branching half is
unrepresentable — that should be checked before authoring.

---

## 4. Speculative — no evidence found

Listed because they are the plausible-sounding candidates a catalogue like this attracts, and because
saying "no evidence" is the useful output.

1. **Splitting one template into disjoint sub-templates by mutually exclusive slot sets.** The
   motivating story (an ambiguous prefix disambiguated only by an obligatory suffix, forcing separate
   singular and plural templates) reaches this repo only through a secondary digest of a single
   pedagogical guide; no grammar in `samples/data/` has a template pair in that relation, and I could
   not verify the primary source. Related to B3 but a different construction (template split vs.
   co-occurrence filter). **No (a), no verified (b).**
2. **Reordering slot chains or continuation classes by observed slot-fill frequency.** A plausible
   constant-factor win on `apply_up` traversal. No linguistic evidence bears on it, and the one
   directly relevant measurement points the other way: the arc-sort threshold (S3) is a two-point
   interpolation that made Sena 1.49× and Amharic 2.05× faster but Indonesian **~30% slower**, with
   no recorded noise margin. Frequency ordering would need its own measurement, not a citation.
3. **Interning identical slot chains across templates.** Pure engineering (content-addressed dedup
   already exists for plan nodes). No morphological property triggers it; whether it pays is a lexc
   size question. Not a switch.
4. **Lazy or query-prefix-driven template expansion.** Would change when the chain is built, not what
   it accepts. No evidence that any grammar's compile time is dominated by template emission —
   Amharic's dominant emit cost is measured to be composite pre-expansion, not slot chains.
5. **A separate construction for subtractive/truncating morphotactics.** Tempting because Aweti was
   thought to need one, and the premise was **refuted by measurement**: `p6-deep-truncation-chain-report.md`
   and `synthetic-stress-grammar-plan.md` record that Aweti's 41 flagged "truncation" mrules are
   floating-consonant *realization*, not truncation, and that the templated path reaches its result
   without a dedicated truncation cascade. Listed here rather than in §2 precisely because the
   evidence that existed turned out to be against it.
6. **A "polysynthesis" umbrella switch.** Fortescue et al. 2017 (cited in M4) is a real source for the
   *phenomenon*, but polysynthesis decomposes into axes already catalogued separately here —
   derivational depth (M3/M4), incorporation (B1), templatic position classes (M1). No evidence that
   the bundle needs a construction distinct from its parts, and `grammar-feature-space.md` §3.5
   already rejected the analogous "non-concatenative" umbrella on the same grounds.

---

## 5. Tally

- **17 candidates with (a) evidence** — a real grammar in `samples/data/` demonstrably exercises the
  trigger: M1–M17. Of these:
  - **11 are already implemented in the shipped mainline** — M1, M2, M3, M5, M6 (single-rule
    recursion only; the mutually-recursive shape is a gap), M7, M8, M9, M11 (with its tight arm
    unreachable), M12 (the `unordered` arm only), M17 (on one path, with a named scope gap).
  - **6 have a real, live trigger and no propose-side construction** — M4 (cyclic-vs-acyclic
    category layering), M10 (rule-level `partial` gating, entry-level only today), M13
    (scale-sized root sections), M14 (stem names — `ConfirmOnly` and inert), M15
    (conditioned vs. unconditioned allomorph sets), M16 (root suppletion — emitted
    unconditionally by design).
- **5 candidates with (b) evidence only** — a published source describes a family requiring it and no
  grammar we have exercises it: B1–B5.
- **6 candidates with no evidence of either kind**, in §4.

Three findings worth surfacing beyond the catalogue: the bound-root discharge is **not** a no-op on a
real grammar (Amharic, 36 entries) contrary to the audit's record; **no reference grammar declares a
`linear` stratum**, so a `Proven` construction has zero real exercise; and **Sena's MPR features are
declared but referenced by nothing**, so the compound-license mechanism has no live trigger anywhere.

---

## Sources

Repo evidence (read directly for this document): `samples/data/amharic-hc.xml`,
`samples/data/indonesian-hc.xml`, `samples/data/sena-hc.xml`, `samples/data/aweti.fwdata`,
`samples/data/aweti.json`; `docs/research/handspun-technique-audit.md`,
`docs/research/grammar-feature-space.md`, `docs/research/mainline-selection-audit.md`,
`docs/research/per-language-fst-synthesis.md`; `docs/fst-plan/linguistic-recipe-harvest.md`,
`docs/fst-plan/large-lexicon-proposal-explosion.md`,
`docs/fst-plan/morphotactic-composite-pruning.md`,
`docs/fst-plan/p6-deep-truncation-chain-report.md`,
`docs/fst-plan/synthetic-stress-grammar-plan.md`;
`docs/conformance/representative-typology-basis.md`.

Published sources cited above:

- Beesley, Kenneth R., and Lauri Karttunen. 2003. *Finite State Morphology.* CSLI Publications. https://www.bibliovault.org/BV.book.epl?ISBN=9781575864341
- Bonami, Olivier. "Stem spaces and predictability in verbal inflection." http://www.llf.cnrs.fr/sites/llf.cnrs.fr/files/biblio/WSPisa_def.pdf
- Bonami, Olivier, and Gilles Boyé. "Verbal stem space and verb to noun conversion in French." *Word Structure.* https://shs.hal.science/halshs-00746304/document
- Fortescue, Michael, Marianne Mithun, and Nicholas Evans, eds. 2017. *The Oxford Handbook of Polysynthesis.* Oxford University Press. https://academic.oup.com/edited-volume/40413
- Good, Jeff. 2011. "The Typology of Templates." *Language and Linguistics Compass* 5(10):731–747. https://compass.onlinelibrary.wiley.com/doi/abs/10.1111/j.1749-818X.2011.00306.x
- Harris, Alice C. 2017. *Multiple Exponence.* Oxford University Press. https://academic.oup.com/book/26649
- Hippisley, Andrew, Marina Chumakina, Greville G. Corbett, and Dunstan Brown. 2004. "Suppletion: frequency, categories and distribution of stems." *Studies in Language* 28(2). https://benjamins.com/catalog/sl.28.2.05hip
- Hyman, Larry M. 2003. "Suffix ordering in Bantu: a morphocentric approach." *Yearbook of Morphology 2002.* http://roa.rutgers.edu/files/506-0302/506-0302-HYMAN-0-0.PDF
- Kulkarni, Amba, and Anil Kumar. 2011. "Statistical Constituency Parser for Sanskrit Compounds." *ICON-2011.* https://sanskrit.uohyd.ac.in/faculty/amba/PUBLICATIONS/papers/samaasa_const_parser_icon2011.pdf
- Lauer, Mark. 1995. *Designing Statistical Language Learners: Experiments on Noun Compounds.* PhD dissertation, Macquarie University. https://arxiv.org/abs/cmp-lg/9609008
- Manova, Stela, and Mark Aronoff. 2010. "Modeling affix order." *Morphology* 20:109–131. https://link.springer.com/article/10.1007/s11525-010-9153-6
- Marušič, Franc Lanko. "Circumfixation." *Wiley Blackwell Companion to Morphology.* https://www2.ung.si/~fmarusic/pub/marusic_2021_circumfixation_MorphCom.pdf
- Mithun, Marianne. 1984. "The Evolution of Noun Incorporation." *Language* 60(4):847–894. https://www.semanticscholar.org/paper/e3c06a38294283769d209a47c571293537cef818
- Paster, Mary E. 2006. *Phonological Conditions on Affixation.* PhD dissertation, UC Berkeley. https://escholarship.org/uc/item/7tc6m7jw
- Rice, Keren. 2000. *Morpheme Order and Semantic Scope: Word Formation in the Athapaskan Verb.* Cambridge University Press. https://www.cambridge.org/us/universitypress/subjects/languages-linguistics/semantics-and-pragmatics/morpheme-order-and-semantic-scope-word-formation-athapaskan-verb
- Sandhan, Jivnesh, et al. 2023. "DepNeCTI: Dependency-based Nested Compound Type Identification for Sanskrit." *Findings of the ACL: EMNLP 2023.* https://aclanthology.org/2023.findings-emnlp.914/
- Surrey Morphology Group. *The Surrey Suppletion Database.* https://www.smg.surrey.ac.uk/projects/suppletion/
- Vadas, David, and James R. Curran. 2011. "Parsing Noun Phrases in the Penn Treebank." *Computational Linguistics* 37(4):753–806. https://aclanthology.org/J11-4006/
- Weber, David John. 1989. *A Grammar of Huallaga (Huánuco) Quechua.* University of California Publications in Linguistics 112. https://escholarship.org/uc/item/85m6h0jn
- Yu, Alan C. L. 2007. *A Natural History of Infixation.* Oxford University Press. https://global.oup.com/academic/product/a-natural-history-of-infixation-9780199279388
- Zwicky, Arnold M., and Geoffrey K. Pullum. 1983. "Cliticization vs. Inflection: English N'T." *Language* 59(3):502–513. https://web.stanford.edu/~zwicky/ZPCliticsInfl.pdf

**Verification caveats, recorded rather than glossed.** The Weber 1989 eScholarship identifier and the
Mithun 1984 landing page were located via search index rather than fetched in full; both should be
confirmed before being quoted verbatim. Good's *Language and Linguistics Compass* article is cited
from its publisher landing page; a preprint of the same material is at
https://www.acsu.buffalo.edu/~jcgood/jcgood-TemplateTypology.pdf but could not be text-extracted
here. Marušič's circumfixation chapter is cited from the author's own hosted copy; the *Wiley
Blackwell Companion to Morphology* volume/page numbers were not verified.
