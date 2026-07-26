# Red team — report 23

Pass-3 adversarial review. Scope per instructions: kill the product as currently specified (D14 warm
cache + D18 flag-only-on-failed-parse + D4 class n-gram + D2 synthetic error model), find the next
unattacked load-bearing assumption, stress-test the internal coherence of D2/D17/D18 specifically, and
report what survives. D16/D17 govern: a synthetic sweep or a four-sample measurement may only ever
*eliminate*, never *validate*; only an argument from architecture or arithmetic gets **BROKEN**.
Everything else that fails to find support is **UNSUPPORTED**, which moves it to the *deferred* column
with a named measurement, not a disproof. All `PLAN.md` line numbers are against the working copy at
this session's start (2026-07-25); the file may have moved since — re-verify before folding anything in.

Evidence tags: `[A]` verified at primary source, quoted; `[M]` my own argument from the plan's text or
arithmetic; `[S]` secondary source I did not independently read, or a claim I could not verify past a
search summary. I did not find or use any citation I could not source — where I looked and failed, I
say so.

---

## 1. Verdict table

| # | Target | Verdict | One-line reason |
|---|---|---|---|
| T1 | **Day-one behavior: D18 "Option A" silently degrades into "Option B" for the majority of tokens in exactly the target language family** | **BROKEN** | Arithmetic composition of two already-accepted facts (F8's OOV rates, D18's own silence rule) that the plan states separately but never combines into their joint consequence. |
| T2 | **Keyman's context-window contract may not deliver enough left context to bootstrap D4's inter-word trigram after a cold start** | **UNSUPPORTED — deferred** | Keyman's own official worked example for exactly this use case requests only 16 left codepoints / 0 right codepoints; a rolling in-session buffer defeats this for continuously-typed text, but does not defeat it for a freshly opened cursor position inside existing text. |
| T3 | **D2's synthetic-corruption sampling weighting is unspecified and reproduces D14's own named frequency defect one section over** | **UNSUPPORTED — deferred** | D14 names "frequency for a generated entry is not observed" as a problem for the warm cache; D2 has the identical unsolved problem for error-training pairs and the plan never connects the two. |
| T4a | **Does the Layer-2 add-on actually have runtime access to Layer-1's `confirm`, given D15's stated boundary?** | **SURVIVES, but unspecified** | D8a already commits both to ship inside one `custom-1.0` model, so nothing structurally blocks the call — but no decision states the runtime calling convention, only the build-time staleness binding. |
| T4b | **Per-word `confirm` at flagging time inherits report 13's own step-cap/timeout tail, inside a `predict()` call with no host-enforced timeout** | **UNSUPPORTED — deferred, flagged urgent** | Report 13 measured non-trivial step-cap/timeout rates (Amharic 9.81% timeout, Sena 12.42% step-capped) on the *research-signal-only* four samples; D18's mechanism reintroduces exactly this per-word cost, synchronously, with no circuit breaker. |
| T5 | **D12 (orthography settled) and "heavily agglutinating/polysynthetic" may be anti-correlated in SIL's actual target population** | **UNSUPPORTED — deferred, product-existential** | A plausible, not provable, skew: orthography negotiation is disproportionately live for exactly the typologically unusual, under-documented languages the morphological speller is built to serve. Cannot be resolved from four samples per D16; needs a project-registry survey. |
| T6 | **Class-cardinality combinatorics may put rung 2/3 out of reach of the low end of D15's own proposed sweep (10^3-10^4 tokens)** | **UNSUPPORTED — deferred** | Sharpens F1/F3 with concrete combinatorics (C³ trigram contexts) rather than restating them; no external tag-cardinality citation could be independently verified, so the argument stays at the level of arithmetic, not measurement. |

---

## 2. The day-one user experience — the most damaging honest account

Assume the plan ships exactly as specified: D14's warm cache and shelved runtime generation, D18's
flagging rule, D4's two-scale n-gram, D2's synthetic error model. Pick the polysynthetic/heavily
agglutinating language the campaign has been implicitly circling (Aweti-shaped, or an
Inuktitut-shaped target — D14's own warning box already names Inuktitut as "one of the languages the
task explicitly names," `PLAN.md:1394-1395`).

**What the user experiences:** silence, almost all the time, on almost everything they write.

The mechanism is a composition of two facts the plan already accepts individually but never states
together:

1. **F8 (already in `REVIEW-LOG.md`, confirmed at primary source):** for exactly this language family,
   the uncached rate is not D14's assumed 1% — it is "one to two orders of magnitude" higher, with
   Inuktitut's own confirmed datum reading *"held-out stories have more than 60% of words
   out-of-vocabulary"* even against a 1.3-million-word lexicon, 130x the size of D14's 10k cache
   (Gupta & Boulianne, LREC 2020, `2020.lrec-1.307`, quoted verbatim in `PLAN.md:1394-1396`).
2. **D18's own decision text** (`PLAN.md:1928-1936`): flagging requires (1) *"an attempted parse that
   failed"* or (2) *"an exhausted generative search"* — and explicitly, *"a skipped parse is not a
   failed parse"* and *"shelved is not exhausted"* (`PLAN.md:1932, 1934`). *"Anything else is silence.
   The system may decline to offer a suggestion; it may not assert an error."* (`PLAN.md:1936`).

Compose them. D14 shelves tiers 1-2 at runtime specifically to avoid invoking the analyzer on a
keystroke (that is the entire latency argument the plan makes for shelving it —
`PLAN.md:1369-1370, 1470-1480`). So for the majority-to-supermajority of tokens F8 says will miss the
cache in exactly this language family, **no parse is attempted, because attempting one is the thing
D14 shelved.** A skipped parse is not a failed parse, so D18's mechanism (1) never fires. Tier 2 is
shelved, not exhausted, so mechanism (2) never fires either. By D18's own rule, the only remaining
outcome is silence.

**This is the finding nobody in the campaign has stated in this composed form.** D18's own table
(`PLAN.md:1943-1949`) presents "A. Flag only on a completed parse" and "B. Never flag; suggest only" as
two different, deliberately choosable products, and calls the choice *"John's call, and it is a
product call, not a technical one"* (`PLAN.md:1953-1954`). **That framing is false as a description of
what ships if D14 stands as written.** Nobody chooses B. B is what A becomes, silently, by arithmetic,
for most tokens in exactly the language family the project exists to serve — because the mechanism
that would let A behave like A (an actual attempted parse) is the mechanism D14 turned off to protect
the keystroke budget. The "choice" the plan thinks it is offering John has already been made by D14,
and made in the direction D14's own traffic-model box calls "the largest or second-largest bucket"
(`PLAN.md:1416-1417` area, restated from F8).

**What this looks like to the user, concretely, on day one:**

- They type a short greeting, a pronoun, a common verb stem — a tier-0 cache hit. The keyboard behaves
  like any keyboard: fast, quiet, correct. This is the 10-40% of tokens (per F8's corrected range) that
  the demo, and any quick smoke test, will show off.
- They then write an actual sentence — the kind of sentence that motivated building a morphological
  analyzer instead of a wordlist in the first place: a long, richly inflected verb with several
  agglutinated affixes, a noun in an oblique case, a form derived on the fly from a stem the linguist
  entered but nobody has typed before. Per F8/D14, this is the *common* case for this language family,
  not the edge case. The keyboard says nothing. Not "looks fine" — nothing. No suggestion appears
  (tier 1/2 supply is shelved), no flag appears (D18 forbids it without an attempted parse that never
  runs). The one thing a "spellchecker" is supposed to do — react — does not happen for the words that
  are actually hard to spell.
- The one time they *do* mistype, if the intended word happens to be outside the 10k cache (again, the
  common case per F8), the same silence applies: not corrected, not flagged. The 9% "mistyped but
  cached" bucket (`PLAN.md:1381-1385`) only ever catches typos of words that were already going to be a
  cache hit; it has nothing to say about a typo in a word the cache never had.
- The net behavior a user reports is: *"it works for the ten words I already knew how to spell, and is
  completely silent on everything else — including the one time I actually mistyped something long."*
  That is the tweet. It is not a crash, not a wrong answer — it is the visible absence of the one
  product capability (morphological awareness) that differentiates this from a plain dictionary
  speller, on precisely the population of words where that capability was supposed to matter.

**One honest complication, stated against my own attack:** D18's silence is *safer* than the
alternative it replaced (F10: "not found ⇒ flag," which would have marked those same words as errors
"en masse," `PLAN.md:1916-1920` in report 20's language). Silence-by-default is a real, defensible,
shippable product (D18's own "B is a real product" framing, `PLAN.md:1951`) — it just is not the
product D18's own table frames it as being chosen deliberately over. The damage is in the gap between
what the plan says it is offering ("your call between A and B") and what actually ships absent further
work (B, involuntarily, for the population that most needed A).

---

## 3. Per-attack detail

### T1 — detailed above in §2. Verdict: **BROKEN.**

This is not new evidence — it is F8 and D18 (both already accepted in `REVIEW-LOG.md`) combined into a
joint consequence neither finding states on its own. The composition is arithmetic (F8's percentages)
plus a direct reading of D18's own conditional logic, which is exactly D17's standard for elimination:
"an argument from architecture or arithmetic... stated so it can be attacked." I looked for the
strongest counter and found one real mitigation (silence is safer than false-flagging) but no argument
that defeats the composition itself.

**Ledger consequence.** This does not create a new ledger row; it sharpens C4 and C6
(`PLAN.md:1963-1971`) by making explicit that they are the *same* decision under D14-as-written, not
two independently choosable ones. C4's candidate (c) — "per-grammar, chosen by D10's calibration" — is
the only escape: a grammar whose calibrated uncached rate is high must default to un-shelving
generation (C4c) *before* C6's choice between "flag" and "suggest-only" becomes a real choice rather
than a foregone one.

### T2 — Keyman's context-window contract vs. D4's inter-word trigram

**The claim under attack:** D8b treats `context.left` as a solved input — *"every word the user types
passes through our hands anyway... we accumulate frequency counts from context"* (`PLAN.md:1137-1140`).
D4's inter-word term needs *"the classes of the preceding words"* (plural, `PLAN.md:352-353`) — a
trigram needs visibility into (at least) the previous two words to be anything more than a bigram or
unigram in practice.

**What I verified.** Keyman's `Capabilities` interface, read directly from
`common/web/types/src/lexical-model-types.ts` `[A]`:

> `maxLeftContextCodePoints`: *"The maximum amount of UTF-16 code points that the keyboard will provide
> to the left of the cursor, as an integer."*

This is a **host-declared ceiling**, not something the model can request upward past. No default or
typical value is stated in the type definition itself `[A]` — the actual number is set per host
platform (Android app, iOS app, web), and neither `PLAN.md` nor reports 03/12 state what any of them
actually declare.

The one concrete number in Keyman's own public documentation is worse than a placeholder — it is an
official worked example, for *this exact use case*. Keyman's own blog post on building an advanced
custom lexical model, "Creating an advanced custom lexical model with Keyman" `[A,
blog.keyman.com/2026/03/creating-an-advanced-custom-lexical-model-with-keyman/]`, motivates the whole
article with:

> *"For polysynthetic languages or those with complex morphologies, it is not practical to list all
> possible word forms."* ... *"For these languages, it makes sense to embed grammar knowledge in to
> the lexical model and reduce the wordlist dramatically."*

— i.e., this is Keyman's own worked example for PanGloss's exact target case — and its `configure()`
example sets:

> `leftContextCodePoints: 16, rightContextCodePoints: 0`

**The arithmetic.** A polysynthetic wordform routinely spans dozens of characters — that is close to
definitional to the typological label, and it is the same fact D14's own warning box leans on when it
cites Inuktitut ("a language whose type count is still growing... has no meaningful head," restating
report 20). A 16-codepoint window can be consumed *entirely* by the tail of a single preceding word,
before a trigram's second word of context is visible at all — and the example sets *zero* right
context, so there is no fallback direction either.

**The honest mitigation, argued against my own attack.** D8b's own mechanism partly defuses this. If
PanGloss maintains its own in-worker rolling buffer of (word, class) pairs as the user types —
something D8b already half-describes ("we accumulate frequency counts from context") — then the
trigram's history for *continuously typed* text does not depend on re-deriving two words' worth of
context from a single `predict()` call's `context.left` string; it depends on PanGloss's own session
memory, which has no 16-codepoint ceiling. **This genuinely survives for the common continuous-typing
case.**

**What it does not defuse:** a user opening an existing document (one they did not type this session —
review, translation-checking, resuming earlier work) and placing the cursor mid-paragraph to type one
new word. PanGloss's session buffer is empty; the only context available to seed the trigram is exactly
the codepoint-limited `context.left` the host declares — and if that ceiling is anywhere near the
worked example's 16, a single preceding polysynthetic word can already exhaust it, leaving the
inter-word term with zero effective history for the first several words of any editing session. This
is a real, unaddressed cold-start gap, not the whole mechanism.

**Verdict: UNSUPPORTED — deferred.** I could not verify the actual ceiling Keyman's shipped Android/
iOS/Windows apps declare (only a tutorial's chosen value), so this does not rise to BROKEN. It is a
load-bearing, unchecked dependency that no report in the series (01-22) examined — reports 03/12
examined `predict()`'s synchronicity and `traverseFromRoot`'s budget, never the context *window size*.

### T3 — D2's synthetic-corruption sampling weighting reproduces D14's own named defect

**The claim under attack** (`PLAN.md:271-273`): *"Sample the grammar's own confirmed generative output
and perturb it, then fit the error model on the resulting (corrupted, correct) pairs."* Nowhere does D2
state the **sampling weight** over that generative output — uniform over lexicon entries? Uniform over
paradigm cells? Weighted by whatever partial corpus frequency exists?

**The internal contradiction this creates.** D14 item 1 (`PLAN.md:1516-1519`) already names exactly
this defect, for a different consumer of the same generative capability:

> *"Frequency for a generated entry is not observed, and ranking within the warm cache needs a
> frequency estimate, and for generated forms the only available estimator is D4's class model — the
> model the cache was partly meant to relieve."*

D2's error-training corpus has the **identical** problem for the identical underlying reason (no
observed frequency exists for the grammar's generative output absent a real corpus), and the plan never
connects the two. If error pairs are synthesized by uniformly sampling stems and paradigm cells (the
only option that needs nothing but the grammar, which is D2's own stated appeal — *"cheap, needs
nothing but the grammar"*, `PLAN.md:275`), a rare, linguist-authored dictionary test entry gets equal
representation with a common noun in the resulting error model. The error model is then fit on a
corruption distribution that does not resemble which words people actually attempt to type (and
therefore mistype) — a distribution D2 needs to approximate for `error_cost` to rank real typos well,
but never states an intention to approximate.

**Why this is not merely "unweighted, fix it later."** D9's ranking rule already depends on the warm
cache's *ordering*, and the plan's own two-halves argument for a generated warm cache (D15 point 2,
`PLAN.md:1620-1623`) explicitly separates "generation needs no corpus, ordering does." D2's error model
is exactly the kind of *ordering* decision (which correction ranks highest given a typo) that the same
argument should apply to, and D2 never makes the corresponding move. This is a specification gap in the
newest decision in the plan, not a settled tradeoff.

**Verdict: UNSUPPORTED — deferred.** I have no evidence the mismatch is fatal (a uniform-sampling error
model might transfer adequately — MAGEC's ~92%-of-labeled-sibling number, already in the plan, is
built from an even cruder confusion-set-inversion method and still worked reasonably) — but the gap is
real, unstated, and structurally identical to one the plan already treats as consequential one section
over.

### T4a — the runtime integration contract between Layer 1 and the add-on

**The question, as posed:** given D15's explicit boundary — *"this is a corpus-trained add-on, not
part of the analyzer pack"* (`PLAN.md:45`) — does the add-on actually have Layer 1's `confirm` callable
at flagging time, or is this an unexamined seam?

**What D15 actually specifies.** Its "binding problem" section (`PLAN.md:1580-1598`) is entirely about
*build-time compatibility* — a content digest over the class-defining inventories, with a stated
staleness policy (*"refuse, or warn-and-degrade to a coarser rung"*) — never about the *runtime calling
convention*. The Layer 1 / Layer 2 table itself (`PLAN.md:1566-1572`) draws the boundary along
lifecycle ("rebuilds when the grammar changes" vs. "rebuilds when the corpus... changes") and artifact
identity (`.pgpack` vs. "an add-on, separately versioned") — never along a runtime process/module
boundary.

**Why it plausibly survives anyway.** D8a's ownership table (`PLAN.md:1072-1079`) already commits both
halves to the same shipped artifact: *"Morphological generation, confirm/trim, analysis | **PanGloss**,
shipped as `pg-wasm` in a `custom-1.0` model"* and *"N-gram weights and ranking (D4) | **PanGloss** —
our own weights, not Keyman's"* — both rows say "ours," both ship inside the one Keyman `custom-1.0`
model. Nothing in D8/D8a/D8b's Keyman-contract reading suggests two separate WASM modules or separate
workers; the natural reading is one artifact, one address space, and Layer 2's code calling Layer 1's
`confirm` is an ordinary in-process function call, no different from any other layered Rust/WASM
crate boundary in this codebase.

**Verdict: SURVIVES, but the plan never says so.** I could not find an architectural reason this
fails. The gap is that nobody has written the sentence "Layer 2 calls Layer 1's `confirm` directly, in
the same process, at runtime" anywhere — D15's boundary language ("not part of the analyzer pack") is
worded in a way that, read carelessly by an implementer, could suggest a harder separation than the
plan actually intends. Cheap fix: state the runtime contract explicitly, one sentence, next to D15's
existing build-time binding paragraph.

### T4b — the per-word `confirm` cost tail, reintroduced by D18, uncircuit-broken

This is the sharper half of the D18/D15 question, and it survives being separated from T4a.

**The mechanism.** D18's mechanism (1) requires `confirm` to actually run, per word, at flagging time,
for every cache-miss word (which per F8/T1 is most words in exactly this language family). D14
dissolved the *bulk*-generation cost by moving it offline (`PLAN.md:1470-1480`), but a **per-word**
`confirm` call at query time is not bulk generation — it is an ordinary propose+confirm analysis, the
same operation Layer 1 does for any text. The question is whether that operation is *uniformly* cheap
enough to run synchronously on a keystroke, and report 13 already measured that it is not, uniformly:

> Coverage table, `PLAN.md:1259-1264` (Rust-HermitCrab-only pipeline, four samples): step-capped
> (200k steps) 12.42% (Sena), 0.00% (Amharic), 0.00% (Indonesian), 40.87% (Aweti); timed out 0.00%
> (Sena), **9.81% (Amharic)**, 0.00% (Indonesian), 6.73% (Aweti).

Per D16 these are research-signal-only, not calibration — I am not claiming these exact percentages
transfer to any real grammar. What transfers is the **shape**: propose+confirm's per-word cost
distribution has a heavy tail, and a nontrivial share of words in at least one of four small samples hit
a resource cap or an outright timeout, not a fast return. D18's flagging path puts precisely this
per-word cost on the critical path of every keystroke that misses the cache — which, per T1, is most
keystrokes in exactly the target language family.

**Why there is no safety net.** Report 12 already established, and `PLAN.md` already records, that
*"`predict()` is synchronous — it returns `Distribution<Suggestion>`, not a Promise"* (`PLAN.md:1008`),
and that *"a model that does its work in `predict()` has no host-enforced timeout at all"*
(`PLAN.md:794-795`, restated 990). `traverseFromRoot`'s 33ms budget is scoped only to Keyman's own
correction search, not to a flagging call made from `predict()`. So if D18's flagging mechanism is
implemented inside `predict()` (the natural place, since that is where PanGloss owns the tier policy
per D8a), **a single slow or step-capped word can block the keyboard with no host circuit breaker to
fall back on** — not a graceful degradation, an actual hang, on the exact word class (long, complex,
uncached) D18 was written to protect.

**Verdict: UNSUPPORTED — deferred, but flagged as urgent.** This is not proven fatal — a real grammar's
propose+confirm cost distribution post-multi-FST-rewrite is explicitly unmeasured (D13's own rewrite
note, `PLAN.md:1277-1337`) — but it is a genuinely new composition: D14 solved the *aggregate* latency
problem by shelving bulk generation; D18 reintroduces a *per-call* latency problem in the one place
(`predict()`) that has no host-enforced ceiling at all, and nothing in D10's calibration scope
(`PLAN.md:1543-1548`, which explicitly "narrows sharply" once tiers 1-2 are shelved) currently covers
per-word flagging latency, because D10's narrowing happened *before* D18 existed.

### T5 — D12's orthography gate and the polysynthetic/agglutinating typology may not co-occur

**The tension.** D12 (`PLAN.md:1181-1199`) scopes out any language *"where no established orthography
exists"* — decided for good reasons (HermitCrab's rules apply over graphemes; an unstable orthography
degrades the parser itself, not merely the ranking). D13's admitted starting set is Sena, Amharic,
Indonesian, Aweti. Of these, only Sena and Aweti carry meaningful agglutination; Amharic is Semitic
(templatic, not the polysynthetic case), Indonesian is largely isolating. Amharic (Ge'ez script,
centuries of standardization) and Indonesian (Latin-script national language, standardized 20th
century) plainly clear D12. Whether Aweti — a small Amazonian language, exactly the profile where
orthography is often a live literacy-committee question rather than a settled fact — clears D12 as
cleanly is genuinely unknown from anything in this plan, and D16 forbids concluding anything about the
general case from four samples in either direction.

**Why this matters beyond the four samples.** SIL's own published framing of its historical mission is
that orthography development is not a side activity but a core, large-scale one: SIL's own site states
it *"has been involved in developing orthographies... in over 1,300 languages"* since 1934
`[S — via search summary; direct fetch of sil.org returned HTTP 403 in this session, so this is not
independently read at primary source and should not be promoted to [A] without a further fetch]`. That
scale of "had no orthography, needed one developed" work is disproportionately concentrated in
under-documented minority languages — the same population that disproportionately includes typologically
unusual, heavily agglutinating or polysynthetic languages (Amazonian, Papuan, many North American
families). If that skew is real, D12 and "the language this whole architecture is built to differentiate
on" may be **substantially anti-correlated** in SIL's actual project pipeline — not universally (Finnish,
Turkish, and Inuktitut are all agglutinative/polysynthetic *and* have reasonably settled orthographies,
so the anti-correlation is a skew, not an exclusion rule) — but enough to matter for how much of the
hardest, most differentiating engineering in this plan (D4's whole reason for existing) ever ships to a
population D12 actually admits.

**Why this is not BROKEN.** I cannot show, and D16 forbids concluding from the four samples, that the
admissible intersection is empty or even small. This is a plausible, well-motivated, but unverified
correlation — exactly the shape D17 reserves for the deferred column.

**Verdict: UNSUPPORTED — deferred, product-existential.** The deciding measurement is cheap and does
not require new grammars or new data collection in the research sense: a survey of SIL's own active
FLEx/documentation project registry, crossed against (i) documented orthography-stability status
(already tracked by literacy/orthography-committee processes in most such projects) and (ii) a rough
morphological-typology tag. This is a metadata query, not a linguistics experiment, and it directly
tells the project whether D12 and the class of language D4 was built for are the same population,
a different population, or an empty intersection.

### T6 — class cardinality vs. D15's proposed 10^3-10^4 sweep floor

**The claim under attack.** D15's own table (`PLAN.md:1606-1611`) says the inter-word class trigram
needs "moderate, and rung-dependent" text and "degrades by rung, not uniformly," and § "What data we
need" proposes sweeping *"10^3, 10^4, 10^5, 10^6 tokens"* (`PLAN.md:1987`) to find where each rung
becomes estimable. This is already honestly hedged (D15 point 1, `PLAN.md:1614-1619`, already concedes
rung 1 is unestimable at any size we will ever have) — the open question is whether the *low end* of
that stated sweep (10^3-10^4) has any usable signal for rung 2/3 at all, or whether it is arithmetically
a null test.

**The arithmetic.** A trigram over *C* distinct classes has up to *C*³ possible three-class contexts.
Rung 2 (POS + a selected feature subset) is exactly the rung D4 says must carry real signal for the
design to work at all (`PLAN.md:163-164`, "the usable ladder... is four rungs rather than six"). Even a
modest *C* in the tens for a single POS category's feature-subset space (plausible for any language
with more than a token amount of case/tense/aspect/person marking — this is a property of real
morphology this plan itself argues for elsewhere, e.g. D1's list of `syn_fs` contents,
`PLAN.md:87`) already puts *C*³ in the tens-of-thousands-to-low-millions range of possible contexts,
before Zipfian sparsity (most contexts occurring once or never) is even applied — and Zipfian sparsity
is exactly the reason smoothing exists, which returns to F3's already-accepted finding that modified
Kneser-Ney's discount terms are validated on integer counts at training sizes larger than PanGloss's
floor (`PLAN.md`, F3 in `REVIEW-LOG.md`). At 10³-10⁴ tokens, most rung-2/3 trigram contexts will be seen
zero or one time regardless of smoother choice — the sweep's low end may report "rung 2 is unusable" not
because rung 2 lacks signal in principle but because 10³-10⁴ tokens is arithmetically too small a sample
for *any* trigram over more than a handful of classes, independent of language.

**Why this is not a new discovery, only a sharper one.** F1 (lattice/fractional-count training
underspecified) and F3 (MKN's fractional-count/small-N mismatch) already flag adjacent parts of this.
What is new here is the concrete combinatorial framing (*C*³ contexts) applied specifically to the low
end of D15's own proposed sweep range, which nobody has stated in this form.

**What I could not verify.** I looked for a citable, primary-source number for real morphological
tag-set/feature-bundle cardinality in an agglutinative language (Turkish morphological disambiguation
literature, Hakkani-Tür & Oflazer) to replace the illustrative "tens" above with a sourced figure, and
could not extract one from a fetchable primary source in this session — search results confirmed the
paper exists and confirmed its own reason for decomposing tags into inflectional groups (large potential
tag-set size from productive derivation) but not a specific cardinality number, and I did not find the
number via a corrected search either. **This stays `[S]`/illustrative, not `[A]`,** and the argument
should be read as "the arithmetic is concerning at plausible class counts," not as a measured fact.

**Verdict: UNSUPPORTED — deferred.** The sweep D15 already proposes running is the right instrument;
this attack's contribution is to flag that its *low end* may be a foregone-null result for arithmetic
reasons that have nothing to do with any particular language, and the report should say so rather than
let a "rung 2 shows no signal at 10^3 tokens" result be read as a finding about rung 2.

---

## 4. What survived

A red team that kills everything is useless, so here is what I tried to kill and could not.

1. **D8's `.zhfst`-is-architecturally-impossible argument.** This is a clean exactness argument
   (overapproximating proposer vs. an acceptor that must be exact) with no escape route the plan itself
   hasn't already checked and closed (`PLAN.md:955-961`, the three-routes table). I tried the obvious
   counters — "compile out the overapproximating constructs," "emit only for grammars simple enough" —
   and the plan already shows both fail or degrade the product to something not worth building. This
   survives cleanly; it is architecture, not calibration, so D16 does not even touch it.
2. **The core intra-word insight — morphemes recur where wordforms don't.** The empirical anchor
   (Finnish word-level OOV 20% → 0% at the morph level, `PLAN.md:390-391`, an externally attested
   result) is solid and not contingent on any of the four samples. Everything about *how* to train the
   term (F1, F3, T6 above) is genuinely contested; the underlying reason the term should help at all is
   not.
3. **The anytime-contract framing of the tier system itself** (tier 0 always answers, refinement is
   optional, `PLAN.md:770-776`). I looked for a way T2's cold-start gap or T4b's per-word latency risk
   breaks the *anytime property* itself, and they don't — they attack what data is available or how
   expensive one path is, not the guarantee that a partial, honest answer is always available fast. The
   property holds even when what it has to say is "nothing yet."
4. **`WordAnalysis.guessed` as a free unknown-word signal (D1).** I could not find an argument that this
   signal is unreliable *in principle* — only that it is untested in practice (report 13 never
   exercised the guess branch, already flagged in-plan at `PLAN.md:1360`). The mechanism itself (the
   parser already distinguishes lexicon-backed from guessed-root analyses, for free) is sound
   architecture, independent of the multi-FST rewrite.
5. **D17's ledger-over-decision-register discipline.** I went in suspecting this was process theater
   layered on top of an unchanged set of conclusions. It is not: D14 and D18 are visibly different
   documents because of it (both carry live alternatives with named deciding measurements instead of
   reading as settled), and this report's own T1-T6 slot cleanly into that ledger shape rather than
   requiring a new structure. The discipline is doing real work.
6. **D8a's "we must ship the engine, no static format works" conclusion.** Once D8 holds, this follows
   necessarily — no static wordlist/trie/foma-compiled format can express "confirm trims this," for the
   same reason a `.zhfst` cannot. I could not find a fourth escape route beyond the three D8/D8a already
   check.

---

## 5. Proposed ledger rows

Per `PLAN.md`'s Candidate-ledger format (`PLAN.md:1963-1971`) — for each item moved to *deferred*, live
candidates and the one deciding measurement.

| # | Question | Live candidates | The deciding measurement |
|---|---|---|---|
| T2 | Can the inter-word trigram get enough left context after a cold start (fresh cursor position in existing, not-this-session-typed text)? | (a) accept whatever `maxLeftContextCodePoints` the host declares and degrade to bigram/unigram when it's short; (b) request the platform maximum and treat a short grant as a per-platform calibration input (D10-shaped); (c) don't rely on `predict()`'s context at all for history — re-run `confirm` over `context.left` once, at cursor-placement time, to seed the rolling buffer, accepting the one-time cost | The actual `maxLeftContextCodePoints` declared by Keyman's shipped Android/iOS/Windows/web hosts (not a tutorial's chosen value), cross-referenced against median/tail wordform length (in codepoints) for the target language family. |
| T3 | What sampling weight does D2's error-corruption corpus use over the grammar's generative output? | (a) uniform over lexicon entries/paradigm cells (today's implicit default); (b) frequency-weighted using whatever partial real corpus exists, however sparse; (c) weighted by D4's own class-model plausibility estimate (bounded-not-circular, per D14's own precedent for the warm cache) | Per D16 point 5 (amended): a synthetic sweep across weighting schemes can only *eliminate* a scheme that fails to beat the generic-Levenshtein floor (candidate B in D2's own ledger row C5) even under generous synthetic assumptions — it cannot validate any scheme as correct. The real decision needs recall@k on real typos, which per C5 does not exist yet. |
| T4b | What happens when a D18-qualifying per-word `confirm` call runs long inside a synchronous, host-timeout-free `predict()`? | (a) impose PanGloss's own internal timeout inside `predict()` and treat a self-imposed timeout as "inconclusive, do not flag" (extends D18's own conservative spirit, matching evaluation report 22's proposal 7); (b) never run `confirm` synchronously in `predict()` for flagging — defer flagging to an async/idle-time pass and only ever *retroactively* mark a word, accepting a UI lag between typing and any flag; (c) restrict D18's mechanism (1) to a per-grammar allow-list of words known (from build-time calibration) to be cheap to confirm, treating everything else as automatically "inconclusive" | Per-word `confirm` latency distribution (not aggregate coverage) measured on the post-multi-FST-rewrite pipeline, at the tail (p99, not just mean) — this is the number D10's calibration harness (`calibrate-fst-resource-envelopes`) is already built to produce and simply has not been pointed at this specific question yet. |
| T5 | Does the admissible (D12-passing) language set actually contain a heavily agglutinating/polysynthetic member, or does D12 disproportionately exclude exactly that typology? | (a) proceed as today, treat D12 as a pure per-language binary gate with no typology cross-check; (b) explicitly survey the active/near-term FLEx project pipeline for the joint distribution of orthography-stability status and morphological typology before further investment in D4's most differentiating machinery; (c) treat "settled enough" as a graded, per-project literacy-committee sign-off rather than binary, to avoid a near-empty admissible set becoming a silent design constraint | A metadata survey of SIL's own project registry (or whatever registry PanGloss draws its target-language list from): count, by typology tag, how many currently-active or near-term projects have a documented settled/negotiated/contested orthography status. No new grammar work or synthetic sweep needed — this is a lookup, not an experiment. |
| T6 | Is 10^3-10^4 tokens (the low end of D15's own proposed sweep) even a meaningful test point for rung 2/3, or is it an arithmetically foregone null result regardless of language? | (a) run the sweep as proposed and report the low end honestly as possibly-uninformative rather than as evidence rung 2 lacks signal; (b) recompute the sweep's low end in terms of *contexts observed at least twice* rather than raw token count, which normalizes for class cardinality; (c) skip 10^3-10^4 for rung 2/3 specifically and start the sweep at whatever token count makes C³ contexts observable at least a few times each, given the rung's actual class count in a real grammar once one exists | Once the rung-aware class n-gram exists in `research/` (per report 22's own recommendation), report the number of distinct rung-2/3 classes it actually instantiates for a given profile, and compare *C*³ against the sweep's token counts before interpreting a low-end null result as a finding about the rung rather than about sample size. |

---

## What I want the parent session to verify before acting on any of this

1. **T2's Keyman quotes** (`leftContextCodePoints: 16, rightContextCodePoints: 0` from the March 2026
   Keyman blog post, and the `maxLeftContextCodePoints` docstring from `lexical-model-types.ts`) — both
   were fetched live this session and quoted as shown, but I did not cross-check them against a second
   independent source, and I could not determine what any *shipped* host app actually declares at
   runtime (only the tutorial's example value).
2. **The SIL "1,300 languages" figure in T5** — this is `[S]`, not `[A]`. A direct fetch of
   `sil.org/orthography` returned HTTP 403 in this session; the figure came from a search-engine
   summary only. Do not promote it to `[A]` without an actual successful fetch of the primary page, and
   treat T5's argument as resting on the *plausibility* of the correlation, not on this specific number.
3. **T4b's reliance on report 13's step-cap/timeout percentages** — these are correctly treated here as
   research-signal-only per D16 (I did not use them to claim any real grammar's actual rate), but the
   *qualitative* claim (propose+confirm's per-word cost distribution has a heavy tail with nontrivial
   mass) is the load-bearing part of the argument and is worth a second look once the multi-FST rewrite
   has any measurable cost distribution of its own.
4. **T6's arithmetic** used an illustrative, unsourced "tens" for plausible rung-2/3 class cardinality
   because I could not extract a citable number from the Hakkani-Tür/Oflazer line of Turkish
   morphological-disambiguation literature in this session (confirmed the papers exist and confirmed
   their own stated reason for decomposing tags, not a specific count). If a real number surfaces later,
   T6's combinatorial argument should be re-run with it rather than left on the illustrative figure.

---

## Parent-session verification (Opus, 2026-07-25, applied before any of this was acted on)

### T1 — conclusion confirmed, mechanism corrected `[M]`

The report argues D18's option A degrades into option B because "no parse is attempted, because
attempting one is the thing D14 shelved." **D14 did not shelve it.** D9 tier 1 is prefix-constrained
generation and tier 2 is error-tolerant traversal of a generative FST — both are **candidate
supply**. Analysing a wordform the user has already typed is a different operation, and it is not
tier 0, 1, or 2. It is `confirm` on one concrete string, and it appears nowhere in the tier
architecture at all.

So the finding is an **absence, not a shelving**: D9 enumerates supply, D18 requires diagnosis, and
the two were written five decisions apart without anyone noticing diagnosis has no home. The
report's conclusion — that the A/B choice is not currently real — is correct and now recorded in
`PLAN.md` under D18 § "Option A is not currently implementable".

Two consequences the report did not draw, both following from the correction:

1. **The fix is an addition, not an un-shelving.** Un-shelving tiers 1-2 would deliver suggestions,
   not diagnosis.
2. **Option A is cheaper than PLAN.md's own cost column claims** — that column says "D14's budget
   question returns", but D14's budget concerned unbounded generative traversal. Analysing one typed
   string is propose+confirm on bounded input. One number had been governing two different
   operations, and the correction makes A more affordable, not less.

T4b remains the honest caveat and is folded in alongside.

### T2 — verified at primary source, with one attribution correction `[A]`

Fetched and confirmed verbatim. `blog.keyman.com`, *"Creating an advanced custom lexical model with
Keyman"* (March 2026) contains exactly:

```typescript
configure(capabilities: LexicalModelTypes.Capabilities): LexicalModelTypes.Configuration {
  return { leftContextCodePoints: 16, rightContextCodePoints: 0, wordbreaksAfterSuggestions: false }
}
```

and states that *"for polysynthetic languages or those with complex morphologies, it is not
practical to list all possible word forms"* — so it is genuinely written for our case.

**Corrections:** this is a **blog post, not the official tutorial** as the report describes it. And
the report's framing of `Capabilities.maxLeftContextCodePoints` as a **host-declared ceiling** is
**not established** by this source — `configure` receives `capabilities` and returns a *requested*
`Configuration`, so 16 may be the author's choice rather than a limit. Whether the host grants more
is the open ask. Recorded that way in D8a; not promoted beyond what the source supports.

### T5 — argument recorded, magnitude not verified `[S]`

The SIL population figure could not be verified (direct fetch 403'd) and is not relied on. The
argument is recorded; the measurement named for it is a project-registry survey, which is an
internal question, not a research one.

### What the parent session added

The three findings F8 (cache), F18 (training corpus) and T2 (context window) are **one pattern**:
every fixed-size resource in this architecture is denominated in units that shrink as morphology
grows, so the languages that most need each resource are the ones least able to feed it. None of the
three reports that found the individual instances stated the pattern.
