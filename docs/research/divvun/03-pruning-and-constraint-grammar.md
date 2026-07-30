# Divvun/GiellaLT pruning research — the pruner, and can it be an FST?

Research agent 3 of 6. Scope per brief: establish what Divvun's pruning layer actually is,
whether it is finite-state, and whether an FST pruner could replace HermitCrab's confirm step.
No code changed. Sources cloned shallow to
`C:/Users/johnm/AppData/Local/Temp/claude/C--Users-johnm-Documents-repos-LCAtom/1b5e24e2-aeac-4668-b883-e199cfb811d9/scratchpad/divvun/a3/{lang-sme,vislcg3}`
(`giellalt/lang-sme` and `unhammer/vislcg3` — see §0 for why the brief's two suggested URLs 404).

**Bottom line up front:** Divvun/GiellaLT already answers half of this question for us, in
production, at scale — and PanGloss's *own* prior investigation (this repo's `docs/fst-plan/`
and a shelved `docs/superpowers/specs/` design) independently rediscovered the same mechanism,
hit the same toolkit limits, and left a paper trail more directly relevant than anything I found
externally. §3 and §4 lean on that internal work heavily, with full citations — it is not
padding, it is the load-bearing evidence.

---

## 0. Source note

The brief's two vislcg3 URLs do not resolve as given:
- `https://github.com/GrammarSoft/vislcg3` → **404** ("Repository not found"), confirmed by direct
  clone attempt.
- `https://github.com/apertium/vislcg3` → **404** as well.

The real upstream is hosted on SourceForge/a self-hosted SVN
(`README.md:9`, `http://visl.sdu.dk/svn/visl/tools/vislcg3/trunk/`); the GitHub mirror that
actually exists is **`unhammer/vislcg3`** ("branches of http://beta.visl.sdu.dk/cg3.html"),
which is what I cloned. Noting this because it's a small but real correction to the brief for
whichever other agent also needed vislcg3.

---

## 1. The (A)/(B) distinction, pinned down with evidence

This is the fulcrum of the whole assignment, so it comes first and is stated as plainly as
possible: **GiellaLT's pruning is split cleanly across a pipeline boundary, and the two halves
answer two different questions.**

```
word ──(A) FST analyser (lexc + twolc + flag-diacritic filters, composed)──> legal readings
     ──(B) vislcg3 (Constraint Grammar, over the sentence)──> one reading, chosen by context
```

- **(A) is entirely inside the FST.** GiellaLT's morphological analyser
  (`analyser-gt-desc.hfstol` / `analyser-gt-norm.hfstol`) is a single composed HFST network:
  lexc lexicon `.o.` twolc phonological rules `.o.` a cascade of `filters/*.regex` transducers
  that use flag diacritics to reject illegal derivation orders, illegal compounds, etc. (detailed
  in §3). This is **exactly** the question HermitCrab's confirm step answers for PanGloss —
  "does the morphology/phonology itself license this candidate" — and GiellaLT answers it with
  **zero non-finite-state engine in the loop.** This is the single most important finding for
  our architecture question and is documented at length in §3.
- **(B) is `vislcg3`, and it runs strictly *after* (A), on (A)'s output.** The disambiguator's
  own pipe directive says this explicitly:
  `lang-sme/src/cg3/disambiguator.cg3:1` —
  ```
  # -*- cg-pre-pipe: "$GTHOME/giella-core/scripts/preprocess ... | hfst-optimised-lookup
  $GTHOME/langs/sme/tools/preprocess/analyser-disamb-gt-desc.hfstol | ...lookup2cg" -*-
  ```
  i.e. the FST (`hfst-optimised-lookup`) runs first and produces the full set of grammatically
  *legal* readings for each word (a "cohort"); `vislcg3` then runs over the **sentence**, using
  SELECT/REMOVE rules conditioned on neighboring words' tags (agreement, valency, barriers,
  LINK chains — the machinery in §2) to throw away readings that are legal-but-wrong-in-context.
  GiellaLT's own docs describe vislcg3 as "Our disambiguator"
  (`https://giellalt.github.io/ling/docu-disambiguation.html`, VERIFIED via fetch) — a compiler
  "originally developed by a.o. Fred Karlsson and Pasi Tapanainen," used to produce
  `disambiguation.cg3`/`functions.cg3`/`dependency.cg3` per language.

**Why this must not blur, stated loudly per the brief's instruction:** vislcg3/CG has **no model
of the morphological derivation whatsoever.** It sees tag *strings* per cohort (`N Sg Nom`,
`V Ind Prs`, ...) and reasons over sequences of cohorts; it cannot ask "did rule prule5 actually
fire," "is this allomorph's environment satisfied," or "does this MPR feature gate hold" — those
questions are already-answered facts baked into the tag string by the FST stage before CG ever
sees the word. **CG is a downstream, sentence-context reading-selector (question B). It is
structurally incapable of doing HermitCrab confirm's job (question A), because by the time CG
runs, the FST has already thrown away every candidate that fails A — CG only ever chooses among
survivors.** Anyone reaching for "compile the CG rules to an FST and use that as the pruner"
is solving the wrong problem for PanGloss: PanGloss analyses isolated words (no sentence
context modeled at all today), so a CG-shaped disambiguator has no natural role here regardless
of its own finite-state status (which §2 shows is murkier anyway).

The practical implication: **the part of Divvun that *is* directly relevant to "replace
HermitCrab confirm with an FST pruner" is §3's flag-diacritic filter mechanism, not Constraint
Grammar at all.** CG is interesting only as a cautionary example of a *different* kind of
pruning that looks superficially similar but answers a different question — worth stating
explicitly since the brief's framing ("proposer FST then pruner X") could otherwise be
mis-read as "vislcg3 is Divvun's pruner X." It answers a real pruning question, just not ours.

---

## 2. Constraint Grammar / vislcg3: operations and computational power

### 2.1 What operations exist (VERIFIED from the vislcg3 manual and real `.cg3` source)

Full rule-type cheat sheet, `vislcg3/manual/rules.xml:21-76`:

```
Reading & Tag manipulations: ADD, MAP, SUBSTITUTE, UNMAP, REPLACE, APPEND, COPY,
                              SELECT, REMOVE, IFF
Dependency manipulation:     SETPARENT, SETCHILD
Relation manipulation:       ADDRELATION(S), SETRELATION(S), REMRELATION(S)
Cohort manipulation:         ADDCOHORT, REMCOHORT, SPLITCOHORT, MOVE, SWITCH
Window manipulation:         DELIMIT, EXTERNAL
Variable manipulation:       SETVARIABLE, REMVARIABLE
Flow control:                JUMP
```

- **SELECT/REMOVE/IFF** (`rules.xml:32-34`) are the disambiguation core: keep-only /
  delete-if-context / delete-unless-context, each gated by `[contextual_tests]`.
- **MAP** appends a tag and *locks* the reading from further MAP/ADD/REPLACE
  (`rules.xml:367-380`); **UNMAP** reopens it, default-restricted to single-reading cohorts
  unless marked `UNSAFE` (`rules.xml:384-399`).
- **Contextual tests** (`vislcg3/manual/contexts.xml`): position offsets (`-1`, `1`, `**1` =
  unbounded scan), **BARRIER**/**CBARRIER** (stop-scanning-at-this-set, Careful mode variant,
  `contexts.xml:42-55`), **span markers** `W`/`<`/`>` to cross sentence-window boundaries
  (`contexts.xml:57-104`), **NEGATE** (inverts a whole LINK-chain, vs. `NOT` which inverts only
  the next test, `contexts.xml:27-40`), **LINK** (chain a test from the position the previous
  test landed on), and **X**/**x** (`test-mark`, `contexts.xml:107-160`) to *set a mark* at one
  point in the chain and *jump back* to it later — i.e. a rule's context can walk right, mark,
  walk further, then jump back and walk left from the mark. **D**/**d** (`contexts.xml:162-192`)
  let a test see deleted/delayed readings.
- Real production examples, `lang-sme/src/cg3/disambiguator.cg3` (2,753 rule lines total,
  `SECTION` count = 8+, confirming a genuinely large, staged grammar):
  - `disambiguator.cg3:6469`: `IFF:loanasAdv ("loanas") IF (0 Adv LINK *-1 ("oažžut") OR
    ("váldit") OR ("jearrat") OR ("addit") BARRIER SV-BOUNDARY);` — unbounded look-back to a verb
    set, gated by a clause-boundary barrier.
  - `disambiguator.cg3:2622`: an `ADD` rule with three alternative LINK chains including
    `BARRIER SV-BOUNDARY` and `*1`/`1` mixed offsets — real long-distance dependency use.
  - `disambiguator.cg3:8619`: `IFF:sapmelas ("sápmelaš") + A IF (0 Attr LINK 1 N LINK NOT 0
    VFIN);` — chained LINK with an embedded NOT.
- **Rule application is cascaded and re-run to a fixpoint, not one pass**: `SECTION` markers
  (`grammar.xml:97-120`; 8+ SECTIONs in the real Sámi grammar) partition the rule file into
  ordered stages; the **ITERATE** rule-option (`rules.xml:801-814`) forces re-running the
  *sections* whenever a state-changing rule fires (`SELECT`/`REMOVE`/`IFF`/`DELIMIT`/`REMCOHORT`/
  `MOVE`/`SWITCH` do this by default); **REPEAT** (`rules.xml:863-876`) re-runs just the current
  rule over the window again (used to make `SUBSTITUTE` exhaustive). This is a "keep applying
  until nothing changes" loop over a nonmonotonic (insert/delete) state, which is exactly the
  shape §2.2's undecidability result is about.

### 2.2 Computational power (VERIFIED via Yli-Jyrä, "The Power of Constraint Grammars Revisited")

Fetched `arxiv.org/abs/1707.05115` (abstract) and `ar5iv.labs.arxiv.org/html/1707.05115` (full
text via ar5iv rendering). Key formal results, quoted/paraphrased with the paper's own numbering:

- The paper reduces CG rule types to three primitives (its **NM-SCG**, "nonmonotonic simple
  constraint grammar," §3.1): `REPLACE(old, new, cond+)`, `INSCOHORT(targ, cond+)`,
  `REMCOHORT(targ, cond+)` — SELECT/DELETE are shorthand for sets of REPLACE rules.
- **Proposition 3: "NM-SCGs are equivalent to TMs"** — nonmonotonic Constraint Grammar
  (insertion + deletion of cohorts under context tests, iterated to a fixpoint) is
  **Turing-complete and therefore undecidable** in the fully general case. This is a formal,
  citable statement that "just compile arbitrary CG to an FST" is **not universally possible** —
  not an engineering gap, a **provable impossibility** for the unrestricted formalism.
- **Proposition 15 (building on Proposition 13, citing Tadaki et al. 2010): "An NM-SCG is
  equivalent to a finite automaton/transducer if its one-tape TM implementation runs in
  o(n log n) time."** This is the resource-bound condition the brief asked about: CG *can* be
  finite-state-equivalent, but only under a runtime bound on the implementing machine, not from
  the rule syntax alone.
- The paper explicitly does **not** isolate one CG construct (barriers, LINK, iteration) as
  *the* source of extra power; it attributes it to the **combination** of unbounded-position
  context testing with nonmonotonic (insert/delete) operations under fixpoint iteration.
- On real-world grammars: Tapanainen's practical CG-2 implementation is empirically
  **O(n log n)** on average (the paper's own Figure 3, cited secondhand via the fetch), which is
  consistent with staying inside the finite-state-equivalent regime in practice — but the paper's
  own conclusion is explicit that **"whether the used grammar is actually equivalent to a
  finite-state transducer is not known."** No blanket "production CG grammars are provably
  regular" claim exists in this source.

**Assessment for our question:** CG-as-a-formalism is *not* provably regular in general; whether
a specific grammar (e.g. `lang-sme/src/cg3/disambiguator.cg3`) happens to fall in the
finite-state-equivalent fragment is an open, per-grammar empirical question this research did
not attempt to answer (it would require reproducing Yli-Jyrä's o(n log n) measurement against
the actual Sámi grammar — out of scope here, and moot per §1: even if it were provably regular,
it answers question (B), not (A)).

### 2.3 Other FST-compilation-of-CG literature found (search results, not independently verified against full text)

- Hulden, **"Constraint Grammar Parsing with Left and Right Sequential Finite Transducers"**
  (ACL Anthology W11-4406, FSMNLP 2011) — title and existence confirmed via search; the PDF
  fetch returned only font/binary stream data (tool limitation, not a content-access refusal),
  so I could not verify its specific claims about which CG subset compiles to sequential
  transducers. **Flagging this as unverified** rather than fabricating a summary — the paper is
  real and on-topic (a CG-to-FST compilation restricted to left/right sequential transducers,
  i.e. a strictly bounded-lookaround fragment) but I do not have first-hand claims to cite.
- Yli-Jyrä, **"An Efficient Constraint Grammar Parser based on Inward Deterministic Automata"**
  (2011) and Yli-Jyrä & Koskenniemi (2004, contextual restrictions compiled to FSA) — titles/
  existence confirmed via search only, not fetched.
- **"Compiling Rewrite Rules to Finite-State Transducers with the Worsening Trick"**
  (arxiv 2606.10059) surfaced in search but is about SPE-style rewrite-rule compilation
  generally (Kaplan-Kay lineage), not CG specifically — noted but not pursued further as
  off-target for this section (it is squarely on-target for §3/§4's rewrite-rule question,
  where PanGloss's own P6 work already independently solved the equivalent problem — see §3).

**Net:** the literature substantiates that CG-to-FST compilation is an active, published research
line, that it works for restricted fragments (bounded lookaround), and that the *general*
nonmonotonic formalism is provably not regular. Nothing found claims a full-power, all-operators
CG compiles losslessly to one FST — consistent with Proposition 3 above.

---

## 3. Where GiellaLT does (A)-style filtering *inside* the FST — the headline finding

This is exactly the mechanism the brief predicted might exist, and it does — in production, for
North Sámi, right now. It also happens to be independently rediscovered (and pushed further, then
partly abandoned for good reasons) inside **this very PanGloss repo**, which makes the citation
chain unusually direct.

### 3.1 The GiellaLT mechanism (VERIFIED from `lang-sme` source)

The analyser is one composed HFST network: **lexc lexicon `.o.` twolc phonological rules `.o.`
a cascade of regex-based "filter" transducers**, several of which convert descriptive tags into
**flag diacritics** to gate combinations the lexc continuation-class structure alone cannot
express — then, in some build targets, **eliminate** those flags again once they've done their
job at compile time. Concretely:

- `m4/hfst.m4:54,90` — the build system requires `hfst-compose-intersect` and `hfst-twolc` as
  toolchain dependencies; `src/fst/docs/docu-sme-flowchart.md:39-58` diagrams the overall
  pipeline: `twol-sme.txt → twol-sme.bin` (twolc-compiled rules) merged with the lex files via
  lexc into `sme.save`, then further composed with `case.regex`-compiled preprocessing into the
  final `sme.fst`.
- `src/fst/filters/` (VERIFIED directory listing) contains regex sources whose names describe
  exactly the (A)-class well-formedness questions HC confirm answers for us:
  `remove-illegal-derivation-strings-flagbased.regex`, `block-illegal_compound-strings.regex`,
  `convert_to_flags-CmpNP-tags.regex`, `insert-default_left_compounding-tags.regex`.
- **`remove-illegal-derivation-strings-flagbased.regex` (full file read)** blocks
  out-of-order derivation chains and double-passive derivations using `@D@`/`@P@`/`@C@` flags,
  e.g. line 21: `"@D.Der1.TRUE@" ... "@P.Der1.TRUE@" "+Der1" <- "+Der1"` — disallow if a `Der1`
  flag was already set positively elsewhere on the same path, else set it. Flags are cleared at
  word boundary (line 32: `"@C.Der1@" ... %# <- %#`), so a legal derivation order is a
  *finite-state acceptance condition over the whole word*, no lookahead/lookbehind of unbounded
  distance needed once flags carry the running state.
- **`block-illegal_compound-strings.regex` (full file read)** is agreement-across-compound-
  boundary logic: number/case features set with `@P.CmpN_Left.SgNom@`-style flags on the
  compound's left member are `@R@`-required (or `@U@`-unified) on the right member — this is
  literally cross-morpheme feature agreement, the same job HC's HeadFeatures/unification does,
  expressed as flag set/require pairs.
- **`docs/docu-sme-flag-diacritics.md` (full file read)** documents this deliberately: "Flag
  diacritics are used in the Saami morphological parser in order to remove illegal compounds, and
  in order to handle automatic downcasing of proper names" (lines 11-13), states the problem in
  HC-confirm-shaped terms ("Too strict: only N+N accepted... Too sloppy: also N+V accepted...
  Correct: accept compound only if 2nd part is N at end of derivation," lines 57-63), and gives
  the exact P/R lexicon sketch (lines 79-93) — U/P/N/R/D/C semantics defined at lines 18-40,
  matching foma's own flag types one-for-one (§4).
- **The full compose pipeline with compile-time flag elimination is spelled out in the build
  system itself**, `src/fst/Makefile.am:15-48` (`.generated/generator-fstspeller-gt-norm.hfst`
  target):
  ```
  read regex @"filters/rename-POS_before_Der-tags.hfst" .o. @"filters/block-illegal_compound-strings.hfst"
  .o. @"filters/split-CmpN-tags.hfst" .o. @"filters/insert-default_left_compounding-tags.hfst"
  .o. @"filters/insert-default-compounding-tags.hfst" .o. @"filters/remove-illegal-derivation-strings-flagbased.hfst"
  .o. @"filters/remove-Use_Minus_PLX-tags.hfst" .o. @"filters/convert_to_flags-CmpNP-tags.hfst"
  .o. @"filters/split-CmpNP-tags.hfst" .o. @"$<" ;
  twosided flag-diacritics
  eliminate flag Der1
  eliminate flag Der2
  ... (Der3, Der4, Der5, Der_PassL, Der_PassS)
  save stack $@
  ```
  (`Makefile.am` lines 26-48, verbatim `xfst`-script-in-Makefile idiom.) This is **exactly**
  `hfst-compose-intersect`'s conceptual job done via `.o.` composition of a lexicon net with a
  chain of small acceptor/transducer filters, each one independently authored, then flags
  eliminated **at compile time** to shrink the shipped network — a direct, real-world instance
  of "narrow an over-generating lexc lexicon with zero non-finite-state engine in the loop."
- **Crucially, the *analyser* build target (as opposed to the *generator* target above) does
  NOT eliminate these flags** — `src/fst/Makefile.am:346-366` runs the same style of `.o.`
  filter chain, then only `twosided flag-diacritics` (a structural repair, not elimination) and
  `save stack`, no `eliminate flag` calls. This means the shipped `analyser-gt-norm.hfst` still
  **carries live flag diacritics that `hfst-lookup`/`hfst-optimized-lookup` interpret at
  apply-time** (obeying them by default, same as foma — see §4). **This is the direct, real-world
  answer to the brief's item-4 question "apply-time interpretation vs. compile-time elimination":
  GiellaLT does both, selectively, per build target** — eliminate where testing shows it's safe
  and cheap (the speller's generator, where `Der1`-`Der5`/`Der_PassL`/`Der_PassS` are eliminated,
  with an explicit code comment at `Makefile.mod-fstbased.am:50-52` warning not to eliminate more
  "without thoroughly testing the effect on fst file size... and linguistic correctness"), keep
  flags live where elimination isn't validated or isn't worth it (the main analyser).
- `Makefile.am:364`, `$(INVERT_HFST)` — confirms the brief's specific guess about `hfst-invert`:
  GiellaLT builds the analyser by composing the *generation*-oriented network and inverting it,
  the standard HFST idiom for getting an upper:lower analyser from a lower:upper generator stack.

### 3.2 Why this is the single most valuable finding of this report

It is **direct, in-production, at-scale (7000+-language-family infrastructure) evidence** that
question (A) — "does the morphology/phonology itself license this candidate" — **can be, and
routinely is, answered entirely inside a composed FST**, with no HermitCrab-equivalent verifier
anywhere in the loop. GiellaLT's flag-diacritic filters are doing precisely the class of check
our `pg-foma`/`pg-rules` confirm step does: derivation-order legality, cross-morpheme feature
agreement, compound-boundary legality. **This is real, working existence proof that the
"HC confirm" role is not inherently non-finite-state** — for the construct classes it covers.

The caveat, made concrete in §4: it covers **bounded, propagate-forward, mostly single-valued
features** (a `Der1..Der5` ordinal counter; a `CmpN` agreement value). It is not evidence that
*all* of HC confirm's job generalizes this way — see §4's precision and §6's blockers for exactly
where PanGloss's own attempt to push this further (full feature-structure/MPR gating) hit real
walls.

---

## 4. Flag diacritics as the pruning primitive

### 4.1 What flags can/cannot express (VERIFIED, foma semantics from `foma-rs` source + `lang-sme` docs)

Flag type semantics, cross-referenced between `lang-sme/docs/docu-sme-flag-diacritics.md:18-40`
(GiellaLT's own documentation) and `foma-rs/crates/foma/src/flags.rs` (the actual Rust port
PanGloss depends on):

| Flag | Semantics | foma-rs citation |
|---|---|---|
| `@U.f.v@` Unify | if `f` unset, sets it to `v`; if set, succeeds iff current value compatible with `v` | `flags.rs:293` (`FlagType::UNIFY`), row table `flags.rs:347-351` |
| `@P.f.v@` Positive-set | unconditionally sets/resets `f` to `v` | `flags.rs:301` (`FlagType::POSITIVE`) |
| `@N.f.v@` Negative-set | sets `f` to the *negation* of `v` | `flags.rs:299` (`FlagType::NEGATIVE`) |
| `@R.f.v@` Require | succeeds iff `f` is currently `v` (or, valueless `@R.f@`, iff `f` is set at all) | `flags.rs:303`, rows `flags.rs:352-363` |
| `@D.f.v@` Disallow | succeeds iff `f` is neutral or incompatible with `v` | `flags.rs:297`, rows `flags.rs:364-375` |
| `@C.f@` Clear | resets `f` to neutral, never takes a value | `flags.rs:295` |
| `@E.f.v@` Equal-test | (declared in the type system, `flags.rs:305`) | see the bug below |

**What flags can express:** a *single scalar value per named attribute*, propagated forward
along the tape (left-to-right, since apply is left-to-right — `apply.rs`'s comment at
`flags.rs:37` on the `flag_build` construction: "the languages FAIL, SUCCEED is then the union of
all symbols that cause compatibility or incompatibility"). This is a **bounded, one-directional,
single-valued** unification-substitute: exactly the `Der1..Der5` ordinal-position or `CmpN`
agreement-value shape §3 showed working in production. It is **not** a general feature
*structure* (a bundle of many co-varying attributes with structural sharing/re-entrancy) — each
attribute is its own independent flag; expressing HC's actual `FeatureStruct` (which supports
nested, shared, multi-valued feature bundles) as flags means one flag per leaf feature, each
tested/set independently, with no built-in notion that two features are parts of the same
structure.

**Unbounded vs bounded:** flags are unbounded in *distance* (a flag set at position 3 can be
tested at position 30,000 — there is no window limit, `flags.rs`'s whole design is "propagate
until tested") but bounded in *state space per attribute* (the attribute has exactly the values
it was ever assigned, a finite set fixed at compile time from the grammar's declared symbol
alphabet). This matches HC's MPR/environment gating shape (a fixed, finite set of named
features) far better than it matches, say, arbitrary numeric agreement.

**Interaction with composition/determinization — the one genuinely new empirical finding here,
found independently inside PanGloss's own P6 work, not in any external source:**
- `fsm_compose` is **not** flag-epsilon-transparent by default:
  `FomaOptions::default().flag_is_epsilon == false`
  (cited at `pg-foma/src/gate.rs:24-33`, matching `vendor/foma/src/options.rs`) — composing a
  flag-free net with a flag-bearing net, with the flag never even set, returns the **empty**
  language rather than the vacuous-pass answer either net alone gives. This must be explicitly
  turned on (`flag_is_epsilon = true`) before any multi-net compose where either side may carry
  flags.
- A flag literal embedded **inside a replace rule's own `||` context** (`LHS -> RHS || ... "@D.X@" ...`)
  **corrupts the compiled network in this vendored foma-rs**: `apply_up`/`apply_down` return a
  nondeterministic mix of fired/not-fired paths for the *same* input regardless of whether the
  flag was ever set, and a context consisting of *just* a flag literal **crashes**
  (`STATUS_STACK_BUFFER_OVERRUN` inside `vendor/foma/src/minimize.rs`) — documented at
  `pg-foma/src/gate.rs:14-23` and independently in `docs/fst-plan/p6-prototype-report.md:370-376`.
  Putting the gate in the LHS/RHS instead of the context does not help.
- **Whether flags "survive" composition/elimination depends on flag *type*, and this is a real
  correctness trap, not just a performance question.** `foma`'s `flag_build` pairwise-compatibility
  table (`foma-rs/crates/foma/src/flags.rs:344-387`) has rows **only** for
  `UNIFY`/`REQUIRE`/`DISALLOW`(/`EQUAL` nominally) against other types — **N/C/P-typed flags have
  no rows at all**, and (found empirically, see below) **E-typed flags have no rows either despite
  being declared in the type system.** `flag_eliminate` (`flags.rs:61-266`) builds its filter only
  from flags that had a row; a flag type with zero rows silently produces *no filter*, and
  `flag_purge` (`flags.rs:393-446`) then **strips the symbol from the alphabet anyway** — so
  eliminating an E/N/C/P-typed flag silently degrades to plain *strip* (illegal paths become
  reachable) while the code path still reports "eliminated." This is documented as PanGloss's own
  **headline finding** in `pg-foma/tests/pk2_eliminate_flag_oracle.rs:59-76`: it passes a naive
  cross-engine (foma-rs vs. C-foma) equivalence oracle, because C-foma has the exact same
  bug-for-bug table — **both engines agree on the wrong answer.** The real gate needed is
  `eliminated == baseline` *within one engine*, not just cross-engine agreement.

### 4.2 Does foma / `foma-rs` support flag diacritics, and at what level? (VERIFIED, direct source inspection)

**Yes, at both apply-time (interpreted) and compile-time (eliminated) levels, with the elimination
path only safe for `U`/`R`/`D`-typed flags in the currently-vendored version:**

- **Apply-time**: `ApplyHandle.obey_flags` defaults to `true`
  (`foma-rs/crates/foma/src/apply.rs:538`, `h.obey_flags = true;` inside `apply_clear`) — matches
  upstream C foma's own default (`apply.c:283`, cited already in PanGloss's
  `docs/fst-plan/foma-fst-plan.md:87`, `obey_flags=1`). Flag suppression on output (a flag prints
  as empty string unless `show_flags` is set) is implemented at `apply.rs:1374-1379`. Flag
  consistency checking during traversal is implemented at `apply.rs:990-1001`, `1115-1123`,
  `1559-1567` (the `apply_check_flag`/`flag_lookup` machinery).
- **Compile-time elimination**: `flag_eliminate` (`foma-rs/crates/foma/src/flags.rs:61-266`) is a
  literal, bug-for-bug Rust port of foma's C `flag_eliminate` (module doc, `flags.rs:1-2`,
  "literal Wave-2 (bug-for-bug) port"). It builds a FAIL/SUCCEED filter per flag attribute,
  composes it onto both tapes (`RESULT = FILTER .o. ORIGINAL .o. FILTER`, comment at
  `flags.rs:46-51`), then purges the flag symbols and re-minimizes. This is the `eliminate flag
  <name>` xfst/foma command GiellaLT's own Makefiles invoke directly (§3.1).
- **foma-rs's own test suite already exercises this correctly for the plain-concatenation case**
  (`flags.rs:1245-1277`, `flag_eliminate_end_to_end`), and PanGloss's oracle test
  (`pg-foma/tests/pk2_eliminate_flag_oracle.rs`) independently cross-checked U/R/D-typed
  elimination against **real C foma 0.10.0alpha** running under WSL and found it
  equivalence-preserving and oracle-faithful for single, chained, valueless, and valued flags,
  including alongside PanGloss's own `<R:nnnn>` multichar tag symbols
  (`pk2_eliminate_flag_oracle.rs:38-46`, "battery a-e"). **The correctness gap is specifically
  the E-type (and by the same table gap, N/C/P-typed) elimination**, plus the two composition
  footguns above — not a general absence of flag support.

### 4.3 Reconciling with PanGloss's own docs (as instructed)

`docs/fst-plan/HERMITCRAB_FST_ADVISOR.md` (grepped, read in full) does **not** mention flag
diacritics at all — it predates the foma pivot entirely (its own header: "LEGACY — superseded by
foma-fst-plan.md," a record of the sunset `hc-hybrid` custom-FST prototype). It is however
directly relevant to §5/§6 below via its **Kaplan & Kay (1994) regularity classification**
(`HERMITCRAB_FST_ADVISOR.md:98-129`): a context-sensitive rewrite rule with regular LHS/RHS and
an unbounded (but regular) environment **denotes a regular relation regardless of environment
length** — the theoretical basis for why PanGloss's replace-rule compiler (§3 of
`p6-prototype-report.md`) can compile e.g. Amharic's 20-alpha-variable vowel-harmony-shaped rule
without needing flags at all for the *rule* itself (only for the separately-gated MPR/POS
*subrule conditions*, which is where flags were actually tried, per §4.1).

The genuinely flag-relevant PanGloss documents are `docs/superpowers/specs/
2026-07-15-fst-precision-knob-design.md` (a full design + implementation-findings doc for exactly
"which HC gate constraints should become flag diacritics") and `docs/fst-plan/p6-prototype-report.md`
§7 (the MPR/POS gating attempt) — both are cited at length in §4.1 and §5/§6 below. Grepping
`docs/` for `flag`/`diacritic` turns up 39 files (mostly incidental mentions); these two are the
substantive ones.

---

## 5. Two-stage (proposer + FST pruner) vs. one-stage (single exact FST): assessing the hypothesis

The user's hypothesis: an over-producer FST plus a separate FST pruner (or several, each only
needing to *reject*) is strictly easier to build than one exact FST, because rejection can be
factored into independent machines. This is **not hypothetical for PanGloss** — it is the
*already-shipped* architecture (`foma-fst-plan.md`, foma proposer + full HermitCrab-engine
confirm), independently measured at **8×-48× total-corpus speedup** over the single "exact"
full-search engine (`foma-fst-plan.md` §3c: Indonesian 8.4×, Sena 23.6× sample / 56.6 words/sec
full corpus, Amharic 48.3×). So the top-level claim is empirically TRUE for the
propose-then-verify split as PanGloss actually built it (proposer FST + a full non-FST verifier).
The narrower question the brief actually wants assessed is **whether composing/intersecting
*several separate FST reject-machines* is easier and smaller than one big exact FST** — and here
the evidence is genuinely mixed, with real wins and real illusions, both documented inside this
repo's own P6 investigation:

### Where it helps

- **Convex cost avoidance (the classic two-level-morphology argument).** Kaplan & Kay's own
  result (§4.3) is that composing many small regular relations sequentially stays regular; the
  practical win of keeping them as *separate, later-composed* machines rather than one hand-built
  exact automaton is well precedented — Karttunen's lenient/intersecting composition line
  (cited in `2026-07-15-fst-precision-knob-design.md:174-177`: Karttunen 1998 lenient
  composition, Karttunen 1994 intersecting composition/two-level runtime rule checking) is
  literally "many independently-authored constraint automata, combined at apply time or by
  composition, rather than one exact hand-fused automaton."
  **Measured, convex cost of the opposite (compiling everything in) is a citable, quantified
  example**: Karttunen (2006), the Finnish numeral transducer, three interacting agreement flags
  eliminated one at a time: **1,946 → 2,635 → 3,706 → 20,498 states** — the *last* constraint
  alone costs 5.5× (`2026-07-15-fst-precision-knob-design.md:161-165`). This is the "N reject
  machines is easier than one exact machine" intuition, empirically measured, in exactly this
  research area.
- **PanGloss's own P6 replace-rule prototype independently confirms the tuple-indexed version of
  this same idea at production scale**: Amharic's 20-alpha-variable CV-merger has a *raw* cross
  product of 121,776 tuples; the joint-agreement filter (the same code that resolved Indonesian's
  1-variable case) collapses it to **312 survivors**
  (`docs/fst-plan/p6-prototype-report.md:222-232`) — "a naive per-variable expander... would be
  the thing that actually explodes; the joint-agreement filter... collapses it to 312 without any
  Amharic-specific logic." This is a real, measured win from factoring a would-be-combinatorial
  problem into independently-resolved, then-combined pieces.
- **GiellaLT's own filter cascade (§3) is itself N independently-authored reject machines**,
  composed in a fixed `.o.` order — and it works, ships, and has for over a decade.

### Where it is an illusion, or at least much harder than it looks

1. **"Independent" reject machines are only free to combine if their contexts are actually
   mutually exclusive — get this wrong and the result is silently incorrect, not just slow.**
   PanGloss's own P6 prototype found this the hard way: the *first* attempt to combine 14
   per-tuple replace-rule branches used `fsm_union` (the natural "these are independent
   alternatives" instinct) and **produced a semantically wrong network** — each per-tuple net is
   a *complete* replace transducer that behaves as identity outside its own context, so unioning
   N of them reintroduces a spurious "did nothing" path at every position, verified empirically
   via `apply_down` returning both the correct output AND a spurious unconverted one
   (`p6-prototype-report.md:105-127`, §2.2). The fix — sequential `fsm_compose` folding, correct
   *only because* the tuples' contexts are mutually exclusive by construction — took the composed
   network from **392,311 states / 6,892,003 arcs down to 38 states / 401 arcs**. That six-figure
   difference is entirely a modeling-error/fix gap, not a fundamental FST-size fact — but it shows
   the "just compose N reject machines" framing hides a real correctness trap: *union* vs.
   *compose* is not a detail, it is the difference between right and catastrophically-wrong-and-
   catastrophically-bloated.
2. **When the "reject machines" need to interact with rewrite rules (not just plain lexc
   concatenation), the current toolchain has real, load-bearing bugs**, not just complexity —
   detailed fully in §4.1: flags inside a replace rule's own context corrupt or crash the compiled
   network in this vendored foma-rs. PanGloss's own conclusion after three independently-confirmed
   toolkit surprises was to **stop trying to build this reject machine as flags at all** and use a
   **static, flag-free partition** instead (compile one whole network per group of lexically-disjoint
   entries that share the same gating outcome, computed once in Rust by calling the real engine's
   own `subrule_applicable` predicate directly — `pg-foma/src/gate.rs:55-78`,
   `p6-prototype-report.md:412-448`). This is itself evidence *for* the two-stage/multi-machine
   idea (it still produces N disjoint FSTs unioned together, with zero non-FST mechanism at
   runtime) but *against* doing it via the "obvious" flag-diacritic reject-machine technique in
   this specific toolchain.
3. **The pruner needing the derivation, not just the surface string, is a real and largely
   unavoidable limitation of a pure acceptor-shaped reject machine.** HC confirm's actual contract
   (`foma-fst-plan.md:93-109`) is not "is this surface string well-formed" — it is "replay the
   engine pinned to this *specific* root + rule-set candidate and check it produces this word,"
   because soundness requires knowing *which* allomorphs/rules combined, not merely that *some*
   combination would produce an acceptable string. A plain FST acceptor over the surface tape
   cannot distinguish "produced legally by rule-set R1" from "produced legally by rule-set R2" if
   both yield the same surface string — that distinction only exists on the analysis tape (the
   `<R:nnnn>`/`<M:nnnn>` tag sequence, `foma-fst-plan.md:132-135`), which is why PanGloss's design
   already puts the burden of derivation-identity on the *analysis* tape and lets `apply_up`'s
   tag output stand in for "the derivation" rather than trying to verify it via a second surface-
   only acceptor. This is exactly why §6 concludes the fully-general MPR/POS-and-beyond gating
   problem is not closable by "one more reject machine" alone — some of what confirm does is
   inherently about *which candidate this is*, which the tag tape already encodes, not a separate
   fact a downstream acceptor could independently re-derive from the bare surface string.
4. **Moving the multiplication doesn't eliminate it — it relocates it, and the new location has
   its own worst cases.** The "Strip" (fully-permissive) default PanGloss ships today avoids all of
   the above bugs by *not* encoding gate constraints as flags at all and letting confirm prune —
   but confirm's own pathological cases are real and measured: three Sena words land at
   candidate-count-driven confirm costs of 171-767ms (`foma-fst-plan.md:292-311`) specifically
   *because* the "approximate only upward" strategy overgenerates heavily on words that have zero
   true analyses (the full engine also finds nothing, but only after confirm rejects many false
   candidates). PanGloss's own measured comparison of Strip vs. AllFlags on Sena
   (`2026-07-15-fst-precision-knob-design.md:263-275`) found AllFlags costs **4× compile time**
   and **~40% propose throughput** for a mere **~0.25% aggregate candidate reduction** — i.e., for
   *this* grammar, compiling the reject machines in barely helps and Strip (defer entirely to
   confirm) is strictly better. This is the mirror image of finding 1 above: sometimes composing
   in the reject machines is a large cost for a tiny win, and the "let confirm handle it" default
   is correct precisely *because* measuring beats guessing here.

### Net technical assessment

The hypothesis is **directionally correct and already validated at the top level** (propose
broadly + a separate, cheaper-to-build rejection mechanism beats one hand-fused exact machine) —
but **"N independent reject machines" is not a free lunch**: it is free only when (a) the
machines' rejection conditions are genuinely mutually exclusive/composable in the right algebra
(union is almost never the right combinator; sequential compose usually is, and getting this
wrong silently produces wrong answers, not just slow ones), (b) the toolchain's primitives behave
as advertised for the specific combination attempted (flags-inside-replace-rules is a documented,
reproducible counterexample in this exact toolchain version), and (c) the thing being pruned is
actually surface-string-decidable and doesn't require re-deriving *which* candidate produced it.
Where all three hold (bounded feature agreement expressed as flags composed with a
lexicon/twolc/filter cascade, §3; tuple-indexed rule compilation with mutually-exclusive
contexts, this section) it is a real, measured win. Where the reject condition needs derivation
identity or interacts with rewrite-rule machinery this toolchain doesn't support cleanly, the
"just add another reject machine" instinct has already been tried, in this exact codebase, and
abandoned in favor of either static (non-flag) partitioning or deferring to a real verifier.

---

## 6. Verdict: can an FST pruner replace HermitCrab's confirm step?

Structured in the three tiers requested, drawing together §1-§5.

### (i) Yes, outright, for these construct classes

- **Bounded phonological rewrite rules (SPE-style, regular LHS/RHS), regardless of environment
  length** — Kaplan & Kay 1994's theorem (`HERMITCRAB_FST_ADVISOR.md:98-107`), and **empirically
  proven at three grammar scales** by PanGloss's own P6 prototype: Indonesian 97/97 recall parity
  via compiled replace rules; Amharic's 20-alpha-variable CV-merger (312 survivors from a raw
  121,776 cross product); Aweti's 18-rule cascade composing in 28.8ms
  (`p6-prototype-report.md` §3-§5). This *is* HC confirm's job for this construct class, done as
  pure FST composition, no verifier needed.
- **Cross-morpheme feature agreement / derivation-order legality expressed as bounded,
  single-valued, propagate-forward flags** — proven in production at scale by GiellaLT's own
  `Der1..Der5`/`CmpN` filter cascade (§3), and independently re-derived (then partly qualified,
  see (ii)) by PanGloss's own precision-knob work.
- **Static (root-fixed) MPR/POS subrule gating** — closed for PanGloss's own grammars, but
  **not** via flag diacritics; via a compile-time static partition into lexically-disjoint,
  separately-compiled-and-unioned networks, computed by calling the real engine's own gating
  predicate once (`pg-foma/src/gate.rs`). Still 100% FST at runtime, zero non-finite-state
  mechanism — just not the "obvious" flag encoding.
- **Bounded reduplication, bounded infixation, bounded deletion-with-reapplication** — classified
  `Regular = true` by PanGloss's own Kaplan-Kay-based advisor (`HERMITCRAB_FST_ADVISOR.md:117-125`),
  reclaimable by bounded-fold/inverse-probe compilation once built.

### (ii) Yes, but with accepted overgeneration / measured cost

- **PanGloss's actual shipped default (Strip: emit everything permissively, let confirm prune) is
  in this tier by design** — and per §5's own measurement, this is usually the *right* choice, not
  a compromise: AllFlags-style exact gate-compilation cost 4× compile time / ~40% throughput for
  ~0.25% candidate reduction on Sena. The FST-only path *could* be pushed further into tier (i) for
  more constraint classes, but PanGloss's own numbers say it usually isn't worth it — the
  overgenerate-and-let-a-verifier-prune strategy is the load-bearing, cost-justified default, and
  Beesley & Karttunen's own literature (`2026-07-15-fst-precision-knob-design.md:166-172`) frames
  exactly this as a known runtime-vs-size dial (~20-70% apply-time slowdown from live flags,
  Greenlandic's flag-bearing network is 140MB vs. a flag-free target that has *never* been
  successfully built at all — flags are load-bearing there, not optional).
- **CG-style contextual disambiguation (§1/§2) is available and real, but sits outside this
  question entirely** — it prunes among already-(A)-legal readings using sentence context PanGloss
  doesn't model. Could be added as a wholly separate later stage if PanGloss ever needs
  cross-word disambiguation, but it is not a substitute for confirm and should not be scoped as
  one.
- **Zero-analysis pathological words** (Sena's `cinacemerwa`/`cinagumanika`/`kamatamisa`,
  171-767ms confirm cost) are the concrete, named cost of staying in this tier
  (`foma-fst-plan.md:292-311`) — accepted, reportable, not hidden.

### (iii) No / requires a non-finite-state check (or requires solving a real open engineering problem first)

- **Unbounded-copy reduplication** (`{ww}`, a whole stem copied without a fixed bound) —
  **provable impossibility**, not an engineering gap: this is a non-regular (in fact non-context-
  free) language by standard formal-language theory, independently reaffirmed by PanGloss's own
  advisor (`HERMITCRAB_FST_ADVISOR.md:110-113,125`, `Regular = false`).
- **Word-edge anchor environments ("nothing else may follow")** are **not expressible via flag
  diacritics specifically** (though they are ordinary regular languages expressible
  structurally) — every accepted path eventually reaches *some* accept state, so no flag "set at
  every accept point" can encode absence-of-further-input; PanGloss's own precision-knob work
  found this and correctly routed it to a structural (`Eliminate`, not `KeepFlag`) encoding
  instead (`pg-foma/src/precision.rs:53-60`).
- **General nonmonotonic Constraint Grammar** (arbitrary insert/delete-to-fixpoint over
  sentence-level cohorts) is **Turing-complete / undecidable in the unrestricted case**
  (Yli-Jyrä Proposition 3, §2.2) — a provable impossibility for the *general* formalism. Not
  directly a blocker for PanGloss (CG isn't the mechanism we'd be replacing HC confirm with, per
  §1), but it forecloses "just compile whatever CG rules exist to an FST, unconditionally" as a
  universal technique.
- **The specific toolkit bugs in this vendored foma-rs are engineering-cost blockers, not
  theoretical ones**: flags-inside-replace-rule-context corruption/crash, `fsm_compose`'s
  non-epsilon-transparent default, and the E/N/C/P-typed `flag_build`-table gap that silently
  degrades "eliminate" to "strip" while still reporting success (§4.1) — all three are
  independently reproducible, documented, and (per PanGloss's own record) each is a real
  surprise that stopped a specific flag-diacritic design rather than a fundamental limit of flag
  diacritics as a concept.
- **Templated morphotactics, circumfix roles, and dynamic mid-derivation MPR propagation in a
  replace-rule-compatible ("underlying tape") emitter are simply not built yet** — costed as
  "medium" to "medium-large" engineering effort, not blocked on any provable impossibility
  (`p6-prototype-report.md` §6 items 2-4).
- **`RewriteMode::Simultaneous` self-opaquing reapplication, `Dir::RightToLeft`, `Quantifier`/
  `OptionalSegmentSequence` patterns, and `MetathesisRuleDef`** are all unimplemented in the
  replace-rule compiler but have known, textbook constructions (RTL via `fsm_reverse`; metathesis
  via the classical marker-insert/reorder/delete trick) — engineering cost, explicitly not
  provable impossibilities (`p6-prototype-report.md` §6 items 5-7).
- **The "exact-compile-in" tier for rewrite-rule gating is currently empirically unreachable for
  PanGloss's own three reference grammars**: 0 of 30 gated subrules across the 3 reference + 4
  conformance grammars are unconditional literal rewrites — every one carries a POS/MPR gate, an
  environment, or alpha-variable agreement (`2026-07-15-fst-precision-knob-design.md:241-245`,
  step 4's finding). This means the `Compose`-everything-in tier of the precision knob has an
  **empty safe subset today**, a measured fact rather than a permanent limit — closing it
  requires the underlying-tape emitter refit (item above), which is itself only engineering cost.

### Summary table

| Blocker | Citation | Label |
|---|---|---|
| Unbounded-copy reduplication (`{ww}`) | `HERMITCRAB_FST_ADVISOR.md:110-113,125` | provable impossibility |
| Word-edge anchors via flags specifically | `pg-foma/src/precision.rs:53-60` | provable impossibility (for this mechanism) |
| General nonmonotonic CG is Turing-complete | Yli-Jyrä Prop. 3, arXiv:1707.05115 | provable impossibility (of the general formalism; not a blocker for our architecture per §1) |
| Flags inside replace-rule `\|\|` context corrupt/crash | `pg-foma/src/gate.rs:14-23`; `p6-prototype-report.md:370-376` | engineering cost (toolkit bug, this foma-rs version) |
| `fsm_compose` not flag-epsilon-transparent by default | `pg-foma/src/gate.rs:24-33`; `vendor/foma/src/options.rs` | engineering cost (toolkit default, must be set explicitly) |
| E/N/C/P-typed flag elimination silently degrades to strip | `foma-rs/crates/foma/src/flags.rs:344-387`; `pk2_eliminate_flag_oracle.rs:59-76` | engineering cost / correctness trap (bug-for-bug with upstream C foma) |
| Templated morphotactics / circumfix / dynamic MPR propagation in underlying emitter | `p6-prototype-report.md` §6 items 2-4 | engineering cost |
| RTL / Simultaneous reapplication / Quantifier patterns / Metathesis unimplemented | `p6-prototype-report.md` §6 items 5-7 | engineering cost |
| Exact-compile-in gating has an empty safe subset today (0/30 unconditional subrules) | `2026-07-15-fst-precision-knob-design.md:241-245` | engineering cost (measured, not permanent) |

**One-sentence verdict:** for the well-formedness-pruning half of HermitCrab confirm's job
(question A), an FST pruner is not a stretch goal — it is a proven, shipping technique
(GiellaLT's flag-diacritic filter cascade) and PanGloss's own replace-rule compiler already
does the phonological-rule half of it at 100% recall parity; the remaining gap is a named,
costed, mostly-engineering (not theoretical) punch list, with exactly two hard theoretical walls
(unbounded-copy reduplication; general nonmonotonic CG) that do not actually block PanGloss's own
architecture because PanGloss doesn't need either.
