# Audit of the parent — cross-check B

Adversarial audit of the *parent session's own* reasoning across the three-pass review campaign
(`REVIEW-LOG.md`, `PLAN.md`). Scope, per instructions: not the six subagent reports — those were
already checked — but the seven load-bearing claims the parent asserted on its own authority while
reviewing them. Citations are by section heading, per this session's no-line-numbers rule.

Evidence tags: `[A]` verified at primary source with a quote; `[M]` my own argument from repo/plan
text or arithmetic; `[S]` secondary/unverified. Only an argument from architecture or arithmetic
earns a **WRONG**/**BROKEN**-strength verdict; failure to find support is **UNSUPPORTED**, not
disproof.

---

## 1. Verdict table

| # | Claim | Verdict |
|---|---|---|
| 1 | D14/D18 mechanism correction (tier separation + "option A is cheaper") | **CORRECT** on the mechanism; **OVERSTATED** on the cost follow-on |
| 2 | "Every fixed-size resource here is denominated in units that shrink as morphology grows" | **OVERSTATED** — real for two of three, forced for the third |
| 3 | The three-product table (prediction/correction/flagging) | **CORRECT** on ordering and shipping logic; **WRONG** on one cell (flagging vs. D18); **OVERSTATED** on another (prediction vs. orthography) |
| 4 | The R4 circularity resolution (ship prediction, harvest its correction log) | **OVERSTATED**, close to **WRONG** — the resolution has its own unexamined selection bias and no-ground-truth problem |
| 5 | The D2 / MAGEC correction | **CORRECT** — arithmetic and framing both hold, with one minor residual caveat |
| 6 | The instrumentation contract (five items) | **WRONG** for items 1 and 4; **CORRECT** for items 2, 3, 5 |
| 7 | Track N's elimination-shaped experiments (N1-N9) | **CORRECT** for N1-N5, N7, N9; **WRONG**/**OVERSTATED** for N6 and N8 |

---

## 2. Per-claim detail

### Claim 1 — The D14/D18 mechanism correction

**What was checked.** D9 § "The tiers", D14, D18 § "⚠ Option A is not currently implementable", and
report 23's own text plus the parent's appended verification in `23-red-team.md`.

**The mechanism claim: CORRECT `[M]`.** D9's tier table is explicit that tier 0 is *"Cache of words
SEEN... hash lookup"*, tier 1 is *"Lexicon stems + grammar-generated inflections, prefix-constrained"*,
and tier 2 is *"Error-tolerant generation — the typed prefix may itself be misspelled."* All three are
candidate **supply** operations keyed on a prefix the user is still typing. None of them is "run
`confirm` on a string the user has already finished typing and see whether it parses." D18 mechanism
1 requires exactly that operation — *"an attempted parse that failed"* — and it genuinely does not
appear anywhere in D9's table. Report 23's own claim was that D14 *shelved* the parse; the text of D9
and D14 shows there was never a parse-on-a-completed-string operation in the architecture to shelve.
The correction is right, and it is right for the reason stated: D14 shelves *generation*
(tiers 1-2), and diagnosis of an already-typed string is a different operation that was simply never
built. Tier 0 does not do this job either — it is a hash membership check against a finite list, not
an analysis; D18 itself distinguishes cache-miss from parse-failure precisely because tier 0 cannot
answer "is this a word," only "is this a word we have already cached."

**The cost claim: OVERSTATED `[M]`.** The parent adds, in `REVIEW-LOG.md` F21: *"option A is cheaper
than the plan's own cost column says: that column invokes 'D14's budget question', but D14's budget
was about unbounded error-tolerant generative traversal; analysing one typed string is propose+confirm
on a bounded input, the most optimised path in the repo."* Two problems:

1. **"The most optimised path in the repo" is asserted, not cited.** A repo-wide search for that
   phrase (and its "heavily optimised" variant) turns up **only** `PLAN.md` and `REVIEW-LOG.md`
   themselves — there is no perf doc, benchmark, or commit message backing the claim anywhere in the
   codebase. It reads as rhetorical force borrowed from the general fact that propose+confirm has
   received real optimization attention (per project memory: Sena build/propose speedups, chunk
   fusion, etc.), not as a claim about *this specific per-word confirm-at-flagging-time call path*,
   which does not exist yet and has never been benchmarked in isolation.
2. **"Bounded" is a category claim, not a latency claim, and the plan's own data contradicts reading
   it as the latter.** Report 13's own numbers — quoted correctly by the parent in D18's own
   caveat — are 12.42% step-capped on Sena and 9.81% timeout on Amharic, **on the bounded,
   already-capped `confirm` pipeline**, not the unbounded generative traversal D14 shelved. "Bounded"
   here means "has a step cap and a timeout," not "fast." A nontrivial double-digit fraction of words
   already hit one of those caps in the smallest, simplest samples available. Calling the bounded
   path "cheaper" without immediately foregrounding that tail risks an implementer reading "cheaper"
   as "cheap enough for a keystroke," which the plan's own C11 ledger row and the T4b caveat it folds
   in both explicitly say is *not yet known*.

The parent does hedge this correctly one paragraph later — *"'Cheaper' is not 'free'"* — and creates
ledger row C11 with a circuit-breaker requirement specifically to keep this honest. So the underlying
document is not wrong on balance; the headline sentence in F21, read on its own, overstates what the
argument actually establishes. The narrow point (one number was wrongly governing two different
operations, so D14's argument against A doesn't transfer) is sound; the broad gloss ("cheaper") is not
fully earned by anything cited.

### Claim 2 — "Every fixed-size resource here is denominated in units that shrink as morphology grows"

**What was checked.** The passage in D8a § "The context window", the parent's addendum to
`23-red-team.md` naming F8/F18/T2 as "one pattern", and the underlying F8, F18, and F23/C12 entries in
`REVIEW-LOG.md`.

**Verdict: OVERSTATED — real for two of the three, forced for the third `[M]`.**

- **F8 (the 10k warm cache) and F23/C12 (Keyman's 16 left-context codepoints) share a genuine
  mechanism.** Both are fixed budgets denominated in a countable unit — wordform-entries, codepoints
  — and both are consumed faster as morphological productivity rises: more inflectional categories
  means more distinct realizable wordforms competing for the same 10k slots, and longer average word
  length means a fixed codepoint budget holds fewer whole words. This is the same underlying
  typological fact (agglutination/polysynthesis inflates type count and word length) expressed through
  two different fixed quantities. The generalization is earned here.
- **F18 (the compounding-bias finding) is a different mechanism wearing the same slogan.** F18's
  content, verified against D14 § "The generated cache is ranked by a model biased against what
  generation is for", is a **selection-bias** argument: a token with no analysis is silently dropped
  from class-LM training, and the dropped share is systematically the complex half. That is not a
  "fixed-size resource consumed faster" story — nothing in F18 is denominated in a unit that runs out
  as morphology grows; it is a claim about *which* tokens survive a coverage filter, not about a
  quantity being exhausted. Forcing it into the same "sized in units that shrink" frame is rhetorically
  satisfying (it completes a rule-of-three) but analytically loose: F8/F23 are exhaustion arguments,
  F18 is a sampling-bias argument. Both are real problems and both do get worse as morphology grows,
  but "gets worse as morphology grows" is a much weaker and more common property than "is denominated
  in a unit that shrinks," and the parent's sentence claims the stronger one for all three.

This matters because the slogan is now written into `PLAN.md` as settled framing (D8a's closing
paragraph) and will likely be cited as "the pattern" going forward. It should be split: two instances
of *fixed-budget exhaustion* (F8, F23/C12) and one instance of *coverage-driven training bias* (F18),
related by typology but not by mechanism.

### Claim 3 — The three-product table

**What was checked.** § "The research programme" § "First: these are three products, not one", the
table itself, D18 in full, D12, and D13's coverage discussion.

**The ordering and the shipping-order inference: CORRECT `[M]`.** The "cost of failure"
row — Low/Medium/High for prediction/correction/flagging — is well-supported: a suggestion ignored
costs nothing, a bad top-*k* costs a moment's frustration, a false accusation about a user's own
language is the specific, cited harm (`translatehouse.org`, already in D18). The dependency rows
(needs an error model, needs orthography, needs coverage, decidable from text alone) are each
individually consistent with that ordering, and "ship cheap-to-fail things first" is a reasonable,
well-supported inference from a table that is itself accurate. This is good synthesis work.

**One cell is WRONG: "Fails dangerously — a gap becomes a false accusation (D18)."** The parenthetical
reads as though D18 is the fix that prevents this. It is not, and this is a real, previously
unexamined hole in D18 itself, not merely in the table. D18 mechanism 1 is: *"An attempted parse that
failed — `confirm` was actually run for this specific word and returned an empty analysis set."* A
genuine coverage gap — a correctly spelled word whose root is known but whose construction an
incomplete grammar simply does not yet cover — produces **exactly this outcome**: `confirm` runs, and
returns empty, because the grammar cannot derive the form, not because the word is misspelled. D18 has
no mechanism anywhere to distinguish "this parse failed because the word is wrong" from "this parse
failed because our grammar is incomplete." D13's "guessed parse" mitigation does not cover this case
either — `guessed` fires when the *root* is unknown; the gap described here is a known root with an
uncovered construction on top of it, which is a total parse failure, not a guess-branch parse. So
D18, exactly as specified, **will** flag correctly-typed words as errors whenever the shipped grammar's
coverage is incomplete for that construction — which, per D13's own measured numbers (24-85% coverage
on the four samples) and its own "coverage axiom" framing (high coverage is assumed, not yet
achieved), is not a hypothetical edge case for the current state of the project. The three-product
table's citation of D18 here is accurate as a *pointer* to where this risk is discussed, but the risk
it points to is **not solved** by D18, and nowhere else in the plan is it solved either. This is the
single sharpest new finding in this audit; see § 4 below for why it should be the top priority.

**One cell is OVERSTATED: "Needs a settled orthography (D12)? No — it predicts what people write."**
True narrowly — prediction requires no error/correct dichotomy to be *defined*. But the plan's own F5
finding (already in `REVIEW-LOG.md`, from the same campaign) establishes that suggestions bias
subsequent output toward the model's own predictions (Arnold, Chauncey & Gajos, IUI 2020). If two or
more spelling conventions genuinely coexist in a language without a settled norm (D12's own scope
condition), a predictor trained on that mixed text will systematically surface whichever variant is
more frequent, and — per F5's own mechanism — using it will *reinforce* that variant over the
alternatives. That is a real, if soft, orthography-imposition effect, and it is the same ethical
exposure D12 names for flagging (*"Flagging a community member's spelling... is imposing an
orthography"*) operating through a different channel. "Needs no settled orthography" is true of
prediction's formal requirements; it understates prediction's actual normative effect on an
*unsettled* one, and the table does not flag this at all, despite the plan elsewhere (F5) having the
exact citation needed to do so.

### Claim 4 — The R4 circularity resolution

**What was checked.** § "The R4 problem, stated plainly", the instrumentation contract item 4, D2,
and D9/D14's supply architecture.

**Verdict: OVERSTATED, close to WRONG `[M]`.** The parent's resolution — ship prediction, and its
correction log ("rejected-and-here-is-what-was-typed-instead") becomes the error corpus R4 needs — is
presented as the programme's one clean answer to its one genuine circularity. Three problems the
"clean" framing does not address:

1. **The log inherits the exact compounding bias the plan already found elsewhere, applied to a new
   victim.** F18 (D14's own compounding-bias subsection) establishes that the words a coverage-limited
   pipeline handles worst are systematically the morphologically complex ones. A correction log can
   only record a "wrong→intended" pair for a word the *prediction* system actually offered a
   suggestion for in the first place (tier 0/1, per D9 — tier 2's error tolerance is exactly what D14
   shelves for Stage 1). Words the system has no supply path for at all generate **no candidate to
   reject**, hence no log entry — the identical "the ranker that decides what ships is trained on data
   that dropped precisely the complex tail" shape the plan already flagged for the warm cache (D14) and
   the training corpus (D15), now recurring a third time in the very artifact meant to fix the first
   two, and unremarked.
2. **The log's "corrected" side has no verified ground truth, unlike D2's synthetic pairs.** D2's
   corruption method produces (wrong, correct) pairs where "correct" is guaranteed by construction — it
   is the grammar's own confirmed generative output before corruption. A correction-log pair's
   "intended" side is only "whatever the user typed next" — there is no confirmation it is itself
   correctly spelled, as opposed to a second attempt that is also wrong, or an unrelated word chosen
   because the user changed their mind about content rather than correcting a typo. Synthetic
   corruption is noisy on the *source* side and clean on the *target* side; a correction log may be
   noisy on **both**. The claim that this "is precisely the (wrong, intended) pair" needed is not
   supported without an added verification step (e.g., confirm the retyped word actually parses) that
   the plan does not mention.
3. **Stage 1 as specified has no active typo-detection mechanism, so it is unclear the log captures
   spelling errors at all.** Under D14, tier 2 (error-tolerant search over a mistyped prefix) is
   shelved for Stage 1. Tiers 0-1 are prefix-constrained on the literal characters typed. A user who
   genuinely mistypes a word gets candidates keyed on their (already-wrong) prefix, which is unlikely
   to surface the intended word at all — so the observable event is not "wrong suggestion rejected,
   correct word typed instead," it is "no useful suggestion, user backspaces and retypes," which is
   difficult to distinguish in the telemetry stream from ordinary composition/revision (changing one's
   mind about wording) that has nothing to do with spelling. The instrumentation contract's item 4
   assumes this distinction is free; nothing in D9/D14 for Stage 1 actually produces it.

None of this eliminates R4's resolution — a correction log is still real signal, and D2's synthetic
approach and a future correction log are not mutually exclusive. But "its correction log *is* the
error corpus" (the plan's own words) overstates the case: it is a biased, partially-unverified,
possibly-typo-sparse supplement to D2's synthetic corpus, not a clean replacement for it, and the
plan should say so rather than treat R4 as resolved by Stage 1's mere existence.

### Claim 5 — The D2 / MAGEC correction

**What was checked.** D2's correction box in full, `REVIEW-LOG.md` F26/F28, and the arithmetic.

**Verdict: CORRECT `[A]` `[M]`.** The arithmetic checks: 64.24 / 69.47 = 0.9247, correctly rounded to
"92.5%" — this is BEA-2019's own low-resource-vs-restricted-track ratio (Grundkiewicz, Junczys-Dowmunt
& Heafield, `W19-4427`), not MAGEC's number, and the correction box states this distinction accurately.
The framing question — is comparing BEA's low-resource track to its restricted track a fair stand-in
for "synthetic vs. labeled sibling"? — holds up better than it might first appear: it is the **same
system, same authors**, entered in both tracks, so it holds architecture constant and varies only the
data condition, which is methodologically *cleaner* than comparing MAGEC (a different architecture
entirely) against an unspecified "labeled-data sibling." The parent's own text already flags the one
real softening needed — the low-resource track permitted a real annotated dev set, so "zero real error
data" overstates that comparison's condition — which is exactly right and already disclosed in the
box. The final anchor, "75-92% depending on language, ~77% for English," is properly attributed to
MAGEC's own Table 4 rather than borrowed from BEA's ratio, which is the correct fix.

**One residual, minor caveat.** The replacement figures themselves (75% German / 77% English / 92%
Russian) are explicitly tagged `[S, not independently verified]` — they are report 24's reading of a
table, not something the parent re-fetched and confirmed. The parent's own framing ("the honest
anchor is...") is careful to say this is the anchor, not that it is fully verified, and the tags are
present in the text — so this is disclosed, not hidden. But it is worth naming plainly: the number
that replaced a debunked ~92% is itself currently resting on one unverified secondary reading, and
should not harden into an `[A]` fact through repeated citation without an actual independent fetch of
MAGEC's Table 4.

### Claim 6 — The instrumentation contract

**What was checked.** § "The instrumentation contract" (five items), cross-referenced against D8a's
own reading of the Keyman `LexicalModel` interface and report 12's full-text confirmation of that
interface.

**Verdict: WRONG for items 1 and 4 `[M]`; CORRECT for items 2, 3, 5.**

Item 1 — *"A provenance bit on every accumulated wordform: `typed` vs `accepted-suggestion`...**One
bit. There is no reason not to.**"* Item 4 — *"A suggestion-outcome record: offered → accepted /
ignored / rejected-and-here-is-what-was-typed-instead... the highest-value item on the list."* Both
require the model to know, after the fact, whether a specific word the user ended up with came from
accepting one of its own suggestions. **D8a's own finding, in the same document, says this signal does
not exist:** *"There is also no learn/persist/accept hook anywhere on the `LexicalModel`
interface — the model is never told that a suggestion was accepted, so it cannot maintain this store
itself even if it wanted to."* Report 12 confirms this by reading the full `LexicalModel` interface
directly from source (`configure`, `applyCasing?`, `toKey?`, `predict`, `wordbreaker?`,
`traverseFromRoot?` — no accept/apply/learn member exists anywhere in it) and by reading the worker's
model-loading and prediction call path in full, finding no callback of any kind fired on suggestion
acceptance. D8b's claim that "the learn signal already exists, disguised as context" only rescues the
*weaker* half of item 1 — it lets the model observe that a word was typed, via `context.left` on the
next keystroke. It does not and cannot distinguish *how* that word arrived: text a user accepted from
the suggestion strip and text the user typed character-by-character produce **identical**
`context.left` strings on the next call. A heuristic (e.g., treating a single bulk multi-character
insert as "probably an accepted suggestion") is not stated anywhere, was not verified by report 12,
and would be confounded by ordinary paste operations in any case. So item 1's "one bit, no reason not
to" and item 4's "highest-value item on the list" both assume a capability the plan's own D8a section,
two headings earlier in the same file, says does not exist. This is a genuine, previously uncaught
internal contradiction — nobody cross-checked the instrumentation contract against D8a's own finding
when the contract was written, closing the campaign.

Items 2 (uncached-token counter), 3 (three-way parse-outcome counter), and 5 (per-grammar D10
operating-point record) are internal PanGloss telemetry with no dependency on any Keyman hook, and
nothing found in D8/D8a/D8b/report 12 contradicts their feasibility. These are correctly "cheap now,
expensive to retrofit."

### Claim 7 — Track N's elimination-shaped experiments

**What was checked.** § "Track N — what runs now, with no real data at all" in full, cross-referenced
against D16's corrected point 5 and D16 rule 1.

**N1-N5, N7, N9: CORRECT — properly elimination-shaped `[M]`.** N1 is infrastructure and claims
nothing. N2, N3, N4, N5 are all framed so that only a *negative* synthetic result is treated as
informative (a class model that cannot beat a surface trigram even on the generator's clean, regular
morphology will not beat it on real morphology; a grid search that cannot recover known weights even
under ideal synthetic conditions will not recover them from messier real data), which is exactly
D16-corrected's asymmetry, applied correctly. N7 is literature reading, exempt from the sweep
doctrine entirely. N9 is a question to an external team, not a sweep.

**N6 and N8 are the two the task specifically asked to check, and both have a real problem.**

- **N6 — "Cache-adequacy simulation... measure what cache size is needed for *X*% token coverage."**
  The table's own "Falsifies" column frames this correctly as elimination-only (a failure to reach 99%
  coverage even under generous synthetic assumptions eliminates "10k suffices"). But the experiment's
  own **stated goal** — "measure what cache size is *needed*" — is written to produce a positive,
  usable number (cache size *K* for productivity profile *P*), which is exactly the kind of output
  D16-corrected forbids treating as validated ("a synthetic sweep may eliminate a candidate and may
  never validate one" — D16 point 5, corrected). If N6 runs and reports "under productivity profile
  *P*, 10k entries reaches 99% coverage," nothing in the experiment's description stops that from being
  read and used as a calibration target ("so 10k is fine for languages like *P*"), which is precisely
  the illegitimate move the rule exists to prevent — success on synthetic data does not transfer, only
  failure does. The table's "Falsifies" framing is elimination-shaped in wording; the experiment's own
  operational description ("measure what is needed") is validation-shaped in substance. This should be
  rewritten to state explicitly that N6 may only ever report a *failure boundary* (sizes that fail even
  under favorable synthetic assumptions), never a "sufficient" size.
- **N8 — "The `confirm`-on-one-typed-word latency distribution... Runnable today on the existing
  grammars via `pg-cli`... it is the deciding measurement for D18's diagnostic path."** This is placed
  under the section header **"Track N — what runs now, with no real data at all"**, but "the existing
  grammars" means the four real (if tiny and unrepresentative) sample projects — Sena, Amharic,
  Indonesian, Aweti — not the synthetic generator. That is real data, not synthetic data, and D16 rule
  1 governs it directly: *"A measurement over the current grammars may motivate research. It may never
  narrow a design, set a default, fix a threshold, or retire a capability."* Calling this measurement
  **"the deciding measurement"** for a design choice (which of C11's three candidates to build) is
  exactly the forbidden verb — "deciding" is a stronger claim than "motivating," and the campaign
  already spent an entire pass-0/pass-2 finding (P0-2/F15) on catching D14 committing precisely this
  violation. N8 commits it again, one section later, inside the very table meant to enforce discipline
  going forward. There is a legitimate asymmetric argument available here — D13's own rewrite note
  argues that a complete grammar's ambiguity (and, by extension, its confirm cost) is *worse*, not
  better, than what the four incomplete samples show, which would license "bad on the incomplete
  samples transfers to at-least-as-bad on the complete grammar" the same way N6's argument transfers
  from synthetic to real. But N8 does not make that argument — it just asserts the four-sample
  measurement is "the deciding measurement," without the transfer justification that would make the
  claim legitimate under D16. As written, N8 should either be moved out of "Track N" (it is not
  synthetic), re-run against synthetic stress grammars instead, or have the D13-ambiguity transfer
  argument made explicit and the word "deciding" downgraded to "motivating."

---

## 3. Where the parent is right

This is a required section and it is not a formality — several of the parent's own contributions hold
up well against direct attack:

1. **The D14/D18 tier-separation argument (claim 1's mechanism) is genuinely good work.** It correctly
   identifies that D9's tier table is exhaustively about candidate *supply*, that D18 needs a
   fundamentally different operation (*confirm* on a completed string), and that nobody — not report
   23, not any earlier reviewer, not D9 or D18's own authors — had noticed the gap was an absence
   rather than a shelving. This survives direct attack and is a real correction, not a rhetorical one.
2. **The D2/MAGEC correction (claim 5) is careful, well-hedged, and correct.** The arithmetic is right,
   the misattribution is genuinely fixed, and the residual uncertainty (the 75/77/92 figures) is
   honestly tagged rather than smuggled in as settled fact. This is the standard the rest of the
   document should be held to.
3. **The "amendments are written at the amended site" convention and the D16/D17 discipline itself are
   sound and are being applied, not just declared.** Track N's N1-N5/N7/N9 (six of nine items) are
   correctly elimination-shaped, which shows the discipline is mostly working, not merely stated. The
   two failures (N6, N8) are real but are exceptions against a background of largely-correct
   application, not evidence the discipline itself is hollow.
4. **The three-product table's central inference — ship in order of ascending data hunger and cost of
   failure — is sound**, independent of the two cell-level problems found above. The table's
   dependency rows (error model / orthography / coverage / decidability) are each individually
   accurate; the ordering they jointly produce is a genuinely useful piece of synthesis that the
   individual reports never assembled.
5. **F8 and F23/C12's shared mechanism (half of claim 2) is real and worth keeping**, even though F18
   does not belong in the same sentence. "A fixed budget denominated in a countable unit gets consumed
   faster as morphological productivity rises" is a clean, transferable, typologically well-grounded
   observation, and it correctly predicts that the languages this project most wants to serve are the
   ones worst served by any such budget.

---

## 4. What nobody has checked yet

1. **D18's coverage-gap flagging hole (§ 2, claim 3) is, as far as this audit can tell, entirely
   new** — not raised by any of reports 19-24, not raised in pass 0, and not caught by the parent when
   writing the three-product table that gestures at it. It should be the top priority before anything
   in D18 is built: as specified, D18 mechanism 1 cannot distinguish "this word is misspelled" from
   "this word is correctly spelled but this grammar's coverage does not yet reach it," and D13's
   current coverage numbers (24-85% on the four samples) mean this is not a theoretical corner case
   for the project's present state. Nobody has proposed a fix.
2. **Whether item 1/4 of the instrumentation contract has *any* viable substitute signal.** This audit
   established that the documented Keyman contract provides none; it did not investigate whether a
   coarser proxy (e.g., logging every `predict()` call's offered distribution and later fuzzy-matching
   against what actually got typed, accepting some false-positive/false-negative rate on the
   provenance bit itself) would be worth the engineering cost, or whether this is better raised as a
   fourth Keyman coordination ask alongside #12124 and #11872.
3. **Whether N6's "measure what cache size is needed" framing has already leaked into any
   *implementation* plan** (as opposed to just this research document) as an actual target cache size.
   This audit only checked the research plan's text.
4. **The R4 correction-log bias (claim 4) has no proposed mitigation anywhere** — not a verification
   step for the "intended" side, not an accounting for which words never generate a loggable event at
   all. Nobody has scoped what it would take to make the log trustworthy rather than merely available.
5. **T4a from report 23** ("does the Layer-2 add-on actually have Layer 1's `confirm` callable at
   runtime") was marked "SURVIVES, but unspecified" and, as far as this audit found, still is — no
   decision states the runtime calling convention between the two layers, only the build-time staleness
   binding (D15). This predates this campaign's parent-session review and remains open.
6. **Nobody has audited the research programme's own internal citations for rot** the way report 21
   audited `PLAN.md`'s. This document is new (written the same day it closes the campaign) and has not
   been through the citation-cross-review pass 24 gave to reports 19-22.
7. **This audit itself is unaudited.** Per the campaign's own stated lesson (REVIEW-LOG.md,
   "Verify at source before folding"), nothing here should be folded into `PLAN.md` without the same
   check the parent gave every subagent finding.

---

## Final summary

- Claim 1 (D14/D18 mechanism): mechanism CORRECT; "cheaper" cost claim OVERSTATED (unsourced
  "most optimised path" rhetoric, tail risk under-weighted in the headline though caveated in prose).
- Claim 2 (fixed-size-resource pattern): OVERSTATED — real pattern for F8/F23, a different mechanism
  (selection bias, not exhaustion) forced into the same slogan for F18.
- Claim 3 (three-product table): CORRECT on ordering/shipping logic; WRONG that D18 prevents
  coverage-gap false accusation (it only prevents the cache-miss version); OVERSTATED that prediction
  needs no orthography (ignores the suggestion-feedback norm-imposition effect the plan's own F5
  already supports).
- Claim 4 (R4 circularity "resolution"): OVERSTATED, close to WRONG — the correction log inherits the
  same compounding bias already found twice elsewhere, has no verified ground truth on its "correct"
  side, and Stage 1 has no mechanism to reliably generate typo-correction events in the first place.
- Claim 5 (D2/MAGEC correction): CORRECT — arithmetic checks, attribution fixed, appropriately hedged.
- Claim 6 (instrumentation contract): WRONG for items 1 and 4 — both assume an accept/learn signal that
  D8a's own verified reading of the Keyman interface says does not exist. CORRECT for items 2, 3, 5.
- Claim 7 (Track N elimination doctrine): CORRECT for six of nine experiments; WRONG/OVERSTATED for N6
  (elimination-framed table cell, validation-shaped experiment description) and N8 (uses real
  four-sample data inside a section titled "no real data at all," and calls it "the deciding
  measurement" — the exact verb D16 rule 1 forbids, and the same violation class the campaign already
  caught once at D14).

**The single thing I would most want changed before this is built on:** close the D18 coverage-gap
hole (§ 2 claim 3, § 4 item 1) before anyone builds "Option A." As written, D18 does not protect
correctly-spelled words from being flagged when the grammar's coverage — not the user's spelling — is
what actually failed, which is the exact harm the entire D18/D14 review thread was created to
eliminate, and it is currently unaddressed anywhere in the plan.
