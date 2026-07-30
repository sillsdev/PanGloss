# HermitCrab → FST expressibility ledger

Research report, agent 5/6. No code changed. Scope: which HermitCrab (HC) grammar constructs
compile into pure finite-state form, which cannot, and what the field (Xerox/HFST/foma/GiellaLT)
does about the constructs that cannot. Claims are marked **VERIFIED** (read directly from a cited
source in this session) or **INFERRED** (reasoned from verified facts, not independently
re-derived from a primary source); "unknown" is stated rather than guessed.

## 0. Method and sources read

Primary sources read in full or in relevant part this session (all `path:line` citations below
are to these unless otherwise marked with a URL):

- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/HERMITCRAB_FST_ADVISOR.md`
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/foma-fst-plan.md`
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/FST_FULL_GRAMMAR_PLAN.md` (first 654 of
  1455 lines — the legacy `hc-hybrid` phase-history; the theory/architecture sections needed for
  this ledger are all in that range, confirmed by the companion `HYBRID_FST_FEASIBILITY.md` which
  restates the same material as a coherent report)
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/HYBRID_FST_FEASIBILITY.md` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/F1_QUIRK_AUDIT.md` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/LEVER_2.md` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/grammar-optimization-techniques.md` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/mpr-overwrite-encoding-research.md` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/docs/fst-plan/morphotactic-composite-pruning.md` (full)
- `C:/Users/johnm/Documents/repos/LCAtom/docs/hc-grammar-map.md` (full)
- `C:/Users/johnm/Documents/repos/LCAtom/docs/hc-surface-scope.md` (full)
- `pg-grammar/src/model.rs` (grep on struct/enum definitions: `PatternNode`, `RewriteRuleDef`,
  `RewriteSubruleDef`, `MetathesisRuleDef`, `OutputAction`, `CompoundingRuleDef`, `MprGroup`,
  `MprGroupOutput`)
- `pg-rules/src/rewrite.rs`, `metathesis.rs`, `morph.rs`, `bridge.rs`, `lib.rs` (read directly)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/rewrite.rs`, `flags.rs`, `apply.rs`,
  and a repo-wide grep for `reduplicat|compile.replace|Redup` (zero hits — see §4)
- Web: Kaplan & Kay (1994), ACL Anthology `J94-3001`; Hulden & Bischoff on 2-way-FST
  reduplication; Chandlee ISL/OSL papers (ACL Anthology `Q14-1038`, `W15-2310`); GiellaLT's own
  flag-diacritics doc page (`giellalt.uit.no/lang/sme/docu-sme-flag-diacritics.html`); HFST/Xerox
  archiphoneme material.

I did **not** clone Divvun/GiellaLT repositories locally — the scratch directory
`.../scratchpad/divvun/a5/` was provisioned but not needed; everything material about GiellaLT's
own toolchain choices was either already documented in this repo's own research (which cites
GiellaLT/Divvun directly, see `grammar-optimization-techniques.md`'s F1 entry) or independently
confirmed via GiellaLT's own public documentation pages (cited above). Where I report on GiellaLT
language-repo *content* (not tooling) without having opened a language repo myself, I mark it
**unknown** rather than assert it.

---

## 1. The construct inventory (from this repo, not memory)

Built from `hc-grammar-map.md`, `hc-surface-scope.md`, and the `pg-grammar`/`pg-rules` source
directly.

| # | Construct | HC/LibLCM source | Rust port |
|---|---|---|---|
| C1 | SPE rewrite rule, 4 shapes (feature-change, deletion, epenthesis, narrowing — dispatched by LHS-vs-RHS child count) | `PhRegularRule`/`PhSegRuleRHS` (`hc-grammar-map.md:20`); `hc-surface-scope.md:34` | `pg_grammar::model::RewriteRuleDef`/`RewriteSubruleDef` (`model.rs:410-430`); applied in `pg-rules/src/rewrite.rs:1-38` |
| C2 | Rewrite-rule direction (`Dir::LeftToRight`/`RightToLeft`) | | `model.rs:390-393` |
| C3 | Rewrite-rule application mode: `Iterative` vs `Simultaneous` | | `model.rs:385-388` |
| C4 | Ordered strata (linear pipeline; stratum *k* sees only *k−1*'s output); `Linear`/`Unordered` per-stratum rule combination | `MoStratum`, `Language.Strata` (`hc-grammar-map.md:49`, explicitly **"never read" — T1∖T2, FieldWorks caps at 3 hardcoded strata**, `hc-surface-scope.md:49`) | `pg-rules/src/stratum.rs`; `morphotactic-composite-pruning.md:220-224` documents Linear-over-approximated-as-Unordered as a deliberate, sound widening |
| C5 | Alpha variables (feature-agreement across positions, real unification variables), 24-name ceiling | `IPhRegularRule.FeatureConstraints` (`hc-grammar-map.md:41`); crash point (`hc-grammar-map.md:74`) | `bridge.rs`'s `VarOccur`/`pattern_var_occurrences` (`bridge.rs:50-85`) — **flagged frozen-contract gap**: the compiled FST cannot bind variables, lowered to `UNCONSTRAINED` (over-approximation), agreement re-checked post-hoc (`bridge.rs:16-25`) |
| C6 | Feature-structure segments matched by unification (not symbol equality); natural classes, intensional (`FeatureNaturalClass`) and extensional (`SegmentNaturalClass`) | `hc-surface-scope.md:36` | `NaturalClassKind::Feature`/`Segments` (`bridge.rs` module doc, lines 10-16); `pg_featstruct` |
| C7 | Boundary markers (`PhBoundaryMarker`, char-def `Type=Boundary`) | `hc-grammar-map.md:20` | `CharDefKind::Boundary`; excluded from v1's rewrite-compiler alphabet (`F1_QUIRK_AUDIT.md` item 2) |
| C8 | Affix process rules, 4 output actions: `Copy`, `InsertSegments`, `InsertSimpleContext`, `ModifyFromInput` — **no delete action** (deletion = omitting a copy) | `hc-surface-scope.md:35` | `OutputAction` (`model.rs:698-712`); `pg-rules/src/morph.rs:1-75` |
| C9 | Affix templates + slots (one flat ordered list per HC; a slot holds competing rules) | `MoInflAffixTemplate`/`MoInflAffixSlot` (`hc-grammar-map.md:25`) | `pg-rules/src/stratum.rs`; `morphotactic-composite-pruning.md`'s `synth_slots_generic` model (§"the engine's synthesis morphotactics") |
| C10 | Compounding rules (endo/exo; head/non-head/output MPR sets; bounded by `MaxStemCount`) | `MoEndoCompound`/`MoExoCompound` (`hc-grammar-map.md:22`) | `CompoundingRuleDef` (`model.rs:714-722`); trie compound loop, `FST_FULL_GRAMMAR_PLAN.md` Phase G2 |
| C11 | Reduplication (idiom: an LHS part named and referenced ≥2× in RHS Copy/Modify actions, plus `ReduplicationHint` — **HC has no dedicated reduplication type**) | `hc-surface-scope.md:46` ("HC has no reduplication *type*") | `pg-rules/src/morph.rs:33-45` `classify_redup`; runtime "peel" proposer, not compiled (`HYBRID_FST_FEASIBILITY.md` §4, §5.4) |
| C12 | Metathesis rules (switch-position pattern match + reorder/feature-union) | `PhMetathesisRule` (`hc-grammar-map.md:20`); **T3 = ✗ in PanGloss Phase A** (`hc-surface-scope.md:47`) | `pg-rules/src/metathesis.rs` (full synthesis/analysis port, 932 lines); FST containment proven bounded (256-combo cap, `FST_FULL_GRAMMAR_PLAN.md` I5) |
| C13 | Circumfix cross-products | `hc-surface-scope.md:47` (T3 ✗, Phase B) | Emitter handles via paired-entries encoding, `foma-fst-plan.md:213` P1d item 3 |
| C14 | MPR features/groups: `Append` (union) vs **`Overwrite`** (non-monotone clear-then-set), `MatchType`, required/excluded consumption | `MoInflClass`, `ProdRestrictOA` (`hc-grammar-map.md:28`); `hc-surface-scope.md:44` | `MprGroupOutput::Overwrite/Append` (`model.rs:843-846`); `mpr-overwrite-encoding-research.md` (the crux document, §3 below) |
| C15 | Disjunctive/free-fluctuation allomorph order — **semantic** (elsewhere-blocking), not positional | `hc-grammar-map.md` (slot order note); `hc-surface-scope.md:42` | Not itself an FST-hard construct; an ordering/priority-union semantics issue, handled at the propose/confirm boundary (confirm re-derives real HC semantics) |
| C16 | Co-occurrence / adhoc-prohibition rules (5 adjacency modes) | `MoAlloAdhocProhib`/`MoMorphAdhocProhib` (`hc-grammar-map.md:26`) | Not directly covered in the crates read this session; treated as a lexical/rule-admission filter, same shape class as C14 |
| C17 | Stem names (region-gated suppletion) | `MoStemName.RegionsOC` (`hc-grammar-map.md:23`); untested in any reference grammar (`hc-surface-scope.md:41`) | unknown — no reference-grammar evidence either way |
| C18 | Realizational affix process rules + `LexFamily` suppletive-stem selection | `hc-surface-scope.md:50` — **T2 = ✗, HCLoader has a literal `// TODO`; unreachable from FieldWorks entirely** | Engine-side port exists (`RealizationalRuleDef` referenced in `morph.rs:139`) but nothing can feed it from the FieldWorks path — out of scope for this ledger's "T2 = full coverage target" |
| C19 | Multi-stratum / multiple phoneme sets / per-level inventories | `hc-surface-scope.md:49,51` — **T1∖T2, structurally unreachable from FieldWorks** | out of scope, same reason as C18 |
| C20 | Deletion re-application count (`DeletionReapplications`) | `hc-grammar-map.md:21` (`<HC>` param) | Structural "floors" cap in the FST inverse compiler, `FST_FULL_GRAMMAR_PLAN.md` I3 |
| C21 | Optional segments / quantified environment spans (`OptionalSegmentSequence`, `min`/`max`) | | `PatternNode::Quantifier` (`model.rs:298-300`) |
| C22 | `NoDefaultCompounding`, obligatory vs. optional rule application, `Obligatory`/blockable | `hc-grammar-map.md:21` | `blockable()` gate, `pg-rules/src/morph.rs:141` `apply_blocking` |

This table is the ledger's row set for §2.

---

## 2. Construct-by-construct verdict

Legend: **(a) FST-exact** — compiles to an exact finite-state construction; **(b) FST-bounded** —
expressible only via a bounded approximation (enumeration/cartesian product/depth cap), with a
named blow-up formula; **(c) not finite-state** — provably outside the regular class, permanent
carve-out.

### (a) FST-exact

| Construct | Construction | Why exact |
|---|---|---|
| **C1 rewrite rule** (obligatory, directional, non-self-recursive) | `replace` (`->`/`@->`) with left/right context restriction (`=>` in xfst terms; foma's `replace` calculus, `pg-foma`'s `crate::replace`) | Kaplan & Kay 1994 (ACL `J94-3001`): a context-sensitive rewrite rule `φ→ψ/λ_ρ` with regular `φ,ψ,λ,ρ`, applied obligatorily and directionally (not recursively into its own unbounded output), denotes a regular relation **however long the contexts are** — the theoretical license this repo's own `HYBRID_FST_FEASIBILITY.md` §5.1 and `HERMITCRAB_FST_ADVISOR.md` §7 both cite verbatim. **VERIFIED** against the paper's own abstract (ACL Anthology `J94-3001`). PanGloss's own P6 prototype (`foma-fst-plan.md` P6 item 1) confirms this empirically: all 18 Aweti phonological rules compose in ~27ms via replace-rule compilation, not enumeration. |
| **C2 direction** | foma's directional `->`/right-to-left compile flag | A direction annotation on an already-regular relation; no additional expressive burden. `pg-rules::metathesis` independently confirms `Direction::RightToLeft` compiles and matches correctly (traversal-relative anchors handled, `metathesis.rs:126-142`). |
| **C4 ordered strata (as composition), when non-cyclic and non-reentrant** | Sequential composition (`.o.`) of per-stratum networks | An ordered cascade of regular relations is itself a regular relation, closed under composition (Kaplan & Kay's own closure argument) — see §5 for exactly when this breaks. |
| **C7 boundary markers** | Ordinary literal symbols in the alphabet (a `Boundary`-typed char-def is just another symbol with its own feature lanes) | `bridge.rs` module doc: "every char-def, segment or boundary, carries a full feat_sys-len-wide lane row" — no special-casing needed at the theory level. (The *v1 C# compiler's* alphabet excluded boundaries as an engineering choice/bug, not a theoretical limit — `F1_QUIRK_AUDIT.md` item 2, fixed in the chain compiler.) |
| **C8 affix process rules: Copy + InsertSegments** | lexc concatenation (root ∘ affix strings) | Pure concatenative morphotactics — the textbook case; PanGloss's own emitter (D3, `foma-fst-plan.md:136-144`) emits these as literal lexc entries at 100% recall on Sena/Indonesian. |
| **C9 affix templates/slots (finite slot count, finite rule-per-slot count)** | lexc continuation classes, one per slot | Finite union/concatenation of a bounded structure — no different in kind from ordinary concatenative morphotactics. |
| **C10 compounding (bounded root count)** | A bounded loop in the lexc network (one join state per attachment site) | Finite for any fixed `MaxStemCount`; PanGloss's own C# hybrid built this as a "compound loop," later *discovered* to be a true graph cycle rather than a DAG (`HYBRID_FST_FEASIBILITY.md` §8.5, corrected 2026-07-11) but still terminating because every lap consumes ≥1 input segment — **VERIFIED** as a real, previously-mis-stated finding in this repo's own history, a useful caution for anyone re-deriving "bounded compounding is trivially finite." |
| **C12 metathesis (bounded window, bounded combination count)** | A bounded window-swap transducer; PanGloss's own C# `RuleInverseCompiler` compiles it with a 256-combination cap (`FST_FULL_GRAMMAR_PLAN.md` I5) | Metathesis over a *bounded* switch span is a finite relabeling of a bounded automaton region — regular by construction once the span is bounded (HC's own `MetathesisRuleDef` pattern is always fixed-width in every attested grammar, `pg-rules/src/metathesis.rs:20-28`). Beesley & Karttunen's `xfst` also treats bounded metathesis as an ordinary composed rewrite; this is textbook, not novel. |
| **C21 quantified environment (bounded max)** | foma's `{min,max}` quantifier over compiled children | Finite unrolling; only unbounded max (`max = -1` / Kleene star as an *environment*, not the rule's own LHS/RHS) stays regular too, per Kaplan & Kay (context length is unconstrained in their theorem) — the `HERMITCRAB_FST_ADVISOR.md` §7 reclaim table makes this exact distinction: "Unbounded-environment rewrite (harmony/spread): `Regular = true` — iff the rule's own Lhs/Rhs are bounded." |

### (b) FST-bounded (enumeration/approximation, with a blow-up formula)

| Construct | Bridging technique used today | Blow-up formula | Where it becomes intractable |
|---|---|---|---|
| **C5 alpha variables** | One representative feature-value probed per variable, rather than enumerating all bindings ("Permissive" tier, `F1_QUIRK_AUDIT.md` item 5, `RuleInverseCompiler.cs:447-486`); PanGloss's P6 prototype instead does **tuple-indexed enumeration**: Amharic's 20-α-variable CV-merger compiles to 312 concrete tuples (`foma-fst-plan.md` P6 item 1) | If genuinely enumerated: `∏ᵢ \|domain(αᵢ)\|` over the *distinct* variables in one rule — bounded by HC's own 24-variable ceiling (`hc-grammar-map.md:74`) but domain sizes multiply; 312 tuples for one Amharic rule is the measured real-world instance | Becomes intractable when domain sizes are large (Amharic's 417-segment Ge'ez alphabet, `FST_FULL_GRAMMAR_PLAN.md`'s Amharic section: 417² ≈ 174k naive probes) unless the alphabet is first quotiented by the features the rule actually distinguishes (queued fix, never shipped in the C# hybrid, "feature-quotient the probe alphabet") |
| **C6 unification matching over a finite feature/segment inventory** | `NaturalClassKind::Feature`→lane constraint; `NaturalClassKind::Segments`→union of member char-defs' feature bundles | Always finite **in principle** (finite feature system → finite natural-class alphabet, this is Xerox/HFST's own foundational assumption) but the *practical* blow-up is the segment-alphabet size itself: Amharic's 417 Ge'ez segments vs Indonesian's 29/Sena's 40 (`FST_FULL_GRAMMAR_PLAN.md` Amharic build-cost section) — probing-style compilers pay `O(alphabet²)` per rule-junction, so 417² ≈ 174k probes per affix, measured at ~112s wall time for Amharic's junction-probing build vs 40ms for the bare FST | See §3 — the crux |
| **C11 templatic/interdigitating infixation (Amharic `-pfv-`/`-conv-`)** | Rule-application pre-expansion: for each (root allomorph × infix-rule allomorph) pair, apply the *real* engine mrule and emit the rendered composite as one lexc entry (`foma-fst-plan.md` P1d item 1) | `O(roots × infix-rules)` — bounded at 10²–10³ today; the P6 successor is compiling as foma rules over root *patterns* rather than per-root enumeration | Aweti (855 roots × 47 fusion-eligible rules, `morphotactic-composite-pruning.md`) blows this up to 2.83M composite entries / 691MB lexc source — the enumeration bridge is confirmed non-viable at FLEx scale for this grammar shape (§ "Aweti end-to-end result," same doc) |
| **C11 boundary fusion / glyph coalescence (Amharic Ge'ez)** | Probe real (left-morph-final, right-morph-initial) adjacency pairs, emit fused-spelling variants (`foma-fst-plan.md` P1d item 2) | Bounded by actual adjacency pairs observed | Same P6 successor as above |
| **C14 MPR `Overwrite` groups** | See §3 — a **reachability proof** (Construction 2) discharges the risky case entirely for every grammar this project has measured; the fallback (Construction 3, dual-rail/bilattice) is a genuine bounded blow-up | `4^k` states threaded through the derivation, `k` = group size, **only if** Construction 2's reachability predicate fails for that group | Not hit by any of the 3 reference grammars today (`mpr-overwrite-encoding-research.md` §5); a hypothetical grammar with a real multi-member `Overwrite` group and genuinely conflicting reachable touches would pay this cost |
| **C13 circumfix cross-products** | Paired prefix+suffix entries sharing one morpheme tag (`foma-fst-plan.md` P1d item 3) | `O(prefix-variants × suffix-variants)`, bounded, small in practice | unknown at what root count this becomes costly — not measured on a real grammar in this repo's evidence |
| **Deletion restoration (C1 special case, C20)** | Structural "floors": `cap+1` automaton copies, restorations strictly ascend (`FST_FULL_GRAMMAR_PLAN.md` I3) | `O(cap)` states per rule, `cap = DeletionReapplications + 1` by default | Known narrowing, not blow-up: one engine "round" restores multiple sites simultaneously while the chain counts restoration *events* — under-covers on multi-site words, falls back rather than exploding (`HYBRID_FST_FEASIBILITY.md` §8.2) |
| **Compounding beyond 2 roots** | Lift the loop bound to `MaxStemCount` | Linear in root count, but interacts with the graph-cycle finding above — needs explicit token-accumulation dedup once genuinely unbounded (`HYBRID_FST_FEASIBILITY.md` §8.5) | unknown — no grammar in evidence needs >2 |

### (c) Provably not finite-state (permanent carve-out)

| Construct | Reason | Citation |
|---|---|---|
| **C11 unbounded-copy reduplication** (copy an arbitrarily long stem: `w → ww`) | The language `{ww : w ∈ Σ*}` fails the pumping lemma for regular languages — no finite-state machine of any size represents unbounded copying, because a regular relation's output length is bounded by a fixed multiple of input length *plus* a state-bounded window, and unbounded copying requires unbounded memory of the whole first copy to reproduce it exactly. | Hopcroft & Ullman (1979), pumping lemma, cited directly in `HYBRID_FST_FEASIBILITY.md` §5.4; independently, Hulden & Bischoff's own 2-way-FST reduplication paper (found via search) states 2-way FSTs are needed to capture "virtually all" reduplicative processes — meaning ordinary (1-way) FSTs cannot, which is the same result from a different angle. **VERIFIED** (both the theoretical pumping-lemma argument and the independent Hulden citation agree). |
| **Unbounded-environment agreement over an unbounded alphabet** (a variable whose domain is not finite — HC does not actually have this: features are always drawn from a finite, declared value set, so this row is a **theoretical boundary check, not a real HC construct**) | If a feature's domain were unbounded, alpha-variable agreement would require unbounded state to remember the bound value; HC's `FsClosedFeature` domains are always finite and declared (`hc-grammar-map.md:19`), so this never actually arises for HC — noted only because the user's brief asked for it explicitly. | **INFERRED** from HC's own closed feature-value model; not a construct any reference grammar exercises. |

---

## 3. Feature-structure unification: the crux, quantified

**Is it always finite?** Yes, in principle: HC's feature system is closed (`FsClosedFeature`,
`hc-grammar-map.md:19`) — every feature has a finite, declared set of values, so the space of
distinct feature-structure equivalence classes over the phonological feature system is finite, and
a natural class (however authored — intensionally as a feature bundle, or extensionally as an
enumerated segment set) always denotes a finite union of concrete segments. `pg-rules::bridge`'s
own module doc states this directly: `NaturalClassKind::Feature` resolves to a per-lane bitset
constraint over a fixed-width lane vector (`bridge.rs:10-16`), and every char-def "carries a full
`feat_sys.len()`-wide lane row" (`bridge.rs` module doc). This is **VERIFIED** from the source, not
an assumption — it is the compiled representation actually shipped.

**What's the real blow-up on a realistic feature system?** Two independent, measured numbers from
this repo's own history establish the shape of the cost, and they are different costs:

1. **Alphabet size, not feature-system size, dominates in practice.** Amharic's real cost driver
   is not "how many features" but "how many *distinct segments* the language's script enumerates" —
   417 Ge'ez fidel-as-segments vs. Indonesian's 29 / Sena's 40 (`FST_FULL_GRAMMAR_PLAN.md`, Amharic
   build-cost section). A probing-style compiler that walks "one representative per alphabet
   member" pays `O(alphabet)` per probe and `O(alphabet²)` for two-neighbor fallback probes,
   which is "dozens²" by the mechanism's own design assumption (`HYBRID_FST_FEASIBILITY.md` §8.3)
   but 417² ≈ 174,000 for Amharic — measured at ~112s wall time for a mechanism that completes in
   40ms on the bare (unprobed) FST. **This is the single most concrete, quantified answer to "what
   does unification-to-symbol compilation really cost."**
2. **Alpha-variable enumeration, when done exactly (tuple-indexed), is grammar-specific but
   bounded.** PanGloss's own P6 prototype measured Amharic's 20-variable CV-merger rule compiling
   to exactly 312 concrete tuples (`foma-fst-plan.md` P6 item 1) — not 417^20 (naive) nor a
   symbolic/lazy representation, but a real enumerated set, because the *rule* only distinguishes a
   small number of the 417 segments' features, and the tuple-indexed model exploits that (this is
   exactly the "feature-quotient the probe alphabet" fix named as queued-but-unshipped for the
   C# hybrid, and evidently *shipped* for the foma-based P6 path).

**Recommended fix (repo's own queued item, never shipped in the retired C# hybrid, apparently
addressed differently in the foma path):** quotient the probe/enumeration alphabet by the feature
values the grammar's rules actually reference, not by the raw segment count. Ge'ez's 417 fidel
mostly differ in features no *rule* mentions (they encode syllable-final-vowel distinctions that
matter to spelling, not to the 7 phonological rules) — so the real number of *rule-relevant*
classes is expected to be "dozens," restoring the probing mechanism's own design assumption.

**What does Xerox/HFST/GiellaLT do instead?** Three established techniques, cross-checked against
this repo's independent research and public GiellaLT documentation:

- **Archiphonemes.** The classical two-level-morphology answer (Koskenniemi 1983, cited directly
  in `grammar-optimization-techniques.md` F1 and `mpr-overwrite-encoding-research.md`): an
  archiphoneme is a placeholder symbol (conventionally in `{}`) standing for an underspecified
  segment whose surface realization is resolved by a *separate*, parallel twolc rule keyed on
  context — e.g. a single archiphoneme `{N}` covers a nasal whose place is not yet decided, and
  three twolc rules realize it as `m`/`n`/`ŋ` in the three relevant contexts. This is functionally
  identical to HC's alpha-variable-governed nasal-place agreement (C5) but pre-committed at
  lexicon-authoring time to a *symbol*, not resolved dynamically by unification — the archiphoneme
  approach moves the "which value" decision from runtime unification into the finite alphabet
  itself, at zero compile-time enumeration cost, because the author (not the compiler) already
  picked the disjoint symbol set. **This is a genuine architectural alternative to alpha-variable
  compilation** worth naming for PanGloss: instead of compiling HC's unification variables, a
  grammar *author* could be asked to declare the archiphoneme inventory directly (a natural-class
  authoring convention), sidestepping the enumeration question entirely — though this changes the
  authoring contract (HC grammars as written do not use this convention; it would require either a
  lossy re-authoring step or accepting archiphonemes as a *compiled artifact* the tool infers, which
  is exactly the alpha-variable-tuple-enumeration PanGloss's P6 already does).
- **`Sets` in twolc / `Rule Variables`.** HFST-twolc and the original Xerox twolc both support
  declaring a `Sets` block of named symbol groups and writing rule schemas parameterized over a
  shared variable that ranges over a set, expanding at compile time into one rule per member — this
  is the textbook version of PanGloss's own "one representative per class" / tuple enumeration,
  confirmed independently via HFST's own documentation (search result: "HFST-TwolC ... supports
  defining a set of similar two-level rules using a rule-schema with variables," matching Xerox
  twolc's semantics). **VERIFIED** the technique exists and matches PanGloss's own approach in
  spirit; not independently confirmed against HFST-twolc's actual source in this session.
- **Multichar flag diacritics** for the cases that are not really "which surface segment" questions
  but "does this long-distance condition hold" questions (MPR-style rule-exception gating, C14).
  This repo's own, very thorough, empirical investigation (`gate.rs`'s module doc, independently
  reproduced by `mpr-overwrite-encoding-research.md` §Construction 4 with fresh throwaway probes)
  found flag diacritics **specifically fail** at the one site HC's real MPR usage sits at — inside
  a `->` replace rule's own context — in this vendored `foma = 0.4.2`: a flag literal inside a
  replace-rule context either produces nondeterministic apply-time results or crashes the minimizer
  (`STATUS_STACK_BUFFER_OVERRUN`), and `fsm_compose` does not treat flags as epsilon-transparent by
  default, silently collapsing a flag-bearing network composed with a flag-free one to the empty
  language. This is a **toolkit-specific finding, not a theoretical one** — GiellaLT's own North
  Sami grammar *does* use flag diacritics successfully, per GiellaLT's own documentation page
  (`giellalt.uit.no/lang/sme/docu-sme-flag-diacritics.html`, confirmed via search: flags there
  "remove illegal compounds" and handle proper-noun downcasing) — so flags are provably usable in
  *some* configurations (standalone, outside a replace-rule context) and provably broken in
  *this project's* vendored foma at the specific configuration HC's MPR groups need. The shipped
  PanGloss replacement is a **static, flag-free lexical partition** (`crate::gate`,
  `PlanNodeKind::Gate`): group lexical entries by which gated subrules they satisfy, compile one
  network per group, union the (lexically disjoint, hence safe-to-union) results
  (`mpr-overwrite-encoding-research.md` §Construction 4, `grammar-optimization-techniques.md` F1).

**Reconciling with this repo's own numbers:** the honest headline is that unification-to-symbol
compilation is finite *by construction* (finite feature system ⇒ finite class alphabet) but the
naive compilation strategies this project actually tried (probe every alphabet member; enumerate
every alpha-variable binding) hit real, measured, alphabet-size-driven blow-ups (Amharic's 417²),
and the fix in both cases is the same idea under two names — **feature-quotienting** (compile
against equivalence classes the rules can distinguish, not raw segment count) — which the older C#
hybrid queued but never shipped, and which the newer foma-based P6 path appears to have
implemented successfully for at least the alpha-variable case (312 tuples, not 417^20).

---

## 4. Reduplication and metathesis

### Reduplication

- **Foma/xfst's actual capability:** `compile-replace` is a genuine, documented two-pass
  preprocessing trick in xfst (Beesley & Karttunen 2000, "Finite-State Non-Concatenative
  Morphotactics," SIGPHON — cited in `HYBRID_FST_FEASIBILITY.md` §5.4) that handles a **restricted**
  class of reduplication (typically CV-template or fixed-pattern copying resolved via an auxiliary
  compile step, not general unbounded copying) — `HYBRID_FST_FEASIBILITY.md` is explicit that this
  is "a two-pass preprocessing trick, not a counterexample" to the pumping-lemma result: it does not
  make unbounded copying regular, it handles the *bounded*, template-shaped cases some languages
  actually need. Hulden's own later line of research (Hulden & Bischoff, found via search: "A
  simple formalism for capturing reduplication in finite-state morphology," and a 2-way-FST
  reduplication paper) generalizes this using **2-way finite-state transducers** (which read their
  input tape in both directions / make multiple passes), explicitly because 1-way FSTs cannot
  capture unbounded/general reduplication — this is the same theoretical result stated
  constructively: if you allow a strictly more powerful machine (2-way FST, still decidable and
  efficiently implementable, but *not* a 1-way regular relation), you recover most reduplicative
  processes.
- **Does `divvun/foma-rs` implement `compile-replace` or the reduplication idiom?** **VERIFIED NO** —
  a repo-wide grep of `C:/Users/johnm/Documents/repos/foma-rs` for
  `reduplicat|compile.replace|compile_replace|Redup` (case-insensitive) returned **zero matches**
  across the entire crate, including `rewrite.rs` (the file that implements foma's `replace`
  calculus proper, read directly — 2000+ lines of `fsm_rewrite`/context/cross-product machinery,
  no reduplication-specific construction anywhere in it). This is a direct, repo-grep-confirmed
  negative result, not an inference from documentation silence. Whatever `compile-replace` does in
  the *original* C `foma`/xfst (Hulden's own tool), **this vendored Rust port does not carry it
  forward** as of the version pinned in this workspace (`foma = "0.4.2"`, per
  `grammar-optimization-techniques.md` A1's citation).
- **PanGloss's own resolution, consistent with the theory:** reduplication is handled entirely
  *outside* the automaton, as a runtime "peel" — scan the surface for a copy, strip it, recurse the
  residual through the FST proposer, wrap with the morpheme, verify-gate the result
  (`HYBRID_FST_FEASIBILITY.md` §4, "Reduplication and infixation (copying)" row; `foma-fst-plan.md`
  D6 "port the peel ... `_eq()`/compile-replace deferred to v2" — and per the grep above, that
  deferral has not been picked up: v2's `compile-replace` never landed, the peel is still the whole
  mechanism). This is architecturally the *correct* response given the math, not a stopgap: an
  O(n²) surface scan is bounded, verify-gated (a wrong peel costs one rejected candidate, never a
  wrong answer), and it is what closed all seven of Indonesian's `-X-X` reduplicated corpus words
  including a suffix stacked outside the copy (`FST_FULL_GRAMMAR_PLAN.md` Phase D/G1).
- **Does GiellaLT ever need reduplication?** **unknown** — I did not find, and did not have time to
  independently verify by cloning a GiellaLT language repo, whether any GiellaLT/Divvun language
  (predominantly Uralic — Sami languages, Komi, Mari, Udmurt, etc. — plus some others) has
  productive reduplication in its shipped grammar. GiellaLT's language set skews toward languages
  without productive full/partial reduplication as a major inflectional device (unlike, say,
  Indonesian/Malay or many Austronesian languages, which is exactly why PanGloss's own reference
  grammar exercises it). This is stated as **unknown**, not "no" — a genuine gap in this session's
  research that a repo clone would resolve.

### Metathesis

- Bounded metathesis (a fixed-width switch span, which is the *only* shape HC's `MetathesisRuleDef`
  or any DTD-legal HC grammar can express, per `pg-rules/src/metathesis.rs`'s own module doc: "a
  real grammar's switch group is always exactly one shape node wide... DTD-legal but fails to
  compile against the real C# engine" for anything wider) is straightforwardly regular — PanGloss's
  own C# hybrid compiled it with a 256-combination cap (`FST_FULL_GRAMMAR_PLAN.md` I5). This is not
  a hard case theoretically; it only *looks* like non-concatenative morphology folklore because
  "metathesis" and "reduplication" are usually grouped together in the literature as "non-
  concatenative," but metathesis (reordering a bounded span) and reduplication (copying an
  unbounded span) are different complexity classes — the former is regular, the latter is not.
- foma's own regex language has an explicit reversal/transposition-adjacent operator family (xfst's
  `[..]` bracket notation for various non-concatenative idioms is part of the classical toolkit,
  per Beesley & Karttunen 2003, cited throughout this repo's `grammar-optimization-techniques.md`);
  I did not independently verify foma-rs implements a metathesis-specific regex idiom (PanGloss's
  own metathesis handling, per `pg-rules/src/metathesis.rs`, is a hand-written matcher/reorder in
  Rust, not compiled through foma's replace calculus at all — the P6 replace-rule-compilation
  workstream is scoped for phonological rewrite rules, and metathesis is separately named as an
  item "now systematically scheduled" but not yet done, per `foma-fst-plan.md` P6's closing bullet
  list: "RTL direction, Simultaneous-mode fidelity, Quantifier patterns, metathesis ... now
  systematically scheduled").

---

## 5. Rule ordering: cascade vs. parallel-intersective, and when the equivalence breaks

HC's ordered strata are a strict pipeline (stratum *k* sees only stratum *k−1*'s output,
`hc-surface-scope.md`'s "What the T1∖T2 gaps actually cost" section) — a **cascade**. Classical
twolc is **parallel-intersective**: every two-level rule constrains the same underlying↔surface
correspondence simultaneously, and the grammar's meaning is the intersection of all rule automata,
not a sequential feed.

**The standard equivalence result** (Kaplan & Kay 1994, the same paper licensing C1): an ordered
cascade of context-sensitive rewrite rules, each individually regular, composes into one regular
relation via sequential composition (`.o.`) — **this holds whenever each rule is applied
obligatorily, directionally, and not recursively into its own unbounded output.** Under those
conditions, a cascade-to-composition translation is exact, and this repo's own evidence backs it up
concretely: PanGloss's P6 prototype composed Aweti's full 18-rule phonological cascade in ~27ms
(`foma-fst-plan.md` P6 item 1), and the MPR-gating fix explicitly relies on stratum-ordered
composition being sound (`foma-fst-plan.md` P6 item 1's "MPR/POS rule-exception gating" bullet:
"one lexc+rule-cascade network is compiled PER GROUP ... groups are lexically disjoint by
construction").

**What breaks, precisely — the four named failure modes:**

1. **Opacity via feeding, when the analysis direction must "see through" a later rule to recover an
   earlier rule's trigger.** This is *not* actually a case where composition fails mathematically —
   Kaplan & Kay's theorem still holds, and PanGloss's own **Lever 2 spike** proved this constructively:
   a hand-built two-rule counterbleeding cascade (`N→n / _t` then `t→∅ / n_`, so `"aNt"→"ant"→"an"`,
   the trigger `t` deleted after triggering assimilation) was recovered **exactly** by lazy
   composition of per-rule inverse transducers, lexicon-constrained (`LEVER_2.md` "The cascade
   test," `LazyComposition_RecoversOpaqueTwoRuleCascade`). What breaks is not the *theory* but the
   **naive compiler strategy**: `LEVER_2.md`'s own finding is that a "B-probe" compiler (probe
   combined *underlying* contexts, attribute the effect to one rule) **misreads** this case — the
   deletion is conditioned on the *surface* `n` that assimilation fed from `N`, not on the
   underlying `N` a single-rule probe would see. The fix is **B-direct**: compile each rule to its
   own transducer and compose the cascade properly (Kaplan–Kay), never probe the combined effect
   and attribute it to a single rule's branch. So: **composition itself does not break under
   feeding/opacity; a probing shortcut that skips real per-rule composition does.**
2. **Counter-bleeding, structurally the same case as (1)** — same resolution (compose per-rule
   inverses properly, don't probe combined contexts).
3. **Cyclic / re-entrant application to a rule's own output within one stratum, when the rule is
   `Iterative` (C3) and self-feeding.** This is the one case this repo's own research found and
   explicitly declined to solve generally: `F1_QUIRK_AUDIT.md` item 6 documents that an *earlier*
   heuristic ("flag a rule iterative-self-feeding whenever its Rhs unifies with its own Lhs or
   environment") was tried and reverted because it produced false positives on essentially every
   ordinary substitution/assimilation rule, degrading a genuinely-Exact Amharic rule to Permissive
   and adding spurious reasons to Indonesian rules. **No general static detection exists**; the
   honest current answer (`HYBRID_FST_FEASIBILITY.md` §8.5, §10.4) is: fall back to the engine for
   genuinely self-feeding iterative rules (verify-safe, never wrong, just under-covers), or — per
   the "100% fast path" plan's own proposed fix — **iterate the rule-inverse dynamically to a
   fixpoint capped at the engine's own reapplication limit**, mirroring what the engine does rather
   than trying to statically classify the rule. This is the sharpest concrete instance of "ordered
   cascade → composition" not being a free translation: `Iterative` mode with genuine self-feeding
   needs either a fixpoint-iteration construction (which is still finite, since it is capped, but
   is not a plain one-shot composition) or an engine fallback.
4. **Cross-stratum interaction where a later stratum's rule needs to distinguish material by *which
   earlier stratum* produced it**, i.e. genuine bracketing paradoxes across strata boundaries.
   `hc-surface-scope.md`'s own "What the T1∖T2 gaps actually cost" section names this directly:
   losing multi-stratum support (T1∖T2, unreachable from FieldWorks anyway) means "no bracketing
   paradoxes" — the point being that HC's *own* T2-reachable subset (FieldWorks-authorable grammars)
   cannot express true multi-stratum bracketing paradoxes at all, since FieldWorks only ever
   produces up to 3 hardcoded strata and HCLoader never lets a user assign per-stratum phonology in
   the T2-relevant sense. **This failure mode is therefore moot for PanGloss's actual input corpus**
   (anything from `.fwdata`/HC-XML-via-HCLoader) — it would only matter for a hypothetical direct-
   HC-XML-authoring path outside FieldWorks, which `hc-surface-scope.md`'s own "Decisions (settled)"
   section rules out as a target (`.fwdata` is the only output; XML is oracle-only).

**Net verdict on §5:** for PanGloss's actual grammars (T2-reachable, FieldWorks-produced), ordered
strata → composition is sound *whenever* (i) rules apply obligatorily/directionally per Kaplan-Kay,
and (ii) the compiler composes per-rule transducers honestly rather than probing combined effects.
The one open gap is genuinely self-feeding `Iterative` rules within a single stratum — a real,
named, unsolved-in-general case, with a concrete (if unbuilt) fixpoint-iteration fix on record.

---

## 6. The replacement plan — direct answers

### 6.1 Can HermitCrab be chucked entirely and go FST-only?

**No, not today, for any grammar with meaningful morphophonology — and the project's own settled
architecture already says so.** `foma-fst-plan.md:19-21` states this as a decided position, not an
open question: *"There is NO per-grammar fallback to full engine search... FST-only (no-verify)
operation is off the table — propose+prune is the permanent shape."* This is not merely a caution
from this ledger; it is the standing architectural decision the whole `pg-foma` line of work is
built on. The reasons this ledger independently confirms are sound:

- **Reduplication (C11, unbounded case) is mathematically outside FST** (§4) — any FST-only design
  either cannot handle a language with productive reduplication at all, or must smuggle in a non-FST
  mechanism (the peel), at which point it is not "FST-only" by definition.
- **Alpha-variable/unification compilation (C5, C6) is finite in principle but only exact when
  enumerated**, and every enumeration strategy tried so far either approximates (Permissive tier,
  dropping the exactness a genuine no-verify design would need) or blows up on large alphabets
  (Amharic's 417² case, §3) — an unverified FST-only analyzer would need every rule at Exact tier,
  which this repo has never achieved on a real grammar (Amharic sits at `Exact=2, Permissive=4,
  IdentitySkip=1` even after I3, `HYBRID_FST_FEASIBILITY.md` §7).
- **MPR `Overwrite` groups (C14)** are non-monotone and only provably safe to compile without loss
  when Construction 2's reachability predicate holds (§3's MPR discussion) — a *hypothetical* future
  grammar with genuinely conflicting reachable touches would need Construction 3's `4^k`-cost
  dual-rail encoding or fall back to `Refuse`, i.e. no-compile.
- **Self-feeding iterative rules (§5 item 3)** have no general static detection; an FST-only design
  would need the fixpoint-iteration construction, unbuilt as of this session.

**Where would FST-only be safe *today*, as a narrower claim?** For the specific, *measured* subset:
a grammar with (i) no productive reduplication, (ii) all phonological rules landing at `Exact` tier
(no alpha-variables needing per-binding enumeration beyond what's been quotiented), (iii) no
`Overwrite` MPR groups with reachable conflicting touches, and (iv) no genuinely self-feeding
`Iterative` rules — **Indonesian, absent its 7 reduplicated words, is close to this** (`Exact=2,
Permissive=3, IdentitySkip=0`, `HYBRID_FST_FEASIBILITY.md` §7) but still not *all* Exact, so even
Indonesian is not a clean FST-only case without accepting Permissive-tier approximation risk with no
verify backstop. **No grammar in this project's evidence is fully FST-only-safe today.** This
matches the project's own explicit position, not just this ledger's independent derivation.

### 6.2 Is it easier with two-or-more FSTs (proposer + specialized pruner(s))?

This is close to what the *existing, sunset* C# `hc-hybrid` architecture already was
(`HYBRID_FST_FEASIBILITY.md` §4's proposer-ensemble table: one shared trie for concatenative
morphotactics, per-rule inverse transducers for phonology, build-time junction probing for the
common boundary-conditioned case, runtime peels for copying) — and the project's own measured
history is a strong, direct answer: **yes, decomposing into multiple *specialized* finite-state
mechanisms plus a runtime peel for the genuinely non-regular part was exactly the architecture that
got Indonesian to 121/121 and Sena to 99.2%+ (later 100%)**, faster than a single monolithic
construction ever achieved, and the project explicitly forbade a single eagerly-composed FST for a
documented reason: "determinizing/minimizing across unification arcs merges genuinely distinct
analysis paths — destroying the multi-analysis enumeration the product needs"
(`HYBRID_FST_FEASIBILITY.md` §4, "Equally important is what the design forbids"). The current foma-
based design keeps this spirit at a higher level: one composed foma network (proposer, over-
generating) plus the real HC engine (pruner) — i.e. today's answer to "how many FSTs" is "one FST +
one *non*-FST pruner," and the ledger's job is to ask whether the pruner itself could become (a set
of) FSTs.

**Can a pruner-as-FST see what it needs?** This is the sharpest open question in the whole brief,
and the evidence in this repo says: **only if the pruner is restricted to constructs whose
gating information can be encoded on the tape itself** (as multichar tags, as flag diacritics where
they're safe, or — critically — **as derivation intermediate strings via composed intermediate
levels**, not just the surface⇄lexical pair). Concrete evidence for each option:

- **Yes, when the derivation intermediate is compiled as a composed level.** This is exactly what
  P6's replace-rule compilation does: instead of a black-box surface⇄lexical FST, it composes a
  *cascade* — lexicon ∘ rule₁ ∘ rule₂ ∘ … ∘ ruleₙ — so the "intermediate strings" a naive B-probe
  compiler would need to guess at are instead *structurally present* as intermediate levels of the
  composed network, and Kaplan-Kay composition correctly propagates feeding/bleeding through them
  (LEVER_2's cascade test, §5 item 1 above). **This is the answer to "does pruning need the
  derivation, not just surface⇄lexical": yes for opaque/feeding interactions, and composing the
  per-rule cascade (not probing the black-box combined effect) is exactly how to get it onto the
  tape.**
- **Yes, for gating conditions, via multichar tags on the analysis tape** — PanGloss's own tag
  alphabet design (`<R:nnnn>`/`<M:nnnn>` multichar symbols, `foma-fst-plan.md` D2) already puts
  morpheme identity onto the tape as symbols the decoder reads back off; the same idiom extends to
  MPR group state in principle, *if* Construction 2's reachability proof holds (making a flag-free
  static partition sufficient, `mpr-overwrite-encoding-research.md` §Construction 2) — this needs no
  new tape encoding since it's a lexical partition, not a state carried on the tape.
- **No, or not yet, for flag-diacritic-encoded long-distance gating inside a replace rule.**
  §3 already covers this: three independent toolkit defects in the vendored foma-rs when a flag
  lives inside a `->` replace rule's own context (`gate.rs`'s findings, independently reproduced).
  This is a **toolkit-specific** obstruction, not a theoretical one (GiellaLT's own sme grammar uses
  flags successfully outside a replace-rule context) — so a pruner that needed flag-encoded gating
  *inside* rewrite rules specifically would need either a different vendored foma version, a
  different toolkit, or the already-shipped flag-free static-partition workaround.
- **Genuinely unclear (unknown, not established either way): a pruner-as-FST for "long-range
  interweaving phonology"** (the brief's own phrase) beyond bounded feeding/bleeding cascades —
  i.e. whether a *single* composed FST cascade can serve as a complete pruner (replacing confirm
  entirely) for a grammar with many interacting rules, rather than just serving as a *better
  proposer* (today's actual, shipped role). Nothing in this repo's evidence directly answers "can
  the composed cascade itself be trusted with zero verify," because the architecture never asks
  that question — verify is permanent by design (§6.1). This ledger states this as **unknown by
  design**: the project has deliberately not tried to answer it, because the architecture forecloses
  the question rather than leaving it open.

**Verdict on 6.2:** decomposing into multiple specialized mechanisms (a shared morphotactic trie/
network, per-rule-cascade phonology composition, a lexical partition for MPR gating, a runtime peel
for copying) is not just "easier" than one monolithic FST — it is the *only* combination this
project's own measured history found to work at all, and the theoretical reason is exactly the
brief's own hypothesis: different construct classes have different natural bounds (concatenative
morphotactics bounds additively; phonology bounds via Kaplan-Kay composition; copying does not bound
at all), so one mechanism per bound-shape is the right decomposition, not an accident of this
project's history.

### 6.3 Minimal set of capability gaps, ordered by difficulty

Ordered easiest-to-close → hardest, using this repo's own measured evidence as the difficulty proxy
(not a guess):

1. **Feature-quotient the enumeration/probe alphabet** (§3). Already informally validated by P6's
   312-tuple Amharic result vs. the naive 417^20/417² blow-ups the C# hybrid measured. Smallest,
   most mechanical fix; mostly needs applying the same idea everywhere probing/enumeration still
   happens (the enumeration-bridge composite builders, `morphotactic-composite-pruning.md`).
2. **MPR `Overwrite` reachability proof (Construction 2)** (§3, `mpr-overwrite-encoding-research.md`
   §6). Scoped as a small, precedented characterizer-side change (same shape as the already-shipped
   `compounding_max_depth` reachability pass); **zero new FST machinery**; the paper already shows it
   admits all 3 reference grammars' groups today. Low difficulty, well-specified, not yet built.
3. **Fixpoint-iteration for self-feeding `Iterative` rules** (§5 item 3). Conceptually simple (cap at
   the engine's own reapplication limit, iterate the rule-inverse to a fixpoint) but explicitly
   *not yet attempted* after an earlier static-detection approach was tried and reverted for false
   positives — medium difficulty because the "honest criterion" problem (distinguishing real
   self-feeding from ordinary rules) was found to be genuinely hard to state statically, even though
   the *dynamic* fix (just iterate, don't classify) sidesteps that entirely and was already proposed
   (`HYBRID_FST_FEASIBILITY.md` §10.4).
4. **Interdigitation/infixation as compiled rules over root *patterns*, not per-root enumeration**
   (P6's own named successor to the pre-expansion bridge, `foma-fst-plan.md` P1d/P6). Medium-high
   difficulty: this is real new compiler surface (compiling rule application into replace-calculus
   form for a construct class — templatic/interdigitating infixation — that doesn't fit the ordinary
   linear-affix replace-rule mold), and is exactly what's blocking Aweti (855 roots, templatic,
   currently the one grammar shape enumeration cannot serve at all,
   `morphotactic-composite-pruning.md`'s "Aweti end-to-end result").
5. **A genuinely bounded/bytes-safe reduplication idiom beyond the runtime peel** (§4) — e.g. an
   in-network encoding for *bounded* CV-template reduplication (which classical `compile-replace`
   handles) rather than routing every reduplicated word through the runtime peel unconditionally.
   Higher difficulty because it requires either porting/reimplementing `compile-replace`-equivalent
   machinery (confirmed absent from `foma-rs`, §4) or building an equivalent bounded-copy
   construction directly in `pg-foma`; the peel already achieves correctness (verify-gated), so this
   is a *performance/architecture-purity* improvement, not a correctness gap — lowest priority of
   the five despite being hardest, because the existing peel is not broken.

---

## 7. Decision table

| | **What it buys** | **What it costs** | **What it forecloses** | **Compatible with Divvun infrastructure?** |
|---|---|---|---|---|
| **FST-only (no verify)** | Maximum speed (no confirm-time re-analysis); simplest runtime shape | Loses soundness-by-construction for every construct not at `Exact` tier; no grammar in evidence reaches all-Exact (§6.1); silently wrong on reduplication, non-reachability-proven `Overwrite` groups, self-feeding iteratives — a correctness regression against propose+confirm's own by-construction soundness guarantee | Explicitly foreclosed by this project's own settled architecture (`foma-fst-plan.md:19-21` — not a live option under any wording of "should we") | **Not compatible as this project defines "Divvun infrastructure" for its own use** — but this *is* structurally how GiellaLT's own shipped analyzers actually run (foma/HFST networks applied directly, no separate confirm engine) — the difference is that GiellaLT grammars are hand-authored directly *for* the FST, with archiphonemes/flag-diacritics/Sets chosen by a linguist who already knows the finite-state target, whereas PanGloss's grammars are *compiled from* HC's unification-based authoring surface, which is the source of the exactness gap. **INFERRED**, not independently verified against a GiellaLT grammar's actual soundness properties in this session. |
| **FST proposer + FST pruner(s) (multi-FST, no HC engine)** | Removes the HC engine dependency entirely; keeps propose+prune shape (soundness-by-construction preserved *if* the pruner cascade is provably exact); composed-cascade pruning already demonstrated exact recovery of opaque feeding/bleeding (Lever 2, §5 item 1) | Requires the pruner cascade to be provably exact for every construct in the grammar — today's evidence (§3, §5) shows this holds for ordinary rewrite cascades but is open/unbuilt for self-feeding iteratives, unproven at scale for `Overwrite` MPR beyond the reachability-safe case, and does not exist at all for reduplication (which the peel, a non-FST mechanism, must still handle) — so this is "FST proposer + FST pruner for phonology, non-FST peel for copying," not a pure multi-FST system | Reduplication support (the peel is not an FST); any grammar needing self-feeding-iterative or non-reachability-safe `Overwrite` support until items 2-3 of §6.3 land | **Partially — this is close to what P6 is already building** (replace-rule-compiled cascades as the "pruner" in spirit, still verified by HC today). The gap to *removing* HC is exactly the open items in §6.3; nothing here is Divvun-infrastructure-incompatible in principle (foma/HFST both run pure-FST pipelines in production), but PanGloss's specific pruner would need the unbuilt fixpoint/reachability/pattern-compilation work first. |
| **FST proposer + HC pruner (status quo, propose→confirm)** | Soundness by construction, unconditionally, for every construct HC itself supports, including the ones no FST compiler handles today (reduplication, self-feeding iteratives, unproven MPR cases) — confirm is the *real* engine, so it is never wrong; measured 8×–48× speedup over full HC search per grammar (`foma-fst-plan.md` P3 timing table) already captured without giving up soundness | Keeps a dependency on the C#-ported HC engine (`pg-parse`) indefinitely; confirm cost is real and grows with proposer looseness (Amharic's `candidates_generated` mean 1.66, Sena's mean 30.1 per word, `foma-fst-plan.md` P3 candidate-count table) — a badly-tuned proposer taxes confirm, not correctness | Nothing — this is the maximally general option, by design (it is the fallback for every construct on this whole ledger) | **Yes, unconditionally** — this is exactly what PanGloss ships today, and it is architecture-agnostic with respect to Divvun's own infrastructure choices, since HC-the-pruner is PanGloss's own component, not something Divvun needs to provide or support. |

**Overall recommendation implied by the evidence, stated plainly:** the project's own settled
answer (propose+confirm permanently, chase FST coverage of *more* constructs without ever removing
confirm) is the only one of the three rows with no open correctness risk today. The honest
"how much closer can we get to not needing HC" answer is: close items 1–3 of §6.3 (cheap-to-medium
difficulty, well-specified) to shrink the *practical* gap on real grammars, and treat items 4–5 as
the long-tail work that determines whether "FST proposer + FST pruner, HC retired" ever becomes
achievable for the hardest attested grammar shapes (Aweti-style templatic interdigitation,
Amharic-style large-alphabet phonology) — neither of which is close today, per this session's own
reading of `morphotactic-composite-pruning.md`'s explicit "not yet a proven fix" status.

---

## 8. Open questions this session could not close (stated as unknown, not guessed)

- Whether any real GiellaLT/Divvun language repository has productive reduplication in its shipped
  grammar, and if so, how it is encoded (compile-replace-equivalent, a hand-authored bounded lexc
  construction, or something else). Would require cloning a specific language repo
  (e.g. `giellalt/lang-sme` or a language independently known to have reduplication) and reading its
  `.twol`/`.lexc`/`.xfst` sources directly — not done this session.
- Whether HFST-twolc's `Sets`/rule-variable expansion has the same alphabet-size blow-up this
  project measured for its own probing compilers, or whether HFST's implementation already
  feature-quotients by construction. Not independently verified against HFST source in this
  session — reported from HFST's own public documentation description only.
- Whether a composed replace-rule cascade could ever be trusted as a *complete* pruner (zero HC
  verify) for any real grammar — this session found the question is architecturally foreclosed by
  PanGloss's own design decision, not answered either way by evidence.
