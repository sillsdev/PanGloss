# Representative typology basis for conformance fixtures

## Preamble

**Purpose.** This document maps every morphological/phonological construct this project's
conformance suite must cover to the typological pattern(s) that genuinely, attestedly exhibit it in
the world's languages. It supplies the *shape* a synthetic fixture should have — how many strata,
which affix positions, what conditions what, what makes the interaction non-trivial and a good FST
stress case. It is the research basis a fixture author reads *before* inventing a `grammar.xml` +
`words.yaml` pair; it is not itself a fixture and contains no fixture content.

**The hard rule this document (and everything built from it) must respect.**
- Fixture/identifier/filename names use **construct or typology ADJECTIVES only**
  (agglutinative, fusional, templatic, polysynthetic, suffixing, prefixal, isolating, tonal,
  recursive, directional, unbounded, replacive, …). **Language- or family-NOUN names are forbidden
  in names** — no "turkic", "latin", "inuit", "quechua", "semitic", "bantu", "athabaskan", etc.,
  anywhere in a filename, identifier, or fixture name.
- A language family or specific language **may be named in a COMMENT**, for context — exactly what
  this document does throughout (e.g. "attested in Rotuman", "the Bantu noun-class system") — that
  is prose, not a fixture name, and is explicitly permitted.
- **No real-language DATA, ever.** This document describes *shapes* ("long-distance backness
  harmony across an agglutinative suffix chain", "a self-feeding `CompoundingRule` whose output
  part-of-speech re-enters its own input part-of-speech") and cites the families that motivate them
  in prose. It transcribes **no real wordlists, no real paradigms, no real morpheme inventories, no
  real lexemes** for reuse as fixture content anywhere below. Every fixture eventually authored from
  this document must invent its own forms matching the described shape — never copy a form quoted in
  a cited source into a `grammar.xml`/`words.yaml`.

**How to use this document.** Section 1 is the construct-by-construct research (organized as
Covered / Needs-a-new-pattern / Permanent-carve-out / No-basis-found). Section 2 is the summary
table. When authoring or extending a fixture per the `conformance-grammars` skill, start from the
matching entry below, read its cited sources for the real structural shape, then invent synthetic
forms that instantiate that shape — never the sources' own examples.

**Method.** The construct vocabulary combines two sources, per the assignment:
`rust/crates/pg-foma/src/capability.rs`'s 20 `CharacteristicKind` variants (the compiler's own
capability-envelope inventory, `CharacteristicKind::ALL`), and `machine/conformance/constructs.txt`'s
25-row conformance checklist (12 of whose rows have no `CharacteristicKind` at all — see
`openspec/changes/plan-construct-coverage-completion/design.md` D3). Coverage status below was
checked directly against every `exercises:` value actually present in
`machine/conformance/languages/*/words.yaml` and `machine/conformance/edge-cases/*/words.yaml`
(not inferred) — a construct is marked "covered" only if some existing fixture's `words.yaml`
actually cites it.

---

## 1. Construct-by-construct basis

### 1.1 Already representatively covered — no new pattern needed

These 21 of 25 `constructs.txt` rows are each cited by at least one existing
`machine/conformance/languages/*` or `edge-cases/*` fixture's `exercises:` list today, and the
existing fixture's own pattern is typologically well-motivated (not a toy-grammar artefact). No new
fixture is proposed for any of these; each entry below is deliberately brief per the assignment's own
"the gap list matters more than the covered list."

| `constructs.txt` row | Existing fixture(s) | The representative pattern already instantiated |
|---|---|---|
| `Stratum (Linear/Unordered rule order)` | all 8 languages | Ordered ("linear") derivation-then-inflection strata vs. unordered ("free") rule sets within one stratum — the core Stratal-Phonology-style level-ordering distinction (Kiparsky 1982; see §1.2's fuller citation under Multi-table, which shares this literature). |
| `AffixProcessRule: prefix/suffix/circumfix/infix` | `suffixing-*` (suffixing), `prefixal-discontinuous-slot-dependency` (prefixing), `metathesis-phase-isolation`/`polysynthetic-stratal-derivation-chain` (infixing, circumfixing e.g. a `ge-...-t`-shaped discontinuous affix) | The four canonical affix-position types (WALS Ch. 26, Dryer, "Prefixing vs. Suffixing in Inflectional Morphology": https://wals.info/chapter/26). |
| `AffixProcessRule: reduplication (ReduplicationHint)` | `metathesis-phase-isolation`, `suffixing-extension-slot-ordering` | Canonical partial/total reduplication (WALS Ch. 27, Rubino: https://wals.info/chapter/27). |
| `AffixProcessRule: subtraction/truncation` | `metathesis-phase-isolation`, `conformance-staging/edge-cases/truncate-morphotactic` | Subtractive/truncating morphology (a marked but well-documented process type; folded into `CharacteristicKind::Affixation` per design.md D3's own judgment call — no distinct characteristic exists). |
| `RealizationalAffixProcessRule` | `fusional-realizational-morphology` | Word-and-paradigm / realizational exponence: ablaut as sole exponent, syncretism, inflection-class-conditioned blocking (Stump 2001 *Inflectional Morphology: A Theory of Paradigm Structure*; realizational-morphology overview: https://grokipedia.com/page/realizational_morphology). |
| `CompoundingRule` | `fusional-realizational-morphology`, `polysynthetic-stratal-derivation-chain`, `conformance-staging/edge-cases/compounding-non-recursive` | Endocentric head/non-head compounding with a bounded (`MaxApplicationCount`-capped) application count — the recursive/self-feeding configuration closed 2026-07-26, tasks.md 4.1 — see §1.2 Compounding for the pattern basis. |
| `MorphologicalOutputAction: CopyFromInput/InsertSegments` | all 8 languages | Ordinary concatenative affixation output. |
| `MorphologicalOutputAction: ModifyFromInput/InsertSimpleContext` | `fusional-realizational-morphology` (ablaut via `ModifyFromInput`), `templatic-root-modification` (`InsertSimpleContext`) | Non-concatenative stem modification: ablaut/mutation as the sole realizer of a feature, and template-internal vocalic insertion (root-and-pattern morphology; Britannica "Root-and-pattern system": https://www.britannica.com/topic/root-and-pattern-system). |
| `RewriteRule Iterative (epenthesis/deletion/feature/expansion/merge)` | `suffixing-vowel-harmony`, `templatic-root-modification`, `polysynthetic-stratal-derivation-chain` | Iterative (rule-reapplies-until-no-longer-applicable) phonological rule application — the classic SPE/Chomsky-Halle iterative-rule model. |
| `RewriteRule Simultaneous` | `templatic-root-modification`, `conformance-staging/edge-cases/simultaneous-epenthesis-cascade` | Simultaneous (all-environments-checked-against-the-input, not the derived output) rule application, contrasted with iterative — see §1.2 for the still-open genuinely-overlapping-subrule configuration, which is oracle-gated, not pattern-gated. |
| `MetathesisRule` | `metathesis-phase-isolation` | Adjacent-segment transposition (left-to-right direction) — see §1.2 Metathesis for the still-open right-to-left configuration. |
| `Affix template slots (obligatory/disjunctive/ordering)` | `prefixal-discontinuous-slot-dependency`, `suffixing-extension-slot-ordering` | CARP-style (Closing-Aspect-Root-Peripheral) fixed position-class templates with obligatory/optional and mutually disjunctive slots — the classic Athabaskan-style verb-template literature (Rice, *Morpheme Order and Semantic Scope*, on position-class templates) and Bantu-style CARP extension-ordering literature motivate this shape; cited here by pattern, not reproduced. |
| `NaturalClass: Segments vs FeatureNaturalClass/SegmentNaturalClass precision` | `conformance-staging` predecessor / `edge-cases/strrep-identity` | Feature-bundle vs. bare-segment-list natural classes — a representational, not typological, distinction; already exercised on a feature-less grammar to probe the identity-vs-feature boundary directly. |
| `Boundary markers (CharacterDefinitionTable)` | `edge-cases/loader-pattern-shapes` | Morpheme-boundary symbols in a character-definition table (a HermitCrab-specific representational device, exercised directly). |
| `MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule` | `templatic-root-modification` (OCP-style co-occurrence), `suffixing-evidential-adjacency-chain` (morpheme-adjacency) | Obligatory Contour Principle-style root co-occurrence restrictions (McCarthy 1986, "OCP Effects: Gemination and Antigemination") and adjacency-conditioned suffix-order constraints. |
| `MPR features/groups` | `suffixing-extension-slot-ordering`, `prefixal-discontinuous-slot-dependency` | Morphological/phonological-rule (MPR) feature gating of allomorph/rule choice by a lexically-preset agreement class or a rule-set feature, including discontinuous (non-adjacent-slot) dependency. |
| `Guesser/LexicalGuess` | `polysynthetic-stratal-derivation-chain` | Unknown-root guessing from an affix-template's own morphotactic shape (a parser-engineering device motivated by the general "unlisted stem" problem every broad-coverage analyzer faces). |
| `Disjunctive allomorphs / free-fluctuation` | `edge-cases/disjunctive-recheck`, `conformance-staging/edge-cases/template-category-sharing` | Free variation between two equally-grammatical allomorphs/analyses of one surface form (the "two distinct roots that happen to share a surface shape" multiplicity case). |
| `Stem names` | `templatic-root-modification`, `suffixing-extension-slot-ordering` | Lexically-listed conjugation/declension classes selecting among competing realizational subrules (the classic "stem class" / inflection-class literature). |
| `Syntactic feature agreement (RequiredHeadFeatures/OutputHeadFeatures/RequiredFootFeatures/OutputFootFeatures)` | `fusional-realizational-morphology` | Head/foot syntactic-feature agreement gating rule application (agreement morphology broadly). |
| `Alpha-variable phonological environments (VariableFeature/AlphaVariables)` | `suffixing-vowel-harmony` | Alpha-variable (feature-copying) rule environments — the mechanism underlying long-distance vowel/backness harmony (Nevins 2010, *Locality in Vowel Harmony*, is the standard reference for the underlying phenomenon; not reproduced here). |
| `CompoundingRule constraints (MaxApplicationCount/Blockable/head-nonhead syntactic features)` | `fusional-realizational-morphology` | A capped compound-of-compounds (`MaxApplicationCount` rejecting a third application) plus head/non-head syntactic-feature-gated compounding. |
| `Ordinary/realizational rule constraints (MaxApplicationCount/RequiredStemName/Blockable)` | `suffixing-extension-slot-ordering` | Rule-level application constraints layered onto ordinary/realizational rules. |

Two `CharacteristicKind`s whose **closed configuration** is already covered this way, with an
**open** configuration that is a genuine gap, are cross-referenced into §1.2 rather than duplicated
here: `SimultaneousRewrite` (RewriteRule Simultaneous row — closed non-overlap case covered; open
genuine-overlap case is oracle-gated, see §1.3), and `CircumfixOutputAction`/`Reduplication` (both
close via the rows above for their in-scope split; their residual open splits are in §1.2/§1.3
respectively).

### 1.2 Constructs that still need a new or additional representative pattern

These are the genuine gaps: either a `CharacteristicKind`'s **open** (`Refuse`) configuration has no
representative fixture anywhere, or a construct is structurally `Unmappable` (no `constructs.txt`
row names it at all — design.md D5) and therefore has never had an occasion to receive one.

#### 1.2.1 Compounding — recursive/self-feeding configuration

1. **Typological pattern.** Recursive endocentric compounding, where a compound's own output
   part-of-speech is identical to (or a supertype of) one of its own input parts-of-speech, so the
   compounding rule can in principle re-apply to its own output indefinitely. This is the ordinary
   engine behind English-style compound stacking ("state government tax policy committee meeting")
   and is described as a general property of N→NN-recursive compounding rules in the compounding
   literature (compound-noun recursion overview: https://www.numberanalytics.com/blog/compounding-linguistic-typology-ultimate-guide;
   a worked corpus study of recursive nominal/adjectival compounding: "The types and categories of
   Old English recursive compounding," https://www.researchgate.net/publication/318733335;
   general treatment: https://grokipedia.com/page/Compound_(linguistics)). Germanic (English,
   German, Dutch) and Sinitic compounding are the most commonly cited families for unbounded
   endocentric N-N stacking; the phenomenon is not family-specific, just most-discussed there.
2. **Structural shape.** One `CompoundingRuleDef` whose `headPartsOfSpeech`/`outputPartOfSpeech`
   overlap with (or restate) its own `nonHeadPartsOfSpeech`/`headPartsOfSpeech` input set — i.e. the
   rule's output PoS re-enters its own input PoS, unlike every existing fixture's compounding rules
   (which keep input/output PoS disjoint, `fusional-realizational-morphology`'s `posCompH` being the
   closest precedent, but capped by `MaxApplicationCount`). The stress case wants the rule's DTD
   `multipleApplication` left uncapped (or capped high enough to matter) so a stratum can, in
   principle, build `((root+root)+root)+root...` to arbitrary depth.
3. **What makes it a good FST stress case.** Unbounded self-recursion is exactly the shape a
   finite-state proposer must refuse gracefully or bound explicitly — it is the direct morphological
   analogue of `QuantifierPattern`'s unbounded-quantifier configuration (already closed, §1.2.6) and of
   `UnorderedMorphRuleApplication`'s chain-depth budget (already closed, §1.1): does the compiler's
   `CompoundingRecursionSafePredicate` correctly distinguish "the rule *could* recurse" from "the
   rule *actually* self-feeds in this grammar," and does the depth-budgeted construction the design
   doc proposes (a rule-graph reachability pass + depth-budgeted cross product, mirroring the
   existing `unordered`/`peel` depth-budget shape) stay recall-preserving up to its budget and refuse
   honestly beyond it?
4. **Proposed fixture name:** `recursive-endocentric-compounding` (an `edge-cases/` fixture per
   `openspec/changes/plan-construct-coverage-completion/design.md` item 2's own ask — "a stratum
   whose `CompoundingRule` output PoS re-enters its own input PoS").
5. **Citations:** https://www.numberanalytics.com/blog/compounding-linguistic-typology-ultimate-guide;
   https://www.researchgate.net/publication/318733335_The_types_and_categories_of_Old_English_recursive_compounding;
   https://grokipedia.com/page/Compound_(linguistics).
   *(Low-trust: an SEO blog, a ResearchGate mirror, and a wiki. Superseded for the empirical claim by
   §1.2.1a's peer-reviewed sources below; retained only as the trail for the FORMAL claim, which
   §1.2.1a now cites properly.)*

##### 1.2.1a The FORMAL claim and the ATTESTED claim are different claims

Item 1 above makes a **formal** claim — the rule is "recursively applicable without limit", so a
stratum can "in principle, build `((root+root)+root)+root...` to arbitrary depth". That claim is
correct and is what a compiler must refuse gracefully. It is **not** a statement about how deep
compounds actually go, and conflating the two is how a construction ends up sized for a depth nothing
observes. Both halves, separated, with what the literature does and does not support:

**FORMAL — unbounded, and someone has said so in exactly these terms.** Lauer (1995, PhD thesis §2.2)
on the rule `N → N N`: it "captures the gross syntactic behaviour of compounds and is recursively
applicable without limit," and "Unboundedness is a property of generative competence, not its
application in performance."

**ATTESTED — sharply skewed to 2 members, and NO published depth histogram exists for any language.**
This is the substantive finding, and the negative half of it matters as much as the positive half:

| Language | Measured distribution | Source |
|---|---|---|
| Sanskrit | 71.4% two-member, **94.1% at ≤3 members**, 98.5% at ≤4, 1.5% at 5+ (n = 29,966 instances, manually tagged 150K-word corpus) | Kulkarni & Kumar 2011, Table 1 |
| Sanskrit | "more than 41% of compounds have 3 or more components" — **conflicts with the row above** (which gives 28.6%); different corpus (Digital Corpus of Sanskrit) and a different threshold for what counts as a compound. Recorded as a disagreement, not averaged. | Krishna et al. 2016, p. 1 |
| German | 86.2% two-member (64,627 of 74,983 GermaNet noun-compound **lexicon types**; not corpus tokens, and "multiple split" means only "≥3 constituents", not a histogram) | Sugisaki & Tuggener 2018 |
| English | **No published member-count histogram.** Every corpus study of N+N stacking restricts itself to THREE-noun compounds because 4+ are too sparse to extract reliably — stated qualitatively, never tabulated. | Ziering & van der Plas 2014; Nakov 2007; Vadas & Curran 2011 |
| Icelandic | 7-constituent forms exist and are "rare" — no number attached | Daðason et al. 2020 |
| Dutch, Finnish, Hungarian, Greek, Chinese | **Not found.** Dutch/CELEX work reports *linking-element* statistics, never constituent counts; Finnish only compound *rate*. | — |

**Maximum attested member counts are much higher than any "practical ceiling":** 16 in Sanskrit
(Sandhan et al. 2023's NeCTIS dataset spans 2–16 components across 17,656 nested spans), 7 in
Icelandic. **So the literature does not license a hard constant, and this document does not assert
one.** What it licenses is a percentile, and only Sanskrit's is actually measured.

**The spec's own caution — "flat concatenation is NOT deep nesting" — checked, and the literature
comes down against it.** Multi-member compounds are analysed as recursively **binary**, and flat
treatments are consistently described as tooling or annotation-scheme limitations rather than
linguistic claims: Kulkarni & Kumar 2011 ("formed with two words at a time and hence they can be
represented faithfully as a binary tree"), Sandhan et al. 2023 ("a valid solution for an N-component
compound consists of N − 1 nested spans"), Daðason et al. 2020 ("can be represented by a full binary
tree"), Henrich & Hinrichs 2011 (criticising German splitters that offer "an analysis of all
component parts of a compound at once without any grouping of subconstituents"). Two pieces of
*empirical* evidence that the nesting is real rather than an analyst's artifact:

- The Penn Treebank originally annotated NP interiors **flat**, and that had to be undone: of 60,959
  ambiguous NPs, **23,129 (37.94%) required brackets to be inserted** (Vadas & Curran 2011).
- Parser F1 **collapses** as members increase — 93.66 for 3-component Sanskrit compounds versus
  **65.4 for 4-component** (Kulkarni & Kumar 2011) — which only makes sense if the grouping is
  genuinely ambiguous. The number of possible groupings is the Catalan number
  `C_n = (2n)!/((n+1)!·n!)`.
- Branching direction is majority-but-not-uniformly left in English (67% left / 33% right on Lauer's
  244 three-noun compounds; 59%/41% on 5,569 PTB three-word compounds, Vadas & Curran 2011) — real
  branching, not a single chain shape. Henrich & Hinrichs 2011's own German examples include a
  *balanced* depth-2 tree (`Autobahnanschlussstelle` = (Auto+Bahn)(Anschluss+Stelle)) alongside a
  left-nested one (`Kraftfahrzeugsteuer` = ((Kraft+Fahrzeug)+Steuer)).

So under the standard binary immediate-constituent analysis, Sanskrit's 94.1%-at-≤3-members
corresponds to **nesting depth ≤ 2** for ~94% of compounds, and German's 86.2%-two-member to **depth
1** for ~86%. That is the honest form of "at most two, maybe three": a percentile derived from
member counts under an assumption the literature states but never itself measures.

**One explicitly proposed constant exists, and it is theoretical rather than measured.** Marcus
(1980), via Lauer 1995 §2.2: processing limits mean the general rule "is not quite idempotent" —
structures in which three or more nouns all premodify another noun are prohibited, i.e. "the rewrite
rule cannot be reapplied to its right member more than twice." That is a parsing-theory proposal,
not a corpus result, and should be cited as such.

**Deliberately NOT transferred:** Karlsson (2007) is the canonical "formally unbounded, empirically
capped at 3" result (maximal multiple clausal centre-embedding is 3 in written language, effectively
0 in speech, across Danish, English, German, Latin and Swedish). It is about **clause embedding**,
not compounding. It is named here only so that nobody silently imports its "3".

**What this repo's own engine does, which is the operative number and needs no literature at all.**
`pg_rules::stratum::AnalyzerConfig::max_stem_count` — C#'s `Morpher.MaxStemCount`, ctor default **2**
(`Morpher.cs:56`) — makes `StratumAnalyzer::apply_one_mrule` reject a `Compounding` rule as soon as
`non_heads.len() + 1 >= max_stem_count`. C#'s own `CompoundingRuleTests.SimpleRules` raises it to **3**
for a genuine three-root compound (`cs:87,105`). So the reference implementation's answer is 2 by
default and 3 when someone means it, which happens to land exactly where the Sanskrit percentile does.
`pg_foma::capability::compounding_max_depth`'s doc carries the separate, measured note on why the
compiler's own `max_depth` (a rule-COUNT ceiling: 9 for a `sena`-shaped grammar with 8 non-repeatable
compounding rules) must not be mistaken for this figure.

**Citations (all peer-reviewed proceedings, a journal article, or an institutionally-hosted thesis):**

- Kulkarni, Amba, and Anil Kumar. 2011. "Statistical Constituency Parser for Sanskrit Compounds." *Proceedings of ICON-2011*. https://sanskrit.uohyd.ac.in/faculty/amba/PUBLICATIONS/papers/samaasa_const_parser_icon2011.pdf
- Krishna, Amrith, Pavankumar Satuluri, Shubham Sharma, Apurv Kumar, and Pawan Goyal. 2016. "Compound Type Identification in Sanskrit: What Roles do the Corpus and Grammar Play?" *WSSANLP 2016*, 1–10. https://aclanthology.org/W16-3701/
- Sandhan, Jivnesh, Yaswanth Narsupalli, Sreevatsa Muppirala, Sriram Krishnan, Pavankumar Satuluri, Amba Kulkarni, and Pawan Goyal. 2023. "DepNeCTI: Dependency-based Nested Compound Type Identification for Sanskrit." *Findings of the ACL: EMNLP 2023*, 13679–13692. https://aclanthology.org/2023.findings-emnlp.914/
- Lauer, Mark. 1995. *Designing Statistical Language Learners: Experiments on Noun Compounds.* PhD dissertation, Macquarie University. arXiv:cmp-lg/9609008. https://arxiv.org/abs/cmp-lg/9609008
- Lauer, Mark. 1995. "Corpus Statistics Meet the Noun Compound: Some Empirical Results." *ACL 1995*, 47–54. https://aclanthology.org/P95-1007/
- Vadas, David, and James R. Curran. 2011. "Parsing Noun Phrases in the Penn Treebank." *Computational Linguistics* 37(4): 753–806. https://aclanthology.org/J11-4006/
- Ziering, Patrick, and Lonneke van der Plas. 2014. "What good are 'Nominalkomposita' for 'noun compounds'…" *COLING 2014*, 1047–1058. https://aclanthology.org/C14-1099/
- Henrich, Verena, and Erhard Hinrichs. 2011. "Determining Immediate Constituents of Compounds in GermaNet." *RANLP 2011*, 420–426. https://aclanthology.org/R11-1058/
- Sugisaki, Kyoko, and Don Tuggener. 2018. "German Compound Splitting Using the Compound Productivity of Morphemes." *KONVENS 2018*, 141–147. https://konvens.org/proceedings/2018/PDF/konvens18_16.pdf
- Daðason, Jón Friðrik, David Erik Mollberg, Hrafn Loftsson, and Kristín Bjarnadóttir. 2020. "Kvistur 2.0: a BiLSTM Compound Splitter for Icelandic." arXiv:2004.07776. https://arxiv.org/abs/2004.07776
- Nakov, Preslav I. 2007. *Using the Web as an Implicit Training Set…* PhD dissertation, UC Berkeley. UCB/EECS-2007-173. https://www2.eecs.berkeley.edu/Pubs/TechRpts/2007/EECS-2007-173.pdf
- Karlsson, Fred. 2007. "Constraints on multiple center-embedding of clauses." *Journal of Linguistics* 43(2): 365–392.
- Marcus, Mitchell P. 1980. *A Theory of Syntactic Recognition for Natural Language.* MIT Press. **Cited via Lauer 1995 — not independently verified here.**
- Finin, Timothy W. 1980. *The Semantic Interpretation of Compound Nominals.* PhD dissertation, UIUC. **Cited via Lauer 1995 — not independently verified here.**

**Two verification caveats, recorded rather than glossed.** The Lauer §2.2 strings above were
recovered from a search index over the thesis PDF; full-text extraction truncated before reaching that
section, so **page numbers should be checked against the PDF before quoting verbatim**. Sandhan et
al.'s "2 to 16 components" range came from text extraction of the Figure 5 discussion rather than from
the rendered figure. Both remain worth citing; neither should be quoted as though rendered.

**Covered already? No** — `compounding-non-recursive` (staged) deliberately avoids this configuration
by name and design; no fixture anywhere exercises genuine self-feeding compounding.

#### 1.2.2 Right-to-left rewrite — additional in-scope pattern shapes

1. **Typological pattern.** Directional (non-simultaneous, non-iterative-from-the-left) phonological
   rule application that scans right-to-left, most commonly attested in stress-assignment and
   syllabification systems: end-rule-right/iterative-right-to-left foot construction is one of the
   two basic parameters in every metrical-stress typology (Hayes 1995, *Metrical Stress Theory*;
   overview: https://www.ling.upenn.edu/~gene/courses/530/readings/Hayes2009_ch14.pdf; WALS Ch. 15
   "Weight-Sensitive Stress," Goedemans & van der Hulst: https://wals.info/chapter/15), and
   right-to-left iterative vowel harmony/deletion is a standard case study in directional
   rule-ordering (see the general directionality-in-phonology literature, e.g. "Directional Effects
   in Phonological Theory": https://www.academia.edu/2131712/Directional_Effects_in_Phonological_Theory_Dissertation_Chapter_4_).
2. **Structural shape.** The existing `RightToLeftRewriteFaithfulReversalPredicate`/
   `compile_rtl_branch_net` construction is already faithful for in-scope pattern shapes. The
   remaining open gaps are specifically pattern shapes it still excludes: a bare `Segments` (rather
   than `FeatureNaturalClass`) LHS/RHS, an `Anchor` (word-edge) constraint combined with
   `Dir::RightToLeft`, or a disagreeing-polarity alpha-variable. The bounded and genuinely
   unbounded `Quantifier` shapes are now covered by their own fixtures, including
   `right-to-left-bounded-quantifier-rewrite`.
3. **What makes it a good FST stress case.** Right-to-left scanning combined with a quantifier or an anchor is exactly where a reversal-based compilation strategy (reverse the tape,
   apply an LTR-equivalent rule, reverse back) can silently mis-handle an edge condition that only
   makes sense relative to the *original* (non-reversed) string edges, or a quantifier whose bound
   must be re-derived post-reversal. It is a direct, per-shape stress test of whether the "safety-net
   union" argument that makes the reversal recall-preserving still holds once anchors/quantifiers/
   alpha-polarity enter the picture.
4. **Fixture name:** `right-to-left-bounded-quantifier-rewrite` (for the `Quantifier` shape;
   name subsequent per-shape fixtures analogously, e.g. `right-to-left-anchored-rewrite`,
   `right-to-left-segment-literal-rewrite`, `right-to-left-alpha-disagreement-rewrite` — one
   `edge-cases/` fixture per newly-supported shape, per design.md's own fixture-enumeration rule D4).
5. **Citations:** https://www.ling.upenn.edu/~gene/courses/530/readings/Hayes2009_ch14.pdf;
   https://wals.info/chapter/15;
   https://www.academia.edu/2131712/Directional_Effects_in_Phonological_Theory_Dissertation_Chapter_4_.

**Covered already?** The quantifier shape is closed by `right-to-left-bounded-quantifier-rewrite`;
the 3 named fixtures already cover the original in-scope RTL cases
(`rtl_plain_rule...`, `rtl_feature_environment_swap...`, `rtl_deletion...`, per design.md's own
citation). The remaining excluded shapes above are still open.

#### 1.2.3 Left-to-right / right-to-left rewrite directionality as its own tagged phenomenon

`LeftToRightRewrite` and `RightToLeftRewrite` are both `Unmappable` today (no `constructs.txt` row
tags *direction itself* as a phenomenon distinct from iterative/simultaneous application order —
design.md D5). `LeftToRightRewrite` itself is `Proven` (no compiler gap), but the pattern that
motivates a dedicated row is the same one in §1.2.2: metrical-stress and vowel-harmony systems
parameterize on rule-application direction *independently* of iterativity (Hayes 1995's
end-rule-left/end-rule-right parameter, cited above, is exactly this independent axis). No new
fixture is proposed for the `LeftToRightRewrite` half specifically — every existing rewrite-rule
fixture already exercises left-to-right application by default, so the representative pattern is
already latent in every one of them; it merely lacks its own checklist row (upstream task, D5, not a
fixture-authoring gap).

**Covered already?** Latent pattern: yes (every LTR rewrite rule in every fixture). Tagged as its own
phenomenon: no — blocked on the upstream `constructs.txt` PR (design.md D5), not on missing pattern
research.

#### 1.2.4 Metathesis — right-to-left direction

1. **Typological pattern.** Directional metathesis where the transposition itself is triggered by
   material to the *right* of the affected segments and resolves leftward — Rotuman's
   morphologically-conditioned metathesis (word-final V-C transposition, analyzed as leftward
   reassociation of an orphaned feature matrix at the CV-skeleton level once a final vowel truncates)
   is the standard textbook case of directional metathesis
   (Hume & Seyfarth, "Metathesis," handbook chapter: https://scottseyfarth.com/docs/HumeSeyfarth.pdf;
   Hume, "Metathesis: Formal and Functional Considerations": https://metathesisinlanguage.osu.edu/pdfs/hume_metathesisS5.pdf).
   The general metathesis-typology literature treats directionality as one of the core parameters
   distinguishing attested metathesis patterns (see also the general survey:
   https://www.researchgate.net/publication/372327706_Metathesis).
2. **Structural shape.** A `MetathesisRule` with `Dir::RightToLeft` — today's
   `compile_metathesis_rule` construction is scoped, by its own doc, to `Dir::LeftToRight` only; there
   is no partial RTL attempt to extend, unlike right-to-left rewrite. The representative shape needs
   a rule whose two swap-targets are identified relative to a right-anchored trigger (mirroring
   Rotuman's word-final conditioning) rather than a left-anchored one.
3. **What makes it a good FST stress case.** A from-scratch directional swap-relation construction
   (not an extension of an existing partial one) is exactly the case where a naive "just reverse the
   tape" strategy is tempting but wrong for the same edge-condition reasons as §1.2.2 — metathesis's
   swap relation is itself asymmetric (which segment moves where), so reversal changes which
   segment counts as "moving" relative to the word edge.
4. **Proposed fixture name:** `right-to-left-metathesis-reversal`.
5. **Citations:** https://scottseyfarth.com/docs/HumeSeyfarth.pdf;
   https://metathesisinlanguage.osu.edu/pdfs/hume_metathesisS5.pdf;
   https://www.researchgate.net/publication/372327706_Metathesis.

**Covered already? No.** design.md itself marks this **NEEDS-DECISION** (is a from-scratch RTL
metathesis construction worth building at all, given its rarity, or a permanent scope boundary?) —
this document supplies the pattern basis for if/when that decision is made; it does not resolve the
decision itself.

#### 1.2.5 Multi-table — shared-representation-across-tables configuration

1. **Typological pattern.** Two coexisting phonological/orthographic subsystems within one language
   that legitimately share a surface grapheme/segment representation while belonging to distinct
   rule sets — the general phenomenon Lexical/Stratal Phonology models as separate "strata" (native
   vs. borrowed vocabulary, or stem-level vs. word-level phonology) that can differ in their
   phonological rule sets while drawing on an overlapping symbol inventory (Kiparsky, "Stratal OT: A
   synopsis and FAQs": https://web.stanford.edu/~kiparsky/Papers/taipei.2014.pdf; loanword-phonology
   overview showing native/loan strata sharing most, but not all, of their segment inventory:
   https://users.castle.unc.edu/~jlsmith/home/pdf/smith2024_CHoP2_LoanwordPhonology_circulate.pdf;
   "Disjunctive Lexical Stratification": https://muse.jhu.edu/article/369835/summary). English's own
   native/Latinate stress-and-affixation split (Kiparsky 1982) is the most commonly cited example of
   exactly two strata sharing most of one alphabet while differing in which phonological rules apply.
2. **Structural shape.** `Grammar::char_tables.len() > 1`, with each stratum's own `StratumDef::table`
   pointing at a *different* `CharacterDefinitionTable`, but this time the two tables are NOT
   pairwise-disjoint in their representations — some literal spelling (e.g. a digraph or a single
   letter) is a legitimate `SegmentDefinition` in BOTH tables, denoting a different segment identity
   in each (mirroring how "native" vs. "borrowed" strata can assign the same spelled sequence
   different phonotactic/phonological behavior). The two existing `MultiTable`-covered fixtures keep
   the two tables' representations disjoint by construction; this is the one remaining case those
   fixtures were built to exclude.
3. **What makes it a good FST stress case.** A shared representation across two tables is exactly the
   ambiguity a lexc-style compiled FST must resolve honestly: which table's rule set governs a given
   occurrence of the shared spelling depends on which stratum (hence which table) the surrounding
   derivation is threading through, which is exactly the "cross-table symbol/representation
   threading" gap design.md D5 names for its own proposed new `constructs.txt` row. It stresses
   whether the compiler's token-space design (today, natural disjointness; the proposed fix is a
   PUA-style reserved-range-per-table encoding) actually keeps the two strata's rules from
   cross-contaminating when the same spelling is legal in both.
4. **Proposed fixture name:** `bistratal-overlapping-segment-representation`.
5. **Citations:** https://web.stanford.edu/~kiparsky/Papers/taipei.2014.pdf;
   https://users.castle.unc.edu/~jlsmith/home/pdf/smith2024_CHoP2_LoanwordPhonology_circulate.pdf;
   https://muse.jhu.edu/article/369835/summary.

**Covered already? No** — the two existing `MultiTable`-covered fixtures explicitly keep table
representations disjoint; the shared-representation case is the residual open split design.md D2
row 13 names, and the token-space redesign it needs is itself "larger in scope... flagging for
explicit prioritization" per that document.

#### 1.2.6 Quantifier pattern — genuinely unbounded quantifier

1. **Typological pattern.** Iconic, scalar reduplication/repetition where the number of copies is not
   fixed at one extra copy but can iterate further for greater expressive/intensive degree —
   "triplication" and beyond is explicitly attested alongside ordinary (single) reduplication in the
   iconicity-of-reduplication literature (the iconic sub-functions of reduplication — intensity,
   iteration, plurality, distributivity — and the note that "triplication is also possible": APiCS
   Ch. 26, "Functions of reduplication": https://apics-online.info/parameters/26.chapter.html; a
   fuller typological treatment of degrees of reduplicative copying: Inkelas, "Reduplication":
   http://linguistics.berkeley.edu/~inkelas/Papers/4.Reduplication_Inkelas.pdf). Southeast Asian and
   Austronesian expressive/ideophone systems are the families most commonly cited for
   scalar/iconic multi-copying beyond simple reduplication.
2. **Structural shape.** A `PatternNode::Quantifier` (`<OptionalSegmentSequence min max>`) whose `max`
   is the Kleene sentinel (`-1`, i.e. genuinely unbounded) rather than any finite cutoff — contrasted
   with the already-covered *bounded* case (`edge-cases/loader-pattern-shapes`'s finite optional
   group/Kleene-star pattern shape). The representative fixture wants a rule whose repeated element
   plausibly models scalar iconic copying (an expressive/intensifying reduplicative-like element)
   with no principled finite bound the grammar itself declares.
3. **What makes it a good FST stress case.** This is this project's most direct real-world analogue of
   "true Kleene star vs. a finite cutoff that must never masquerade as unbounded" — foma's own
   pattern language is a regular language, so a genuinely unbounded quantifier is representable
   natively (Kleene star), and the surrounding lowering machinery now hosts truly unbounded
   repetition without substituting a finite bound elsewhere in the pipeline. The representative
   fixture is the concrete artifact that closed this question empirically.
4. **Proposed fixture name:** `unbounded-iterative-quantifier-expansion`.
5. **Citations:** https://apics-online.info/parameters/26.chapter.html;
   http://linguistics.berkeley.edu/~inkelas/Papers/4.Reduplication_Inkelas.pdf.

**Covered already? Yes.** `unbounded-iterative-quantifier-expansion` closes the former
**NEEDS-DECISION** sub-split: genuinely unbounded quantifier lowering is supported and covered by a
`ConfirmOnly`-shaped fixture.

#### 1.2.7 Subrule-level phonological gating (`SubruleGating`) — its own tagged phenomenon

1. **Typological pattern.** A single phonological alternation whose environment is identical across
   two morphosyntactic contexts, but whose *application* is restricted to only one of them by the
   morphological class of the material it would apply within — i.e. gating a phonological rule (not a
   whole morphological rule) by a required/excluded morphosyntactic feature or part-of-speech. The
   general phenomenon — phonological rules whose domain of application is keyed to morphological
   structure/class rather than pure phonological environment — is well established in the
   phonology-morphology interface literature (Zwicky, "Rules of allomorphy and phonology-syntax
   interactions": https://web.stanford.edu/~zwicky/RulesOfAllomorphy.pdf), and the internal-vs-external
   sandhi distinction in the Sanskrit grammatical tradition is a classic concrete case: broadly the
   same phonological alternations apply word-internally and across compound/word boundaries, but
   specific sandhi rules are blocked or altered specifically at compound-internal boundaries
   (https://www.learnsanskrit.org/references/sandhi/internal/;
   https://sanskritstudio.wordpress.com/2014/01/22/sanskrit-external-sandhi-overview/).
2. **Structural shape.** A `PhonRuleDef` with (at least) two `RewriteSubruleDef`s sharing the same
   `Lhs`/environment shape, but one subrule declares a nontrivial `required_pos`/`required_mpr`/
   `excluded_mpr` that the other does not — so the same phonological environment is reached from two
   different morphological contexts, and only one is licensed to actually rewrite. This is
   distinguished from `constructs.txt`'s existing "MPR features/groups" row, which is about
   *morphological*-rule-level (`MorphologicalRule`) MPR gating, never a phonological subrule.
3. **What makes it a good FST stress case.** `gate.rs`'s existing partition mechanism already handles
   this faithfully (`SubruleGating` is `Proven`, not a compiler gap) — the value of a dedicated
   fixture is exercising the partition itself under a genuinely ambiguous surface environment (the
   same string could, in principle, satisfy either subrule's phonological trigger; only the
   morphosyntactic gate disambiguates which one is licensed), which is exactly the kind of
   over-generation a propose-then-confirm pipeline must resolve at confirm time, not silently admit
   at propose time.
4. **Proposed fixture name:** `subrule-morphosyntactic-gating`.
5. **Citations:** https://web.stanford.edu/~zwicky/RulesOfAllomorphy.pdf;
   https://www.learnsanskrit.org/references/sandhi/internal/;
   https://sanskritstudio.wordpress.com/2014/01/22/sanskrit-external-sandhi-overview/.

**Covered already? No representative fixture directly targets subrule-level (as opposed to
morphological-rule-level) MPR/POS gating today**, though several existing `PhonologicalSubrule`s
across the 8 languages do carry `requiredPartsOfSpeech` incidentally. `SubruleGating` is
`Unmappable` (design.md D5 proposes a new row, "PhonologicalSubrule required/excluded MPR or POS
gating... distinct from MorphologicalRule-level MPR features/groups") — blocked on the same upstream
`constructs.txt` PR as §1.2.3, but unlike that entry, no fixture anywhere yet targets this shape
*specifically* (as opposed to latently), so a new fixture is proposed regardless of when the upstream
row lands.

#### 1.2.8 Circumfix output-action — missing structural-composite shapes

1. **Typological pattern.** Circumfixation (a discontinuous affix realized as simultaneous prefix +
   suffix material around a root) is already representatively covered for the in-scope case (§1.1,
   `AffixProcessRule: prefix/suffix/circumfix/infix`, e.g. the German-style `ge-...-t` participial
   circumfix already present in `polysynthetic-stratal-derivation-chain`). The **open** gap here is
   narrower and purely a compiler-construction question, not a missing typological pattern: design.md
   itself states the census of exactly which circumfix-shaped allomorph configurations fail today's
   `crate::emit::is_structural_rule`/`build_structural_composites` "is not enumerated in any doc read
   for this plan" — i.e. nobody has yet named which *specific* structural shapes the existing
   structural-composite builder cannot handle.
2. **Structural shape.** Unknown pending census — cannot be responsibly guessed without inventing a
   shape that may not correspond to any real compiler gap. Candidate directions worth checking once
   the census runs (not asserted as the actual gap): a circumfix combined with root-internal
   non-concatenative modification in the same allomorph (stacking §1.1's `ModifyFromInput` pattern
   inside a circumfix rather than beside one), or a circumfix whose two parts are gated by different,
   independently-varying MPR features.
3. **What makes it a good FST stress case.** Whatever the census finds, by construction it will be a
   shape the existing structural-composite builder silently fails to reach — exactly the kind of
   silent-recall-hole `crate::capability`'s own no-catch-all discipline exists to surface.
4. **Proposed fixture name:** none proposed yet — premature before the census.
5. **Citations:** none new beyond §1.1's circumfix citations; this entry is a scoping note, not
   independent typological research.

**Covered already?** The general typological pattern: yes (§1.1). The specific compiler-gap shape:
**honestly unknown** — flagged per this task's own instruction to "flag honestly any construct where
you could NOT find a solid representative pattern, rather than inventing a plausible-sounding one."
This is the one entry in this document where the correct next step is the design.md-recommended
census, not typological research.

### 1.3 Permanent carve-outs — no fixture will ever close these (documented for completeness)

These constructs are architecturally closed *without* a further representative pattern: either the
disposition is unconditionally `ConfirmOnly` with no reachable further split (ADR 0001's own framing),
or the residual gap is a resource ceiling or an oracle-verification gap rather than a missing pattern.
Each is included here only so a future reader does not mistake "not discussed above" for "overlooked."

- **`RealizationalMorphology`, `MprGroupAppend`, `CoOccurrenceConstraint`** — always `ConfirmOnly`,
  no `Refuse` split exists for any of them (design.md D2 rows 1, 4, 12); already representatively
  covered (§1.1).
- **`Reduplication` × `RealizationalRule`** — a deliberate, faithfully-preserved oracle-parity
  carve-out (`crate::peel::is_reduplication_rule`'s own doc), not an unproven construction; no
  fixture is proposed.
- **`UnorderedMorphRuleApplication` (unbounded configuration)** — a genuine combinatorial resource
  ceiling (`DEFAULT_ORDERING_MULTIPLICITY_BUDGET`), not an unproven construction; no fixture closes a
  resource ceiling.
- **`SimultaneousRewrite` (genuine-overlap configuration)** — the one construct ADR 0001 itself names
  as unverified against the C# oracle (`hc.dll`) at all; per design.md D6, this needs
  `add-reference-hermitcrab-parity`'s oracle harness before a fixture can even be authored, not new
  typological research (the *pattern* — two rewrite subrules whose environments provably overlap —
  is already understood; what's missing is independent ground truth for what HermitCrab itself does
  in that case).

### 1.4 No solid representative pattern found

- **`Tracing (TraceType)`** — the one `constructs.txt` row cited by **zero** fixtures anywhere in
  `machine/conformance/languages/`, `machine/conformance/edge-cases/`, or `conformance-staging/`
  (confirmed by direct search: `Tracing`/`TraceType` appear only in `constructs.txt` itself,
  `parity-check.py`, and `README.md`). `machine/conformance/README.md` states this explicitly:
  *"`Tracing (TraceType)` is out of scope — it was never in `expected.tsv`'s domain."* This is
  flagged honestly rather than papered over: **Tracing is not a cross-linguistic morphological or
  phonological phenomenon at all** — it is an engine-internal verification mechanism (does a traced
  rule-application log match the analysis's declared rule list), so no typological pattern research
  applies to it in the way it applies to every other row above. No amount of typological research
  would produce a "representative pattern" here because the construct is not typological; closing
  this row (if ever desired) is a harness-instrumentation task, not a fixture-authoring task.

---

## 2. Summary table

| Construct | Proposed fixture name | Representative pattern | Covered already? |
|---|---|---|---|
| Stratum (Linear/Unordered rule order) | — | Level-ordered vs. free-rule strata | Yes |
| AffixProcessRule: prefix/suffix/circumfix/infix | — | The four canonical affix positions | Yes |
| AffixProcessRule: reduplication | — | Partial/total reduplication | Yes |
| AffixProcessRule: subtraction/truncation | — | Subtractive/truncating morphology | Yes |
| RealizationalAffixProcessRule | — | Word-and-paradigm realizational exponence | Yes |
| CompoundingRule (bounded) | — | Endocentric head/non-head compounding | Yes |
| MorphologicalOutputAction: Copy/InsertSegments | — | Concatenative affixation output | Yes |
| MorphologicalOutputAction: Modify/InsertSimpleContext | — | Ablaut/mutation, templatic vocalism | Yes |
| RewriteRule Iterative | — | Iterative rule reapplication | Yes |
| RewriteRule Simultaneous (non-overlapping) | — | Simultaneous rule application | Yes |
| MetathesisRule (left-to-right) | — | Adjacent-segment transposition, LTR | Yes |
| Affix template slots | — | CARP-style position-class templates | Yes |
| NaturalClass precision | — | Feature-bundle vs. bare-segment classes | Yes |
| Boundary markers | — | Morpheme-boundary symbols | Yes |
| MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule | — | OCP-style co-occurrence restriction | Yes |
| MPR features/groups | — | Feature-gated allomorph/rule choice | Yes |
| Guesser/LexicalGuess | — | Unlisted-stem guessing | Yes |
| Disjunctive allomorphs / free-fluctuation | — | Free variation between allomorphs | Yes |
| Stem names | — | Lexical conjugation/declension classes | Yes |
| Syntactic feature agreement | — | Head/foot agreement gating | Yes |
| Alpha-variable phonological environments | — | Feature-copying harmony environments | Yes |
| CompoundingRule constraints | — | Capped compound-of-compounds | Yes |
| Ordinary/realizational rule constraints | — | Rule-level application constraints | Yes |
| **Compounding — recursive/self-feeding** | `recursive-endocentric-compounding` | Unbounded compound-of-compounds recursion | **No** |
| **RightToLeftRewrite — remaining excluded shapes** | per-shape siblings | Directional stress/harmony rule scanning, anchor/segment/alpha-var edge cases | **No** |
| LeftToRightRewrite (as its own tagged phenomenon) | — (upstream `constructs.txt` row only) | Directionality independent of iterativity | Latent yes / tagged no |
| **Metathesis — right-to-left** | `right-to-left-metathesis-reversal` | Rotuman-style right-anchored transposition | **No** |
| **MultiTable — shared representation** | `bistratal-overlapping-segment-representation` | Native/loan (stratal) phonology sharing a spelling | **No** |
| **QuantifierPattern — unbounded** | `unbounded-iterative-quantifier-expansion` | Scalar/iconic triplication-and-beyond | **No** |
| **SubruleGating** | `subrule-morphosyntactic-gating` | Internal/external-sandhi-style subrule blocking | **No** |
| CircumfixOutputAction — missing structural-composite shapes | none (premature) | Unknown pending census | Unknown |
| RealizationalMorphology / MprGroupAppend / CoOccurrenceConstraint | — | (permanent carve-out, no split) | Yes |
| Reduplication × RealizationalRule | none (permanent carve-out) | Oracle-parity quirk | N/A |
| UnorderedMorphRuleApplication (unbounded) | none (resource ceiling) | N/A | N/A |
| SimultaneousRewrite (genuine overlap) | none yet (oracle-gated) | Overlapping rewrite-subrule environments | Needs oracle, not pattern |
| **Tracing (TraceType)** | none | **No typological pattern exists — not a linguistic phenomenon** | **No, and none applies** |

**The real gap list.** Almost all of it closed on 2026-07-25/26; the list is kept with outcomes rather
than rewritten, so the pattern research that motivated each fixture stays traceable to what shipped.

1. `Compounding` — recursive/self-feeding — **CLOSED 2026-07-26** (tasks.md 4.1). Fixture
   `recursive-endocentric-compounding`; the compound loop now unrolls `max_depth - 1` levels from an
   exact, always-finite per-rule bound, and the predicate is `ConfirmOnly` unconditionally.
2. `MultiTable` — shared representation — **CLOSED 2026-07-26** (4.4b). Fixture
   `two-table-shared-representation-recall`; note the token-space fix turned out *smaller* than this
   list feared, because the original disjoint-range approach was withdrawn as the wrong fix (it
   entrenched a false negative) in favour of render-time cross-table aliasing.
3. `SubruleGating` — **CLOSED**. Fixture `subrule-morphosyntactic-gating`; and its `exercises:` tag had
   to be corrected from a characteristic name to the real `constructs.txt` row id, without which it was
   silently crediting nothing.
4. `RightToLeftRewrite` — excluded pattern shapes — **PARTIALLY CLOSED**. Bounded and genuinely
   unbounded quantifier shapes are supported; the bounded shape landed
   (`right-to-left-bounded-quantifier-rewrite`) and exposed a real recall bug:
   `reversed_slots` was a shallow reverse that left a repetition group's contents in document order, so
   the mirror was not the reverse of the original. Remaining shapes are tasks.md 4.2.
5. `Metathesis` — right-to-left — **CLOSED 2026-07-26** (4.6). Fixture
   `right-to-left-metathesis-reversal`, `ConfirmOnly` as predicted.
6. `QuantifierPattern` — unbounded — **CLOSED 2026-07-26** (4.5). Fixture
   `unbounded-iterative-quantifier-expansion`. This one mattered more than the research predicted: it
   was blocking a *reference* grammar on the compiled path, not just a coverage row.
7. `CircumfixOutputAction` — **CENSUS DONE, all three closed** (4.3a/4.3b/4.3c;
   `docs/conformance/circumfix-structural-composite-census.md`). The census found the *mechanism* was
   already allomorph-complete and every gap was in candidate *selection*. C2 — **CLOSED 2026-07-27**,
   decided jointly with row 11's `Reduplication` carve-out as flagged: `CircumfixPrefix` now wins over
   `Reduplication` when an RHS is both circumfix- and reduplication-shaped, which closes a genuine
   recall gap (`crate::peel::ReduplicationPeeler`'s one-sided scans cannot recall that combined
   shape), not merely an ownership relabeling like C3. Row 11's carve-out itself is unchanged — it is
   a rule-kind distinction, orthogonal to C2's Role-shape one. Fixture
   `circumfix-reduplication-precedence`.
8. `SimultaneousRewrite` — genuine overlap — **ORACLE GAP DISCHARGED 2026-07-26**, which this table
   above still lists as "Needs oracle, not pattern". Fixture
   `simultaneous-subrule-genuine-overlap`, the repo's first with `hc.dll` ground truth: the two engines
   agree, and the agreement *discriminates* resolution order rather than being a shared silence. The
   proposer-side refusal is unchanged and correct — that was always a construction question, not an
   oracle one.
9. `Tracing (TraceType)` — not a pattern gap at all; still out of scope for typological research, and
   still the one row this document honestly reports as having no typological basis.

## Confirmation

No real-language wordlists, paradigms, morpheme inventories, or lexemes are transcribed anywhere in
this document — every structural claim above is described abstractly (positions, conditions,
boundedness, directionality) and every family/language name appears only in prose/citation context,
never as fixture content. Every proposed fixture name above uses construct/typology adjectives only
(recursive, endocentric, right-to-left, bounded, bistratal, overlapping, unbounded, iterative,
subrule, morphosyntactic, metathesis-reversal) — no language or family noun appears in any proposed
name.
