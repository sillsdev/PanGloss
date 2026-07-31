# Ideas worth borrowing from Divvun

Divvun has been building finite-state morphology for twenty years and has invented working
solutions to problems PanGloss is currently solving differently, or not solving. This is the list
of their techniques that look valuable to us, with the production precedent for each.

**Framing, which matters for reading everything below.** We are **keeping the HermitCrab `confirm`
step.** None of these ideas is a route to deleting it, and none of them needs to be. Correctness
already comes from `confirm`. What a tighter proposer buys is **speed** — fewer candidates reaching
`confirm` per word — plus, in a couple of cases, coverage of constructs the current emitter handles
by enumeration and therefore does not scale on.

Nothing here has been built or measured. These are candidates with precedent, not results. Each
entry states what would have to be true for it to pay off, so the first person to try one knows
what they are testing.

---

## 1. Trigger diacritics for long-range phonological gating

**Priority: highest.** This is the single most promising finding of the whole investigation.

**What it is.** A distinguished, otherwise-unused symbol is attached to the *affix* that requires a
phonological effect on the *stem*. The symbol rides along the tape, unmodified, through however
much intervening material separates it from where it matters. A later rule writes its context to
include the trigger symbol at that distance and fires only when it is present. A cleanup rule at
the end of the cascade deletes it, so it never reaches the surface.

**Their precedent.** Two independent, shipping instances:

- `lang-sme` (two-level): ~30 symbols `X1`–`X9` / `Q1`–`Q9` / `W1`–`W9`, declared `X1:0 … W9:0` at
  `phonology.twolc:72-75`, carrying the consonant-gradation grade from the suffix back to the
  gradation site across boundary symbols and stem consonants. Trigger sets `WeG`, `Dummy`,
  `NonMPDummy` at `phonology.twolc:110-116`.
- `lang-kal` (**ordered replace cascade — our formalism**): `%^GEM`, `%^GEMS`, `%^GEMEQ`, `%^GEMC`,
  `%^Loan`, `%^T`, `%^ProgI`, declared as ordinary alphabet members in the `Dummy` set at
  `phonology.xfscript:49`, consumed by rules reaching across skippable material, deleted by
  explicit cleanup rules (`InflBorderDel`, `DummyDeletion`, `ProgIVaek`) at
  `phonology.xfscript:50,356-357`. Worked example at `phonology.xfscript:240-249`:

  ```
  define geminationS (r) g -> [ k k ], j -> [ t s ], m -> [ m m ], … 
      || [ Vow|%^T ] ( %> ) _ Vow (Cns) %^GEMS %^T %< NonUvular ;
  ```

  `%^GEMS` sits several segments to the *right* of the gemination site, in the rule's own
  right-context — exactly the "skip across intervening material to find the trigger" shape.

**Why it matters to us.** This mechanism is functionally what HermitCrab's MPR features *are*: a
morphophonological trigger transported across intervening material as a symbol on the tape rather
than held in engine state. That the `lang-kal` instance runs in an ordered replace cascade — not
two-level — is the load-bearing detail: it confirms the device transfers to our formalism unchanged,
as observed shipping code rather than as a hypothesis.

**What it would replace.** `rust/crates/pg-foma/src/gate.rs` currently handles MPR/POS subrule
gating with a static partition, having ruled flags out (see idea 2 — that rule-out is over-scoped).
Static partitioning cannot express a gate whose trigger is arbitrarily far from its effect without
enumerating the intervening material.

**What has to be true for it to pay off.** The gate must fire correctly with the trigger transported
across intervening material, *and* the compiled net must not exceed the static-partition baseline in
states. Trigger symbols enlarge the alphabet, and alphabet size — not lexicon size — is where both
projects' compile costs actually live. That is the measurement, and it has not been taken.

---

## 2. Insert-then-read flag diacritics for morphotactic legality

**What it is.** Flag diacritics (`@U.` unify, `@P.` set, `@R.` require, `@D.` disallow, `@C.` clear)
are zero-width state tests carried on the tape and interpreted at apply time. They express
"if you took this path earlier, you may not take that path now" without enumerating legal paths.

**Their precedent.** 1,118 flags in `lang-sme/src/`, split 672 `@U.` / 214 `@P.` / 94 `@C.` /
79 `@R.` / 59 `@D.`. The exemplar is `src/fst/filters/remove-illegal-derivation-strings-flagbased.regex`
— an inverse-replace rule that *inserts* flag pairs around each derivation tag:

```
"@D.Der1.TRUE@" "@D.Der2.TRUE@" … "@P.Der1.TRUE@" "+Der1"  <-  "+Der1" ,
```

so "derivations must ascend Der1→Der5, no double passives" becomes flag unification at apply time
rather than an enumerated list of legal derivation sequences.

**Why we previously ruled this out, and why that was wrong.**
`rust/crates/pg-foma/src/gate.rs:1-20` concludes `->` and flags "do not mix safely in this port,
full stop", after a prototype hit three separate toolkit issues. That observation is correct; the
conclusion is over-scoped. Verified from source in both `foma-rs` and upstream C: the replace
calculus is entirely **flag-blind** (`grep -ci flag` = 0 in both `rewrite.rs` and `rewrite.c`),
while `apply` treats flag-shaped symbols as **zero-width unconditionally**
(`foma/apply.c:1084`). The two only collide when a flag occupies a **matched** role — inside a `||`
context, compiled through a `NotContain` construction.

The Divvun idiom above has **no `||` clause at all**, so `rewr_context_restrict` is never invoked
for it (`rewrite.rs:383` gates the whole construction on `rewrite_contexts.is_some()`). The flags
are pure inserted output material. The precise safe/unsafe line is *"does this flag occurrence
require compile-time matching against real tape content"* — not "which side of the arrow is it on".

**What it would buy us.** In-network morphotactic legality gating — derivation ordering, compounding
legality — which today is either enumerated or deferred to `confirm`.

**What has to be true for it to pay off.** The idiom must compile under `foma-rs` `0.4.2` and
correctly reject `Der2`-before-`Der1` while accepting ascending order. That is a day's work and
either outcome is worth having: if it fails, `foma-rs` has a defect on the exact idiom Divvun's own
grammars depend on, which is a significant upstream finding.

**Caveat to carry.** PK2's finding, recorded at `rust/crates/pg-foma/src/precision.rs`, is that
`@P`/`@R`-typed flags are **not eliminable** by `foma-rs`'s `flag_build` decision table. They stay
live in the network and must be interpreted at apply time. Divvun has the same property — their
flags stay live for `hfst-optimised-lookup` to interpret, and `eliminate flag` has **zero** hits in
`lang-sme/src/` or `giella-core/am-shared/`. So whatever consumes our network must interpret flags,
same as theirs. `foma-rs` does have `crates/foma/src/flags.rs`.

---

## 3. Enumerate-and-parallel-replace for alpha-variables

**What it is.** Replace calculus has no shared-variable binding construct, so there is no direct
equivalent of twolc's `Where … matched` (or HermitCrab's alpha-variables). The recipe is:
**enumerate the variable's domain into one concrete disjunct per member, join them with `,,`
(parallel replace, so all fire simultaneously and no inter-disjunct ordering question arises), then
`.o.` compose the cleanup and placeholder-deletion rules.**

**Their precedent — and this is the single best artifact found in the whole investigation.**
`lang-crk/src/fst/morphology/phonology.xfscript:477-483` preserves the twolc original in comments
directly above its live hand-translation, so the mechanical recipe is visible side by side:

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

**It also closes the reduplication question.** That Cree rule *is* reduplication — placeholder
symbols `d1`/`d2` matched against the stem-initial consonant, then deleted. Bounded copy by
placeholder-and-match, requiring no `compile-replace`, which `foma-rs` does not have (zero hits
repo-wide). Unbounded-copy reduplication remains provably outside the model; bounded
CV-template reduplication now has an in-network idiom that did not appear to exist.

**Status in our code — partially there already.**
`rust/crates/pg-foma/src/replace.rs` already does the enumeration half: `resolve_alpha_tuples`
gathers every slot referencing a given `VarId`, enumerates the cross product, and keeps only
combinations where same-`VarId` slots agree. Two gaps:

- That module is an explicit **prototype, not wired into the mainline `emit`/`analyzer` path** (its
  own module doc says so). It is exercised only by `examples/p6_replace_prototype.rs`.
- The `,,` parallel join is the piece that removes the inter-disjunct ordering hazard. **The
  vendored parser supports it** — `foma-0.4.2/src/regex.rs:624`: *"Each ReplaceRule (a `,,`-separated
  block) becomes one rewrite_set node"* (verified 2026-07-31 against the pinned crate). So this is
  an emitter change, not a toolkit blocker.

**What has to be true for it to pay off.** Domain sizes must stay small. In `phonology.twolc` the
observed `matched` domains are 2–10 members (`Cx in (z m h p g b d)`), so enumeration does not bite
there. It bites when the **alphabet** is large — Amharic's 417-segment inventory, where PanGloss
already measured the blow-up. Those are orthogonal axes and the distinction is worth keeping
straight.

---

## 4. A separate composed filter stage for tag-string legality

**What it is.** Rather than building legality into the lexicon, compose an ordered `.o.` cascade of
small regex filters onto the *finished* network, each removing a class of illegal tag string.

**Their precedent.** ~20 `.regex` filters in `lang-sme/src/fst/filters/` —
`block-illegal_compound-strings.regex`, `remove-illegal-derivation-strings-flagbased.regex`,
`convert_to_flags-CmpNP-tags.regex`.

**Why it matters to us.** Tag-string legality is a *regular* property of the output string, which
makes it the cheapest possible thing to check — and our analysis reached the same conclusion
independently: `MorphemeCoOccurrence` is proven categorically simpler as a filter, **and the tags it
needs are already on the tape**. That last clause is the rare case where the conservation law
(a filter can only reject on information present on the tape, so encoding new facts for the filter
enlarges the proposer) does not bill us anything.

**Where the limit is, so nobody rediscovers it painfully.** Co-occurrence in `Anywhere` mode is a
tight **2^k** Myhill–Nerode bound — achieved, not merely feared — and the non-reachability-provable
MPR `Overwrite` case is **4^k**. Those two do not look soluble and should not be attempted as
filters.

---

## 5. `compose-intersect` rather than free composition at scale

**What it is.** Their build compiles the whole lexicon in a single `hfst-lexc` pass, then runs
`determinize → minimize → hfst-compose-intersect → minimize` against the rules transducer — rather
than freely composing rules onto the lexicon.

**Why it matters to us.** Finnish `lang-fin` is 1,156,174 lines of lexc and ships, so this strategy
demonstrably survives ~10x our FLEx-scale target. Their bottleneck is the compose-against-rules step
and **alphabet size**, not lexicon size — which is exactly what PanGloss measured on Amharic. This
suggests our `Compose` primitive wants a restricted/intersecting mode.

**Status:** unexamined on our side. `rust/crates/pg-foma/src/compose_budget.rs` exists and is where
this would be evaluated.

---

## 6. A separate, cheaper, deliberately approximate guesser

**What it is.** GiellaLT's answer to "what do you do with a word the lexicon does not contain" is
**not** to run the real grammar more permissively. It is a structurally separate, hand-built
transducer with its own phonotactic approximation of legal syllable and cluster shapes —
`lang-sme/src/fst/guesser.xfscript`:

```
define PossWord (s) cons^{0,2} [vowel|dipth] (i) [cons|cons2|cons3|cons^3] vowel cons^{0,1};
```

substituted in for a placeholder root (`^GUESSNOUNROOT`) in a saved lexicon.

**Why it is worth noting.** Open-vocabulary handling is a live question for us, and the instinct is
to reach for "loosen the real grammar". They deliberately did not, because loosening the real
grammar costs you precision everywhere, whereas a separate approximate FST costs you nothing on
words the real lexicon covers. Whether we want this depends on product scope, not on finite-state
theory.

---

## 7. `Err/` and `Use/` tags — admit non-normative forms, tagged, and route per consumer

**What it is.** Rather than rejecting non-standard forms, tag them and let each downstream consumer
decide. `Err/Orth` (substandard spelling), `Err/CmpSub`, `Err/Lex`, `Err/MissingSpace` and ~11
others mark *why* a form is non-normative; `Use/-Spell`, `Use/-GC`, `Use/-PLX`, `Use/-TTS` then let
the speller, grammar checker, dictionary and TTS each independently honor or suppress a reading.

**Scale of use.** `Err/Orth` alone: **1,890 occurrences** in `lang-sme`'s lexc sources, plus 67
`Err/MissingSpace`, 58 `Err/Lex`, 48 `Err/DerSub`. This is a large, actively maintained mechanism,
not a vestige.

**The related move:** different analyzer *variants* (normative / dict / descriptive) deliberately
carry different over-generation tolerances for different consumers, rather than one universal
accept/reject boundary — realized as the `CmpNP` compounding restriction in `lang-fin`.

**Why it matters to us.** This is product design, not compiler design, and it is directly relevant
to the spellchecker scope. A speller and an analyzer want different answers to "is this a word",
and their solution is to answer once and tag, rather than to build two grammars.

---

## Already done

**`BoundRoot` discharged at compile time.** Report 12's finding was that a bound root that can never
appear bare needs no filter at all — it is a compile-time topological fact, so simply omit the
bare-root arc. Zero new states, cheaper than any filter. **This landed** in commit `0ec6007`,
documented at `docs/fst-plan/bare-root-compile-time-discharge.md` and implemented in
`rust/crates/pg-foma/src/emit.rs:1478`.

---

## Deliberately not borrowed

- **Constraint Grammar as a replacement for `confirm`.** Different problem (contextual
  disambiguation, not well-formedness), and general nonmonotonic CG is Turing-complete
  (Yli-Jyrä, Prop. 3) — finite-state equivalent only under a runtime bound production grammars are
  not shown to satisfy.
- **`twolc` as an authoring formalism.** We would have to reformulate every rule and abandon
  abstract intermediate segments, and their own build has the `.twolc → .foma` path commented out as
  non-working. ~30 of their language repos already decline it too.
- **Their remediation workflow** — hand-tagging, commenting out over-generating rules, hand-listing
  110,000 compounds. It works for them because a linguist owns each grammar permanently. It is the
  exact cost PanGloss exists to remove.
- **Shipping the FST without a verifier.** See
  [why-not-just-use-divvun.md §4](why-not-just-use-divvun.md).
