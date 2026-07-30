# Divvun / GiellaLT investigation — synthesis and decision

Date: 2026-07-30. Branch: `divvun-research`. Status: research complete, no code changed.

Six parallel research agents investigated Divvun/GiellaLT to answer whether PanGloss can (a) run its
grammars on Divvun's architecture, and (b) replace the HermitCrab prune engine with FST-based
pruning. Their reports are `01`–`06` in this directory. This document synthesizes them, records
which claims were independently re-verified, corrects three that were wrong or over-scoped, and
names the experiments that would decide the question.

**Verification policy for this document.** Every claim below marked **[V]** was re-checked directly
against source by the synthesizing session, not taken from an agent's summary. Claims marked **[A]**
rest on an agent's report only. Claims marked **[?]** could not be verified and are stated as open.

---

## 1. The verified architecture: five mechanisms, one per bound-shape

North Sámi (`lang-sme`) is GiellaLT's deepest grammar. Its division of labor:

| Layer | Mechanism | Measured | Job |
|---|---|---|---|
| Phonology | `twolc` two-level rules, parallel-**intersected** | 112 named rules; 65 `Where…matched`; **0 flag diacritics** **[V]** | consonant gradation, umlaut |
| Trigger transport | deletable diacritics `X1-X9/Q1-Q9/W1-W9` mapped `:0`, plus archiphoneme `º` at the gradation site | ~30 symbols; trigger sets `WeG`, `Dummy`, `NonMPDummy` (`phonology.twolc:72,110-116`) **[V]** | carry grade trigger from *affix* to *stem* |
| Morphotactic legality | flag diacritics | 1,118 total in `src/`: 672 `@U.`, 214 `@P.`, 94 `@C.`, 79 `@R.`, 59 `@D.` **[V]** | derivation order, compounding legality |
| Post-lexical pruning | ordered `.o.` cascade over tag strings | ~20 `.regex` filters in `src/fst/filters/` **[V]** | remove illegal tag strings |
| Disambiguation | `vislcg3`, strictly downstream | `disambiguator.cg3:1` pipe: `preprocess \| hfst-optimised-lookup …hfstol \| lookup2cg` **[V]** | sentence-context reading selection |

**The magic is the division of labor, not a single trick.** Each mechanism is matched to a different
bound-shape: concatenative morphotactics bounds additively (lexc), phonology bounds via
intersection/composition (twolc), ordering constraints bound monotonically (flag diacritics),
tag-string legality bounds regularly (filter cascade), and genuine ambiguity is deferred to context
(CG). Report `05` reached the identical "one mechanism per bound-shape" conclusion from finite-state
theory alone, without reading any Sámi source. That independent convergence is the strongest single
result of this investigation.

### The pruning distinction that the whole question turns on

- **(A) grammar-internal well-formedness** — reject a candidate because the morphology/phonology
  forbids it. This is what PanGloss's HC `confirm` step does.
- **(B) contextual disambiguation** — the word is legitimately ambiguous; pick readings from
  sentence context. This is what Constraint Grammar does.

**GiellaLT does (A) entirely inside the FST, with no non-finite-state engine in the loop.** **[V]**
The existence proof is `src/fst/filters/` — `block-illegal_compound-strings.regex`,
`remove-illegal-derivation-strings-flagbased.regex`, `convert_to_flags-CmpNP-tags.regex`. The
flag-based one is the exemplar: an inverse-replace rule that *inserts* `@D.`/`@P.` flag pairs around
each derivation tag, so "derivations must ascend Der1→Der5, no double passives" becomes flag
unification at apply time rather than an enumeration of legal derivation sequences.

**CG is (B) only and is not a substitute for `confirm`.** **[V]** The FST has already decided
well-formedness before CG sees anything. CG is also a poor candidate on its own terms: Yli-Jyrä's
*Power of Constraint Grammars Revisited* shows general nonmonotonic CG is Turing-complete (Prop. 3),
finite-state-equivalent only under an `o(n log n)` runtime bound that production grammars are not
shown to satisfy **[A]**.

---

## 2. Three premises corrected

### 2.1 Divvun does not run standalone foma for real languages

`lang-sme/configure.ac:167-171` **[V]**:

```
AC_SUBST([DEFAULT_FOMA], [no])
AC_SUBST([DEFAULT_HFST], [yes])
AC_SUBST([DEFAULT_HFST_BACKEND], [foma])
```

Standalone foma is **off**. HFST is on, with foma's C library as HFST's *internal storage backend* —
`HfstDataTypes.h:51` defines `FOMA_TYPE, /**< A foma transducer, unweighted. */` **[V]**. Those are
different things and the shared word "foma" makes the conflation easy.

Further, foma cannot compile their phonology. `giella-core/am-shared/` has `.foma` build rules for
`%.lexc`, `.regex`, and `.xfscript`, but **no `.twolc → .foma` rule** **[V]**. And
`twolc-include.am:42-57` contains a *commented-out* foma path for phonology with the reason stated
verbatim **[V]**:

```
######## Foma build rules (based on Hfst): #######
### Commented out for now, they interfere with proper Foma builds, and do not
### work for the Hfst + Foma combo.
```

The disabled code split twolc rules into `RULE_PARTS`, intersected them pairwise, dumped to AT&T
text and read that into foma. They attempted the two-level→foma bridge and abandoned it.

**Why this does not block us:** we emit replace-calculus rules, which is the `.xfscript` route, and
that route *is* foma-compilable in their own build. The twolc gap constrains our ability to *consume*
GiellaLT grammars, not to *produce* ours.

**Not verified:** report `01` quoted `gt_PROG_FOMA` as activating only "if Xerox tools and Hfst are
not found". The macro is invoked at `lang-sme/configure.ac:181` but its body is absent from the
shallow `giella-core` clone. **[?]** — treat as unconfirmed; `DEFAULT_FOMA=no` is the solid form of
the same point.

### 2.2 `foma-rs` is a backend component of a young, one-person effort

Every upstream commit is dated 2026-07-16/17, all by one author (Brendan Molloy), with nothing in the
13 days since **[V]**. Report `01` establishes it is a sibling crate plugged in *underneath*
`hfst-rs` as its unweighted backend, that `divvun-runtime` depends on `hfst-rs`/`cg3-rs` rather than
on `foma` directly, and that none of the Rust stack appears in the production `lang-sme` build **[A]**.

"Divvun is moving to foma-rust" is better stated as: *one engineer began porting the entire native
toolchain to Rust two and a half weeks ago, and we adopted one crate before the effort had a track
record.* Not a reason to reverse course — we already ship on it — but a dependency risk to hold
consciously.

### 2.3 Flag diacritics do not carry the long-range phonology

This was the pivotal open question after the first three reports, and the answer is **no**.
`phonology.twolc` contains **zero** flag diacritics **[V]**. The 1,118 flags do morphotactic and
derivational bookkeeping only. Gradation is done by the trigger-diacritic mechanism in the table
above — deletable symbols on the affix, mapped to zero on the surface, read by two-level rules on
the stem.

**This is the most promising finding for PanGloss.** That mechanism is functionally what HC's MPR
features are: a morphophonological trigger transported across intervening material as a symbol on
the tape. Report `05` independently concluded that a pruner-FST needs exactly this — gating
information encoded on the tape as symbols rather than held in engine state.

---

## 3. Answers to the five original questions

**Q1. Use the recipe system to make any language work on Divvun's architecture.**
Not blocked where expected. Binary formats are tractable: foma's native format is a gzip-wrapped
text dump (`##foma-net 1.0##` / `##props##` / `##sigma##` / `##states##` / `##end##`) **[V]**, and
HFST links real libfoma. The blockers are elsewhere — see §4, items 1, 2, 8.

**Q2. Do we produce a standard foma grammar that can run anywhere, including Divvun?**
Half right. We produce compilable lexc plus replace rules, and their build compiles both to `.foma`.
But standalone foma is off in their production build, the twolc bridge is disabled, and — decisively
— **our emitted network is not a standalone analyzer**. `pg-foma/src/tags.rs:2` **[V]**: the upper
tape carries `<R:nnnn>` (root morpheme) and `<M:nnnn>` (non-root morpheme) ID tags, *not* linguistic
tags. It is a proposer whose output is meaningless without our decoder and `confirm` step.

**Q3. Proposer FST + pruner X is a 30-year-old architecture; the pruner would need to be an FST too.**
Correct, and Divvun proves FST-native (A)-pruning ships in production. But CG is not that pruner —
it is (B), downstream, and not finite-state in general.

**Q4. How did they get it working — what magic?**
§1. Five mechanisms, one per bound-shape, *hand-authored for the finite-state target by linguists who
already know it*. Plus sustained hand-maintenance: Greenlandic's derivations file shows a five-year
running battle against a single recursive suffix (`TIP`) fought with flags *and* forked lexica *and*
per-entry tuning **[V]**, annotated in Danish and still being edited as of 2024-08-30.

**Q5. Can we replace HC completely? For some? Hard blocker? Easier with two-stage?**
- **Completely, today: no.** Unbounded-copy reduplication is provably outside FST (pumping lemma;
  corroborated by Hulden & Bischoff's 2-way-FST result) and needs the non-FST peel **[A]**. No
  PanGloss grammar has been measured all-Exact — though report `05` correctly notes nobody ever
  tried, because the architecture forecloses the question.
- **For some: plausibly yes**, for grammars with no productive reduplication, no self-feeding
  iterative rules, and no `Overwrite` MPR groups with reachable conflicting touches. Indonesian is
  closest (`Exact=2, Permissive=3, IdentitySkip=0`) but not clean **[A]**.
- **Two-stage is genuinely easier**, and this is the one place the project's own history and the
  Sámi evidence agree without qualification: one mechanism per bound-shape, not one monolithic FST.
  Report `05` also notes a real counterweight — a union-vs-compose bug in P6 bloated a network from
  38 states to 392,311 **[A]** — so "N reject machines" is not free.

---

## 4. Hard blockers, classified

| # | Blocker | Class | Notes |
|---|---|---|---|
| 1 | **Two-level vs ordered-cascade seam.** Their gradation is parallel-intersected `twolc`; our phonology is an ordered replace cascade. Not generally equivalent — simultaneous constraints vs sequential rewriting. | technical-hard (design) | Reports `02` and `05` independently flag this as where a naive HC→FST lowering silently overgenerates. **This is the blocker that decides whether HC can go.** |
| 2 | **Upper-tape convention.** We emit `<R:nnnn>`/`<M:nnnn>` morpheme IDs; GiellaLT and CG expect `word+N+Sg+Nom`. | technical-cost | Structural mapping is mechanical; choosing the tagset is a per-language human decision. |
| 3 | **Unbounded derivational recursion** in polysynthetic languages. | technical-hard | Not cleanly solved by anyone. Greenlandic contains it with belt-and-braces hand-maintenance over years. Do not promise a compiler will do this automatically. |
| 4 | **Unbounded-copy reduplication.** | provable impossibility | Permanent carve-out. Requires the non-FST peel or 2-way FSTs. `foma-rs` has **no** `compile-replace` (zero hits repo-wide) **[V]**, so even bounded CV-template reduplication has no in-network idiom available. |
| 5 | **`foma-rs` flag/replace-rule defects.** `pg-foma/src/gate.rs:8-53` concludes "`->` and flags do not mix safely in this port, full stop" **[V]**. | technical-cost, **possibly mis-scoped by us** | See §5, Experiment A. Divvun uses 1,118 flags in production, including flags inserted *by* `<-` rules. Both cannot be wholly true. |
| 6 | **`foma-rs` dependency risk.** One author, 13 days quiet, not in Divvun's own production build. | social / project-risk | We have an open upstream issue already (`pg-foma/Cargo.toml:20`). |
| 7 | **No artifact-level drop-in.** GiellaLT's autotools build compiles lexc/twolc from source it controls; there is no "hand us a compiled FST" path **[A]**. | technical-cost | The realistic seam is **text-level lexc/xfscript**, not binary conversion. |
| 8 | **Governance and licensing.** UiT copyright, org-admin-gated repo creation, `gut` CLI, Buildkite CI auto-wiring; FLEx/SIL data-provenance policy undocumented **[A]**. | social-or-licensing | Needs a human decision, not engineering. |
| 9 | `divvunspell` has **zero** foma awareness and reimplements the HFST binary format itself **[V]**. | technical-cost | Relevant only if we target spellers rather than analyzers. |

---

## 5. The experiments that would decide it

### Experiment A — scope the flag/replace-rule defect (cheap, decisive)

Our `gate.rs` finding and Divvun's 1,118 production flags are in direct conflict. The likely
resolution is idiom position: Divvun puts flags in a replace rule's **replacement**
(`"@D.Der1.TRUE@" … "+Der1" <- "+Der1"`), whereas our bug was a flag literal in the rule's **`||`
context**.

Take the `remove-illegal-derivation-strings-flagbased.regex` idiom verbatim, compile it under
`foma-rs`, and apply it. **Success criterion:** the derivation-ordering constraint compiles and
correctly rejects `Der2`-before-`Der1` while accepting the ascending order.

- If it passes: `gate.rs`'s "full stop" is over-scoped, the flag path reopens for morphotactic
  legality pruning, and blocker 5 downgrades to a documentation fix.
- If it fails: `foma-rs` has a defect on the exact idiom Divvun's own grammars depend on. That is a
  significant upstream finding and explains why the port is not yet in their production build.

Either outcome is worth having, and it is a day's work.

### Experiment B — trigger-diacritic transport for MPR gating

Compile one HC MPR-gated rule using the GiellaLT trigger-diacritic technique — a deletable symbol
carried on the affix, mapped to zero on the surface, read by the rule on the stem — instead of our
static flag-free partition. **Success criterion:** the gate fires correctly with the trigger symbol
transported across intervening material, and the compiled net does not exceed the static-partition
baseline in states.

This is the mechanism report `05` says a pruner-FST needs, now with a working production precedent
to copy. It is the first real step toward an FST pruner.

### Experiment C — the two-level vs cascade seam (the one that matters)

Take the smallest PanGloss grammar, hand-write the intersected two-level equivalent of its
phonology, and check whether the ordered cascade and the two-level system agree on the **full** word
set — not a sample. **Success criterion:** exact set parity, or a characterized, enumerable
disagreement class.

This is blocker 1, and it decides whether HC can be retired at all. It is scoping work before it is
coding work.

---

## 6. Cross-agent disagreements, adjudicated

- **Report `03`** claimed `eliminate flag` is used selectively at compile time in some build targets.
  **Zero hits** in `lang-sme/src/` or `giella-core/am-shared/` **[V]**. Flags appear to stay live for
  apply-time interpretation by `hfst-optimised-lookup`. This matters: whatever consumes the FST must
  interpret flags, and `foma-rs` does have `crates/foma/src/flags.rs` **[V]**.
- **Report `04`** claimed Greenlandic's flag-diacritic approach to recursion control was "explicitly
  abandoned" in favor of lexicon forking. Too strong. Both are live: `derivations-inflections.lexc`
  shows the forked lexicon at `:39110` (`…men uden TIP for at blokere rekursive TIP`) *and* active
  flag maintenance at `:10982`, `:27590`, `:115737`, the last edited 2024-08-30 **[V]**. The honest
  reading is harsher and more useful: nobody has cleanly solved this.
- **Report `05`**'s §6.1 headline ("No, not today, for any grammar") leans partly on
  `foma-fst-plan.md:19-21` — our own *settled decision* that FST-only is off the table. That is
  circular when the decision is what's under review. The report is candid about this elsewhere
  (§6.2 "unknown by design", §8), but the confident framing overstates the evidence. Defensible
  version: *no grammar has been measured all-Exact because nobody tried.*
- **Report `06`** spawned its own four subagents and synthesized their findings rather than reading
  everything first-hand. Its central claim was re-verified directly **[V]** and holds; treat its
  peripheral details with proportionate caution.

---

## 6a. Follow-on findings (reports 07–09) — three blockers resolved

Reports `07`, `08`, `09` were commissioned after the first six to settle the open items. All three
landed, and they resolve blockers 1 and 5 and reframe the soundness question.

### Blocker 5 resolved: the flag idiom, proven from source (report `07`)

`gate.rs`'s conclusion — "`->` and flags do not mix safely in this port, full stop" — is **correct in
observation, over-scoped in conclusion**. The mechanism, verified: the replace calculus is entirely
**flag-blind** (`grep -ci flag` returns **0** in both `foma-rs/crates/foma/src/rewrite.rs` and
upstream `foma/foma/rewrite.c`) **[V]**, while `apply` treats any flag-shaped symbol as **zero-width
unconditionally** (`foma/apply.c:1084`: *"For flags, we consume 0 symbols of the input string,
naturally"*) **[V]**.

The collision occurs only when a flag occupies a **matched** role — inside a `||` context, compiled
through a `NotContain` construction — where the builder treats it as a real symbol and apply treats
it as consuming nothing. A flag merely **inserted** by a rule and read at plain apply time never
enters that construction. **That is exactly Divvun's idiom** (`<-` rules with no `||` clause), which
reconciles their 1,118 production flags with our finding at no contradiction.

All three findings are **inherited from upstream C, not port regressions** — independently confirmed
for finding 2 via `foma/mem.c:25` (`int g_flag_is_epsilon = 0;`) **[V]**. Nothing to file upstream
except possibly the crash, observed against foma `0.1.1` while we now pin `0.4.2`.

**Consequence:** flag diacritics are available to PanGloss for morphotactic legality gating, provided
we insert-and-read rather than match-in-context.

### Blocker 1 resolved: decline the two-level seam (report `09`)

The `00-` synthesis originally called the two-level-vs-cascade seam the blocker that decides whether
HC can be retired. **That was over-weighted.** PanGloss never needs to emit or consume `twolc`, so
the hard translation direction (cascade → two-level, which requires a linguist to reformulate every
rule and abandon abstract intermediate segments) is a job we can decline.

The decisive evidence is GiellaLT's own portfolio:

- **`lang-kal` (Kalaallisut/West Greenlandic) writes all its phonology as a replace-rule cascade.**
  7 `.xfscript` files, `phonology.xfscript` = 512 lines, and **zero `.twolc` files anywhere in the
  repo** **[V]**. It ships a released grammar bundle and a packaged speller **[A]**. The hardest
  polysynthetic language in the portfolio uses our formalism.
- **The build asymmetry is exact.** `giella-core/am-shared/xfscript-include.am:28-29` has a **live,
  uncommented** `.xfscript → .foma` rule invoking `$(FOMA) -l $< -e "save stack $@" -s` **[V]**,
  against `twolc-include.am:42-57` commented out as interfering with foma builds **[V]**. Our
  formalism is on the supported side of that line; theirs is not.
- **~30 `lang-*` repos contain `phonology.xfscript`** **[A]** (org-wide code search, not verified
  repo-by-repo here).
- **No live phonology requiring two-level was found.** The one mutual-conditioning candidate
  (diphthong simplification needing gradation-grade context, `phonology.twolc:1204-1291`) was
  attempted as a true two-level construct three times and abandoned each time by GiellaLT's own
  developers in favor of the trigger-diacritic device **[A]**, which transfers to the cascade
  unchanged (`lang-kal`'s `%^GEMS`/`%^GEMEQ`/`%^Loan` family does attach → transport →
  consume-in-context → delete-late) **[A]**.

### The reusable construction: alpha-variables, from their own source

`lang-crk/src/fst/morphology/phonology.xfscript:477-483` preserves a `twolc` alpha-variable rule in
comments directly above its live hand-translation **[V]**:

```
! twolc original:
! d1:Cx <=> _ (0:i 0:y) [ a: [ y2: | ý2 ] | â: h ] (%^IC:0) ( %-: ) %<:0 Cx: ;
!    where Cx in ( c k m n p s t w y ) ;

! live replace-calculus translation:
define ReduplRule [ [ d1 | d2 ] -> c || _ [ \%< ]+ %< c ,,
                    … nine disjuncts, one per consonant …
                  ] .o. [ [ y2 | ý2 | y3 ] -> y || [ d1 | d2 ] ?* _ ]
                    .o. [ [ d1 | d2 ] -> 0 ] .o. [ [ y2 | y3 ] -> 0 ] ;
```

The recipe: **enumerate the variable's domain into concrete disjuncts, join with `,,` (parallel
replace, so all fire simultaneously and no inter-disjunct ordering question arises), then `.o.`
compose the cleanup and placeholder-deletion rules.** This is report `05`'s "FST-bounded, enumerate
the binding domain" tier with a working production instance and the ordering hazard already removed.

**It also closes report `05`'s open reduplication question.** That Cree rule *is* reduplication —
placeholder symbols `d1`/`d2` matched against the stem-initial consonant, then deleted. Bounded copy
by placeholder-and-match, requiring no `compile-replace` (which `foma-rs` lacks).

### The soundness question reframed (report `08`)

GiellaLT's analyser is **not tested for soundness, by construction**:

- `giella-core/scripts/run-morph-tester.sh.in:146` passes `--ignore-extra-analyses`
  **unconditionally, for every language in the org** **[V]**. The harness cannot detect
  over-generation in the analysis direction.
- The tool supports negative assertions (`~`-prefixed "detested" forms) and they appear in
  **0 of 1,572** YAML test files across the cloned languages **[V]**.
- `lang-sme/docs/docu-sme-testplan.md:138-153` states it outright **[V]**: *"When we test whether
  words are let through or not, we do not test whether the parser actually gives correct
  analyses."* It then enumerates *"correctly spelled, but given a grammatical analysis that it should
  not have had"* as a known category discovered downstream in the disambiguator, not by tests.
- The **generation** direction *is* exact-set tested; `--ignore-extra-analyses` relaxes analysis only
  **[A]**. Their strict bar is on the speller-facing direction.

**Verdict on the project owner's trichotomy:** it splits by subject.

- **Soundness → (i), Divvun has not solved it.** They minimize damage and document doing so. Nothing
  to reuse.
- **Formalism and toolchain → (iii), solved and reusable.** The cascade route is supported, compiles
  to foma, and carries a shipping polysynthetic flagship.

Divvun runs a two-stage architecture too — over-generating FST plus a downstream filter (CG or tag
cascade). The difference is that **no stage of theirs is a general-purpose well-formedness verifier**,
and they make no claim that one is. PanGloss's `confirm` is a strictly stronger contract than
anything in their pipeline. The question is therefore not "is FST-only possible" but "**what soundness
bar do we need**", which is a product decision, not a finite-state one.

### The path this produces: X, Y, Z

- **X** — stay in the replace cascade end to end; never emit or consume `twolc`.
- **Y** — adopt both tape-encoding idioms: trigger diacritics for phonological gating, insert-then-read
  flags (never match-in-context) for morphotactic legality, and enumerate-plus-`,,` for
  alpha-variables.
- **Z** — move HC from **runtime verifier** to **build-time oracle**: enumerate the grammar's word
  space, use HC as ground truth to construct and certify the filter set, then ship FST + filters with
  no HC at runtime. This is the step Divvun structurally cannot take (they have no oracle), and it is
  what would let PanGloss ship FST-only *with* a soundness claim.

Whether Z is tractable reduces to whether our over-generation factors into independent filter
dimensions — the subject of report `10`, outstanding at the time of writing. The theory: a filter
exists iff the target relation is regular (so unbounded-copy reduplication is permanently excluded),
factored representations can be exponentially more compact than the monolithic minimum (`n·k` vs up
to `kⁿ`) **but only when constraints are near-independent**, and a conservation law applies — a filter
can only reject on information present on the tape, so encoding derivation facts for the filter
enlarges the proposer. Both stages cannot be simplified at once.

## 7. Open questions

- `gt_PROG_FOMA`'s macro body — whether foma really is the fallback of last resort **[?]**.
- Whether any GiellaLT language has productive reduplication, and how it is encoded **[?]** (report
  `05` flagged this; report `02` did not close it).
- Whether `hfst-twolc`'s `Sets`/rule-variable expansion suffers the same alphabet blow-up PanGloss
  measured on Amharic's 417-segment inventory **[?]**.
- No FST state/arc counts, compile times, or memory figures are published anywhere reports `02`/`04`
  could find — only lexicon sizes and release-artifact bytes **[A]**. Our own measured numbers may be
  the better baseline.

## 8. Scale, for the record

Finnish `lang-fin` is **1,156,174 lines of lexc** **[V]** and ships. That is roughly 10× the
FLEx-scale target of 10⁴–10⁵ entries, so tractability at scale is settled. Report `04` locates the
bottleneck at the compose-against-rules step and alphabet size rather than lexicon size, which
matches what PanGloss already measured on Amharic. Their build strategy — whole-lexicon single
`hfst-lexc` compile, then `determinize → minimize → hfst-compose-intersect → minimize` against the
rules transducer rather than free composition **[A]** — suggests our `Compose` primitive wants a
restricted/intersecting mode.

Greenlandic inverts the profile: its affix file is 127,966 lines against ~47K of stems combined
**[V]**. In polysynthesis the derivational machinery dwarfs the lexicon.
