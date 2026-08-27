# Report 3: What is the input space?

Scope: characterise the space of HermitCrab grammars this system must handle, and find the axes
along which grammars genuinely differ in ways that should change how the FST is built. Read-only
research; no code or docs changed as part of producing this report.

All fixture content cited below is synthetic per this repo's hard rule (`CLAUDE.md`'s "Synthetic
conformance only" memory entry); language-family names appear only where the source material itself
uses them in prose/comments, never as a claim about fixture data.

---

## 1. The construct inventory

Three vocabularies have to be reconciled, and they disagree in informative ways:

- **`pg-grammar/src/model.rs`** — everything the loader can parse into a `Grammar` at all.
- **`pg-foma/src/capability.rs`'s `CharacteristicKind`** (22 variants, `CharacteristicKind::ALL`,
  `capability.rs:201-224`) — everything the FST compiler has an opinion about, each with a
  `Disposition` (`capability.rs:70-81`): `Proven` (admission-filtering allowed, no predicate
  needed), `ConfigPredicate` (a registered predicate decides `Admit`/`ConfirmOnly`/`Refuse` per
  grammar), or `ConfirmOnly` (compiles, but no proven no-false-negative admission filter exists —
  the FST over-proposes and HermitCrab confirm cleans up).
- **`machine/conformance/constructs.txt`** — 29 rows (lines 19-47; the doc's own header calls it a
  "25-row checklist" because 4 rows — `RewriteRule direction: left-to-right/right-to-left`,
  `RewriteSubruleDef gating`, `CharacterDefinitionTable: more than one table` — were added later,
  "G9" in `pg-foma/src/conformance_coverage.rs:165-174`) — the conformance harness's own
  coverage key, cross-referenced against every fixture's `exercises:` tags.

### 1.1 Model constructs → characteristic → disposition → conformance status

| `model.rs` construct | `CharacteristicKind` | Disposition (default) | `constructs.txt` row | Exercised by a fixture? |
|---|---|---|---|---|
| `MorphRuleDef::AffixProcess` (model.rs:554) | `Affixation` | **Proven** | `AffixProcessRule: prefix/suffix/circumfix/infix` + `.../subtraction/truncation` (folded in, `conformance_coverage.rs:154-163`) | Yes — most fixtures |
| `MorphRuleDef::Realizational` (model.rs:557, `RealizationalRuleDef` model.rs:611) | `RealizationalMorphology` | **ConfirmOnly, permanent** (`capability.rs:232`) | `RealizationalAffixProcessRule` | Yes — `fusional-realizational-morphology` |
| `MorphRuleDef::Compounding` (model.rs:555, `CompoundingRuleDef` model.rs:713) | `Compounding` | **ConfigPredicate** → `ConfirmOnly` unconditionally today (`CompoundingRecursionSafePredicate`, `capability.rs:233-245`) | `CompoundingRule` (+ constraints row, orphaned — see below) | Yes; recursive-self-feeding shape closed 2026-07-26 (`conformance-staging/edge-cases/recursive-endocentric-compounding`) |
| `MorphRuleOrder::Linear`/`Unordered` (model.rs:1068-1071) | `OrderedMorphRuleApplication` (**Proven**) / `UnorderedMorphRuleApplication` (**ConfigPredicate**, `ConfirmOnly` bounded / `Refuse` unbounded, `capability.rs:246-258`) | — | `Stratum (Linear/Unordered rule order)` | Yes — all 8 `languages/` fixtures |
| `RewriteMode::Iterative`/`Simultaneous` (model.rs:386-388) | `IterativeRewrite` (**Proven**) / `SimultaneousRewrite` (**ConfigPredicate**) | — | `RewriteRule Iterative (...)` / `RewriteRule Simultaneous` | Yes both |
| `Dir::LeftToRight`/`RightToLeft` (model.rs:392-394) | `LeftToRightRewrite` (**Proven**) / `RightToLeftRewrite` (**ConfigPredicate**, `capability.rs:264-272`) | — | direction rows (G9-added) | Yes — `right-to-left-anchor-environment` and several `right-to-left-*` staged fixtures |
| `PhonRuleDef::Metathesis` (model.rs:405, `MetathesisRuleDef` model.rs:466) | `Metathesis` | **ConfigPredicate** (`ConfirmOnly` for in-scope shapes, `capability.rs:273-279`) | `MetathesisRule` | Yes — `metathesis-phase-isolation` (LTR); RTL closed in staging |
| empty-LHS `RewriteRuleDef` (model.rs:417's own doc) | `Epenthesis` | **ConfigPredicate** | folded into the Iterative row | Yes |
| `RewriteSubruleDef` gating (model.rs:423-427) | `SubruleGating` | **Proven** (`gate.rs`'s partition, `capability.rs:281`) | G9-added row | Yes — `subrule-morphosyntactic-gating` |
| Circumfix-shaped `OutputAction` drop (model.rs:696-709) | `CircumfixOutputAction` | **ConfigPredicate** | folded into affixation row | Yes — multiple |
| `AffixAllomorphDef::redup_hint` + true copy-≥2 RHS (model.rs:682,690-694) | `Reduplication` | **ConfigPredicate** (peel-eligible only for `AffixProcess`, never `RealizationalRule` — a permanent carve-out, `capability.rs:283-294`) | `AffixProcessRule: reduplication (ReduplicationHint)` | Yes |
| `MorphemeCoOccurrenceRuleDef`/`AllomorphCoOccurrenceRuleDef` (model.rs:527-550) | `CoOccurrenceConstraint` | **ConfirmOnly, permanent** | `MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule` | Yes |
| `NaturalClassKind::Feature`/`Segments` (model.rs:361-367) | `NaturalClassDefinition` | **Proven** (representational only) | `NaturalClass: ...precision` | Yes |
| `Grammar::char_tables.len() > 1` (model.rs:1111) | `MultiTable` | **ConfigPredicate** | G9-added row | Yes — 2 disjoint-representation fixtures + 1 shared-representation fixture in staging |
| `PatternNode::Quantifier` (model.rs:298-304) | `QuantifierPattern` | **ConfigPredicate** (bounded and now genuinely-unbounded both compile, `capability.rs:304-313`) | pattern-shapes row | Yes — `loader-pattern-shapes` (bounded), `unbounded-iterative-quantifier-expansion` (genuinely unbounded, closed 2026-07-26) |
| `RootAllomorphDef::stem_name` (model.rs:798) | `StemName` | **ConfirmOnly, permanent** — no admission filter exists at all (`capability.rs:314-321`) | `Stem names` | Yes — `templatic-root-modification`, `suffixing-extension-slot-ordering` |
| Multi-allomorph `LexEntryDef` disjunctive re-check (model.rs:777) | `FreeFluctuation` | **ConfirmOnly, permanent** (`capability.rs:322-328`) | `Disjunctive allomorphs / free-fluctuation` | Yes — `disjunctive-recheck`, `template-category-sharing` |
| `MprGroupOutput::Append`/`Overwrite` (model.rs:842-845) | `MprGroupAppend` (**ConfirmOnly**) / `MprGroupOverwrite` (**ConfigPredicate**, `FailClosed` unconditionally — no safe superset construction exists even in principle for history-dependent overwrite semantics; see `mpr_add_output`'s doc, model.rs:926-945) | `MPR features/groups` | Yes both |

Three rows in `constructs.txt` map to **no** `CharacteristicKind` at all by deliberate, documented
choice (`Unmappable` would be the status if evidence were checked, but there is nothing to check —
`conformance_coverage.rs:139-152`): `LeftToRightRewrite`/`RightToLeftRewrite` used to be two of
these before G9 added their rows; `SubruleGating` likewise. After G9, `zero_unmappable_after_g9`
(`conformance_coverage.rs:551-571`) asserts every `CharacteristicKind` maps to at least one row.

Ten `constructs.txt` rows map to **no** `CharacteristicKind` in the other direction — real,
conformance-worthy phenomena the FST-capability ledger simply does not characterize as their own
axis (`ORPHAN_CONSTRUCT_ROWS`, `conformance_coverage.rs:241-309`), each with a stated reason:
`MorphologicalOutputAction` primitives (folded into whichever rule characteristic uses them),
"Affix template slots" (folded into `Affixation`/`OrderedMorphRuleApplication`), "Boundary
markers" (representational), `Guesser/LexicalGuess` (a runtime heuristic, not a compiled-FST
capability — **outside this crate's characterization scope entirely**), syntactic feature
agreement (assumed-faithful confirm-time unification), alpha-variable environments (folded into
Iterative/Simultaneous), the two constraint-configuration rows for Compounding/Ordinary rules
(sub-configurations of an already-characterized construct), and `Tracing` (not a linguistic
phenomenon at all — an engine debug feature, confirmed by `docs/conformance/
representative-typology-basis.md` §1.4 as the one row with **no typological basis of any kind**,
cited by zero fixtures anywhere).

**Refused constructs**, i.e. where the compiler currently has no construction at all: none of the
22 `CharacteristicKind`s default to unconditional `Refuse` today. `UnorderedMorphRuleApplication`
and `Compounding`'s recursive configuration both carry a `Refuse` *tail* for the genuinely
unbounded case (a stratum's loose-rule count over `DEFAULT_ORDERING_MULTIPLICITY_BUDGET`, or a
`multipleApplication` count that would blow `crate::emit::DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET`
(200) — checked *before* any lexc text is written, never a hang or OOM). `MprGroupOverwrite` is
the one construct that is `FailClosed` unconditionally, by structural argument, not by resource
budget.

---

## 2. The conformance corpus as it stands

Three tiers, in increasing distance from "ships as an upstream `sillsdev/machine` PR":

- **`machine/conformance/languages/`** (8 fixtures) — the canonical, graduated grammars, one per
  construct bundle: `fusional-realizational-morphology`, `metathesis-phase-isolation`,
  `polysynthetic-stratal-derivation-chain`, `prefixal-discontinuous-slot-dependency`,
  `suffixing-evidential-adjacency-chain`, `suffixing-extension-slot-ordering`,
  `suffixing-vowel-harmony`, `templatic-root-modification`.
- **`machine/conformance/edge-cases/`** (13 fixtures) — narrower, single-construct probes:
  `bistratal-overlapping-segment-representation`, `deep-optional-affix-nesting`,
  `diacritic-segments`, `disjunctive-recheck`, `loader-default-symbol`, `loader-isactive`,
  `loader-pattern-shapes`, `mpr-gated-exception`, `right-to-left-anchor-environment`,
  `simultaneous-epenthesis-cascade`, `strrep-identity`, `subrule-morphosyntactic-gating`,
  `truncate-morphotactic`.
- **`conformance-staging/edge-cases/`** (24 fixtures) — this repo's own pre-graduation staging
  area (`conformance-staging` skill), including the recipe-optimizer's four synthetic plan-shape
  fixtures (`recipe-gated/ordered/strata/template-generic` — explicitly **not** the "four
  languages" of §4 below, a naming collision `four-grammar-recipe-evidence-2026-07-28.md:3-10`
  flags directly) and a dozen construct-gap fixtures authored against
  `docs/conformance/representative-typology-basis.md` (recursive/self-feeding compounding, RTL
  metathesis, unbounded quantifier, subrule gating, shared-representation multi-table, circumfix ×
  reduplication precedence, etc.).

### 2.1 Gaps: constructs with no fixture

`representative-typology-basis.md` §1.2 is the authoritative, dated gap list (method: it checked
every `exercises:` tag actually present, not inferred, §1's own "Method"). As of its last update,
every gap it identified has a tracked outcome (§2, "The real gap list"):

| Gap | Status |
|---|---|
| Compounding, recursive/self-feeding | **Closed 2026-07-26** — `recursive-endocentric-compounding` |
| Multi-table, shared representation across tables | **Closed 2026-07-26** — `two-table-shared-representation-recall` |
| `SubruleGating` as its own tagged phenomenon | **Closed** — `subrule-morphosyntactic-gating` |
| Right-to-left rewrite — anchor/segment/alpha-polarity excluded shapes | **Partially closed** — bounded and genuinely-unbounded quantifier shapes landed (the bounded shape *found a real recall bug* in `reversed_slots`, which was corrected); anchor/segment-literal/alpha-disagreement siblings remain open |
| Metathesis, right-to-left | **Closed 2026-07-26** — `right-to-left-metathesis-reversal` |
| `QuantifierPattern`, genuinely unbounded (Kleene, not finite) | **Closed 2026-07-26** — `unbounded-iterative-quantifier-expansion`; the doc flags this mattered more than expected: it was blocking a reference grammar on the compiled path, not just a coverage row |
| `CircumfixOutputAction`, missing structural-composite shapes | **Census done, all three closed** — the census found the mechanism was already allomorph-complete; every gap was in candidate *selection* |
| `SimultaneousRewrite`, genuine subrule-environment overlap | **Oracle gap discharged 2026-07-26** via `simultaneous-subrule-genuine-overlap`, this repo's first fixture with real `hc.dll` (C#) ground truth — the two engines agree and the agreement *discriminates* resolution order |

Three items remain genuinely open, by design, not by omission:

1. **`right-to-left-*` sibling shapes** (anchor, bare-`Segments`, disagreeing-polarity alpha-var)
   — `tasks.md 4.2`, not yet authored.
2. **`MprGroupOverwrite`** — permanent carve-out, no fixture will ever close it (structurally
   unsound, not merely unproven).
3. **`Tracing (TraceType)`** — no typological pattern exists to write a fixture *for*; it is an
   engine-debug feature, not a linguistic phenomenon. This is the one row this repo's own research
   honestly reports as having no basis, rather than inventing one.

### 2.2 The inverse gap: fixtures exercising no distinct construct

`coverage.csv` (`machine/conformance/coverage.csv`) is the generated cross-reference; several
patterns are visible directly in it:

- Many words are **negative controls** with an empty `construct` column signature (e.g.
  `coverage.csv:5-6`, `BistratalOverlappingSegmentRepresentation,basi,,...` and `abis,,...` —
  these carry a construct tag but a blank ground-truth signature, i.e. "this string should NOT
  parse under this construct"). They are not redundant — `dead-end-census`'s own doctrine and
  every `expect_fail: true` fixture entry treat the negative witness as load-bearing (proves the
  rule genuinely fires/gates, rather than being vacuously inapplicable) — but they are not
  *additional construct coverage* either.
- Several fixtures each tag the *same* row from multiple angles on purpose (e.g.
  `templatic-root-modification` tags `RewriteRule Simultaneous`, `MorphologicalOutputAction`,
  `MorphemeCoOccurrenceRule`, and `Stem names` across its ~20 words) — this is deliberate
  construct-bundling per §3's typology answer below (real grammars bundle constructs; a
  one-construct-per-fixture corpus would under-stress interaction), not waste.
- The `conformance_coverage.rs` "structural-witness gate" (`conformance_coverage.rs:91-120,
  391-512`) exists precisely because some `constructs.txt` ids are shared by a coarser and a finer
  `CharacteristicKind` (4 pairs: Ordered/Unordered, Iterative/Epenthesis, Affixation/
  CircumfixOutputAction, MprGroupAppend/Overwrite) — a fixture can keep a finer characteristic's
  `Covered` status alive by accident, tagging only the coarser sibling, which is a real
  "fixture exercises no distinct [finer] construct" trap this repo built a dedicated mechanical
  gate for rather than trusting hand review.

---

## 3. The axes that should matter to FST construction

This section reasons from the morphology first, then checks each candidate against what this
repo's own measured evidence (dead-end census, recipe-parity runs, `GrammarSemantics`) actually
found. `docs/fst-plan/linguistic-recipe-harvest.md` is the team's own typology survey (18
languages/families, ranked sources) and is the single richest source for this section; it is
explicitly "an input to recipe-space design, not an independent literature review" (its own §1),
so its "Likely Plan primitives" column is read here as a hypothesis, not a settled fact.

### 3.1 Templatic/position-class morphology vs. purely concatenative affixation — **KEEP, and it is already load-bearing**

This is the single axis the codebase has already built dedicated machinery around, because it
produced a measured 4.5× over-proposal bug when ignored. `docs/fst-plan/recipe-parity-plan-
2026-07-30.md:30-35`: a phonology-free, template-heavy corpus (called "Sena" there) was routed
through `uflexc::emit_underlying_filtered_with_budget`, a "self-looping prefix/suffix chain" whose
own module doc says it is "not intended to generalize to a templated grammar... as-is" — because
its applicability gate was `HasPhonology`, and this corpus has **zero** phonological rules, so
`uflexc` was its *only* offered model, never compared against the template-aware
`emit_underlying_templated` compiler at all. The fix (`Applicability::HasPhonologyOrTemplates`,
`recipe_registry.rs:54-67`) widens the gate to `declared_phonology() || declared_templates()`.

- **Signal in the grammar definition:** `GrammarSemantics::declared_templates()`
  (`grammar_semantics.rs:324-326`, `!grammar.templates.is_empty()`) and `declared_phonology()`
  (`grammar_semantics.rs:310-312`, `!grammar.prules.is_empty()`) — both **O(1)** field checks on
  an already-loaded `Grammar`, computed eagerly in `GrammarSemantics::derive` (no compile, no
  corpus). `template_count()` gives the magnitude.
- **What it should imply:** a grammar with templates and no phonological cascade needs the
  template-aware morphotactic compiler regardless of whether phonology is present; a grammar with
  phonology but no templates can use the plain cascade; a grammar with both needs both composed
  in the template→phonology order the harvest table calls out repeatedly (Turkish, Bantu).

### 3.2 Depth and branching of derivational chains — **KEEP, distinct from stratum *count***

Depth (how many strata/rules a word passes through) and branching (how many strata feed into the
next, and whether a stratum re-enters itself — e.g. derivation feeding inflection,
`polysynthetic-stratal-derivation-chain/grammar.xml:287-424`'s three-stratum
Phonology→Derivation→Inflection cascade where a `posN`-derived `posV` stem crosses strata) are
different costs from a stratum's own internal rule *count*:

- **Signal:** `GrammarSemantics::ordered_operations`/`ordering_dependencies`
  (`grammar_semantics.rs:179-194`, `Σ (prules+mrules+templates) per stratum` and its
  `saturating_sub(1)`) is a cheap structural proxy for total ordering pressure, and
  `stratum_count` for depth. `pg_foma::capability::compounding_max_depth` gives an **exact,
  closed-form, always-finite bound** (no compile) for one specific chain shape: `1 +
  multipleApplication` — verified directly against `recursive-endocentric-compounding`'s `cr1`
  rule (`multipleApplication="9"` → bound 10 stems, `STAGING.md:88-93`).
- **What it should imply:** chain depth is exactly the axis the codebase already treats as
  needing an explicit **budget**, not an unconditional construction: `crate::emit`'s bounded
  compound loop unrolls `max_depth - 1` extra levels (a real construction, proven contained
  against a raised-cap oracle, `STAGING.md:122-138`); `DEFAULT_ORDERING_MULTIPLICITY_BUDGET`
  bounds unordered-stratum chain depth the same way. The general lesson: depth/branching wants a
  *depth-budgeted cross-product construction with a checked ceiling*, not an unbounded unrolling
  and not a refusal — this repo has working precedent for exactly that shape now for
  compounding.

### 3.3 Reduplication, and whether it is bounded — **KEEP, boundedness is the actual fork, not "reduplication present/absent"**

`linguistic-recipe-harvest.md`'s cross-language constraint 8 states this precisely: "Copying is
split by boundedness. Fixed-CV partial reduplication can be regular and compiled as a bounded
process; arbitrary full-stem reduplication requires a specialized search/peel branch... A
detected 'reduplication' switch alone does not choose the recipe." The codebase's own
`ReduplicationPeeler` (`capability.rs:283-294`) implements exactly this split, and it carries a
**permanent, faithfully-preserved carve-out**: reduplication is peel-eligible only when the owning
rule is `MorphRuleDef::AffixProcess`, never `MorphRuleDef::Realizational`, "even if one of its
allomorphs would classify as `Role::Reduplication`" — a real C# quirk this port preserves rather
than smooths over.

- **Signal:** `GrammarSemantics::reduplicative_allomorph_count` (`grammar_semantics.rs:118,
  167-171`, via `rhs_has_true_reduplication` — true copy-≥2 in the RHS, **not** every allomorph
  carrying a `ReduplicationHint`, since `Implicit` is the DTD default for non-reduplicating
  affixes too, `capability.rs:136-142`) is cheap: an O(rules) structural scan, no compile.
  Boundedness itself, though, is not a separate scannable field — it is a property of *how much*
  material a `PatternNode::Quantifier` inside the reduplicative pattern can match (§3.5), so
  detecting "bounded partial reduplication" vs. "unbounded full-stem copying" is a composite of
  this signal and the quantifier-boundedness signal, not a single flag.
- **What it should imply:** bounded copying compiles into the same construction as ordinary
  affixation; unbounded/full-stem copying needs the runtime peel-and-verify branch, per the
  harvest table's "Hybrid copying branch" family.

### 3.4 Phonological rule density, and how much rules feed each other — **KEEP, but density alone is the wrong proxy; overlap/interaction is**

Density (`RewriteRule` count) is cheap but was directly falsified as a cost predictor by this
repo's own measurement, cited verbatim in the `dead-end-census` skill: **"Sena has 72 env
constraints and zero rewrite rules, yet d1 was <2% and d5 dominated"** — i.e. a grammar with *no*
phonological rules at all can still be the slowest/most over-proposing grammar in the corpus,
because its cost lives entirely in morphotactic ordering (d5), not phonology. What *does* matter,
per the harvest's constraint 1 ("Rule order is a dependency graph, not a permutation" — Indonesian
assimilation must precede deletion, Awngi docking must precede deletion or the trigger
disappears) is whether rules **feed** each other (a later rule's environment depends on an earlier
rule's output), which the corpus already stress-tests directly:
`polysynthetic-stratal-derivation-chain`'s `prSimulFeeding`/`prIterFeedingControl` pair
(grammar.xml:264-301) is the identical Lhs/Rhs/Env rule run once in `Simultaneous` mode and once
in `Iterative` mode specifically to demonstrate that feeding/bleeding is a function of
*application mode*, not rule content — Simultaneous rewrites every matching position against the
*original* input (feeding), Iterative rewrites left-to-right against the *already-mutated* shape
(bleeding after the first hit).

- **Signal:** `metathesis_rule_count` and rule count are cheap counts
  (`grammar_semantics.rs:173-177`). Genuine interaction/overlap detection is **not** cheap:
  `SimultaneousRewrite`'s own predicate (`SimultaneousSubruleOverlapPredicate`) needs
  `LoweredSpan` — an actual `foma::types::Fsm` built per subrule (`capability.rs:373-382`), i.e. a
  real automaton construction, not a structural field read. This is one of exactly two `OnceLock`
  memoized-because-expensive facts in `GrammarSemantics` (§5).
- **What it should imply:** density should not gate strategy selection by itself; feeding/
  bleeding interaction and genuine subrule-environment overlap should, and those require at least
  a partial compile to detect faithfully.

### 3.5 Non-concatenative processes: metathesis, infixation, subtractive morphology — **KEEP as three separate axes, not one "non-concatenative" bucket**

The codebase already treats these as structurally distinct, with distinct dispositions:
metathesis has its own `PhonRuleDef` variant and its own swap-relation construction
(`compile_metathesis_rule`), gated by direction (`Dir::LeftToRight` was the only supported shape
until the RTL mirror-and-reverse construction landed, `MetathesisDetail`'s doc,
`capability.rs:485-490`). Infixation and subtraction, by contrast, are **not** distinct
`CharacteristicKind`s at all — they fold into plain `Affixation` because `capability.rs`'s
granularity has no finer characteristic for a single-part-LHS truncating affix
(`conformance_coverage.rs:154-163`'s explicit judgment call), and a multi-part-LHS
circumfix/discontinuous shape (which subsumes true infixation-with-material-drop) is
`CircumfixOutputAction`, keyed on `allomorph_drops_lhs_material` (`capability.rs:131-135`) rather
than on any interior-insertion-specific test.

- **Signal:** metathesis — `metathesis_rule_count` (cheap count). Circumfix/subtractive shape —
  `grammar_has_circumfix_shaped_allomorph` (`conformance_coverage.rs:472-480`) calls the
  compiler's own `classify_affix` classifier directly (deliberately, so the gate and the compiler
  cannot drift apart) — still a pure structural scan over the loaded model, no compile, but it
  reuses compiler-internal logic rather than a field read.
- **What it should imply:** these three deserve three separate construction decisions, not one.
  Metathesis needs the dedicated swap relation; subtractive/infixing affixation is cheap (folds
  into ordinary affixation compilation); genuinely discontinuous circumfixation needs the
  structural-composite resynthesis path (`build_structural_composites`, which re-derives every
  candidate via the real morphological engine rather than splicing literal text).

### 3.6 Lexicon size and the shape of allomorphy — **KEEP, this is the axis the current corpus tests least**

`entry_count` is trivially cheap (`grammar_semantics.rs:120,242`), and this repo's own standing
rule (`build-for-full-scale-grammars` memory entry, restated at
`docs/fst-plan/dead-end-census`'s own "Scale-gate on a synthetic 10⁴-entry lexicon before
default-flip — reference grammars are small") is that **lexicon size is not merely a magnitude
knob — it changes which construction is viable**, because several compiled constructions are
combinatorial in entry count × allomorph count (α-tuple expansion, MPR partition count,
compound-license cross products). `linguistic-recipe-harvest.md`'s Latin/Zapotec/Spanish rows are
specifically about allomorphy *shape* rather than size: shared allomorphs across declension
classes (`-is` licensed by more than one class — "duplicating it per class is semantically
unnecessary and expands the plan"), lexically-selected inflection classes with subclass
hierarchies, and specificity-ordered allomorph alternatives (stressed-`a` before general
feminine/default before lexical exception).

- **Signal:** `entry_count` cheaply gives magnitude. Allomorphy *shape* — how many distinct gate
  keys the lexicon actually realizes — is the one genuinely expensive `GrammarSemantics` fact:
  `entry_partition()` (`grammar_semantics.rs:283-298`) is **O(entries × gated subrules)** and
  calls the real engine's own `pg_rules::rewrite::subrule_applicable` predicate for every pair —
  the second of the two `OnceLock`-memoized facts, explicitly because it is not affordable to
  compute eagerly for every consumer (module doc, `grammar_semantics.rs:99-107`).
- **What it should imply:** a small, cheap `entry_count` check can rule *in* concern early, but
  cannot itself decide construction; the partition shape needs the O(entries × gated subrules)
  pass, and — per the dead-end-census's own finding that a construction must be scale-gated on a
  synthetic 10⁴-entry lexicon before it defaults on — even a correct construction at reference
  scale is not evidence it stays viable at production scale.

### 3.7 Compounding, and whether recursive — **KEEP; "could recurse" and "does recurse deeply" are two different facts, and only one is cheap**

Already covered structurally in §1/§3.2. The typological-basis research (§1.2.1a) is unusually
careful about a distinction directly relevant here: the **formal** claim ("this rule is
recursively applicable without limit" — true the instant `outputPartOfSpeech` re-enters the
rule's own input PoS set) is a cheap structural fact, entirely independent of the **attested**
claim (how deep compounds actually go in any real corpus — sharply skewed to 2-3 members even in
languages with no formal ceiling, per the Sanskrit/German corpus statistics that document cites,
with **no published depth histogram for English at all**). Conflating the two is explicitly named
as the trap: "how a construction ends up sized for a depth nothing observes."

- **Signal:** `CompoundingRecursionSafePredicate`'s own structural test (does the rule's
  `headPartsOfSpeech`/`outputPartOfSpeech` overlap with its own input set?) is a pure model-graph
  reachability check, cheap. The exact depth bound (`compounding_max_depth`, an arithmetic
  function of `multipleApplication`) is likewise cheap and closed-form.
- **What it should imply:** recursion *possibility* is a cheap yes/no gate on whether to even
  consider the depth-budgeted construction; the *budget* itself should be sized from the rule's
  own declared cap, never from an assumed corpus depth — exactly what `build_compound_chain`
  already does (`max_depth - 1` levels, no more, no less).

### Two candidates evaluated and downgraded

- **A dedicated "phonological rule density" score** — evaluated above (§3.4) and downgraded to a
  secondary signal: measured evidence (Sena: 72 constraints, 0 rewrite rules, still dominated by
  a non-phonological dead-end class) directly falsifies density as the primary cost driver.
- **A single "non-concatenative" umbrella axis** — evaluated in §3.5 and rejected as too coarse:
  the codebase's own disposition table treats metathesis, subtraction, and circumfixation as three
  separately-gated mechanisms with different construction costs and different permanent
  carve-outs (e.g. `Reduplication × RealizationalRule`, but not `Metathesis × RealizationalRule`).

---

## 4. The gap to the real world

The historical "four hand-tuned grammars" this task refers to are named directly in
`docs/fst-plan/recipe-parity-plan-2026-07-30.md:1-6`: **"the four language corpora (Indonesian,
Amharic, Sena, Aweti)"** — explicitly distinguished there from the *unrelated* "four synthetic
promoted plan-shape fixtures" of `four-grammar-recipe-evidence-2026-07-28.md` (a naming collision
that document flags directly, `four-grammar-recipe-evidence-2026-07-28.md:3-10`). **Unverified**:
no `grammar.xml`/corpus file under these four names exists anywhere in this worktree's tracked
files (`git ls-files | grep -iE "indonesian|amharic|sena|aweti"` returns nothing) — they are
referenced only in prose/comments/test labels (e.g. `recipe_optimizer.rs:1541`'s
`"indonesian-shape"` corpus-hash label), consistent with this repo's synthetic-data policy having
been enacted after these four were established as internal engineering fixtures; where their
actual grammar files live (a separate `samples/data`-style location, or since removed) is not
determinable from this worktree and should be treated as unverified.

What is directly measured (`recipe-parity-plan-2026-07-30.md:9-26`) about where these four sit in
the space:

| Corpus | Measured shape |
|---|---|
| Indonesian | Recipe optimizer already ahead on every metric (437 steps vs. hand-spun's 451) — the "easy" point in the space |
| Sena | 0 phonological rules, template-heavy, `templates` on but `declared_phonology()` off — the case that exposed the `HasPhonology`-only applicability gate bug (§3.1) |
| Amharic | Candidate-evaluation cost dominated by *provably redundant* per-candidate work (re-running an O(minutes) `emit(grammar).report` per candidate that four of seven candidates don't even need) — a search/engineering cost, not a morphological one |
| Aweti | Templated + a now-refuted one-sided-truncation hypothesis (41 "truncation" mrules turned out to be floating-consonant realization, not truncation) + a genuine evaluator resource-cap bug (`Morpher::new(grammar, usize::MAX)` hanging, fixed) |

None of these four stresses: genuine tonal/suprasegmental alternation, transparent-vowel-skipping
long-distance harmony, lexicon-scale allomorphy (all four are reference-grammar-sized, per the
standing "reference grammars are small" rule), or a *combined* templatic+heavy-phonology+deep-
derivation grammar in one object (Sena is templatic with none; Amharic and Aweti are phonology/
template-heavy but shallow strata; none combines all three at once the way
`polysynthetic-stratal-derivation-chain`'s synthetic 3-stratum cascade does).

Five additional language-family-motivated stress shapes, each targeting a property the current
corpus (the four real corpora **and** the synthetic conformance suite) does not yet combine:

1. **Long-distance harmony with transparent (harmony-invisible) segments** — Finnish-style, per
   `linguistic-recipe-harvest.md`'s own entry: "Transparent vowels do not reset the harmony
   controller. A nearest-vowel recipe that treats every vowel as decisive is unrealizable." The
   corpus's `suffixing-vowel-harmony` fixture is a plain adjacent a/i/alpha-variable harmony
   (`words.yaml` `satun`/`setin` pair, `coverage.csv:279-280`) with no transparent-vowel class at
   all — this would stress whether a compiled harmony construction can carry state *through* a
   skipped segment rather than resetting on it, a genuinely different automaton shape from
   adjacent agreement.
2. **Cross-cutting agreement + phonological conditioning in the same grammar** — Swahili-style
   noun-class concord (a lexically-selected partition axis) composed with verb-extension
   vowel-height harmony (a phonologically-conditioned axis) *in the same word*, per the harvest's
   own constraint 4: "Lexical class and phonological conditioning are different partition axes...
   Conflating them creates both bad gates and duplicate recipes." No current fixture forces a
   `Gate`-by-class construction to also thread a shared `Replace`-harmony cascade underneath it;
   `prefixal-discontinuous-slot-dependency` and `suffixing-extension-slot-ordering` each stress
   one of the two, not their composition.
3. **Ordered inflection→derivation→inflection layering with an incomplete inner layer** —
   Huallaga-Quechua-style, per the harvest: "Inner inflection must be incomplete in a controlled
   way ('requires more derivation') before outer inflection; flattening all slots into one
   template admits illegal orders." `polysynthetic-stratal-derivation-chain` has
   derivation-then-inflection (two layers), not the three-layer inflection-derivation-inflection
   sandwich, and does not test what happens when the *inner* inflection layer's own template is
   deliberately partial.
4. **Genuinely large, deep, obligatorily-co-occurring template with lexically class-conditioned
   stem selection at scale** — Zapotec/Latin-style class hierarchies where a superclass allomorph
   licenses several subclasses and specificity ordering must resolve overlapping-eligible
   allomorphs, exercised at the 10³-10⁴-entry scale this repo's own standing rule says is the real
   target (§3.6) rather than the handful-of-lexemes scale every current fixture uses. This
   stresses the one axis (§3.6) with the least corpus coverage today.
5. **A single grammar combining heavy templatic morphotactics, a dense feeding/bleeding
   phonological cascade, AND deep (3+) stratal derivation** — none of Sena (templates, no
   phonology), Amharic/Aweti (phonology/templates, shallow strata), or the synthetic
   `polysynthetic-stratal-derivation-chain` (deep strata, light phonology) combines all three at
   once; this is the most direct test of whether the recipe optimizer's family search degrades
   gracefully or combinatorially when every axis in §3 fires simultaneously in one grammar.

---

## 5. Which axes are cheap to detect and which are not

`pg-foma/src/grammar_semantics.rs` is the concrete, already-built answer to this question for the
axes it covers: its own module doc states the design rule directly — "`derive` has to be cheap
enough that every consumer can afford to call it... The eager facts are all O(rules + strata +
entries) scans over already-loaded vectors. The two lazy ones are not" (`grammar_semantics.rs:99-
107`).

**Cheap — O(1) or O(rules+strata+entries), pure structural reads, computed eagerly, no compile,
no corpus:**
- `declared_phonology`, `cascade_phonology` (two genuinely different facts — grammar-wide vs.
  stratum-reachable phonology, `grammar_semantics.rs:49-62` documents why they can and do
  disagree)
- `declared_templates`, `template_count`
- `has_morphology`, `mrule_count`
- `reduplicative_allomorph_count` (structural true-reduplication test, not merely
  hint-presence)
- `metathesis_rule_count`
- `entry_count`, `stratum_count`
- `ordered_operations`, `ordering_dependencies` (per-stratum operation-count proxy for
  ordering pressure)
- `char_table_count`, primary-table boundary-symbol inventory
- `has_gated_exceptions` (from `find_gated_subrules`, itself a pattern-shape scan, not an
  evaluation)
- `compounding_max_depth` and the recursion-possible/not test (arithmetic on a declared
  attribute, and a rule-graph reachability check)

**Expensive — requires either real automaton construction or O(entries × rules) evaluation, and
is deliberately memoized behind `OnceLock` rather than computed eagerly:**
- `entry_partition()` — O(entries × gated subrules), evaluates the real engine's
  `subrule_applicable` predicate for every pair (`grammar_semantics.rs:283-298`).
- `characteristics()` (the full `CharacteristicsProfile`) — builds real `foma::types::Fsm`
  networks for every `Simultaneous`-mode subrule's span (`LoweredSpan::Ok`,
  `capability.rs:373-382`) as part of characterizing `SimultaneousRewrite` overlap. This is a
  genuine partial-compile, not a structural read.

**More expensive still — requires an actual corpus and a real propose+confirm run, not just a
loaded grammar:**
- The dead-end census's d1-d6 attribution (`dead-end-census` skill) is explicitly *not*
  predictable from grammar structure — its own "hard-won lessons" state this as the reason the
  whole skill exists: "the dominant cause is not predictable from grammar structure (Sena has 72
  env constraints and zero rewrite rules, yet d1 was <2% and d5 dominated)." It additionally
  requires finding a grammar's *worst words*, which "usually live in the tail" of the corpus, not
  the front — so even the *sampling* of what to measure is not cheap or obvious.
- Pareto-frontier build/apply-time measurements (`four-grammar-recipe-evidence-2026-07-28.md`'s
  own detailed case study) are noisy enough that five immediate repetitions of the *same* grammar,
  seed, and budget selected three *different* winning Plans — this is why the ranking key was
  later moved off wall-clock entirely onto deterministic work counters (HC confirmation steps),
  and even that is described as "quality=exact only in the algorithmic sense... it does not
  establish a statistically stable fastest Plan."
- Genuine subrule-environment overlap detection for `SimultaneousRewrite` sits at an interesting
  middle point: the automaton construction itself is affordable per-rule (the `characteristics()`
  cost above), but interpreting whether real HermitCrab (`hc.dll`) *agrees* with the FST's overlap
  resolution required, per `representative-typology-basis.md` §1.3, an actual C# oracle harness
  run (`add-reference-hermitcrab-parity`) — a fact no amount of grammar-model inspection could
  supply, because it was cross-engine ground truth, not a property of the grammar file at all.

**The practical ordering this suggests:** every `GrammarSemantics` eager field is affordable as a
*pre-filter* run on every grammar unconditionally (this is already the existing design — a
registry-applicability check "must not pay for" the two lazy facts, `grammar_semantics.rs:106-
107`). The two `OnceLock` facts are affordable per-grammar once, not per-candidate-plan (the exact
bug `GrammarSemantics` itself was introduced to fix — `characterize` used to be recomputed
unconditionally on every `compose_envelope` call, and `select_plan` called it once per candidate
plan, `grammar_semantics.rs:1-16`). Corpus-dependent measurement (dead-end census, Pareto
timing) is the most expensive tier and the least stable; it should run last, on a small number of
already-structurally-promising candidates, and its *ranking* output should be treated as noisy at
sub-millisecond deltas even when its *classification* output (which dead-end class dominates) is
trustworthy at reasonable corpus size (the skill's own ~400-word minimum-signal threshold for
Amharic-class grammars).

---

## 6. Axis correlation, binary-vs-continuous, and cluster count

**Upfront limit, stated plainly per the brief: this corpus cannot support a statistical
correlation analysis, and nothing below should be read as one.** The entire known grammar
population is ~12 objects — 8 synthetic `languages/` fixtures, a handful of synthetic
`edge-cases`, and the 4 real corpora (Indonesian/Amharic/Sena/Aweti, §4, whose own grammar files
are not even present in this worktree to re-inspect). Most axes in §3 are present (nonzero) in 1-3
of those 12, never enough to distinguish "these two properties co-occur because real morphology
bundles them" from "these two properties co-occur because the fixture author happened to write
them into the same file." Where evidence exists below it is either (a) a direct reading of the
attested-typology literature `linguistic-recipe-harvest.md` already surveyed across 18
languages/families, which is a real, if secondary, evidence source independent of this repo's own
corpus, or (b) a structural fact read directly off the production recipe registry's own
applicability predicates, which is code, not a statistic, but is at least independently
engineered evidence rather than this report's own inference. Both are flagged as such below;
nothing here is invented to fill a gap.

### 6.1 Three convergent measurements of "how many clusters," none of them this report's own guess

The most load-bearing finding in this section is that **three independent efforts already
converged on a small family count**, without coordinating with each other:

1. **The typological survey** (`linguistic-recipe-harvest.md`, written from 18 attested
   languages/families, §"Revised bounded recipe families") collapsed its own cross-language
   evidence into exactly **7 families**: Ordered morphophonology, Class/exception-partitioned
   cascade, Complete-template alternatives, Specialized morphology branch, Hybrid copying branch,
   Bounded metathesis cascade, Layered morphology — with the explicit conclusion "This catalog is
   deliberately not the product of all seven families [combined combinatorially]. HC construct
   dependencies choose one base family and add only demonstrated gates/branches."
2. **The production recipe registry** (`recipe_registry.rs:800-895`), engineered independently
   against measured grammar behavior rather than typology, has **9** family constants today. The
   first 7 (`FAMILY_ORDERED_MORPHOPHONOLOGY`, `FAMILY_CLASS_EXCEPTION_CASCADE`,
   `FAMILY_COMPLETE_TEMPLATE`, `FAMILY_SPECIALIZED_BRANCH`, `FAMILY_COPY_BRANCH`,
   `FAMILY_BOUNDED_METATHESIS`, `FAMILY_LAYERED_MORPHOLOGY`) are a name-for-name port of the
   typological survey's own 7. The 2 later additions are informative in different directions:
   `FAMILY_SURFACE_PROBE_MORPHOLOGY` is **not** a linguistic cluster at all — it is the legacy
   hand-spun baseline compiler, kept for comparison (`Applicability::Always`, same as the
   general-purpose family). `FAMILY_TOKEN_CASCADE_MORPHOLOGY` **is** a genuine 8th cluster, and it
   was discovered empirically, not predicted: it exists specifically because the Sena-shaped
   corpus (templates present, zero declared phonology) fell through every applicability gate that
   existed before it (§3.1). `four-grammar-recipe-evidence-2026-07-28.md:81` independently
   describes "the registry has seven recipe families" at the point it was written (before
   `TOKEN_CASCADE_MORPHOLOGY` existed) — a third, dated data point on the same trajectory.
3. **Each family's applicability predicate is, in code, essentially a single axis from §3**, not
   a hand-tuned combination — see the table below.

**Answer to "how many clusters": ~7-8 real morphological clusters, plus one non-cluster (the
legacy baseline).** This lands close to, if slightly above, the owner's own "about five or six"
guess, and the qualitative conclusion is the same either way: **the attested space is a small,
enumerable menu, not an open combinatorial product** — exactly what the typological survey's own
words say ("choose one base family and add... gates/branches," not multiply seven families
together). The one caveat worth stating plainly: the menu is not permanently closed at a fixed N.
It grew by one (7→8 real clusters) the one time this codebase actually met a grammar shape (Sena)
the prior menu had not anticipated, and that addition was cheap — a widened `Applicability`
predicate on an existing family, not a new compiler. The realistic description is "a small,
slowly-growing enumerable menu with cheap additions when a genuinely new shape appears," not
"five fixed compilers forever" and not "open composition."

### 6.2 Axis → family mapping (code-grounded, not this report's inference)

| Family | `Applicability` predicate (`recipe_registry.rs`) | §3 axis it corresponds to |
|---|---|---|
| `ordered-morphophonology` | `Always` | Baseline — no axis distinguishes it; every grammar gets this candidate |
| `class-exception-cascade` | `HasGatedExceptions` | §3.4/§1's `SubruleGating` — MPR/POS-gated subrule exceptions |
| `complete-template` | `HasTemplates` | §3.1 — templatic/position-class morphology |
| `specialized-branch` | `HasSplittableGateGroup` | §3.6 — allomorphy/partition shape (lexicon axis) |
| `copy-branch` | `HasReduplication` | §3.3 — reduplication |
| `bounded-metathesis` | `HasMetathesis` | §3.5 — metathesis |
| `layered-morphology` | `HasSplittableGateGroup` (same gate as `specialized-branch`, different transform: `PartitionFanOut` vs. `PartitionBisect`) | §3.6, but see 6.3 below |
| `surface-probe-morphology` | `Always` | Not an axis — the legacy oracle-comparison baseline |
| `token-cascade-morphology` | `HasPhonologyOrTemplates` | §3.1/§3.4 combined — the one family keyed on an OR of two axes, not one |

This table is the cleanest available evidence that **most axes in §3 map to exactly one family
each**, one-to-one — which is itself evidence *against* needing general composition: if axis→family
were many-to-many in practice, a fixed menu could not represent the space; here it mostly is
one-to-one.

### 6.3 The one place composition is visibly earned in the registry today

`specialized-branch` and `layered-morphology` share the **identical** applicability gate
(`HasSplittableGateGroup`) but produce **different transforms** (`PartitionBisect` vs.
`PartitionFanOut`) — i.e., detecting one structural condition does not determine which of two
recipes is right; both are legitimate candidates and the optimizer has to build and compare both.
This is the one concrete, code-visible counterexample to "one axis, one recipe" in the current
registry, and it is worth naming explicitly because it is exactly the shape that would justify
*some* combinatorial search even under a small-menu view — not composition across independently-
varying axes, but multiple candidate recipes for the *same* detected condition.

### 6.4 Qualitative co-variance reading (typology-literature-sourced, not measured)

- **Templatic morphology (§3.1) × absence of a phonological cascade.** Observed together exactly
  **once** in the whole known corpus (Sena) — not enough to call a correlation, only a single
  co-occurring data point that happened to be expensive enough to force a code change. The
  production fix (`HasPhonologyOrTemplates`, an OR) treats templates and phonology as two
  independently-triggerable conditions rather than collapsing them into one "templatic cluster"
  flag — i.e. the codebase's own design choice already assumes these are independent axes that
  can each show up without the other, not a bundle. **Verdict: independent by construction, N too
  small to say more.**
- **Reduplication (§3.3) × phonological rule density/feeding (§3.4).** The *attested-typology*
  evidence (not this repo's corpus) says these interact strongly **when both are present in one
  grammar** — Indonesian's copied span must re-enter the same assimilation/deletion cascade, and
  Tagalog's reduplication depends on where the morphologically-constructed stem edge falls
  post-affixation (`linguistic-recipe-harvest.md`'s Indonesian/Tagalog rows, constraint 2). That is
  an **interaction cost when co-present**, not evidence the two properties are statistically
  correlated *across* grammars — reduplication appears in only 2 of this repo's own 8 `languages/`
  fixtures (`metathesis-phase-isolation`, `suffixing-extension-slot-ordering`), each bundled with a
  visibly different phonology profile (a metathesis-plus-epenthesis cascade in one, a lighter
  nasal-assimilation pair in the other). **Verdict: genuinely ambiguous at this sample size — real
  languages that reduplicate do not obviously all share one phonology profile, but the one
  well-documented case this project treats as its "principal adversarial fixture" specifically
  because copying and cascade interact.** Do not treat this as either proven independent or proven
  correlated.
- **Subrule/MPR gating (§1, `SubruleGating`) × allomorphy/partition shape (§3.6).** Loosely
  coupled, not identical: `HasGatedExceptions` and `HasSplittableGateGroup` are two distinct
  booleans in the registry (a grammar can have a gated subrule without its entries actually
  partitioning into a *profitably splittable* group, and vice versa), but a gated subrule is the
  usual *source* of a splittable partition in practice. **Verdict: related but not
  interchangeable — worth keeping as two axes, per the registry's own choice to gate them
  separately.**
- **Metathesis (§3.5) × reduplication (§3.3).** The one fixture with metathesis
  (`metathesis-phase-isolation`) also has reduplication — but N=1 for metathesis in this whole
  corpus, so this is not evidence of either correlation or independence, only a note that no
  fixture isolates metathesis without reduplication to test the counterfactual.
- **Compounding recursion (§3.7) × lexicon size (§3.6).** The clearest candidate for genuine
  independence in this whole set: recursion-possibility is a fact about one rule's own
  `headPartsOfSpeech`/`outputPartOfSpeech` overlap, computed from the rule graph alone; lexicon
  size is a count over `entries`. Nothing in the model, the registry, or the measured evidence
  ties them together — a large lexicon with no recursive compounding rule and a tiny lexicon with
  one are both straightforwardly representable. **Verdict: independent, with reasonable
  confidence despite the small N, because the independence follows from the two facts' disjoint
  data sources (rule definitions vs. entry list), not merely from absence of a counterexample.**
- **Derivational depth/branching (§3.2) × templatic morphology (§3.1).** Mixed: Sena (templatic)
  is shallow; Aweti (templatic, per `synthetic-stress-grammar-plan.md`'s "Aweti templated path")
  reportedly has 3 strata ("3 strata proven (Aweti rules half)"). Two data points, disagreeing.
  **Verdict: no usable signal — flagged rather than resolved.**

### 6.5 Binary or continuous, per axis

| Axis (§3) | Binary or continuous | Note |
|---|---|---|
| Templatic vs. concatenative | **Binary switch** (`declared_templates`) | Magnitude (`template_count`) is a secondary continuous fact, not needed for the routing decision itself |
| Derivational chain depth/branching | **Continuous dial, needs a threshold** | `stratum_count`/`ordered_operations` are unbounded counts; every construction that consumes depth (compound-chain unrolling, unordered-stratum budget) is gated by an explicit numeric budget, not a switch |
| Reduplication | **Two stacked binary switches** | Presence (`reduplicative_allomorph_count > 0`) is one switch; boundedness (finite quantifier vs. genuine Kleene) is a second, independent switch layered on top — not one dial |
| Phonological rule density | **Continuous, but the wrong dial** — measured to mispredict cost (Sena: dense allomorphy, zero rules, still the slow grammar) | The actionable form is closer to a pairwise boolean ("do these two subrules' environments genuinely overlap?"), evaluated per rule-pair, not a scalar count |
| Metathesis / infixation / subtraction | **Three independent binary switches**, each further split by direction (LTR/RTL, also binary) | Not one "non-concatenative" dial — three separately-gated mechanisms |
| Lexicon size | **Continuous, effectively unbounded** | The one axis this repo's own standing rule says every current fixture under-samples (reference-scale, not 10³-10⁴) |
| Allomorphy/partition shape | **Small discrete cardinality, not binary and not a smooth dial** | `partition_count` — "how many distinct gate-key groups" — is a small integer with real structure, closer to "which of a handful of shapes" than a continuous knob |
| Compounding recursion | **Binary switch, plus a continuous depth-bound dial once true** | Recursion-possible is a yes/no rule-graph fact; `max_depth` (arithmetic on `multipleApplication`) is the dial that only exists once the switch is on |

**Why this matters for the composition-vs-menu question, restated in the owner's own terms:** the
axes that are cleanly binary (templates, metathesis, reduplication-presence, compounding-
recursion) are exactly the ones that map one-to-one onto a registry family in §6.2 — a switch
choosing a menu item is cheap composition, if it is composition at all. The axes that are
continuous dials (chain depth, lexicon scale, rule-pair overlap) are handled today not by a
recipe *choice* but by a **budget parameter inside an already-chosen recipe** (compound-chain
depth cap, `DEFAULT_ORDERING_MULTIPLICITY_BUDGET`, the O(entries×subrules) partition pass) — i.e.
this codebase's own working solution to "dials are harder to turn into a discrete choice" has so
far been to *not* turn them into a choice at all, but into a numeric ceiling on the one recipe the
binary switches already selected. That pattern — small discrete menu selection by binary switches,
continuous parameters tuned within the selected recipe — is a real, already-load-bearing design,
and it is one more piece of evidence that a bounded menu (of ~7-8 items, occasionally growing by
one) fits the measured shape of this problem better than a general composition mechanism whose
main justified use case so far (§6.3) is two recipes competing for the same trigger, not many
axes combining pairwise.

---

## Sources cited

- `rust/crates/pg-grammar/src/model.rs` (construct vocabulary)
- `rust/crates/pg-foma/src/capability.rs` (`CharacteristicKind`, `Disposition`, predicates)
- `rust/crates/pg-foma/src/conformance_coverage.rs` (construct↔characteristic mapping, orphan
  rows, structural-witness gate)
- `rust/crates/pg-foma/src/grammar_semantics.rs` (cheap vs. expensive signal taxonomy)
- `rust/crates/pg-foma/src/recipe_registry.rs` (family constants, `Applicability` predicates —
  §6's code-grounded axis→family mapping)
- `machine/conformance/constructs.txt`, `machine/conformance/coverage.csv`
- `machine/conformance/languages/*/grammar.xml` (`templatic-root-modification`,
  `polysynthetic-stratal-derivation-chain` read directly)
- `docs/conformance/representative-typology-basis.md` (gap analysis, citations to the broader
  typological literature)
- `docs/fst-plan/linguistic-recipe-harvest.md` (cross-language recipe-family survey)
- `docs/fst-plan/recipe-parity-plan-2026-07-30.md`, `docs/fst-plan/four-grammar-recipe-evidence-
  2026-07-28.md`, `docs/fst-plan/recipe-optimizer-strategy-calibration.md`
- `.claude/skills/dead-end-census/SKILL.md`
- `conformance-staging/edge-cases/recursive-endocentric-compounding/STAGING.md`,
  `.../unbounded-iterative-quantifier-expansion/STAGING.md`,
  `.../deletion-reduplication-exception-composite/STAGING.md`
