# "Why can't we just use Divvun? They're doing the same thing!"

This is the right question to ask, and it gets asked about twice a year. This document is the
answer, written for someone who has not read any of the research and does not want to.

**The short version:** Divvun and PanGloss both produce a finite-state transducer that analyzes
words in morphologically complex languages, so they look like the same thing. They are not. In
Divvun's system the FST *is* the analyzer — whatever it accepts is the answer. In PanGloss the FST
is a *proposer*, and a second engine checks every candidate it proposes against the real grammar.
That single difference is the product. You cannot move onto Divvun's architecture without deleting
the checking step, and the checking step is the thing we are selling.

Nothing below says Divvun is bad. They ship working language tools for languages nobody else
serves, and they have been doing it for twenty years. Several of their techniques are worth
stealing, and [ideas-worth-borrowing.md](ideas-worth-borrowing.md) lists the ones we want.

---

## 1. What a finite-state transducer actually is

Skip this section if you already know.

An FST is a machine that reads a word one character at a time. It sits in a numbered state; each
character it reads moves it to another state and optionally emits some output. If it runs out of
characters in an accepting state, the word is accepted and the emitted output is the analysis.

```
    "cats"  →  [FST]  →  cat+N+Pl
```

Two properties matter for everything below:

- **It is fast and small.** Analysis is microseconds and the whole language fits in a file you can
  ship to a phone. This is why every serious morphological analyzer in the world is an FST.
- **It has no memory except which state it is in.** It cannot "remember" that it saw a particular
  prefix ten characters ago unless that fact was baked into the state it is now sitting in, or was
  written down on the tape where a later rule can read it.

That second property is the entire engineering problem. Every clever technique in both projects —
flag diacritics, trigger symbols, archiphonemes, filter cascades — is a way of smuggling
information the machine would otherwise forget into a place the machine can still see it.

The consequence that never goes away: **an FST cannot check anything that requires unbounded
memory.** Most morphology does not. A few things do, and those are permanently outside the model,
not merely hard.

---

## 2. The two architectures, side by side

### Divvun / GiellaLT

```
  hand-written lexc (lexicon)
  hand-written twolc or xfscript (phonology)      ──compile──▶   ONE FST
  hand-written flag diacritics (legality)                          │
  hand-written filter cascade (tag legality)                       │
                                                                   ▼
                                                          all accepted analyses
                                                                   │
                                                                   ▼
                                          Constraint Grammar (vislcg3) picks the
                                          reading that fits the surrounding sentence
```

The FST is authored by a linguist, by hand, for that one language. Whatever it accepts is by
definition the analysis. The Constraint Grammar stage afterwards does **not** check whether an
analysis is well-formed — it assumes they all are, and chooses among them using sentence context.

### PanGloss

```
  FLEx grammar (lexicon + rules, authored once by the linguist in FLEx)
            │
         compile
            │
            ▼
        FST (deliberately loose — accepts a superset)
            │
            ▼
     candidate analyses
            │
            ▼
  HermitCrab `confirm` — replays the actual grammar on each candidate
  and throws out the ones the grammar forbids
            │
            ▼
         answer
```

The FST here is a fast filter that gets us from "all conceivable segmentations" down to "a handful
worth checking". The correctness guarantee comes from `confirm`, not from the FST. This is a
deliberate, documented, permanent decision:
`docs/fst-plan/foma-fst-plan.md:20` — *"FST-only (no-verify) operation is off the table —
propose+prune is the permanent shape."*

### The word "prune" means two different things

This is the single most common source of confusion when comparing the two systems, so it is worth
being pedantic:

| | What it does | Who does it |
|---|---|---|
| **(A) Well-formedness pruning** | The candidate is *not a word of this language*. The morphology or phonology forbids it. Throw it away. | PanGloss: HermitCrab `confirm`. Divvun: nothing at runtime — it is baked into how the FST was hand-written. |
| **(B) Contextual disambiguation** | The word is genuinely ambiguous and all readings are legitimate. Pick the one that fits the sentence. | Divvun: Constraint Grammar. PanGloss: not our problem (yet). |

"Divvun has a pruner too, so we could use theirs" conflates these. Their pruner is (B). Ours is
(A). Swapping one for the other does not typecheck.

---

## 3. How Divvun gets a single FST to be accurate enough

They do not use one trick. They use five mechanisms, each matched to a different *shape* of
constraint. This is the genuinely impressive part of their design and it is worth understanding
before dismissing anything.

| Layer | Mechanism | Plain English |
|---|---|---|
| Phonology | `twolc` two-level rules, applied simultaneously (intersected) | "b becomes p between these sounds" — 112 named rules in North Sámi |
| Trigger transport | Deletable marker symbols (`X1`–`X9`, `Q1`–`Q9`, `%^GEMS`, …) written on the tape and erased at the end | The trick that lets a *suffix* tell the *stem*, several characters away, to change shape |
| Morphotactic legality | Flag diacritics (`@U.`, `@P.`, `@R.`, `@D.`, `@C.`) | "if you took this path earlier you may not take that path now" — 1,118 of them in North Sámi |
| Tag-string legality | An ordered cascade of ~20 regex filters composed onto the finished network | "no double passives; derivations must ascend Der1→Der5" |
| Ambiguity | Constraint Grammar, strictly afterwards | pick the reading the sentence supports |

The insight worth taking: **one mechanism per constraint shape.** Concatenative morphotactics grows
additively, so a lexicon handles it. Phonology composes, so rules handle it. Ordering constraints
are monotonic, so flags handle it. Tag legality is a regular property of the output string, so a
filter handles it. Genuine ambiguity is not a well-formedness question at all, so it is deferred.

Notably, a research report that reasoned purely from finite-state theory — without reading a single
line of Sámi source — arrived at the same "one mechanism per bound-shape" conclusion independently.
That convergence is the strongest result of the whole investigation.

---

## 4. The accuracy question, which is where the comparison actually turns

The intuition behind "why not just use Divvun" is usually: *they ship production analyzers for
polysynthetic languages, so their approach must be accurate enough; ours is over-engineered.*

The premise is not supported by their own evidence, and this is not a dig — it is their stated,
deliberate design.

### Their own definition of "done" has no accuracy criterion for the analyzer

GiellaLT publishes a maturity classification. For **spell checkers** it names precision at every
tier ("Production: false positives less than 5%"). For **grammar checkers** likewise ("Production:
precision of at least 80%"). For the **morphological analyzer** — the artifact that would replace
our proposer+confirm pair — the criteria at every tier are lexicon size, grammar completeness, and
*running-text coverage*, which is a recall measure.

The word "precision" does not appear in the analyzer criteria at any maturity level. **An analyzer
can be certified Production — their highest tier — without anyone ever measuring how much it
wrongly accepts.**

### Their test harness cannot detect over-acceptance, org-wide

- `giella-core/scripts/run-morph-tester.sh.in:146` passes `--ignore-extra-analyses`
  **unconditionally, for every language in the organization**. That flag disables the one check
  that would catch an analyzer returning readings it should not.
- The harness supports negative assertions (`~`-prefixed "this must NOT parse" forms). They appear
  in **0 of 1,572** YAML test files across the languages examined.
- Their own testing documentation says it outright — `lang-sme/docs/docu-sme-testplan.md:138-153`:
  *"When we test whether words are let through or not, we do not test whether the parser actually
  gives correct analyses."*

One important nuance, in their favor: the **generation** direction *is* held to a strict exact-set
standard. The artifact that must never emit garbage (generation, which feeds the speller) is tested
for soundness. The artifact that is allowed to over-generate because a downstream engine discards
the excess (analysis) is tested for recall only, on purpose. So their shipped-in-production evidence
that "FST-only works" is evidence about **the generator, not the analyzer** — and the analyzer is
the artifact that would replace `confirm`.

### The numbers that do exist

No published GiellaLT work measures analyzer over-generation directly; no such benchmark exists
anywhere. What exists are downstream figures, which bound it indirectly:

| Measurement | Number | Source |
|---|---|---|
| Compound-error candidates proposed vs. real | 4,437 proposed, 458 real — **10.3% precision** (CG lifts the end result to 76.6%) | Wiechetek, Unhammer & Moshagen 2019 |
| Inari Saami L2 grammar checker on proofread text | **19.5% precision** — 136 of 169 alarms false | Trosterud, Olthuis & Wiechetek 2023 |
| North Sámi grammar checker on a corpus assumed error-free | **precision 0.46** (988/2139) | `lang-sme/devtools/report.correct.txt:7924-7925` |
| Same checker against a hand-annotated gold corpus | precision 85.4%, later 88.1% | `lang-sme/devtools/report.goldstandard*.txt` |

And the most rigorous speller evaluation they have (Kaalep, Pirinen & Moshagen 2022) states in its
own text that it says nothing about "how many misspelled words are falsely recognized as correct"
— then filters observed false-accepts out of the test set before computing anything, without ever
counting them. **The number we would want has never been computed by anyone, and they had the
data.**

### How they fix over-generation when it hurts

Not with a better compiler. By hand, and by giving up coverage:

- **Dynamic compounding** — the worst source — was abandoned as a generative mechanism because a
  permissive design "led to many false positives". It was replaced by **110,000 hand-listed
  lexicalised compounds** covering 90.5%, accepting the recall loss to buy precision.
- In `adjectives.lexc` the same move appears **nine times**, each annotated "we overgenerate" and
  each commented out.
- `Err/Orth` hand-tagging runs to **1,890 occurrences** in the North Sámi lexc sources — non-standard
  forms are not rejected, they are tagged so downstream consumers can route around them.
- `joavdalas` — a specific named over-generation bug — is real, documented at
  `lang-sme/docs/docu-sme-bugs.md:73`, and still unresolved. `lang-sme#447` and `#563` carry dozens
  of native-speaker false-positive reports, open since 2019, labeled low priority.

**None of this means their tools are bad.** It means precision at the analyzer layer is not a
constraint they have chosen to operate under, and their shipping record is therefore not evidence
that our checking step is unnecessary.

---

## 5. The question, answered directly

### "They ship real languages for real users. Why not just adopt their stack?"

Because what ships is *hand-written per language by a linguist who already knows finite-state
methods*. The thing PanGloss automates — turn a FLEx grammar into a working analyzer — is the thing
their architecture assumes a human already did. Adopting their stack does not get us their
languages; it gets us their authoring burden.

### "Their FST has no verifier and it works. Doesn't that prove FST-only is fine?"

It proves it *ships*. It does not prove it is *sound*, and their own test infrastructure is
structurally incapable of telling the difference (§4). PanGloss's `confirm` is a strictly stronger
contract than anything in their pipeline. The real question is not "is FST-only possible" but "what
accuracy bar do we need" — a product decision, not a finite-state one.

### "Can't we just hand them our compiled FST?"

No, for two independent reasons.

1. **There is no artifact-level seam.** Every path in GiellaLT's autotools build compiles
   lexc/twolc from source it controls. There is no "hand us a compiled transducer" entry point
   anywhere in the tooling.
2. **Our FST is not an analyzer.** Its upper tape carries `<R:nnnn>` and `<M:nnnn>` morpheme ID
   tags (`rust/crates/pg-foma/src/tags.rs:1-5`), not linguistic tags like `+N+Sg+Nom`. It is
   meaningless without our decoder and our `confirm` step. Handing it over would ship a transducer
   that accepts strings the real grammar rejects — the exact failure their quality bar assumes
   away.

### "Then can we generate their source format instead?"

This is the seam that actually exists, and it is a real option. We emit lexc plus replace rules,
and their build compiles both. Two costs:

- Someone must design a **tagset** per language. GiellaLT treats the analysis tagset — names,
  ordering, feature inventory — as a linguist's decision, not something a generator derives. That
  is a human bottleneck per language regardless of who builds the FST.
- We would be shipping the proposer *without* `confirm`, which is exactly the accuracy giveaway
  in §4 unless we solve it first.

### "Could Constraint Grammar replace HermitCrab?"

No. CG answers question (B) in §2; `confirm` answers (A). Separately, general nonmonotonic CG is
Turing-complete (Yli-Jyrä, *Power of Constraint Grammars Revisited*, Prop. 3) — finite-state
equivalent only under a runtime bound production grammars are not shown to satisfy. It is not a
smaller, cheaper verifier; it is a different tool for a different job.

### "We both use foma, so aren't we already compatible?"

No, and the shared word "foma" makes this trap easy.

- `lang-sme/configure.ac:167-171` sets `DEFAULT_FOMA=no`, `DEFAULT_HFST=yes`,
  `DEFAULT_HFST_BACKEND=foma`. **Standalone foma is off.** They run HFST, which uses foma's C
  library as an internal storage format (`HfstDataTypes.h:51`: `FOMA_TYPE`). Those are different
  things.
- foma cannot compile their phonology at all: there is **no `.twolc → .foma` build rule**, and
  `giella-core/am-shared/twolc-include.am:42-57` contains a commented-out foma path with the reason
  stated verbatim — *"Commented out for now, they interfere with proper Foma builds."* They tried
  the bridge and abandoned it.
- `foma-rs`, the Rust port we depend on, is **one engineer's work** (Brendan Molloy), every commit
  dated 2026-07-16/17, and it does **not** appear in Divvun's own production build. It is plugged
  in underneath `hfst-rs` as an unweighted backend. "Divvun is moving to foma-rust" overstates a
  young, one-person effort.

### "Doesn't the two-level vs. cascade difference block everything?"

It was thought to, and it does not. Their phonology is written as simultaneous two-level rules;
ours is an ordered rewrite cascade, and those are genuinely different mathematical operations
(intersection vs. composition — Kaplan & Kay). But we never need to *emit or consume* twolc, so the
hard translation direction is a job we can decline. Their own portfolio backs this up: roughly 30
of their ~155 language repos write phonology as an ordered replace cascade in `phonology.xfscript`
— **our formalism** — including `lang-kal` (Kalaallisut/West Greenlandic), the hardest polysynthetic
grammar they ship, which contains **zero** `.twolc` files anywhere. And the build asymmetry is
exact: the `.xfscript → .foma` rule is live and uncommented; the twolc one is commented out. Our
formalism is on the supported side of that line.

### "Their languages are harder than ours, so surely the approach scales."

It scales with *hand-maintenance*, which is the cost we are trying to remove. Greenlandic's
derivations file records a five-year running battle against a single recursive suffix (`TIP`),
fought with flag diacritics *and* forked lexica *and* per-entry tuning, annotated in Danish, still
being edited as of 2024-08-30. Nobody has cleanly solved unbounded derivational recursion. Do not
promise a compiler will do it automatically.

For the record, raw scale is genuinely settled in their favor: Finnish `lang-fin` is **1,156,174
lines of lexc** and ships. That is ~10x our FLEx-scale target. Their bottleneck is the
compose-against-rules step and alphabet size, not lexicon size — which matches what PanGloss already
measured on Amharic.

### "So is there nothing to learn from them?"

There is a great deal, and it is the point of this whole investigation. See
[ideas-worth-borrowing.md](ideas-worth-borrowing.md). The three best are trigger diacritics
(their mechanism for long-range phonological triggering, which is functionally what HermitCrab's
MPR features are), insert-then-read flag diacritics for morphotactic legality, and the
enumerate-and-parallel-replace recipe for alpha-variables — all with working production precedent
we can copy rather than invent.

---

## 6. What we would give up, in one list

If someone proposes moving to Divvun's architecture wholesale, this is the bill:

1. **The soundness guarantee.** `confirm` replays the real grammar per candidate. Nothing in their
   pipeline does this, and their tests could not tell you if it were missing.
2. **Compilation from FLEx.** Their grammars are hand-authored for a finite-state target by
   linguists fluent in finite-state methods. That is the labor PanGloss exists to eliminate.
3. **Constructs the FST cannot express.** Unbounded-copy reduplication is provably outside the
   model (pumping lemma). We handle it in the non-FST peel; they hand-list it or live without it.
4. **Per-language automation.** Their tagset, their CG rules, their error model, and their
   hyphenation FST are all per-language human deliverables we do not currently produce.

And what we would gain, honestly stated: a mature distribution channel (Páhkat, Divvun Manager,
mobile keyboards), a real CG disambiguation layer, packaging we currently lack, and twenty years of
accumulated per-language linguistic assets.

Those are worth wanting. They are just not worth the first item on the list.

---

## Where the details live

- [what-divvun-actually-does.md](what-divvun-actually-does.md) — the verified research: architecture,
  sources, line-level citations, and what is known vs. assumed.
- [ideas-worth-borrowing.md](ideas-worth-borrowing.md) — the transferable techniques, with the
  production precedent for each.
