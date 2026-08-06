# Candidate FST-construction switches — the phonological and non-concatenative half

Read-only research. No code was edited, no builds or tests were run, no git commands were run.

This is an **evidence-backed catalogue**, not a design. It covers one half of the switch space:
rewrite-rule interaction, application mode and direction, metathesis, reduplication,
interdigitation, subtraction, junction phenomena, character-table/representation aliasing, and
long-distance phonological dependency. The morphotactic/lexicon half is somebody else's report.

## The evidence rule, and how it was applied

Every switch below carries evidence of exactly one of two kinds:

- **(a) A real grammar we already have.** The four are `samples/data/amharic-hc.xml`,
  `samples/data/indonesian-hc.xml`, `samples/data/sena-hc.xml`, and `samples/data/aweti.json`
  (the `aweti.fwdata` snapshot's extracted form). Every (a) claim below was measured by reading
  those four files directly for this report; the measuring is described so it can be re-run.
- **(b) A published source**, cited with author, title, and URL.

Switches with neither are in §4, "Speculative — no evidence found". That section is a real output.
Nothing was moved out of it to make the catalogue look fuller.

**Fixtures are synthetic and named for the construct, never for a language** (repo rule). Language
families appear below only as prose motivation for a (b) citation.

### What the four grammars actually contain (measured for this report)

| | Amharic | Aweti | Indonesian | Sena |
|---|---|---|---|---|
| Phonological rules | 7 | 18 | 5 | **0** |
| …referencing a morpheme boundary in LHS or environment | 2 | **15** | 4 | — |
| …deletion (empty RHS) | 1 | 2 | 2 | — |
| …epenthesis (empty LHS) | **0** | **0** | **0** | — |
| Rule direction | (attribute absent in HC XML) | 16 LTR, **1 RTL**, **1 simultaneous** | (absent) | — |
| Metathesis rules | **0** | **0** | **0** | **0** |
| α-variables on one rule (max) | **20** (`prule6`, `prule7`) | 0 | 1 (`prule4`) | — |
| Unbounded quantifier in a rule environment | 0 | 0 | **1** (`prule3`) | — |
| Morphological subrules | 93 | (fwdata) | 13 | 239 |
| …true copy-≥2 (reduplication) | **0** | — | **3** | **0** |
| …`redupMorphType` declared | **5** | — | 3 | 0 |
| …interdigitating (insert between *distinct* copies) | **3** | — | 0 | 0 |
| …subtractive (an input part never referenced by the output) | **4** | — | 0 | 0 |
| Allomorph environments (`RequiredEnvironments`) | 1 | — | 0 | **72** |
| Character-definition tables | 1 | 1 | 1 | 1 |
| Segment defs with >1 representation | **41** of 418 | **22** of 37 | 1 of 30 | 1 of 41 |
| Boundary-kind defs | 3 (incl. the `^0`/`*0`/`&0`/`∅` null family) | 2 (`+`, `#`) | 3 (same null family) | 3 (same null family) |
| Allomorphs whose whole shape is boundary-only | 1 (`+`) | — | 13 (`+`×10, `-`×3) | **15** (`+`×8, `^0+`×7) |

Method: `re`-based structural counts over the XML/JSON; the "true copy-≥2" test looks for the same
`PhoneticSequence` input id appearing twice in one `MorphologicalOutput`, and "interdigitating"
for an `InsertSegments` strictly between two copies of *different* input parts. These are the same
distinctions `pg-foma`'s own `classify_affix` and `rhs_has_true_reduplication` make.

### Confronting the density falsification, up front

The standing measured result is that **phonological-rule density does not predict cost**: Sena has
zero rewrite rules, 72 allomorph environment constraints, and is still the slowest grammar, its
cost dominated by morphotactic dead-ends (d5), not phonology (`.claude/skills/dead-end-census/
SKILL.md`; `docs/research/grammar-feature-space.md` §3.4; `docs/research/
per-language-fst-synthesis.md`'s signal table).

Three consequences observed throughout this catalogue:

1. **No switch below is triggered by a rule count.** Every trigger is a *structural predicate* over
   a rule's own shape (does its environment cross a boundary; does its output class re-enter its
   own input; is its LHS empty; does a copy appear twice), or a *product* computed from the
   grammar's own text (the representation-variant product, §3.11). Counts appear only as
   magnitudes reported alongside a structural trigger, never as the trigger.
2. **Every switch is stated with its behaviour on Sena.** A phonology switch that is *inert* on
   Sena is behaving correctly — Sena's cost is not phonological and no switch here claims to
   address it. What would be wrong is a switch that *fires* on Sena and changes its construction,
   because that is the shape that would regress the grammar the density heuristic already
   mispredicted. Only two switches below fire on Sena at all (§3.11 trivially, §3.13 genuinely),
   and §3.13's is the one whose absence produced Sena's measured 425× blow-up.
3. **The relevant magnitude, where one exists, is an *output* count, not an input count.** This is
   the same correction `EnumerationBudget` already had to make: probe count did not predict the
   Aweti disaster; emitted-entry count did (`docs/research/handspun-technique-audit.md` §2.15).
   §3.11 below follows that rule — its magnitude is the emitted variant product, not the number of
   aliasing segments.

---

## 1. What the shipped compiler already does in this half

Recorded first, because an existing implementation is stronger evidence than a citation, and
several switches below are "make an existing implicit choice explicit" rather than "build a thing".

| Phenomenon | Shipped mainline construction | Where |
|---|---|---|
| Junction phonology | `PhonologyProbe`: runs the **real** synthesis cascade over a bounded ±1-neighbour window and bakes the results into literal lexc strings; `None` for a zero-rule grammar | `junctions.rs`, `emit.rs:1514-1525` |
| Deletion at a junction | every root gets a `{name}Stripped` sibling lexicon, **deliberately ungated** by onset class | `emit.rs:129-149` |
| Interdigitation / boundary fusion | `preexpand.rs` replays the real engine per (root, rule) chain to depth 3 and emits fused composites; the dominant emit cost on Amharic | `preexpand.rs` |
| Reduplication | runtime `ReduplicationPeeler`, four O(word) scans, **never compiled into the FST** | `peel.rs` |
| Representation aliasing | cartesian product of every matched char-def's representations, capped at `REP_VARIANT_CAP = 64`, overflow reported as uncovered | `emit.rs:563-611`, `emit.rs:246` |
| Metathesis | **no construction at all.** Metathesis (or an empty-LHS rewrite) trips `probe_would_refuse`, which routes *every ordinary affix rule in the grammar* onto the real-synthesis composite path | `emit.rs:1939-1944`, `emit.rs:1959-1967` (S1 in `mainline-selection-audit.md`) |
| Direction / application mode | no explicit handling; correctness is inherited by delegating to the real engine inside the probe | — |
| Multi-table | **not read.** `emit.rs:2013` builds one `SegAlphabet` from `surface_table(g)` | `mainline-selection-audit.md` Part C |
| Null-morph boundary markers | mainline never puts boundary tokens on the queryable tape; a second build path did and produced a 425× blow-up, fixed by `reroute_null_shaped_affix_chains` | `emit.rs:575`, `build.rs:189-287` |

Two things this table makes visible and that recur below. First, **the mainline's answer to most of
this half is "delegate to the real engine at emit time, then over-generate and let confirm prune"**
— which is correct but pays for itself in emitted-entry count. Second, **the capability layer
grades several of these constructs against the prototype compiler, not the shipped one**, and four
of the seven predicates that can return `Refuse` are prototype-graded (`mainline-selection-audit.md`
§C5). §3.3 below is where that stops being abstract.

---

## 2. Two corrections to prior audits, found while gathering evidence

Recorded here rather than buried, because both change what the catalogue can claim.

### 2.1 Aweti *does* use right-to-left and simultaneous rewriting

`handspun-technique-audit.md` §2.27 states: *"None of Indonesian/Amharic/Aweti's 5+7+18 rules use
any of these three [RTL, genuine metathesis, overlapping Simultaneous subrules] at all"*, citing
`p6-prototype-report.md` §3.

That is **false for Aweti**. Reading `samples/data/aweti.json`'s `phonology.rules` directly:

```
 0 [LTR] 'FC>NC after NV'        [FC] -> [NC] / [NV] _
 1 [LTR] 'FC>NC before NC'       [FC] -> [NC] /  _ {+} [NC]
 2 [RTL] 'NV>OV before NC'       [NV] -> [OV] /  _ [NC]      <-- direction = "rightToLeft"
 3 [SIM] 'XV>OV'                 [XV] -> [OV]                <-- direction = "simultaneous"
```

16 of 18 rules are `leftToRight`, one is `rightToLeft`, one is `simultaneous`. The direction field
is `direction` on each rule object; the count is a one-line read.

Why it matters: `RightToLeftRewriteFaithfulReversalPredicate` is one of the four `Refuse`-capable
capability predicates that reason about `replace::pattern_slots` — the **prototype** compiler
(`mainline-selection-audit.md` §C5). Capability enforcement is on by default for `--engine=foma`
(`main.rs:438-451`). So the one real grammar in the corpus that exercises RTL is the one whose
capability verdict is decided by a compiler that grammar's actual run never invokes. This is not a
hypothetical: it is the concrete instance of the prerequisite `per-language-fst-synthesis.md`
already flags.

### 2.2 `redupMorphType` is a 5/5 false-positive signal on a real grammar

Amharic declares `redupMorphType` on 5 morphological outputs (`mrule3`, `mrule7`, `mrule8`,
`mrule14` = `prefix`; `mrule31` = `suffix`). **None of the five is a copy-≥2 RHS.** Four are
proclitics whose output is `InsertSegments("ላ"/"ካ"/"ባ") + CopyFromInput(part 2)`; the fifth is a
`ModifyFromInput` ablaut. Indonesian declares it 3 times and all three *are* true copies
(`mrule7`, `mrule13`, `mrule15`).

`capability.rs:136-142` already says hint presence is not the test (`Implicit` is the DTD default).
This is the measured confirmation on a real grammar: a hint-based reduplication detector would
misfire on 5 of Amharic's 93 subrules and route them to the peeler, where the four proclitic rules'
stem-initial-vowel deletion (§3.9) would be silently lost.

---

## 3. The catalogue

Each entry gives: **the construction difference**, **the trigger** (and whether it is cheap),
**hard evidence**, **families**, and **the synthetic fixture**.

---

### 3.1 `P1` Junction-locality partition: morph-internal rules vs. boundary-crossing rules

**Construction difference.** Today `junctions.rs` builds one `PhonologyProbe` per grammar and runs
the *whole* cascade over a ±1-neighbour window for every affix and every root. Split the rule set
in two by whether a rule can ever see material from more than one morpheme:

- **Morph-internal rules** (no boundary node anywhere in LHS or environment) can be applied **once
  per morph, at emit time, to that morph's own text**, with no neighbour probe and no
  (root, rule) pair enumeration. The result is a rewritten literal, not an extra lexc path.
- **Boundary-crossing rules** keep the neighbour probe (or, on the composed path, a real cascade).

This is exactly the fidelity boundary `handspun-technique-audit.md` §2.10 identifies from live
measurement — *"whether the phenomenon needs to see material that lives in more than one
morpheme's own text at once"* — but that finding was never turned into a partition; the probe runs
the full cascade either way.

**Trigger.** Does any node in the rule's `PhoneticInput` or `Environment` (`LeftEnvironment` /
`RightEnvironment`) have kind `Boundary`? **Cheap** — one pass over each rule's pattern nodes,
O(rules × pattern size), no compile, no lexicon.

**Hard evidence — (a), and it is an authored minimal pair.** Amharic contains the same rule twice:

- `prule6` "Consonant-Vowel merger **inside**": LHS = `[nc15 α×14] [nc16 α×6]`.
- `prule7` "Consonant-Vowel merger **at morpheme boundaries**": LHS = `[nc15 α×14]
  **BoundaryMarker char418** [nc16 α×6]`.

Identical natural classes, identical 20-variable α structure, identical output class — the *only*
difference is the `+` boundary node in the LHS. The grammar author treated locality domain as a
first-class distinction and wrote one rule per domain. Amharic's remaining five rules: `prule1-4`
and `prule6` are morph-internal; `prule5` ("a deletion before a") has the boundary in its right
environment.

Corroborating scale, same measurement: **Aweti has 15 of 18 rules referencing a boundary marker** —
its phonology is overwhelmingly junctural, and rule 13 (`[NC] {+} ° -> [NC] {+} [OO]`) has the
boundary *inside its own focus*, so it can never be applied morph-locally at all. Indonesian: 4 of
5 (`prule1`, `prule2`, `prule4`, `prule5`).

**Honest caveat, found in the same measurement.** Indonesian `prule3`'s environment crosses the
reduplication separator `-`, but that separator is declared as an ordinary `SegmentDefinition`
(`char17`, a member of the `C` class), not a `BoundaryDefinition`. So the boundary-node test
undercounts by one on Indonesian. A grammar that spells its separators as segments defeats the
cheap trigger; the switch must fail toward "treat as boundary-crossing", never the other way.

**Families.** Junctural phonology is the default case cross-linguistically for concatenative
morphology; the interesting fact is the *complement* — grammars with substantial stem-internal
phonology that never sees an affix. Semitic stem phonology (Amharic's `prule6`) is the case in
point; Tupian junctural phonology (Aweti) is the opposite extreme.
(b) corroboration: Kaplan & Kay, ["Regular Models of Phonological Rule
Systems"](https://aclanthology.org/J94-3001/), *Computational Linguistics* 20(3), 1994 — the
standard treatment of rewriting-rule contexts as regular relations, and of why context width and
context content are separate facts.

**Fixture.** `junction-locality-partition`: one stratum with two rewrite rules that are *identical*
except that one has a boundary marker in its LHS and the other does not, plus a root whose internal
shape triggers the internal rule and an affix whose adjacency triggers the boundary one. Words
must include a negative control where the internal rule would have fired across a boundary if the
partition leaked. This is Amharic's `prule6`/`prule7` shape rendered synthetically.

---

### 3.2 `P2` Rule-dependency depth (feeding/bleeding), not rule count

**Construction difference.** The mainline's junction probe runs the whole cascade in engine order
per probe — correct, but it pays cascade cost even for a grammar whose rules cannot interact.
Compute the dependency graph over rules (edge `i → j` iff rule `i`'s output class intersects rule
`j`'s input class or any of its environment classes) and use its **depth**, not its size:

- **Depth 0/1 (no interaction)** — rules can be applied independently and their results merged; no
  ordered cascade is needed and the per-probe cost drops to one pass.
- **Depth ≥ 2 (a feeding or bleeding chain)** — the ordered cascade is load-bearing and cannot be
  flattened, reordered, or unioned. This is where the `Compose`-not-`Union` discipline that
  `handspun-technique-audit.md` §3.3 spends a whole section on becomes mandatory rather than
  merely conventional.

**Trigger.** A natural-class intersection graph. **Cheap but not free**: O(rules² × class size)
over the fixed, small character inventory — never lexicon size N. It is the same complexity class
as `RepresentationAliasMap`'s overlap computation (§2.23 of the technique audit) and requires no
compile. It is *not* a rule count, which is the point.

**Hard evidence — (a).** Indonesian's five rules form a genuine chain, visible in the classes:

| rule | | effect |
|---|---|---|
| `prule1` | `char29(ⁿ) -> char16(ng) / _ {+} [V]` | archiphoneme default |
| `prule4` | `char29(ⁿ) -> [nc11 α] / [V] _ {+} [nc12 α]` | place assimilation, α-agreeing |
| `prule2` | `char29(ⁿ) -> 0 / _ {+} [nc7]` | nasal deletion |
| `prule5` | `[nc13] -> 0 / char1 [nc14] {+} _ [V]` | voiceless-obstruent deletion, MPR-gated |

`prule4` and `prule2` both consume `char29`; `prule2` bleeds `prule1`/`prule4` by removing their
input; `prule5` deletes the very segment `prule4` agreed with. The published analysis of this
system (assimilation **must** precede deletion) is recorded in `docs/fst-plan/
linguistic-recipe-harvest.md`'s Indonesian row and independently traced through
`meN+tulis → menulis` in `p6-prototype-report.md`. Aweti shows the same shape at larger scale:
rules 0/1 produce class `[NC]`, and rules 11/12/13 read `[NC]` in their contexts.

**The density confrontation, explicitly.** Sena's dependency graph is *empty* — no rules, hence no
edges, hence depth 0. This switch is inert on Sena and correctly so. That is the difference between
this trigger and the falsified one: a count would have scored Sena's 72 allomorph environments as
"dense phonology" (they are phonological conditions, they are just not rewrite rules); a
*dependency-graph* trigger reads zero, because there is nothing for a cascade to order.

**Families.** (b) Eric Baković, ["Opacity and
ordering"](https://home.uni-leipzig.de/muellerg/bakovicopacity.pdf), in *The Handbook of
Phonological Theory*, 2nd ed. (Goldsmith, Riggle & Yu eds.), Blackwell, 2011 — the reference
typology of feeding/bleeding/counterfeeding/counterbleeding, and the argument that these are
pairwise *interactions*, not properties of individual rules. Counterbleeding in particular is the
case where the composed order is not recoverable from the surface, so the construction cannot be
flattened.

**Fixture.** `feeding-chain-depth-three`: three rewrite rules where rule 1's output class is rule
2's input class and rule 2's output bleeds rule 3, plus a sibling `disjoint-rule-set-control` with
the same rule *count* and provably disjoint classes. The pair is the point: same density, different
depth, and the words must discriminate. `polysynthetic-stratal-derivation-chain`'s
`prSimulFeeding`/`prIterFeedingControl` pair is the nearest existing thing and covers mode, not
depth.

---

### 3.3 `P3` Application direction and mode (LTR / RTL / simultaneous)

**Construction difference.** Three genuinely different relations from one rule text:

- `leftToRight` iterative — rewrite scanning left to right against the already-mutated prefix.
- `rightToLeft` iterative — the mirror; on the composed path built as mirror-rule + `fsm_reverse` +
  `union_checked` (`compile_rtl_branch_net`), which is a *different automaton*, not a flag.
- `simultaneous` — every match evaluated against the original input, so the rule cannot feed itself.

The shipped mainline builds none of these explicitly; it inherits correctness by calling the real
engine inside the junction probe. The composed path builds all three. **The capability layer,
however, decides admissibility for the mainline using the composed path's predicates.**

**Trigger.** A per-rule enum read. **Cheap, O(1) per rule.** (Genuine `Simultaneous` *subrule
overlap* is the exception — `SimultaneousSubruleOverlapPredicate` builds real `foma::types::Fsm`
spans; that is a partial compile, and it is one of the two `OnceLock`-memoised facts in
`GrammarSemantics`.)

**Hard evidence — (a), and it overturns a prior claim.** See §2.1: Aweti has 16 `leftToRight`, 1
`rightToLeft` (rule 2, `NV>OV before NC`), 1 `simultaneous` (rule 3, `XV>OV`, context-free). The
direction is being used semantically, not decoratively: rule 2's context is to its *right*, and
under left-to-right scanning a right-context change cannot propagate leftward across a run, while
under right-to-left scanning it can. The author selected the one direction that makes the rule
spread.

**(b) corroboration.** C. Douglas Johnson, *Formal Aspects of Phonological Description*, Mouton,
1972 — the original result that rewriting rules are finite-state **provided no rule reapplies
directly to its own output**, and the source of the iterative/simultaneous distinction as a formal
one ([De Gruyter](https://www.degruyterbrill.com/document/doi/10.1515/9783110876000/html),
[scanned copy](https://pages.ucsd.edu/~ebakovic/compphon/Johnson%201972%201-up.pdf)). Kaplan & Kay
1994 (above) is the modern construction.

**Families.** Directional application is a property of rule systems generally rather than of a
family; the Tupian nasal-spreading case above is the concrete attested instance in the corpus.

**Fixture.** Two exist and are the right shape (`right-to-left-anchor-environment` upstream,
`right-to-left-metathesis-reversal` and `right-to-left-bounded-quantifier-rewrite` staged). What is
missing is the **direction-discriminating minimal pair**: `direction-discriminating-spread`, one
rule text instantiated twice, once LTR and once RTL, over a run of ≥3 eligible segments, with words
whose analyses differ *only* because of direction. Also missing: a fixture where the RTL rule is
the *only* non-default-direction rule in an otherwise ordinary grammar, so that a capability
`Refuse` sourced from the prototype's reversal predicate would visibly block a grammar the mainline
handles — Aweti's actual situation.

---

### 3.4 `P4` Self-feeding rules (spreading), and whether the closure is bounded

**Construction difference.** A rule whose output class re-enters its own input or context can
propagate along a run of eligible segments. Compiled, that is a **Kleene closure over the rewrite
relation**, not a single application; approximated, it is a bounded unrolling with a chosen bound.
The distinction from §3.2 is that this is one rule feeding *itself*, so no cascade ordering fixes
it — the depth is a property of the *word*, not of the grammar.

**Trigger.** Does the rule's output natural class intersect its own input class or any class in its
own environment? **Cheap** — the same class-intersection machinery as §3.2, restricted to the
diagonal. Whether the closure actually propagates additionally depends on the direction flag
(§3.3), so the two triggers must be read together.

**Hard evidence — (a), with an honest limit.** Aweti rule 1 (`FC>NC before NC`) has output class
`[NC]` and right-context class `[NC]` — the diagonal test fires. But its direction is
`leftToRight`, and under LTR scanning the right neighbour has not been rewritten yet, so it does
**not** in fact chain. Aweti rule 2 is the `rightToLeft` one, and its output (`OV`) differs from its
context class (`NC`), so it does not chain either. So: the trigger fires on a real grammar, and on
that grammar the closure turns out to be depth-1. **That is a useful negative**: it shows the
structural test alone over-reports and must be conjoined with direction, and it means no grammar we
currently hold demonstrates an unbounded compiled spread.

**(b) — the phenomenon is well attested and is precisely the residue of the locality result.** Jane
Chandlee, [*Strictly Local Phonological
Processes*](https://chandlee.sites.haverford.edu/wp-content/uploads/2015/05/Chandlee_dissertation_2014.pdf),
PhD dissertation, University of Delaware, 2014: a survey of roughly 5,500 patterns from about 500
languages finding **94% input-strictly-local**, with the residual 6% consisting of suprasegmental,
iterative, and long-distance harmony processes. That is the quantified statement of "spreading is
rare but real, and it is the part that needs a different automaton class". Gunnar Ólafur Hansson,
[*Consonant Harmony: Long-Distance Interaction in
Phonology*](https://escholarship.org/uc/item/2qs7r1mw), UC Publications in Linguistics 145,
University of California Press, 2010, is the cross-linguistic survey of the consonantal case.

**Families.** Nasal harmony in Tupí-Guaraní and Tupian more broadly (Awetí included — Sebastian
Drude, ["On the position of the Awetí language in the Tupí
family"](https://www.researchgate.net/publication/335232740_On_the_Position_of_the_Aweti_Language_in_the_Tupi_Family),
in Dietrich & Symeonidis eds., *Guaraní y Mawetí-Tupí-Guaraní*, Lit Verlag, 2006; further Awetí
materials indexed at the [Biblioteca Digital Curt
Nimuendajú](http://www.etnolinguistica.org/lingua:aweti)); sibilant harmony in Athabaskan and
Chumash per Hansson.

**Fixture.** `self-feeding-spread-closure`: one rule whose output class is its own context class,
over a natural class with ≥4 members, with words containing runs of length 1, 2, 3 and 5 so that a
bounded unrolling at any fixed depth fails a longer word. Include the LTR and RTL instantiations as
two fixtures, since only one of them spreads.

---

### 3.5 `P5` Unbounded context with a transparent span (long-distance agreement)

**Construction difference.** An α-variable agreement whose controller and target are separated by
an *unbounded, harmony-invisible* run needs an automaton that carries the agreement feature as
**state across the skipped material**. A nearest-segment construction is not merely less precise —
it computes a different relation. The mainline's ±1-neighbour probe cannot see it at all; the
composed path renders it as a quantified context inside the α-tuple product.

**Trigger.** A rule with an `AlphaVariable` binding whose environment contains a quantified node
(`OptionalSegmentSequence` with `max = -1`, or a Kleene node) *between* two α-bound occurrences,
where the quantified node's own class is **not** α-bound. **Cheap** — a linear walk over the rule's
pattern nodes, no compile. This is a sharper trigger than "has α-variables" or "has a quantifier",
either of which fires on rules with no long-distance behaviour.

**Hard evidence — (a), a real grammar, and it is the exact transparent-vowel shape.** Indonesian
`prule3`, "Nasalization in reduplication":

```
[nc8 α] -> [nc9 α]  /  [nc3 V] char29(ⁿ) [nc10 α] ( [nc6] )*0..∞ char17(-)  _
```

The α-binding controller (`[nc10 α]`) is separated from the α-binding target by
`OptionalSegmentSequence min="0" max="-1"` over `nc6`, and **`nc6` is the class `A` containing all
29 segments in the table** — every consonant, every vowel, and the hyphen. So the intervening span
is arbitrary-length and entirely transparent to the agreement. This is the only genuinely unbounded
quantifier in any rule environment across the four grammars.

Two things follow. It confirms the note in `docs/conformance/representative-typology-basis.md` that
closing the unbounded-quantifier gap "mattered more than expected: it was blocking a reference
grammar on the compiled path". And it means the existing `suffixing-vowel-harmony` fixture — plain
adjacent a/i alternation with no transparent class — does **not** cover the shape a real grammar we
hold already contains.

**(b).** Andrew Nevins, [*Locality in Vowel
Harmony*](https://mitpress.mit.edu/9780262513685/locality-in-vowel-harmony/), Linguistic Inquiry
Monographs 55, MIT Press, 2010 — the monograph treatment of transparent versus opaque neutral
vowels and of why a nearest-vowel rule is unstatable for Finnish-type systems. Hansson 2010 (above)
for the consonantal analogue. Chandlee 2014 (above) for the computational classification.

**Families.** Finno-Ugric front/back harmony with transparent `i`/`e` (Finnish, Hungarian); Turkic
harmony with a second rounding dimension; Athabaskan and Chumash sibilant harmony.

**Fixture.** `transparent-span-alpha-agreement`: an α-agreeing rule whose environment is
`[α-class] (transparent-class)* _`, with the transparent class **disjoint** from the α-bearing
class, and words with 0, 1, 2 and 4 intervening transparent segments plus a negative control where
an *opaque* segment intervenes and agreement must not reach across. The 0/1 words alone are
satisfiable by a nearest-segment construction, which is what makes the 4-segment word load-bearing.

---

### 3.6 `P6` Multi-slot interdigitation (root-and-pattern)

**Construction difference.** Today: `preexpand.rs` replays the real engine over
(root, rule-chain) pairs to depth 3 and emits one fused lexc entry per surviving pair — the
measured dominant emit cost on Amharic (~305k pairs probed pre-pruning, 2,930 interdigitation +
51,023 fusion entries, 30-47s), and the mechanism that OOMed on Aweti. The alternative is the
classical one: keep root, template and vocalism on separate tapes (or compile the template
in-place) and *intersect*, so the cost is the template inventory rather than roots × rules^depth.

**Trigger.** A `MorphologicalSubrule` whose `MorphologicalInput` has **≥2 `PhoneticSequence` parts**
and whose `MorphologicalOutput` places an `InsertSegments` strictly between copies of **distinct**
parts. **Cheap** — pure structural, O(subrules × output length), no compile. Note this is a
*different* test from "has an infix role": it requires the multi-part decomposition that makes the
rule templatic rather than merely interior-inserting.

**Hard evidence — (a).** Amharic has exactly 3 such subrules, and one of them is textbook:

```
mrule13  <Gloss>-pfv-</Gloss>  Name: -ää-
  Input:  part1 = (boundaries)* (Any)* (boundaries)*
          part2 = [nc2]   part3 = [nc2]   part4 = [nc2]      ; nc2 = consonant class
  Output: Copy(part1) Copy(part2) Insert("ä") Copy(part3) Insert("ä") Copy(part4)
```

The stem is decomposed into three separate consonant slots and a vowel melody is interleaved
between them — a discontinuous consonantal root with an aspectual vowel pattern, authored directly
in the grammar. `mrule4` and `mrule6` (`-ä-1`, `-ä-2`) are the single-insertion siblings. Neither
Indonesian, Sena, nor Aweti has any.

**(b).** Martin Kay, ["Nonconcatenative Finite-State
Morphology"](https://aclanthology.org/E87-1002/), EACL 1987 — the four-tape transducer (root /
template / vocalism / surface) that operationalises McCarthy's autosegmental analysis, and the
reason the *intersection* formulation is cheaper than enumerating stems. Kenneth Beesley & Lauri
Karttunen, *Finite State Morphology*, CSLI Publications, 2003, ch. on `compile-replace`
([overview](https://web.stanford.edu/group/cslipublications/cslipublications/koskenniemi-festschrift/8-karttunen-beesley.pdf))
— the in-place alternative that avoids extra tapes.

**Families.** Semitic (Ethiosemitic, Arabic, Hebrew) root-and-pattern verbal morphology.

**Fixture.** Exists in staging: `infix-interdigitation`. What it should be extended with — and this
is the load-bearing part for a switch — is a **slot-count scaling series**: the same construct with
2, 3 and 4 consonant slots and a melody per slot, so that the enumeration cost and the intersection
cost separate measurably. A single-slot fixture cannot distinguish the two constructions.

---

### 3.7 `P7` Reduplication: structural copy detection, and boundedness

**Construction difference.** Three constructions, not two:

- **Fixed-size partial copy** (copy exactly C, or CV) — a regular relation; compile it in.
- **Unbounded full-stem copy** — provably not a regular relation; must be a runtime peel
  (`peel.rs`) or a `compile-replace`-style two-phase compile.
- **Copy with fixed material interleaved between the copies** — one-sided scans cannot recover it;
  today this is routed to the O(roots × rules^depth) enumeration path.

**Trigger.** Two stacked, both cheap. (i) *Is it a copy at all*: the same input part id appears ≥2
times in one `MorphologicalOutput` — structural, O(output length), **never the `redupMorphType`
hint** (see §2.2). (ii) *Is it bounded*: the copied part's own pattern is a fixed-length segment
sequence rather than an unbounded quantifier — a read of the referenced `PhoneticSequence`.

**Hard evidence — (a), all three cases, in one real grammar.** Indonesian:

| | shape | class |
|---|---|---|
| `mrule7` `-Cont` | `Copy(p1) Insert("+") Insert("-") Copy(p1)` | full-stem copy; `p1` is `(Any)*` → **unbounded** |
| `mrule13` `-Pl` | identical shape | unbounded |
| `mrule15` `REDUP-meN` | `Copy(p2) Insert("-") Insert("+") Copy(p1) Insert("+") Copy(p2)` | **copy with fixed material between the copies** |

`mrule15`'s `p1` is the literal three-segment sequence `m e ⁿ`, so the rule matches a stem already
carrying the nasal prefix and wraps a copy around it. That is the
circumfix-and-reduplication shape `handspun-technique-audit.md` §2.19 case (3) records as a **real
recall gap**: the peeler's four scans are each one-sided and cannot recall a wrap-both-sides shape,
so `classify_affix`'s precedence was changed to route it to enumeration instead.

Amharic and Sena have zero true copies; Amharic's five `redupMorphType` declarations are all false
positives (§2.2).

**(b).** Carl Rubino, ["Reduplication"](https://wals.info/chapter/27), WALS Online chapter 27 (Dryer
& Haspelmath eds.), MPI-EVA — the typological survey: of the languages with productive
reduplication, 277 have both full and partial and 35 have full only, so **the bounded/unbounded
split is not a rare corner**. Christopher Culy, ["The complexity of the vocabulary of
Bambara"](https://link.springer.com/article/10.1007/BF00630918), *Linguistics and Philosophy* 8:345-351,
1985 — the formal result that unbounded copying takes a natural language out of the context-free
languages, hence a fortiori out of the regular ones. Beesley & Karttunen (above) for
`compile-replace` on Malay full-stem copying, the standard finite-state workaround.

**Families.** Austronesian (Malay/Indonesian full-stem; Tagalog fixed-CV partial); Bambara
(Mande) for the formal case.

**Fixture.** Three, deliberately separate, because they select three constructions:
`bounded-cv-copy` (copy exactly one CV from the stem — must compile in),
`unbounded-full-stem-copy` (copy an unbounded part — must peel), and
`copy-with-interleaved-fixed-material` (the `mrule15` shape). The third overlaps
`circumfix-reduplication-precedence` in staging, which pins the *routing* bug; what is missing is a
fixture that pins the *boundedness* fork, since no current fixture makes a bounded partial copy that
a compiled construction should handle without the peel.

---

### 3.8 `P8` Copy-sensitive phonology (a rule whose context spans the copy)

**Construction difference.** If a phonological rule's context reaches across a reduplicative copy
boundary, then peeling the copy at *query* time and running the phonology at *compile* time are no
longer independent: the peel produces a residual that the compiled network was never built to
accept, and the compiled network's junction probe never saw the copy. Either the peel must rejoin
the same cascade, or the copied span's phonology must be pre-computed as a correspondence relation.

**Trigger.** Conjunction of §3.7(i) — grammar has a true copy — and a rule whose environment
reaches across the copy's own separator or spans a quantified region wide enough to contain a copy.
**Cheap** as a conservative over-approximation (any rule with an unbounded environment in a grammar
that has a true copy); exact detection is not cheap and probably not worth it.

**Hard evidence — (a), and the grammar author named it.** Indonesian `prule3` is literally called
**"Nasalization in reduplication"**, and its left environment (quoted in §3.5) walks from the `meN-`
nasal, past the α-agreeing controller, across an unbounded transparent span, to the separator
`char17` = `-`, with the target on the far side. It is a single rule that only makes sense against a
reduplicated form, and Indonesian is also the grammar with the three true copies (§3.7). The
interaction is not hypothetical in this corpus — it is one grammar, three copy rules, and one rule
written specifically about them.

**(b).** `docs/fst-plan/linguistic-recipe-harvest.md`'s cross-language constraint 2 states the
general form ("a specialized branch must preserve morpheme boundaries, copied-span identity, and
the stratum at which it rejoins") citing the Tagalog case, where the imperfective copy is taken
from the *morphologically constructed* stem so causative material determines the copied edge; the
underlying reference is Alan C. L. Yu, [*A Natural History of
Infixation*](https://www.cambridge.org/core/journals/journal-of-linguistics/article/abs/c-l-alan-yu-a-natural-history-of-infixation-oxford-studies-in-theoretical-linguistics-15-oxford-oxford-university-press-2007-pp-x264/3E004FAE5686934780E193D31D8A72C9),
Oxford Studies in Theoretical Linguistics 15, OUP, 2007, on pivots being computed over derived
stems. Beesley & Karttunen (above) show why the copy must be compiled *before* the surface cascade
if it is compiled at all.

**Families.** Austronesian (Indonesian, Tagalog).

**Fixture.** `copy-sensitive-junction-rule`: a grammar with one unbounded full-stem copy and one
rewrite rule whose environment references material on both sides of the copy separator, with words
where the copy is phonologically *altered* relative to the base. The negative control is the same
word with the rule's trigger removed, so the copy surfaces identical to the base. No current
fixture composes copying with a copy-spanning rule; `deletion-reduplication-exception-composite`
in staging is the nearest and pairs copying with deletion, not with a copy-spanning context.

---

### 3.9 `P9` Subtractive morphology (an input part the output never references)

**Construction difference.** A rule that decomposes its input into parts and then omits one from
the output is a *deletion of a matched span*, not a concatenation. Compiled naively as an affix, it
over-generates in the wrong direction — analysis has to restore material that is not present. The
mainline routes this into `build_structural_composites`' real-synthesis path (a `structural_rule`
per `emit.rs:1884-1925`); a composed path can express it directly as a replace relation.

**Trigger.** Set difference: the `PhoneticSequence` ids in `MorphologicalInput` minus the ids
referenced by any `CopyFromInput`/`ModifyFromInput` in `MorphologicalOutput`. Non-empty ⇒
subtractive. **Cheap**, pure structural, O(subrules).

**Hard evidence — (a).** Amharic has exactly 4, and they are a coherent set — the preposition
proclitics `ለ=` (`mrule3`), `ከ=` (`mrule7`, `mrule8`), `በ=` (`mrule14`). Each declares a two-part
input where part 1 is the single segment `char9` (the `አ`/`ኣ` grapheme pair) and part 2 is the
remainder, and each output is `InsertSegments("ላ"/"ካ"/"ባ") + CopyFromInput(part2)` — **part 1 is
matched and then dropped**. The stem-initial vowel is deleted and a fixed proclitic replaces it.
Indonesian, Sena and Aweti have none.

**A refuted hypothesis, recorded because it is the caution for this trigger.** Aweti was believed
to have 41 one-sided-truncation mrules; `docs/fst-plan/recipe-parity-plan-2026-07-30.md` records
that this "turned out to be floating-consonant realization, not truncation". A trigger for this
switch must therefore be the structural set-difference test above, computed from the rule's own
declared parts — not an inference from a rule's surface effect being shorter than its input.

**(b).** Stela Manova, ["Subtraction in
Morphology"](https://oxfordre.com/linguistics/display/10.1093/acrefore/9780199384655.001.0001/acrefore-9780199384655-e-572),
*Oxford Research Encyclopedia of Linguistics*, 2019 — the cross-linguistic survey, which also
records that subtraction is not widespread, so a switch here should expect to be inert on most
grammars. Birgit Alber & Sabine Arndt-Lappe, ["Templatic and subtractive
truncation"](https://www.uni-trier.de/fileadmin/fb2/ANG/Linguistik/Arndt-Lappe/Alber_ArndtLappe12_draft.pdf),
in Trommer ed., *The Morphology and Phonology of Exponence*, OUP, 2012 — the distinction between
subtractive truncation (delete a specified span) and templatic truncation (retain a prosodic
template), which are two different constructions and should not share one switch.

**Families.** Muskogean (Koasati plural), Uto-Aztecan (Tohono O'odham perfective), Ethiosemitic
proclitics as above.

**Fixture.** `truncate-morphotactic` exists upstream and covers the basic case. Missing:
`subtractive-with-replacement`, the Amharic shape — a rule that deletes a matched initial segment
*and* inserts fixed material in its place, since the delete-and-insert combination is what makes
the analysis direction ambiguous. Also missing entirely: a templatic (prosodic) truncation fixture,
which is a different construction per Alber & Arndt-Lappe.

---

### 3.10 `P10` Metathesis: presence, direction, and whether it crosses a boundary

**Construction difference.** Metathesis needs a dedicated swap relation
(`compile_metathesis_rule`), built as a union of fully-literal slot-assignment branches — safe
because each branch is a complete transducer with no spurious identity path
(`handspun-technique-audit.md` §2.27). **The mainline has no metathesis construction at all.**
Instead, `probe_would_refuse` (`emit.rs:1939-1944`) returns true if *any* rule is a metathesis or an
empty-LHS rewrite, and that flag then widens **every ordinary prefix/suffix/infix rule in the whole
grammar** onto the real-synthesis composite route (`emit.rs:1959-1967`). So one metathesis rule
changes the construction of every affix in the grammar. That is the single largest
grammar-keyed construction fork in the shipped compiler, and it is hardcoded.

**Trigger.** `MetathesisRuleDef` present — **cheap, boolean**. Direction likewise. Whether the swap
crosses a morpheme boundary is the same boundary-node test as §3.1, also cheap.

**Hard evidence — (b) only; the (a) evidence is negative and worth stating.** **None of the four
grammars has a single metathesis rule** (0 `<MetathesisRule>` in all three HC XMLs; all 18 Aweti
rules have `"kind": "rewrite"`). So the `probe_would_refuse` widening never fires for metathesis on
any grammar we hold — it fires, if at all, for an empty-LHS rewrite, and there are none of those
either (§3.15). The whole S1 fork is dead on the current corpus.

(b): Juliette Blevins & Andrew Garrett, ["The evolution of
metathesis"](https://linguistics.berkeley.edu/~garrett/Blevins-Garrett-2004.pdf), in Hayes, Kirchner
& Steriade eds., *Phonetically Based Phonology*, Cambridge University Press, 2004, 117-156 — the
empirically motivated typology of metathesis, which is what establishes that the phenomenon is
recurrent rather than sporadic. Owen Edwards, [*Metathesis and unmetathesis in
Amarasi*](https://langsci-press.org/catalog/book/228), Studies in Diversity Linguistics 23,
Language Science Press, 2020 (open access) — the case that matters most for a *switch*, because in
Amarasi **metathesis alone, with no other phonological difference, is the sole exponent of a
morphosyntactic category**; a construction that treats metathesis as an optional phonological
touch-up cannot analyse such a form at all. Rotuman is the other classical Austronesian case.

**Families.** Austronesian (Amarasi and other Timor languages, Rotuman); Arawakan (Caquinte, where
`linguistic-recipe-harvest.md` records boundary-crossing metathesis).

**Fixture.** `metathesis-phase-isolation` (upstream, LTR) and `right-to-left-metathesis-reversal`
(staged) cover direction. Two genuine gaps: `boundary-crossing-metathesis`, where the two swapped
segments belong to *different* morphemes so the swap cannot be pre-applied to either morph's own
text (the Caquinte shape, and the case §3.1's partition must not mis-assign); and
`metathesis-as-sole-exponent`, where two analyses differ only by the metathesis having applied —
the Amarasi shape — which is the fixture that would prove the swap relation is doing work rather
than being absorbed by over-generation plus confirm.

---

### 3.11 `P11` Representation-alias scale: cartesian product vs. char-def-identity alphabet

**This is the strongest measured switch in this report.**

**Construction difference.** Two ways to accept every spelling of the same segment:

- **Path A (shipped):** `surface_variants` re-segments each morph's authored text and emits the
  **cartesian product** of every matched char-def's representations as literal lexc alternatives,
  capped at `REP_VARIANT_CAP = 64`; overflow drops spellings and reports an uncovered item
  (`emit.rs:563-611`).
- **Path B:** the `SegAlphabet` PUA token alphabet — one Private-Use codepoint per `CharDefId`, so
  every representation of a segment is the *same token* and the product never arises at all
  (`handspun-technique-audit.md` §2.25). The cost is an illegible lower tape, which the
  propose→confirm contract does not need.

**Trigger.** The **emitted variant product per morph**, computed by replaying the loader's own
greedy-longest-match segmentation over the char table. **Cheap** — O(total authored text length),
no compile, no lexicon join. Deliberately an *output* magnitude rather than "how many aliasing
segments exist", per the `EnumerationBudget` lesson.

**Hard evidence — (a), measured for this report over all four grammars.** Replaying the loader's
segmentation against each grammar's own char table (for Aweti, filtering phoneme representations to
the vernacular writing system exactly as `pg-grammar/src/compile/chardef.rs` does via
`ws_forms(..., default_ws)`):

| Grammar | Aliasing segment defs | Max variant product on one morph | Morphs over `REP_VARIANT_CAP = 64` |
|---|---|---|---|
| Sena | 1 of 41 (`char4` = m/n) | 2 | 0 |
| Indonesian | 1 of 30 (`char28` = g/G) | 2 | 0 |
| Amharic | **41 of 418** (Geʽez homophones: ጽ/ፅ, ጸ/ፀ, ሳ/ሣ, አ/ኣ, ሴ/ሤ …) | 8 | 0 |
| **Aweti** | **22 of 37 phonemes** | **4,096** (`tạtupewỵpẹpo`) | **110 of 1,211 (9.1%)** |

Aweti's distribution has a long tail: 3,072 (×2), 2,304 (×3), 1,536 (×5), 1,024 (×3), and so on.
The `a` phoneme alone carries six vernacular forms (`a A á Á à À`), `y` carries four. **Nine percent
of Aweti's allomorph forms cannot be emitted faithfully by the shipped cartesian-product
construction** — they overflow the cap, drop spellings, and are reported as uncovered. Under the
token alphabet the same forms cost exactly one lexc entry each.

This is a measured, re-runnable number from a grammar we hold, and it is a construction difference
of three orders of magnitude on the same input. Note also that the two mechanisms differ in *kind*,
not degree: the product is an under-approximation once it overflows (silent recall loss, visibly
reported), while the token alphabet is exact.

**Caveat, stated because it affects how the number should be read.** Aweti's aliases are largely
case and accent variants introduced by the FieldWorks phoneme codes, not homophonous graphemes in
the linguistic sense. Amharic's 41 *are* genuinely homophonous graphemes. The construction cost is
identical either way — the emitter cannot tell them apart, and neither can the cap — but a reader
should not infer "Aweti has extraordinary grapheme homophony" from the 4,096.

**(b) for the linguistic case.** Multiple Ethiopic grapheme series are homophonous in Amharic
following historical mergers (the /sʼ/ series ጸ/ፀ, the /h/ series ሀ/ሐ/ኀ, the glottal series አ/ዐ),
and speakers vary both in which series they prefer and in actual use — see Richard Ishida,
["Amharic orthography notes"](https://r12a.github.io/scripts/ethi/am) (W3C i18n script notes), and
the survey discussion in Baye Yimam / Menuta as summarised in Tsegaye et al., ["The Ethiopic
script"](https://journals.uio.no/osla/article/download/4422/3888), *Oslo Studies in Language*.
Amharic's 41 multi-representation segment defs are exactly this phenomenon encoded in the grammar.

**Families.** Ethiosemitic (Geʽez script); more generally any orthography with historical
grapheme mergers.

**Fixture.** `representation-alias-product-overflow`: a char table where six segments each declare
three representations, and a root whose text uses all six — product 729, over the cap. Words must
include a form spelled with a *non-first* representation at every position, so a construction that
truncates the product fails to recall it. Contrast fixture
`representation-alias-single`: one two-representation segment, product 2, which is what Sena and
Indonesian actually exercise and which the current corpus already covers. The pair is the switch.

---

### 3.12 `P12` Multi-table character definitions and cross-table aliasing

**Construction difference.** With more than one `CharacterDefinitionTable`, either (i) each table's
rules compile against their own alphabet and a rule silently fails to fire on material spelled from
the other table (a **false negative**), or (ii) each rule's atoms are rendered as the union of
every table's token for the same normalised spelling before compiling
(`RepresentationAliasMap`, `handspun-technique-audit.md` §2.23).

**Trigger.** `char_table_count > 1`, plus an overlap computation over normalised spellings —
O(tables² × table size) over the small fixed inventory, never lexicon size. **Cheap.**

**Hard evidence — (b) only, and there is a code finding worth more than the citation.** All four
grammars declare exactly **one** table (verified: one `<CharacterDefinitionTable>` in each HC XML;
the Aweti snapshot compiles one). So there is no (a) evidence, and none is likely to appear soon.

The code finding: **the shipped emitter reads one table regardless.** `emit.rs:2013` builds a single
`SegAlphabet` from `surface_table(g)` (`mainline-selection-audit.md` §A3, last row). Meanwhile
`MultiTable`'s capability predicate reasons entirely about `replace::RepresentationAliasMap` and
`lower::render_slots` — the prototype — and then *discards the detail it computes*
(`let Some(_detail) = …`, `capability.rs:2350`). So for a multi-table grammar today, the question
the predicate answers is not the question the shipped path poses. `mainline-selection-audit.md`
names this "the cleanest instance and worth fixing first"; this report concurs and adds only that
the absence of any multi-table grammar in the corpus is why it has stayed broken.

**(b).** Serbian is the standard synchronous-digraphia case — one language, two officially equal
scripts, with a near-one-to-one letter correspondence between them; see Jelena Ivković, ["Pragmatics
meets ideology: Digraphia and non-standard orthographic practices in Serbian online news
forums"](https://www.benjamins.com/catalog/jlp.12.3.02ivk), *Journal of Language and Politics*
12(3), 2013. A grammar written for such a language is precisely the "two tables, overlapping
normalised spellings" shape.

**Families.** Serbian (Cyrillic/Latin); Hindi-Urdu; Konkani; Punjabi (Gurmukhi/Shahmukhi).

**Fixture.** Three exist in staging (`two-table-shared-representation-recall`,
`multi-table-metathesis-shared-representation`,
`right-to-left-cross-table-segments-environment`) and cover the prototype's behaviour. The gap is a
fixture that is **run through the shipped mainline** and fails: `two-table-mainline-recall`, where
a rule declared in table B must fire on a root spelled from table A, with words the single-table
`SegAlphabet` provably cannot recall. Until such a fixture exists, `strategy_coverage`'s
`tuned_surface_probed` row — a single match arm returning `Represents` for all 22 kinds, including
this one — cannot be falsified.

---

### 3.13 `P13` Boundary-symbol semantics: null-morph markers vs. cosmetic separators

**Construction difference.** A `Boundary`-kind char-def can be a cosmetic separator (`+`, `.`) or a
**semantically load-bearing zero-morph marker** (the `^0` / `*0` / `&0` / `∅` family). The two must
not be handled identically:

- **Mainline (correct by construction):** boundary matches are dropped at emit time and never
  placed on the queryable tape (`emit.rs:575`).
- **The other build path (broken, then fixed):** every boundary was emitted onto the tape and
  deleted afterwards by one blanket context-free regex. An affix allomorph whose *entire* shape is
  boundary-only then degenerates to a zero-width, tag-bearing entry sitting on a self-looping
  continuation — freely repeatable, consuming no input. Fixed by
  `reroute_null_shaped_affix_chains` (`build.rs:189-287`), which reroutes such a line onto a
  one-shot non-reentrant successor.

**Trigger.** An affix allomorph whose entire authored text segments to `Boundary`-kind char-defs
only. **Cheap** — one segmentation pass per allomorph, no compile.

**Hard evidence — (a), with the largest measured blow-up in the corpus.** Measured for this report:
all three HC grammars declare three boundary definitions, one of which is the four-representation
null family (`^0`, `*0`, `&0`, `∅`). Boundary-only allomorph shapes:

- **Sena: 15** — `+` ×8 and **`^0+` ×7**. The seven `^0+` are the compounding allomorph.
- Indonesian: 13 — `+` ×10, `-` ×3.
- Amharic: 1 — `+`.

Sena's seven `^0+` allomorphs are exactly the shape that produced the measured **425× proposal
blow-up** (127 → 53,992 proposals across five words; `mbali` alone 104 → 53,720, i.e. 516×),
reduced to 575 by the fix (`docs/fst-plan/large-lexicon-proposal-explosion.md`;
`handspun-technique-audit.md` §2.20). **This is the one switch in this report that fires on Sena and
must**, and the reason is not phonological density — Sena has no rules — but the interaction of a
zero-width tag with a self-looping continuation class.

**A named, still-open scope gap.** The shipped guard matches on two literal lexicon names
(`PrefixChain`/`SuffixChain`). `uflexc`'s later bounded-compound loop introduced its own
self-looping per-level lexicons (`UCmpPfx0`, `UCmp2Pfx0`, …) which recreate the identical hazard
and are invisible to a name-based guard — "a name-based guard cannot defend a lexicon that did not
exist when the guard was written" (`build.rs:270-287`). Any switch built here should be keyed on the
*structural* property (is this continuation self-looping) rather than the lexicon's name.

**(b) not required** — the (a) evidence is a measured 425×. For completeness, zero-exponence
(morphemes with no phonological content) is a standard morphological category and is what the
`^0` family encodes.

**Fixture.** No current fixture has a boundary-only affix allomorph on a self-looping continuation.
`null-shaped-affix-on-self-loop`: one affix whose entire underlying shape is a null-morph boundary
marker, in a grammar with at least one ordinary stackable prefix and one ordinary stackable suffix,
with words requiring the null affix both *before* and *after* an ordinary affix (the ordering case
that broke the first attempt at the fix, `MultiplicityMismatch { word: "ps", expected: 3, actual: 2 }`),
and an assertion on total proposal count so a regression shows up as a magnitude, not a wrong answer.

---

### 3.14 `P14` Deletion-junction root partitioning: ungated vs. onset-class gated

**Construction difference.** When a prefix triggers deletion of a root's initial segment, the
mainline gives *every* root a `{name}Stripped` sibling entry holding its text minus the first
segment, and routes every deletion junction there — **deliberately ungated by onset class**, an
explicit upward approximation whose extra candidates confirm prunes (`emit.rs:129-149`). The
alternative gates the stripped partition by the deleting subrule's own input class, so only roots
whose initial segment could actually delete get a stripped sibling.

**Trigger.** For each deletion subrule (empty `PhoneticOutput`), does the root's initial segment
belong to that subrule's LHS class? **Cheap** — O(roots) set-membership against a natural class,
the same bitset test `compound_license` already uses.

**Hard evidence — (a), with the exact size of the win.** Indonesian `prule5` ("Voiceless obstruent
deletion") has LHS `nc13`, realised as the class `DelOb` = {p, t, k, s}. Measured over Indonesian's
66 root allomorphs: **20 of 66 (30%) begin with a DelOb segment.** So 46 of 66 stripped entries —
**70% of the stripped lexicon** — are provably dead: no deletion rule in the grammar can produce
them, and every candidate they generate is rejected by confirm. `prule2` ("Nasal deletion") deletes
the archiphoneme `char29`, which no root begins with, so it contributes nothing to the stripped
partition at all.

At Indonesian's 66 roots this is 46 wasted lexc lines — negligible. The switch exists because the
standing rule is to design for 10⁴-10⁵ entries (`build-for-full-scale-grammars`), where the same
70% is 70,000 dead entries plus their downstream candidates, and because the *fraction* is a
property of the grammar's phonology (which onsets delete), not of its size.

**Honest counter-consideration.** The current ungated design was chosen to avoid a data dependency
(per-junction neighbour-class lane data) the emitter has no other use for. The measurement above
shows the data is cheaper than assumed for the simple case — a deletion subrule whose LHS is a
single natural class — but a deletion subrule with a multi-node LHS or an α-bound class is not a
simple membership test, and the switch should refuse to gate in that case rather than gate wrongly
(gating wrongly loses recall; not gating only wastes lines).

**(b).** Nancy Hall, ["Vowel
Epenthesis"](https://onlinelibrary.wiley.com/doi/abs/10.1002/9781444335262.wbctp0067), in van Oostendorp
et al. eds., *The Blackwell Companion to Phonology*, 2011, covers the mirror-image junction repair
and establishes that junction repairs are class-conditioned rather than blanket.

**Fixture.** `deletion-junction-onset-gated`: a grammar with a deletion subrule whose LHS is a
three-member natural class and a lexicon where roughly a third of roots begin with a member, plus
words proving (i) a root with a deleting onset is recalled through the junction and (ii) a root with
a non-deleting onset is *not* analysable as if it had been stripped. Assert the emitted stripped-entry
count, so a regression to the ungated construction is visible as a count, not only as a proposal
volume.

---

### 3.15 `P15` Epenthesis (empty-LHS rewrite) and its blast radius

**Construction difference.** An empty-LHS rewrite inserts material with no input to consume, so it
cannot be pre-applied to a morph's own text and cannot be discovered by a probe that only rewrites
existing segments. The shipped compiler's response is drastic and indirect: `probe_would_refuse`
treats an empty-LHS rewrite exactly like a metathesis rule, and **one such rule routes every
ordinary affix rule in the grammar onto the real-synthesis composite path** (`emit.rs:1939-1967`).
The switch is to make that scope explicit — route only the *affected* rules, or make the widening a
named policy with a measurable alternative.

**Trigger.** `RewriteRuleDef` with an empty `PhoneticInput`. **Cheap, boolean per rule.**

**Hard evidence — (b) only; the (a) evidence is again negative and again informative.** **None of
the four grammars has an empty-LHS rewrite.** Amharic `prule5`, Indonesian `prule2`/`prule5`, and
Aweti rules 16/17 are all *deletions* (empty **output**), which is the opposite direction and does
not trip `probe_would_refuse`. Combined with §3.10's zero metathesis rules, the conclusion is that
**`probe_would_refuse` returns false on all four grammars**, and the S1 widening — the mainline's
single largest grammar-keyed construction fork — has never fired on a real grammar in this corpus.
That is worth knowing before anyone tunes it.

(b): Nancy Hall, ["Vowel Epenthesis"](https://onlinelibrary.wiley.com/doi/abs/10.1002/9781444335262.wbctp0067)
(above) and Juliette Blevins, ["Consonant Epenthesis: Natural and Unnatural
Histories"](https://julietteblevins.ws.gc.cuny.edu/files/2016/10/Blevins2008d-ConsonantEpenthesis.pdf),
in Good ed., *Linguistic Universals and Language Change*, OUP, 2008 — the two standard surveys, both
establishing epenthesis as recurrent and as class- and position-conditioned rather than free.
`linguistic-recipe-harvest.md` additionally records Caquinte epenthetic consonants repairing vowel
clusters, interacting with boundary-crossing metathesis.

**Families.** Arawakan (Caquinte); Semitic and Japanese loan phonology are the classical
vowel-epenthesis cases.

**Fixture.** `simultaneous-epenthesis-cascade` exists upstream and covers the construct. What is
missing is the **blast-radius fixture**: `epenthesis-widening-scope`, a grammar with several
ordinary affix rules and exactly one empty-LHS rewrite, asserting the emitted entry counts for the
affix rules with and without the epenthesis rule present. Today that widening is a hardcoded
whole-grammar consequence of a single rule and nothing measures it.

---

## 4. Speculative — no evidence found

Candidates considered and **not** admitted to the catalogue, because neither a grammar we hold nor
a published source establishes that a *construction* would differ. Listing them is the point; none
should be built on this basis.

1. **Tone and other suprasegmental tiers as a switch.** HermitCrab has no autosegmental tone tier,
   none of the four grammars encodes tone, and no conformance fixture does either. The floating-tone
   docking-before-deletion case (Awngi) that `linguistic-recipe-harvest.md` describes is cited there
   to Black (2025) and to Kenstowicz & Kisseberth (1979:64) / Halle & Clements (1983:93) — secondary
   citations this report could not verify at first hand, and in any case the phenomenon would have
   to be encoded as ordinary segments before it reached the compiler. **No evidence for a switch.**

2. **A "non-concatenative" umbrella switch.** Already evaluated and rejected in
   `grammar-feature-space.md` §3.5 and re-confirmed here: metathesis (§3.10), interdigitation
   (§3.6), reduplication (§3.7) and subtraction (§3.9) have four different triggers, four different
   constructions, and four different failure modes in this corpus. Nothing found suggests they
   co-occur or should share a switch.

3. **Rule-density or rule-count routing.** Falsified. Kept in this list explicitly so the
   falsification is not re-derived: see the header section and `dead-end-census`.

4. **Gemination / length as its own representation tier.** Amharic encodes length with the modifier
   `ː` inside grapheme text (e.g. `ጽː`, `ስጥːህ`) and `prule4` "remove consonant length from lexical
   forms" is an ordinary class-to-class rewrite over it. So length is already just segments, and
   nothing found shows a construction that would differ if it were a tier.

5. **Cyclic or stratal *re*application of the phonological cascade beyond HermitCrab's stratum
   model.** All four grammars use three strata with phonology declared on at most one of them
   (Amharic: stratum 2; Indonesian: stratum 1; Sena: none; Aweti: fwdata strata). Nothing found
   requires a cycle within a stratum.

6. **Metathesis × reduplication interaction.** `metathesis-phase-isolation` happens to contain both,
   but N=1 for metathesis in the whole corpus and no source found argues the two constructions
   interact. Flagged, not claimed.

7. **Per-word (rather than per-grammar) selection of the phonological construction.** No evidence
   any grammar needs one; `mainline-selection-audit.md` §C3 independently lists this as a condition
   that would invalidate the whole per-grammar approach, and finds no instance.

8. **Alpha-variable polarity `minus` ("disagree") as a switch.** Unimplemented in `replace.rs`, zero
   occurrences across the four grammars (verified: every `<AlphaVariable>` in Amharic and Indonesian
   is a plain agreement binding). A construction difference is plausible in principle — disagreement
   is not the complement of agreement under the joint-agreement filter — but nothing found
   demonstrates it.

9. **Prosodic (foot/syllable) conditioning of infix placement.** Yu 2007's pivot theory distinguishes
   edge pivots from *prominence* pivots (stressed foot/syllable/vowel), and only the former are
   expressible in HermitCrab's segmental pattern language. This is a real published distinction, but
   this report found no grammar and no HermitCrab construct that would let a prominence pivot be
   *stated*, so there is nothing to switch between. Recorded as a modelling gap, not a switch.

10. **Rule-level exception "unless" environments as a phonological switch.** `ExcludedEnvironments`
    is declared in the schema and used **zero** times across all three HC grammars (measured). MPR
    gating is the mechanism that is actually used, and it belongs to the other half of this space.

---

## 5. Summary

| # | Switch | Trigger cost | Evidence | Fires on Sena? | Mainline already? |
|---|---|---|---|---|---|
| P1 | Junction-locality partition | cheap | **(a)** Amharic `prule6`/`prule7` minimal pair; Aweti 15/18 | no | partly (`junctions.rs`, unpartitioned) |
| P2 | Rule-dependency depth (feeding/bleeding) | cheap (O(rules²×class)) | **(a)** Indonesian 4-rule chain + **(b)** Baković 2011 | no (graph empty) | no |
| P3 | Direction / application mode | cheap | **(a)** Aweti 16 LTR / 1 RTL / 1 SIM + **(b)** Johnson 1972 | no | prototype only; **capability grades the wrong compiler** |
| P4 | Self-feeding spread closure | cheap | **(a)** Aweti rule 1 (fires, depth-1 in fact) + **(b)** Chandlee 2014, Hansson 2010 | no | no |
| P5 | Unbounded transparent-span agreement | cheap | **(a)** Indonesian `prule3` (`max=-1` over all 29 segments) + **(b)** Nevins 2010 | no | no (probe is ±1) |
| P6 | Multi-slot interdigitation | cheap | **(a)** Amharic `mrule4`/`mrule6`/`mrule13` + **(b)** Kay 1987 | no | yes (`preexpand.rs`, by enumeration) |
| P7 | Reduplication: copy test + boundedness | cheap ×2 | **(a)** Indonesian 3 true copies, Amharic 5/5 hint false positives + **(b)** Rubino WALS 27A, Culy 1985 | no | yes (`peel.rs`, unbounded only) |
| P8 | Copy-sensitive phonology | cheap (over-approx.) | **(a)** Indonesian `prule3` × 3 copy rules | no | no |
| P9 | Subtractive morphology | cheap | **(a)** Amharic 4 subrules + **(b)** Manova 2019 | no | yes (structural composites) |
| P10 | Metathesis presence/direction/boundary | cheap | **(b)** Blevins & Garrett 2004, Edwards 2020 — **(a) is 0/4** | no | no construction; trips S1 widening |
| P11 | Representation-alias scale | cheap | **(a) measured**: Aweti 110/1211 over cap, max 4,096 + **(b)** Ethiopic homophony | trivially (product 2) | yes (product, capped at 64) |
| P12 | Multi-table cross-table aliasing | cheap | **(b)** Serbian digraphia — **(a) is 0/4**; shipped emitter reads one table | no | prototype only |
| P13 | Null-morph boundary markers | cheap | **(a)** Sena 7×`^0+`, measured 425× blow-up | **yes, and must** | yes (`build.rs`, named scope gap) |
| P14 | Deletion-junction onset gating | cheap | **(a) measured**: Indonesian 20/66 roots, 70% of stripped lexicon dead | no | yes, deliberately ungated |
| P15 | Epenthesis and its widening scope | cheap | **(b)** Hall 2011, Blevins 2008 — **(a) is 0/4** | no | yes, as a whole-grammar widening |

**Counts: 12 switches with evidence of kind (a), 3 with kind (b) only, 10 candidates with neither
(§4).**

Every trigger in the catalogue is cheap — structural or a text-length pass — which is the expected
result: the expensive facts in this half (genuine subrule-environment overlap, the α-tuple survivor
count, the per-(root, rule) composite outcome) are *outcomes* of a chosen construction, not inputs
to choosing one. The one place a magnitude is load-bearing (P11) is an emitted-output count, not an
input count, following the correction `EnumerationBudget` already had to make.

Three findings are worth acting on independently of any switch: Aweti's RTL and simultaneous rules
(§2.1) mean a real grammar's capability verdict is decided by a compiler it never runs; nine percent
of Aweti's forms already overflow `REP_VARIANT_CAP` (§3.11); and `probe_would_refuse` — the largest
grammar-keyed fork in the shipped emitter — is dead on all four grammars (§3.10, §3.15).

## Sources

- Eric Baković, ["Opacity and ordering"](https://home.uni-leipzig.de/muellerg/bakovicopacity.pdf), *The Handbook of Phonological Theory* 2e, Blackwell, 2011.
- Kenneth R. Beesley & Lauri Karttunen, *Finite State Morphology*, CSLI, 2003; ["Twenty-Five Years of Finite-State Morphology"](https://web.stanford.edu/group/cslipublications/cslipublications/koskenniemi-festschrift/8-karttunen-beesley.pdf).
- Juliette Blevins, ["Consonant Epenthesis: Natural and Unnatural Histories"](https://julietteblevins.ws.gc.cuny.edu/files/2016/10/Blevins2008d-ConsonantEpenthesis.pdf), OUP, 2008.
- Juliette Blevins & Andrew Garrett, ["The evolution of metathesis"](https://linguistics.berkeley.edu/~garrett/Blevins-Garrett-2004.pdf), *Phonetically Based Phonology*, CUP, 2004.
- Jane Chandlee, [*Strictly Local Phonological Processes*](https://chandlee.sites.haverford.edu/wp-content/uploads/2015/05/Chandlee_dissertation_2014.pdf), PhD diss., Univ. of Delaware, 2014.
- Christopher Culy, ["The complexity of the vocabulary of Bambara"](https://link.springer.com/article/10.1007/BF00630918), *Linguistics and Philosophy* 8, 1985.
- Sebastian Drude, ["On the position of the Awetí language in the Tupí family"](https://www.researchgate.net/publication/335232740_On_the_Position_of_the_Aweti_Language_in_the_Tupi_Family), Lit Verlag, 2006; [Awetí bibliography](http://www.etnolinguistica.org/lingua:aweti).
- Owen Edwards, [*Metathesis and unmetathesis in Amarasi*](https://langsci-press.org/catalog/book/228), Language Science Press, 2020.
- Nancy Hall, ["Vowel Epenthesis"](https://onlinelibrary.wiley.com/doi/abs/10.1002/9781444335262.wbctp0067), *The Blackwell Companion to Phonology*, 2011.
- Gunnar Ólafur Hansson, [*Consonant Harmony: Long-Distance Interaction in Phonology*](https://escholarship.org/uc/item/2qs7r1mw), UC Press, 2010.
- Richard Ishida, ["Amharic orthography notes"](https://r12a.github.io/scripts/ethi/am); Tsegaye et al., ["The Ethiopic script"](https://journals.uio.no/osla/article/download/4422/3888), *Oslo Studies in Language*.
- Jelena Ivković, ["Pragmatics meets ideology: Digraphia … in Serbian online news forums"](https://www.benjamins.com/catalog/jlp.12.3.02ivk), *Journal of Language and Politics* 12(3), 2013.
- C. Douglas Johnson, [*Formal Aspects of Phonological Description*](https://www.degruyterbrill.com/document/doi/10.1515/9783110876000/html), Mouton, 1972 ([scan](https://pages.ucsd.edu/~ebakovic/compphon/Johnson%201972%201-up.pdf)).
- Ronald M. Kaplan & Martin Kay, ["Regular Models of Phonological Rule Systems"](https://aclanthology.org/J94-3001/), *Computational Linguistics* 20(3), 1994.
- Martin Kay, ["Nonconcatenative Finite-State Morphology"](https://aclanthology.org/E87-1002/), EACL, 1987.
- Stela Manova, ["Subtraction in Morphology"](https://oxfordre.com/linguistics/display/10.1093/acrefore/9780199384655.001.0001/acrefore-9780199384655-e-572), *Oxford Research Encyclopedia of Linguistics*, 2019; Birgit Alber & Sabine Arndt-Lappe, ["Templatic and subtractive truncation"](https://www.uni-trier.de/fileadmin/fb2/ANG/Linguistik/Arndt-Lappe/Alber_ArndtLappe12_draft.pdf), OUP, 2012.
- Andrew Nevins, [*Locality in Vowel Harmony*](https://mitpress.mit.edu/9780262513685/locality-in-vowel-harmony/), MIT Press, 2010.
- Carl Rubino, ["Reduplication"](https://wals.info/chapter/27), WALS Online ch. 27.
- Alan C. L. Yu, [*A Natural History of Infixation*](https://www.cambridge.org/core/journals/journal-of-linguistics/article/abs/c-l-alan-yu-a-natural-history-of-infixation-oxford-studies-in-theoretical-linguistics-15-oxford-oxford-university-press-2007-pp-x264/3E004FAE5686934780E193D31D8A72C9), OUP, 2007.

Repo sources: `samples/data/{amharic-hc,indonesian-hc,sena-hc}.xml`, `samples/data/aweti.json`
(all read directly for this report); `docs/research/handspun-technique-audit.md`,
`docs/research/grammar-feature-space.md`, `docs/research/mainline-selection-audit.md`,
`docs/research/per-language-fst-synthesis.md`; `docs/fst-plan/linguistic-recipe-harvest.md`,
`docs/fst-plan/large-lexicon-proposal-explosion.md`,
`docs/fst-plan/recipe-parity-plan-2026-07-30.md`;
`docs/conformance/representative-typology-basis.md`;
`rust/crates/pg-foma/src/{emit,junctions,peel,preexpand,capability,build}.rs`;
`rust/crates/pg-grammar/src/compile/chardef.rs`; `.claude/skills/dead-end-census/SKILL.md`.
