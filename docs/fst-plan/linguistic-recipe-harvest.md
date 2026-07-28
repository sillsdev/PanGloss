# Linguistic construct harvest for realizable FST recipes

## Purpose and scope

This note harvests the most useful language-specific material in the sibling
`../linguistic-assistant` repository. It is an input to recipe-space design, not an
independent online literature review. The central question is:

> Which combinations of constructs are actually attested, and what dependencies do
> they impose on a realizable `Leaf` / `Compose` / `Union` / `Gate` / `Replace` plan?

The answer is materially narrower than a Cartesian product of independently detected
features. Real languages repeatedly bundle morphology, ordered phonology, lexical
classes, and exceptions in ways that force a small number of recipe families.

## Source inventory

Sources were ranked by their usefulness for reconstructing an executable grammar.

| Rank | Sibling source | What it contributes | Reliability for this harvest |
|---|---|---|---|
| 1 | `docs/reference/flex-conceptual-intro-fulltext.txt` | Full 2025 H. Andrew Black FLEx/HermitCrab guide; attested examples, derivations, implementation recipes, bibliography | Strongest source; direct local extraction of the shipped SIL guide |
| 2 | `docs/reference/flex-conceptual-intro-notes.md` | Close-reading digest of the same guide, organized by HC mechanism | Strong secondary map; checked against the full text for the important interactions |
| 3 | `docs/phonology-architecture.md` | Existing synthesis of ordered SPE-style HC rules, strata, complementary distribution, and verification | Strong for architectural consequences; some citations are web links rather than locally archived papers |
| 4 | `docs/plans/fable-flex-methodology.md` | Maps Black's analyses to concrete grammar-induction tasks and tests | Strong for intended implementation, not independent linguistic evidence |
| 5 | `research/corpus/ebible/README.md` and `research/docs/from-scratch-8lang-status.md` | Current corpus targets and observed construct bundles | Useful project evidence, but not a substitute for language grammars |
| 6 | `research/gip-phonology-induction.md` | Focused review of harmony discovery and natural-class induction | Strong research lead; explicitly records which papers were only abstract-level verified |
| 7 | `docs/w6-coverage-experiment.md` | Detector outcomes and false-positive warnings across several languages | Useful negative evidence for pruning realizable recipes |

## Attested combinations and recipe implications

“Likely Plan” uses the current closed executable vocabulary. A specialized runtime
peeler remains outside the five-node Plan language; where required, the plan should
branch to it explicitly rather than pretending unbounded copying is an FST.

| Language | Attested construct combination | Interaction and order constraints | Likely Plan primitives / recipe family | Citations preserved from sibling research | Confidence and gaps |
|---|---|---|---|---|---|
| Indonesian / Bahasa Indonesia | `meN-` archiphoneme + place assimilation + segment-specific deletion + full reduplication + lexical/stratal exceptions | Nasal place assimilation must precede deletion of `{p,t,k,s}`. Full reduplication is morphologically introduced before the phonological cascade, but the copied surface reflects assimilation/deletion, requiring an additional copy-correspondence/renasalization rule. `per-`, loans, and monosyllabic loan stems block deletion through exception properties. | **Partitioned ordered cascade with a copying branch**: `Gate(Compose(Leaf(group lexicon/morphology), Replace(ordered meN cascade), Leaf(cleanup)))`, unioned with a reduplication/runtime-peel branch when copying is unbounded. Gate key must include the relevant MPR/exception property; the redup branch must rejoin before the same phonology or receive an equivalent group-specific `Replace`. | Black (2025), §§6.1.2.1.4–6.1.2.1.4.3; Halle & Clements (1983:125); Sneddon (1996:12–13); sibling `p6-prototype-report.md` independently traces `meN+tulis → menulis` through assimilation then deletion. | **High.** This is the best realizability benchmark. Gap: whether the copy-sensitive rule's unbounded left context can be compiled within current caps or must stay in the peeler/verification tier. |
| Indonesian / Bahasa Indonesia | Rich affixation and circumfixes + `meN-` morphophonology | A circumfix's two members obligatorily co-occur; independent prefix/suffix choices are not realizable. Phonology applies after morphological construction. | **Single morphological leaf/process, not two free leaves**, followed by `Replace(cascade)`. If morphological alternatives are separately compiled, `Union` only complete paired alternatives. | Black (2025), §§4.3, 6.1.1.3; Sneddon (1996); sibling `research/corpus/ebible/README.md`. | **High** for the structural constraint; **medium** for which corpus entries require circumfix analysis. |
| Tagalog | Infixation + partial reduplication + voice/focus morphology | `-um-` inserts after the initial consonant. Imperfective reduplication copies initial C(V) from the morphologically constructed stem; causative material can determine what counts as the stem edge. An apparent surface substring is insufficient evidence for either process. | **Branched morphology**: `Union(Compose(Leaf(ordinary/infix morphology), Replace(cascade)), Compose(Leaf(bounded-redup morphology or runtime peel), Replace(cascade)))`. Share the cascade only if boundary/interface state is preserved. POS/voice template constraints should prune incompatible branches before compilation. | Black (2025), §§1.1.6, 3.2, 3.3, 6.1.1.1–6.1.1.2; Yu (2007); sibling `research/corpus/ebible/README.md`. | **High** that the combination is attested; **medium** on exact branch allocation because HC affix-process representation may encode bounded partial redup directly while full copying requires a peel. |
| Swahili | Noun-class concord + complex subject/object agreement + verb-extension vowel-height harmony | Concord is feature-conditioned and can involve separate agreement bundles (subject/object or noun/possessor); harmony is phonological conditioning over affix variants. These are cross-cutting rather than alternative analyses. Harmony must see the selected morphemes; class agreement should prune impossible morphological combinations before the rewrite cascade. | **Feature-gated morphology plus shared harmony cascade**: `Gate(Compose(Leaf(class/POS-compatible lexicon and templates), Replace(harmony), Leaf(cleanup)))`. Partition keys should encode only attested agreement dependencies, not the full Boolean product of all classes. | Sibling `research/corpus/ebible/README.md`; `research/docs/from-scratch-8lang-status.md`; Black (2025), §§2.1.2.7 and 6.1.2. Existing sibling harmony research cites Chomsky & Halle (1968) and the HC documentation. | **Medium-high.** The sibling run reports 13/14 clean concord classes, but the precise interaction of concord and extension harmony needs direct language-grammar reverification. |
| Turkish | Agglutinative suffix chains + front/back harmony + secondary rounding harmony + case paradigms | Suffix slot order is fixed; each harmonic suffix family selects a realization from the preceding stem/word vowel class. Rounding is a second dimension and cannot safely be conflated with a binary front/back split. | **Templated leaf followed by one or two ordered/class-sensitive `Replace` stages**. Prefer shared natural-class rewrites over enumerated suffix allomorph leaves only after round-trip coverage and overgeneration improve. Gate/partition only when the rewrite compiler cannot carry the conditioning compactly. | Baker, “Two Statistical Approaches to Finding Vowel Harmony” (U. Chicago TR-2009-03); Steuer et al. (2023), [arXiv:2308.04885](https://arxiv.org/abs/2308.04885); sibling `research/gip-phonology-induction.md`; `research/docs/from-scratch-8lang-status.md`. | **High** on the language facts; **medium** on automatic class recovery. The sibling review says Baker handles transparent Finnish vowels better than Turkish rounding harmony. |
| Finnish | Front/back harmony + transparent neutral vowels | Transparent vowels do not reset the harmony controller. A nearest-vowel recipe that treats every vowel as decisive is unrealizable. | `Compose(Leaf(morphology), Replace(harmony with transparent-class context))`; retain a bounded context state or feature-class rewrite rather than partitioning lexica by last literal vowel. | Baker, U. Chicago TR-2009-03, as summarized in sibling `research/gip-phonology-induction.md`. | **Medium.** The sibling review directly records the result but the paper should be reread online before making this a conformance fixture. |
| Awngi | Floating high tone + docking + deletion | High tone docks onto a following low-tone vowel across zero or more consonants; the floating tone is then deleted. Docking **must precede deletion**, or the trigger disappears and docking never applies. | **Mandatory two-stage `Replace` cascade** in one stratum/order: `Compose(Leaf(morphology-with-floating-tone), Replace(dock, delete), Leaf(cleanup))`. Do not split into independent `Union` alternatives. Unbounded consonant context may force a capability/tier check. | Black (2025), figures 48–49; Kenstowicz & Kisseberth (1979:64); Halle & Clements (1983:93). | **High** on ordering; **medium** on current compiler realizability of the unbounded context and tone-symbol encoding. |
| Caquinte | Discontinuous future `n-…-e` + epenthetic consonants + boundary-crossing metathesis | Both future members must occur. Epenthesis repairs vowel clusters and is not a free morpheme choice. Metathesis can cross a morpheme boundary, so compiling morphology and phonology as independent alternatives loses the dependency. | **Paired circumfix/template leaf → epenthesis/metathesis cascade**. Prefer `Compose(Leaf(required paired morphology), Replace(epenthesis, bounded metathesis), cleanup)`. If metathesis combinations exceed the compiler cap, retain an honest fallback tier; do not enumerate arbitrary surface allomorph cross-products. | Black (2025), §§1.1.4, 1.1.9–1.1.10, 2.1.2.4, 3.4–3.5; Swift (1988). | **High** for the combination and co-occurrence; **medium** for exact epenthesis/metathesis order in every cited paradigm because this harvest did not reconstruct all Caquinte examples. |
| Selaru | Morphologically conditioned metathesis | Two segments exchange positions under a local context; this requires the dedicated metathesis relation rather than an ordinary one-symbol replace. | `Compose(Leaf(morphology), Replace(bounded metathesis), cleanup)` where the `Replace` implementation admits a metathesis rule under its combination cap. Do not generate factorial segment permutations. | Black (2025), §6.1.2.3; Coward & Coward (2000); Coward (2005). | **High** on construct; **medium** on current Plan implementation because PanGloss still records explicit caps and historical dropped paths for metathesis. |
| Orizaba Nahuatl | Person/number prefix ambiguity + obligatorily co-occurring plural marker + feature-conditioned stem alternation + partial reduplication | One optional-slot template overgenerates: `ti-` is both 2sg and 1pl, but 1pl additionally requires `-h`. Singular and plural require separate templates. Stem allomorph sets are conditioned by mutually exclusive feature bundles; partial reduplication is a process over the selected stem. | **Union of complete templates, then specialized morphology**: `Union(Leaf(singular template), Leaf(plural template with obligatory -h))`, composed with bounded reduplication and any shared rewrite cascade. Never independently toggle ambiguous prefix and number suffix. Feature-conditioned stem alternatives form a `Gate`/leaf partition, not a free allomorph union. | Black (2025), §§2.1.2.1–2.1.2.3, 3.7.1, 6.1.1.1; Tuggy (1991). | **High.** Directly demonstrates that syntactically possible optional-slot combinations are not realizable recipes. |
| Huallaga Quechua | Inflection → derivation → inflection layering | Inner inflection must be incomplete in a controlled way (“requires more derivation”) before outer inflection; flattening all slots into one template admits illegal orders. | **Ordered multi-leaf morphology**: `Compose(Leaf(inner inflection), Leaf(derivation), Leaf(outer inflection), Replace(phonology))`, or a single leaf whose internal automaton preserves these layers. Only ordering-compatible groupings are realizable. | Black (2025), §2.1.4; Weber (1989). | **High** on ordering. Gap: current Plan fragmentation may treat all morphology as one compiled leaf, making this a logical rather than physical recipe decision. |
| Yalálag / Isthmus Zapotec | Lexically selected inflection class + class hierarchy/subclasses + subject/object-dependent suffix co-occurrence | Shape choice is stem-class-conditioned, not phonological. A superclass allomorph may serve subclasses; subclass-specific forms require matching stems. Subject/object combinations additionally constrain suffix co-occurrence. | **Class-derived `Gate` partition around complete morphotactic leaves**, followed by shared phonology. Canonicalize inherited/default-class groups; do not compile the full product of stems × allomorphs × subject × object. | Black (2025), §§2.1.2.6–2.1.2.6.2 and ad hoc co-occurrence discussion; López & Newberg (1990); Pickett, Black & Marcial Cerqueda (2001), [SIL archive 35304](https://www.sil.org/resources/archives/35304). | **High** on class semantics; **medium** on whether all subject/object constraints can be represented without ad hoc rules. |
| Latin | Declension classes + shared allomorph across more than one class | A surface allomorph such as `-is` may be licensed by multiple classes. Duplicating it per class is semantically unnecessary and expands the plan. | `Gate` by class with canonicalized/shared leaf material, or one leaf whose entry is labeled with multiple classes; shared downstream `Replace`. | Black (2025), §2.1.2.6. | **High.** Useful deduplication fixture, though the sibling guide is the only preserved citation in this pass. |
| Spanish | Gender/number agreement + phonologically/lexically conditioned article allomorphy (`el/la`, stressed initial `a`) + lexical exceptions | Agreement features prune noun/adjective combinations; article choice uses ordered specificity: stressed-`a` conditioned form before the general feminine/default choice, then lexical exceptions as the most specific lexical override where appropriate. A single flat phonological rule is insufficient if stress or lexical exception state is not present. | **Feature/POS gate plus ordered allomorph alternatives**, likely compiled into the lexicon leaf when conditioning is local, then shared `Replace`. If lexical exceptions are MPR-like, partition them statically. | Black (2025), §§2.1.2.7, 3.1.3–3.1.4; sibling `docs/workflow.md`; Velásquez de la Cadena et al. (1974). | **Medium.** The general ordering principle is strong; the exact article analysis and stress representation should be reverified against a modern Spanish source. |
| Ket / compounds | Compounding + member-category constraints + exception-feature gating | Compound member exception semantics are OR-like, unlike the AND-like semantics for affix exception features. Reusing one generic Boolean gate calculation is incorrect. Headedness determines which member passes category. | **Compound-specific gated leaf/branch**, then `Union` with noncompound morphology and compose with shared phonology. Gate partition semantics must be construct-aware (compound OR vs affix AND). | Black (2025), §2.2 and §2.1.6. | **Medium-high.** Clear HC semantics; direct Ket grammar citation is not preserved in the digest and needs source recovery. |
| Root-and-pattern languages (guide example is schematic) | Discontinuous consonantal root + aspectual vowel melody / multiple infix slots | Default item-and-arrangement requires one infix per melody vowel in ordered template slots; process morphology can realize the melody as one rule. These are two alternative encodings of the same analysis, not combinable independent choices. | **Recipe alternative**: `Union(Leaf(enumerated ordered infix-template encoding), Leaf(process-rule encoding))` only for comparative optimization; production should select one verified branch, followed by shared phonology. | Black (2025), §§3.3.2 and 6.1.1.2.1. | **Medium.** Mechanism is clear, but the harvested sibling notes do not preserve the language/data citation for the example. |

## Cross-language constraints that prune the recipe space

These constraints are supported by multiple attested examples and should be extracted
from HC before counting “realizable” recipes.

1. **Rule order is a dependency graph, not a permutation.** Indonesian assimilation
   precedes deletion; Awngi docking precedes deletion. Any candidate reversing those
   edges is unrealizable and should never reach empirical evaluation.

2. **Morphology precedes the relevant phonology, but process morphology may expose
   special interfaces.** Tagalog reduplication/infixation and Indonesian
   reduplication are constructed before the surface cascade. A specialized branch
   must therefore preserve morpheme boundaries, copied-span identity, and the stratum
   at which it rejoins.

3. **Obligatory co-occurrence collapses Boolean products.** Caquinte future,
   circumfixes, and Nahuatl plural templates must be compiled as paired or complete
   units. The two halves are not two independent include/exclude knobs.

4. **Lexical class and phonological conditioning are different partition axes.**
   Zapotec/Latin declension choices are lexically selected; Turkish/Swahili harmony
   is phonological. Conflating them creates both bad gates and duplicate recipes.

5. **Exception semantics vary by construct.** Indonesian rule exclusions, affix
   exception features, compound-member exception features, POS restrictions, and
   stem classes cannot all share one truth-table policy.

6. **Allomorph ordering is disjunctive.** Earlier, more specific environments
   implicitly exclude later alternatives; the elsewhere form comes last. A `Union`
   of unordered allomorph transducers overgenerates unless the priority relation has
   already been compiled into the leaf.

7. **Natural-class collapse is conditional, not automatically superior.** Keep an
   archiphoneme + rewrite only when round-trip coverage is retained and
   overgeneration decreases. Turkish rounding harmony and Indonesian's custom
   orthographic classes are warnings against assuming one universal feature split.

8. **Copying is split by boundedness.** Fixed-CV partial reduplication can be regular
   and compiled as a bounded process; arbitrary full-stem reduplication requires a
   specialized search/peel branch plus full HC verification. A detected
   “reduplication” switch alone does not choose the recipe.

9. **Negative language evidence prunes whole families.** The sibling experiments
   identify Spanish reduplication/infixation and Russian infixation as detector false
   positives, while Vietnamese supplies useful correct-null cases. Productivity and
   attested-base gates must run before adding specialized branches.

## Revised bounded recipe families

The language evidence suggests replacing broad combinatorial knobs with the following
small family catalog.

| Family | Plan shape | Admitted when |
|---|---|---|
| Ordered morphophonology | `Compose(Leaf(complete morphology), Replace(dependency-ordered rules), Leaf(cleanup))` | All relevant rules are regular/compilable and share one lexical domain |
| Class/exception-partitioned cascade | `Gate(Compose(Leaf(group), Replace(group-specific ordered rules), Leaf(cleanup)))` | HC supplies a finite proven class/POS/MPR partition and dynamic feature propagation is not required across the gate |
| Complete-template alternatives | `Union(Leaf(template A), Leaf(template B), …)` then shared `Replace` | Alternatives encode obligatory co-occurrence or incompatible slot systems (Nahuatl, circumfixes), not arbitrary subsets |
| Specialized morphology branch | `Union(Compose(Leaf(ordinary), Replace(cascade)), Compose(Leaf(process morphology), Replace(cascade)))` | Infixation, bounded partial reduplication, root-and-pattern, or circumfix process genuinely needs a distinct compiled morphology |
| Hybrid copying branch | compiled ordinary plan + runtime reduplication peel/search + HC confirmation | Copied span is unbounded or copy-sensitive phonology cannot be faithfully compiled |
| Bounded metathesis cascade | morphology `Compose` `Replace(metathesis)` under explicit combination cap | Input/result slots have a finite compilation bound; otherwise honest fallback |
| Layered morphology | `Compose(Leaf(inner layer), Leaf(derivation), Leaf(outer layer), Replace(cascade))` | Stratal/template order is linguistically significant and is not already internal to one morphology leaf |

This catalog is deliberately not the product of all seven families. HC construct
dependencies choose one base family and add only demonstrated gates/branches.

## Consequences for the optimizer proposal

1. Compute the **attested construct dependency graph** before computing search-space
   magnitude. Nodes should include morphological layers/processes, rule strata,
   lexical partitions, and exception domains; edges should include order, required
   co-occurrence, mutual exclusion, and interface-state dependencies.

2. Count only candidates generated by a language-backed family whose prerequisites
   are present. For example, `reduplication=true` does not admit a full-stem FST
   branch until copied-span boundedness is known.

3. Treat HC as more than a source of counts. It supplies semantic facts that make
   most recipes impossible: document order/strata, template slots, category
   restrictions, MPR policies, allomorph environments, process-rule shape, and
   metathesis/copy bounds.

4. Preserve complete linguistic units during canonicalization. Deduplicate Latin's
   shared `-is` material, but do not separate a circumfix, split a Nahuatl template,
   or hoist a rewrite across a gate when its trigger/property does not cross that
   interface.

5. Evaluate the realizable space in two stages:

   - **Static realizability:** dependency/order/co-occurrence/capability checks.
   - **Empirical realizability:** full HC confirmation for recall and soundness,
     including lexical exceptions and copy-sensitive outputs.

6. Use Indonesian as the principal adversarial fixture. A candidate generator that
   cannot express its assimilation → deletion ordering, reduplication interaction,
   and exception partitions is not searching the space of actual grammars, however
   elegant its search algorithm may be.

## Citation ledger for online reverification

The following citations/links were already present in the harvested sibling sources.
They should be reverified before being treated as final bibliography.

- H. Andrew Black. *A Conceptual Introduction to Morphological Parsing for
  FieldWorks Language Explorer*. SIL, 3 July 2025. Local full-text extraction in the
  sibling repository.
- SIL FieldWorks, [HermitCrab parser documentation](https://downloads.languagetechnology.org/fieldworks/Documentation/en/User_Interface/Menus/Parser/Parsing_words_(HermitCrab).htm).
- H. Andrew Black, “An Efficient Parsing Technique for Lexical Functional Grammar,”
  sibling architecture link labeled
  [ordered-rules parsing](https://arxiv.org/pdf/cmp-lg/9411015) — title/authorship
  should be checked from the paper.
- Morris Halle and G. N. Clements. *Problem Book in Phonology*. 1983.
- James Neil Sneddon. *Indonesian: A Comprehensive Grammar*. Routledge, 1996.
- Kenneth Swift. *Morfología del Caquinte*. 1988.
- David and Naomi Coward. “A Phonological Sketch of the Selaru Language.” 2000.
- David Forrest Coward. *An Introduction to the Grammar of Selaru*. 2005 manuscript.
- David Tuggy. *Curso del Nájuatl Moderno*. 1991.
- David John Weber. *A Grammar of Huallaga (Huánuco) Quechua*. 1989.
- Filemón López L. and Ronaldo Newberg Y. *La Conjugación del Verbo Zapoteco;
  Zapoteco de Yalálag*. 1990.
- Velma B. Pickett, Cheryl Black, and Vicente Marcial Cerqueda. *Gramática Popular
  del Zapoteco del Istmo*, 2nd ed., 2001,
  [SIL archive](https://www.sil.org/resources/archives/35304).
- Alan C. L. Yu. *A Natural History of Infixation*. Oxford University Press, 2007.
- Adam C. Baker. “Two Statistical Approaches to Finding Vowel Harmony,” University
  of Chicago Technical Report TR-2009-03.
- David Steuer, Johann-Mattis List, M. Abdullah, and Dietrich Klakow.
  “Information-Theoretic Characterization of Vowel Harmony: A Cross-Linguistic
  Study on Word Lists,” SIGTYP 2023,
  [arXiv:2308.04885](https://arxiv.org/abs/2308.04885).
- Chomsky & Halle (1968), Kenstowicz & Kisseberth (1979), and Kaplan–Kay-style
  finite-state rewrite work are cited by the sibling architecture/Plan research but
  need a normalized bibliography entry in the parent review.

## Known gaps

- This harvest intentionally did not browse; the parent review should confirm the
  strongest language-specific claims against primary grammars.
- The Indonesian account is unusually complete. Swahili concord + extension harmony,
  Spanish article allomorphy, Ket compound restrictions, and the unnamed
  root-and-pattern example need targeted primary-source verification.
- The sibling harmony review explicitly marks parts of Calamaro & Jarosz, Steuer et
  al., and related work as abstract-level or extraction-limited. Those limitations
  carry forward.
- Corpus detector outputs are evidence about the current system, not proof of a
  language fact. They are used here chiefly to require productivity gates and correct
  null handling.
