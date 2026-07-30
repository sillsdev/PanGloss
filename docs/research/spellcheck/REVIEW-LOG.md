# Review campaign — spelling correction & word suggestion plans

**Commissioned by John, 2026-07-25:** *"review the plans for spelling correction and word suggestion
against top literature and for consistency using at most 2 sonnet subagents simultaneously. Do at
least 3 passes."* And earlier the same turn: *"Stress test the plans, not with real data (we only
have non-representative sample projects now) but with analysis, papers and other report and cross
reviews."*

This file is the campaign's index and running record. It is **not** a decision surface — decisions
live in `PLAN.md`, ideas in `00-synthesis.md`. This file records what was reviewed, by whom, what
came back, and what the parent session (Opus) accepted, rejected, or corrected.

## Standing constraints on every reviewer

1. **D16 governs.** The four sample grammars are small and unrepresentative. A measurement over them
   may motivate research; it may never narrow a design, set a default, or retire a capability.
   Reviewers use *published* corpus statistics for other languages instead.
2. **No real data.** Stress-testing is by analysis, published literature, and cross-review only.
3. **Evidence tags mandatory** — `[A]` attested externally with a real citation, `[M]` measured in
   this repo (re-cited by `file:line`), `[S]` speculative.
4. **Fabrication is the cardinal sin.** "Unverified" is a valid and valuable answer; a confident
   wrong number is not.
5. Subagents write files only. The parent session reviews every claim against source before
   folding anything into `PLAN.md`, and owns all commits.

## Passes

### Pass 1 — literature audit of the two halves (dispatched 2026-07-25)

| Report | Scope | Status |
|---|---|---|
| `19-review-prediction-model.md` | D4's two-scale class n-gram: factorization validity, EM/fractional-count training, MKN on class+morpheme vocabularies, intra-word term redundancy, self-updating feedback loops, whether "classical beats neural at low data" has aged, and what the plan never considers | dispatched |
| `20-review-correction-and-candidates.md` | The correction half: D2's missing error model, touch/keyboard models, the edit unit, **D14's 90/9/1 traffic model against published OOV curves**, error-tolerant search over a finite list in WASM, and the unspecified flagging decision | dispatched |

The two highest-stakes questions in this pass, stated in advance so the answers can be judged
against the ask rather than against themselves:

- **Is D4 a probability model or a feature combination?** The factorization
  `P(w|ctx) ≈ P(class|ctx)·P(w|class)` is exact only under Brown-style deterministic classing, and
  `class(w)` here is one-to-many by construction. If training also requires EM over the analysis
  lattice, Merialdo's negative result on unsupervised tagger training is the relevant prior art and
  the plan does not cite it.
- **Is D14's 1% uncached bucket really 1%?** For an agglutinating language a 10k-entry lexicon is
  small against published type/token curves. If the true figure is an order of magnitude larger, the
  decision to shelve runtime generation is wrong, and it is the load-bearing decision of the current
  design.

Both **COMPLETE**. Report 19 landed F1-F7; report 20 landed F8-F10, including the campaign's most
consequential finding and two holes that became decisions D2 and D18.

### Pass 2 — internal consistency and evaluation validity

**COMPLETE.**

| Report | Scope | Outcome |
|---|---|---|
| `21-review-consistency-register.md` | Mechanical contradiction register over D1-D18 plus the round-2 findings; dependency graph; D16 compliance sweep; D17 re-read | F14-F20. Independently re-derived pass-0's withheld P0-2 and P0-4; found four repo-level defects pass 0 could not (a renamed openspec change, three rotted citations, a missing table row, five leading candidates with no alternative) |
| `22-review-evaluation-validity.md` | Whether any of this is *measurable* — in particular whether synthetic sweeps can answer what D16 assigns them | F11-F13. D16 point 5 was **false**: `research/models/` holds one surface trigram, and the generator has no rung hierarchy to sweep. Produced the campaign's most durable rule (F12) |

### Pass 3 — red team and cross-review

**COMPLETE.**

| Report | Scope | Outcome |
|---|---|---|
| `23-red-team.md` | Kill the product as specified; find the next unattacked load-bearing assumption; stress D2/D17/D18, which had never been reviewed by anyone | F21-F25. D18's option A is not implementable, so the product ships as "suggest only" whether or not anyone chooses it. Mechanism corrected by the parent session, which **reversed the finding's sign** |
| `24-citation-cross-review.md` | Audit the reviewers, not the plan — every load-bearing citation in reports 19-22 and its promotion into `PLAN.md` | F26-F28. D2's headline number was two different papers. Three citations demoted, five strengthened |

**Note on a scope change.** Pass 3's original brief for report 23 was *"the steelmanned case that the
add-on loses to a simpler baseline (warm cache + surface trigram + phrase table)."* It was
re-scoped to a general red team because F8 had already broken the warm cache's premise mid-campaign
— the steelman's own baseline was no longer safe to assume. **The original question was therefore
never asked, and it is still open**: recorded here rather than quietly dropped, and it maps to
ledger row C7, whose surface-trigram floor is the same comparison.

## Pass 0 — parent session's own read (Opus, 2026-07-25, before any reviewer reported)

Recorded **before** dispatching the pass-2 consistency agent and deliberately **withheld** from its
brief, so that its contradiction register is an independent derivation. Agreement between the two
lists is evidence; disagreement is the interesting part. Compared in pass 2.

All page references were originally `PLAN.md` line numbers at commit `5ddbeea`. **They have since been converted to section references** (cross-check A found 13 that had rotted as the document grew ~500 lines during the campaign). Where a bare line number survives below it is describing the state of the document *at the time of the finding*, not a location to look up today.

### P0-1 — D9's load-bearing sentence directly contradicts D14 `[S]`

D9 § "The tiers" states, as the distinction that separates this design from the abandoned delete-table plan:
*"**The cache is of words seen, never of words constructible.**"* D14's assumed reading is that the
warm cache is **generated** at pack-build time — words nobody has seen. D14's amendment note in D9 § "The tiers" adjusts *when* tiers run but never repeals that sentence, which still stands as written.

This is not a wording nit. That sentence's argument is a termination argument (the inventory is
10^4-10^8 per stem, so materializing it does not terminate), and D14 answers it properly in
§ "Why build-time generation is safe here" — *budgeted sample, never the inventory*. The fix is to
carry D14's answer up into D9 rather than leaving two sentences that contradict each other in
the same document. An implementer reading D9 alone builds the wrong thing.

### P0-2 — D14 contains exactly the D16 violation D16 exempts it from `[S]`

D16 § "What this does and does not invalidate" says *"**D14 in particular is untouched**: the traffic model is a statement about how
people type... not derived from these grammars."* True of the traffic model. **Not true of D14's
argument for its own assumed reading.** D14 rules out the "observed-only cache" alternative with:
*"It cannot reach 10k. Report 13's corpora are 6,973 wordforms (Sena 3), 673 (Amharic), 121
(Indonesian). Three of four grammars have nowhere near 10k observed types."*

That is a sample-driven narrowing of a design, prohibited by D16 rule 2 (*absence in the samples is
not evidence of absence*) and rule 3. A real project with real text may well hold 10k observed
types. The assumed reading may still be right — but this particular argument for it is invalid
under the rule the plan adopted an hour later, and D16's exemption of D14 is therefore too broad.

Note the direction of the error: it makes the *generative* reading look more necessary than the
evidence supports. Since the generative reading is what keeps D4 load-bearing, this is a
motivated-reasoning shape and worth flagging as such.

### P0-3 — compounding bias: the generated cache is ordered by a model biased against what generation is for `[S]`

Assembled from three places that are each individually stated but never composed:

1. D14 item 1 — frequency for a *generated* entry is not observed, so ranking within the warm cache
   must come from D4's class model.
2. D15 § "Coverage does not merely gate this layer — it biases it" — a token with no analysis
   contributes no class and is silently dropped from class-LM training, and the dropped portion is
   *systematically the morphologically complex portion*.
3. D14's purpose — generation exists to supply forms the corpus does not contain, which are
   disproportionately the morphologically complex ones.

Composed: the ranker that decides which generated forms are worth shipping is trained on a corpus
whose complex half was dropped, and is then asked to rank precisely the complex forms. The bias does
not cancel; it compounds. D14 calls the build-time/query-time separation "not circular", which is
correct about *circularity* and silent about *bias*. Nothing in the plan states this composition.

### P0-4 — three D9 provisions superseded in substance but not in text `[S]`

- **D9 § "The ranking rule".** States a *binary* seen/unseen split with one large fixed
  penalty. D14 item 2 establishes three populations (typed-by-this-user > shipped-warm-cache >
  generated) and says the penalty belongs between rungs 2 and 3. D9's text is unamended.
- **D9 § "Consequences", "D4's intra-word term earns its keep."** The stated mechanism is that *runtime*
  tiers 1-2 emit zero-count forms. D14 shelves those at runtime and relocates the job to build time.
  D14 says so; D9 does not.
- **D10 body, 638-763.** Reads throughout as though tiers 1-2 are runtime concerns with thresholds
  to calibrate. D14's closing section narrows D10's scope sharply, but D10 itself carries no marker,
  so D10 read on its own overstates what must be calibrated.

Each is individually minor and jointly they are the main readability risk in the document: **the
amendments are all written at the amending decision, never at the amended one.** A reader entering
at D9 or D10 gets a superseded design with no signal that it is superseded. Recommend a mechanical
convention — every superseding decision leaves a one-line back-reference at the site it supersedes.

### P0-5 — D2 is load-bearing and does not exist `[M]`

`PLAN.md`'s decision table listed D2, at the time of this finding, as *"direction settled, not designed"*, and the document has **no D2 section**
(headings run D1, D4, D5, D3, D9, ...). Yet D4 § "The design: two n-grams at two scales" composes both of its terms into "the same
unified weighted composition as the error-model cost (D2)", and the tier architecture assumes an
error model exists. Every quantitative claim in the plan about ranking therefore rests on an
unwritten component. This is the single largest structural gap and it is assigned to reviewer 20.

## Pass 2 — the withheld-register experiment, scored

The pass-0 findings above were recorded before report 21 was dispatched and **withheld from its
brief**, so its contradiction register is an independent derivation. Both lists are now in. The
comparison is the point, and it is more informative than either list alone.

| Pass-0 finding | Report 21 | Reading |
|---|---|---|
| **P0-2** — D16 exempts D14 from the sample-narrowing rule; D14 commits exactly that violation | **Found** (finding #1, and again in its § 4 compliance sweep) | Two independent derivations, one mechanical and one argumentative, landing on the same sentence. This is the campaign's most secure finding. |
| **P0-4** — amendments are written at the amending decision, never the amended one | **Found** (finding #3, register rows 3/4/8) | Same. 21 additionally caught that the tiers *table*'s existing banner explicitly scopes itself to the table, which is why the ranking rule underneath it looked marked and was not — a detail pass 0 missed. |
| **P0-1** — D9's "cache is of words seen, never of words constructible" was never repealed | **Missed** | A single unrepealed sentence, not a cross-reference defect. Mechanical contradiction-hunting keys on citations and structure; this one is only visible if you read the sentence as an *argument* and notice D14 contradicts it. |
| **P0-3** — the compounding bias between D14's generation and D15's coverage-driven training drop | **Missed** | It is not stated anywhere; it is a *composition* of three statements that are each individually fine. No register of contradictions can find it, because no two sites conflict. |
| **P0-5** — D2 was load-bearing and did not exist | Resolved before 21 ran; 21 then correctly classified the new D2 as the document's model D17-compliant section | — |
| — | **#2** — D13 cites a renamed and philosophically reversed openspec change | Pass 0 never left the document. **Everything that required reading the repo, pass 0 missed and 21 found**: the dangling openspec path, the rotted line citations, the D8b table omission. |
| — | **#5, § 5** — D8b missing from the status table; five leading candidates with no live alternative | Same. Mechanical completeness sweeps are exactly what a fresh reader with grep does better than the author. |

**The lesson worth keeping.** The two methods are not redundant and neither dominates. A mechanical
audit finds *broken references and missing rows*; it cannot find *an argument that is wrong*. A
close argumentative read finds unrepealed claims and compositional bias; it does not notice that a
cited directory was renamed this morning. **Run both, and never let one substitute for the other.**
The overlap (P0-2, P0-4) is where confidence should be highest — and both of those are now fixed at
the *amended* site, not only the amending one.

## Findings ledger

Filled in as reports land and are verified. Nothing enters `PLAN.md` from here without the parent
session checking it against source first — the round-2 pass caught two mis-tagged claims that way
(`PLAN.md` § "Verification note"), so the check is not ceremonial.

Verdicts are read through **D17**: `BROKEN` (an argument from architecture or arithmetic) is the
only verdict that *eliminates*. `UNSUPPORTED` moves an item into the **deferred** column with a
measurement attached — it is not a disproof, and reviewers' prose sometimes reads as though it were.

| # | Finding | Report | Verified? | Disposition |
|---|---|---|---|---|
| F1 | **Lattice/fractional-count training is asserted, never designed.** "Summing over context analyses weighted by their own scores" (D4 § "Why an n-gram and not a learned ranker", restated in D15 § "The one constraint to place on the rewrite") names no procedure. The two honest readings — uniform 1/k weighting, or EM over the lattice — have very different costs and the plan picks neither. | 19 | **Yes** — quotes verified verbatim | **BROKEN as specified.** D4/D15 must name the procedure. Not an elimination of the approach; a demand to specify it. |
| F2 | **Merialdo (1994) `J94-2001` and Elworthy (1994) `A94-1009` are the governing prior art and are cited nowhere.** Both find Baum-Welch re-estimation can *degrade* tagger accuracy, with the sign of the effect depending on seed quality and seed/target similarity. | 19 | **Yes** — both are real, correctly identified; exact crossover numbers flagged unverified by the reviewer, correctly | Fold into D4 as a **named risk with a mitigation**, not a blocker. This was my own top-of-list hypothesis before dispatch and it landed. |
| F3 | **Modified Kneser-Ney is defined over integer counts-of-counts; lattice training produces fractional ones.** Zhang & Chiang, ACL 2014 `P14-1072` exists specifically to patch this; Levit et al., Interspeech 2018 independently. Plan cites Chen & Goodman and neither patch. | 19 | **Yes, and stronger than reported** — P14-1072's own stated motivating applications are *"training on uncertain data"* and *"language model adaptation"*, i.e. D4/D15 **and** D9/D10 | Adopt the expected-count generalization explicitly. This is the cleanest, most actionable finding in pass 1. |
| F4 | **The intra-word term `P(morphemes\|class)` has never been measured at the rung D4 actually uses.** | 19 | **Partly — reviewer's inference corrected.** The 93.5-100% singleton figure is *definitionally* true at rung 1 (the morpheme sequence is part of the rung-1 label, D1 § "Backoff ladder") and carries no empirical content there. The open question is rung 2, and it is genuinely unmeasured. | **UNSUPPORTED → deferred**, with a named synthetic sweep: unconditioned `P(morphemes)` vs `P(morphemes\|class)` at rung 2. Correction written into report 19 § 2b. |
| F5 | **Suggestion-acceptance feedback loop is unaddressed anywhere in the plan.** Nothing distinguishes "the user typed this" from "the system offered this and the user did not reject it", so a wrong suggestion accepted once is reinforced. Arnold, Chauncey & Gajos, **IUI 2020** (venue corrected from the reviewer's "CHI 2020") measures the precondition: suggestions bias output toward the model's own predictions. | 19 | **Yes** — paper and finding confirmed; venue corrected in place | **Genuine omission, cheap to fix now.** One bit on the accumulation record (`typed` vs `accepted-suggestion`). D15's own "cheap to honour now, expensive to retrofit" test applies. |
| F6 | **D5's evidence table cites a result with a known reproduction bug.** gzip+kNN's reported figures reflect a top-2 accuracy miscount; corrected, it reverses on at least one of its own benchmarks. TabPFN (*Nature*, 2025) additionally now beats GBDT on small tabular data, which the table's GBDT row does not mention. | 19 | **Not yet** — deferred to pass 3 cross-review (report 24) | Weakens D5's table without collapsing it. The *conclusion* (classical first at our scale) has independent support; one row of the supporting table does not. |
| **F8** | **D14's 1% uncached bucket is wrong by one to two orders of magnitude.** Published token-level OOV curves: Inuktitut **>60% OOV against a 1.3M-word vocabulary**; Finnish 20% word-level OOV at 40M training tokens; Turkish ~15% at 64k, >5% at 500k. A 10k cache cannot serve 99% of tokens in an agglutinating language. | 20 | **Yes — the Inuktitut figure confirmed verbatim at primary source** (Gupta & Boulianne, LREC 2020, `2020.lrec-1.307`). TTR figure adjusted: could not confirm 0.144, but independently found **0.1938**, which is worse for D14. Turkish figures correctly self-flagged by the reviewer as secondary-summary. | **The campaign's most consequential finding.** D14 not un-decided; the split is reclassified as an unvalidated placeholder, "shelve completely" becomes ledger row **C4** (one calibrated operating point), and D16's exemption of D14 is withdrawn. |
| F9 | **D2 was never written, and the answer was already in the repo.** `09-training-without-data.md` contains a full treatment of error models with no error corpus — MAGEC's zero-real-error-data system at ~92% of a labeled sibling, and Zarma's synthetic-corruption result where the *non-neural* baseline won. None of it had ever reached `PLAN.md`. | 20 | **Yes** — the report's citations into report 09 check out | **Promoted to D2** (decided): synthesize error pairs by corrupting the grammar's own output, with a generic weighted-Levenshtein floor as the baseline it must beat (ledger row C5). |
| F10 | **No decision specifies when a word is flagged as misspelled**, and it composes with F8 into an active harm: with tiers 1-2 shelved, cache membership is the only signal left, so "not found ⇒ flag" becomes the design by omission — flagging correctly-typed complex words hardest. | 20 | **Yes** — D9 § "Tiers govern supply, never flagging" verified; the gap is real | **Promoted to D18** (decided): a cache miss is never grounds to flag; flagging requires a completed failed parse. **Coupled to D14 — the two cannot be fixed independently.** "Never flag, suggest only" recorded as a live product option. |
| **F11** | **D16 point 5's "answerable today on synthetic data" was false.** `research/models/` holds exactly one model — a plain **surface** trigram — so there is no rung *k* in the codebase to become estimable. Separately, the generator has no nested rung hierarchy (feature richness is one Bernoulli over a flat 3-feature pool), so building the model would require pre-deciding the rung-3 encoding the sweep is supposed to discover. | 22 | **Yes — verified directly in code.** `models/` contains only `base.py` + `ngram_baseline.py`; `generator.py:106-113` builds `stem+p{n}+affixes` with affixes drawn unordered and with replacement; `generator.py:120-129` is a single Bernoulli over a flat pool | Struck the false sentence in D16 § "What replaces sample-driven design"; amended D16 point 5; recorded the per-row answerability split. **The overclaim was this document's prose — the harness's own code documents its limits honestly.** |
| **F12** | **The durable rule neither report stated:** a synthetic sweep **may eliminate a candidate but may never validate one**, because the generator's morphology is more regular than any real language — so a failure transfers and a success does not. | parent session, on 22 | n/a — it is an argument, not a citation | **Written into D16 as the replacement rule.** It is D17's asymmetry one level down, and it makes the harness useful rather than decorative: its job is to kill candidates cheaply. A sweep whose only possible outcome is "it worked" is not worth running. |
| F13 | **Weight stability is not a proxy for weight correctness.** The plan's own proposed measurement — "how large a gold set must be before grid-searched weights stop moving" — measures the wrong quantity; a grid search converges to a stable *wrong* optimum on a small non-representative validation set. Report 22 proposes a weight-*recovery* test against known synthetic ground truth instead. | 22 | Not yet re-verified in detail | **Accept in principle** — recovery against known ground truth is exactly the elimination-shaped experiment F12 licenses. Fold in pass 3. |
| **F14** | **D13's whole "this is not a new gate" argument cites an artifact that no longer exists and now argues the opposite.** `openspec/changes/certify-four-language-matrix/` was renamed to `run-synthetic-conformance-matrix` by commit `bf3d12c` on the morning of 2026-07-25 — **hours before D13 was written** — and rewritten to *"retire the 'certify a language' framing."* | 21 | **Yes — verified in-repo `[M]`.** The directory does not exist; the quoted phrase *"only when analysis-level corpus recall is complete"* appears nowhere under `openspec/`; commit `bf3d12c` confirmed by `git log`. | **BLOCKING, and it changes ownership.** D13's move was "we are declaring an existing bar, not inventing one." There is no existing bar. **The coverage gate is now D13's own requirement**, to be justified on its merits — ledger row **C8**. The substance (don't ship a speller for a language the grammar can't analyze) survives; the inheritance does not. Note the retired framing was certification against the four unrepresentative samples, so the rename was itself a D16-shaped correction. |
| **F15** | **D16's "D14 in particular is untouched" is D16 committing the violation D16 exists to prevent** — and both of the exemption's halves fail: QAC corroboration is a property of web-search traffic, and D14 *does* narrow its own design with sample wordform counts (6,973 / 673 / 121). | 21, independently matching the parent session's pass-0 **P0-2** | **Yes** — sentence located and quoted verbatim; the 6,973 argument confirmed in D14 | **The strongest agreement in the campaign.** Two independent derivations, one withheld from the other. Exemption struck in place at D16 (not only at D14), the narrowing added to the Provisional narrowings table, and the general rule written: **a meta-decision may not exempt an object-level decision it has not audited line by line.** |
| **F16** | **D9's ranking rule and all of D10 read as live design with no supersession marker**, though D14 retires most of what they say — an implementer entering at D10 would build the runtime tier-calibration harness D14 shelves. | 21, independently matching the parent session's pass-0 **P0-4** | **Yes** — confirmed; the tiers *table* carries a D14 banner that explicitly scopes itself to the table, and the very next subsection (the part D14 actually rewrites) carries none | **Second independent agreement.** Banners added at D9 § "The ranking rule", D9 § "Consequences", and the top of D10. Each states **what survives**, not only what changed — three of the four amendments changed the mechanism and left the conclusion intact. Convention adopted document-wide: § "Amendments are written at the amended site". |
| **F17** | **D9's load-bearing sentence — "The cache is of words seen, never of words constructible" — was never repealed** and directly contradicts D14, which ships a generated cache. | parent session pass-0 **P0-1**; **not** found by 21 | **Yes** — still present, unamended, until this pass | **Repealed in place.** The *termination* argument it carried is correct and is preserved; what changed is that a **budgeted sample** of constructible words terminates precisely because it is budgeted. Worth noting that the independent reviewer missed this one — mechanical contradiction-hunting found the amendments-at-the-wrong-site pattern but not the single unrepealed sentence. |
| **F18** | **Compounding bias: the generated cache is ranked by a model trained on a corpus whose complex half was systematically dropped, and is asked to rank precisely the complex forms.** D14 argues the design is "not circular" — correct about circularity, silent about bias. | parent session pass-0 **P0-3**; **not** found by 21 or any reviewer | n/a — a composition of three statements already in the plan, each verified in place | **Written into D14 as its own subsection.** Deferred, not eliminating. Its important property: **invisible to any held-out set drawn from the same corpus**, because the held-out set inherits the same drop. Detectable only against text the grammar cannot fully analyze — which makes it genuinely attackable synthetically. Research programme item **N4**. |
| **F19** | **Five "leading candidates" carry no live alternative, violating D17's own rule** — D9 (named by D17 itself and the sharpest gap), D4 (alternatives exist but are scattered across C1/C2/C3/C7 with no pointer from D4), D8b (an open spike with no stated fallback), D10 (post-D14 scope never restated), D15 (binding digest offered with no alternative). Separately, **D8b has no row in the plan's own decision-status table.** | 21 | **Yes** — D8b's absence from the table confirmed by inspection | Ledger rows **C8** (D13's bar), **C9** (D8b's spike fallback), **C10** (D9's tiering architecture) added; a reader-pointer block added under the ledger mapping every decision to its rows; D8b row added to the status table. **D10 and D15 are recorded as known remaining gaps rather than papered over.** |
| **F20** | **Three internal citations rotted as the document accreted.** D18 cites `D9:612-619`, which is now D3's CG-licensing discussion; `composite.rs:525` drifted to `:566`; D2 cited report 22 as in-flight. | 21 | **Yes — all three confirmed by direct inspection** | All three repaired, and the underlying cause addressed: **cite by section heading, not line number.** This file grew ~400 lines during the review campaign alone; absolute line numbers in it are a guaranteed future defect. |
| **F21** | **D18's option A is not implementable, so the A/B "product call" is not a real choice** — the product ships as B whatever John picks. Report 23 reached this by composing F8 with D18's silence rule. | 23, **mechanism corrected by the parent session** | **Conclusion yes, mechanism no.** 23 argued D14 shelved the parse. It did not: D14 shelves tiers 1-2, which are *candidate supply* (generation). **Analysing a string the user already typed is a different operation and appears nowhere in D9's tier architecture** — not tier 0 (cache lookup), not 1, not 2. Nothing turned it off; nothing ever turned it on. | **The sharpest finding of pass 3, and the correction reverses its sign.** The fix is an *addition* (a "tier A" diagnostic path), not an un-shelving — un-shelving supply would deliver suggestions, not diagnosis. And **option A is cheaper than the plan's own cost column says**: that column invokes "D14's budget question", but D14's budget was about unbounded error-tolerant *generative traversal*; analysing one typed string is propose+confirm on a bounded input, the most optimised path in the repo. One number was governing two very different operations. Ledger row **C11**. |
| **F22** | **`confirm` at flagging time inherits a measured heavy tail, inside a `predict()` with no host-enforced timeout.** Report 13's shape: ~9.81% timeout (Amharic), ~12.42% step-capped (Sena). D10's calibration scope was narrowed *before* D18 existed, so nothing budgets for this. | 23 (T4b) | **Yes** — the gap is real; the report 13 values are research-signal-only per D16 and are used for *shape*, not magnitude | **Folded into D18 as the caveat that keeps F21 honest.** A diagnostic path needs a circuit breaker from its first line — and **a word whose analysis the breaker cut off is a *skipped* parse, not a failed one, so it must produce silence, not a flag.** That is D18's own rule applied to its own implementation, and it is the thing most likely to be got wrong under deadline. C11 candidate (b) — batch/idle diagnosis off the keystroke path — sidesteps it entirely and is underrated. |
| **F23** | **Keyman's left context is denominated in codepoints, and its own worked example for polysynthetic languages requests 16.** One Inuktitut-scale word can exceed that alone, so D4's inter-word term may not see even one full preceding word at cold start. | 23 (T2) | **Yes — fetched and quoted verbatim at primary source by the parent session.** Attribution corrected: it is a Keyman *blog post* (March 2026), not the official tutorial as the report said. **That 16 is a *ceiling* is NOT established** — `configure` returns a *requested* configuration, so it may be the example author's choice; that is the concrete ask for the Keyman team. | Written into D8a as a third coordination item; ledger row **C12**. **The pattern is now worth naming:** this is the third instance — with F8's cache and F18's training corpus — of *a fixed-size resource sized in units that shrink as morphology grows*. The languages that most need each resource are the ones least able to feed it. Cold start is the exposed case; continuous typing is defended by a rolling buffer we own. |
| **F24** | **D2 never specifies the sampling weight over the grammar's generative output**, and it is the identical unsolved problem D14 names one section over ("frequency for a generated entry is not observed"). | 23 (T3) | **Yes** — verified; the plan solves it for the warm cache and leaves it unstated for the error model | Open item on D2. **The two are one problem and should have one answer** — and note that D14's answer (rank by D4's class model) is the one F18 shows is biased against complex forms, so importing it wholesale imports the bias. Candidate (b) — weight by **character-level** frequency from raw running text, needing no analysis and therefore immune to coverage bias — appears nowhere in the plan and is the interesting entry. |
| **F25** | **D12 (orthography must be settled) may be anti-correlated with the typology that makes this architecture worth building.** Orthography negotiation is disproportionately live for exactly the under-documented languages a morphological speller serves. | 23 (T5) | **Not verified, and not verifiable as stated.** The supporting SIL population figure is `[S]` (a direct fetch 403'd). The *argument* stands on its own; the *magnitude* is unknown | **Deferred, product-existential, and explicitly not answerable by research** — per D16 it cannot be settled from four samples. The measurement is a **project-registry survey**: of projects with a grammar complete enough to pass D13, how many have a settled orthography? That is an internal question with an internal answer, and nobody has asked it. If the overlap is small, the addressable market for *flagging* is small — which is another argument for the prediction-first shipping order. |
| **F26** | **D2's headline number is misattributed and cherry-picked.** "MAGEC reached ~92% of a labeled-data sibling from zero real error data" conflates two papers, quotes the best of three languages, and overstates the data condition. | 24 | **Yes — re-verified at primary source by the parent session.** The 64.24 / 69.47 F₀.₅ figures belong to Grundkiewicz, Junczys-Dowmunt **& Heafield**, BEA 2019 `W19-4427` (first place, low-resource and restricted tracks, W&I+LOCNESS test) — **not** the MAGEC paper, Grundkiewicz & Junczys-Dowmunt, W-NUT 2019 `D19-5546`. Both confirmed by direct fetch. Per-language ratios 75/77/92 and the dev-set permission are report 24's readings, tagged `[S]`, not independently confirmed. | **The pass-3 citation review justified itself on this one finding.** D2 stands — MAGEC's abstract says verbatim it beats SOTA for German and Russian *"without using any real error-annotated training data"*, which is exactly the claim D2 needs, at the right paper. **But the anchor is now "75-92% depending on language, ~77% for English", not "~92%".** English is the shared task's own language and is the low end. Correction box written into D2. |
| **F27** | **Two corroborating OOV citations are weaker than tagged.** The Turkish 15%@64k figures were traced to a *different paper* than the one cited (Çarki, Geutner & Schultz, ICASSP 2000, not Arısoy et al. 2006), and neither could be read — both paywalled. Hirsimäki's "40M training tokens" is contradicted by an independent source giving 96.4M for the same corpus. | 24 | **Partly** — the misattribution is report 24's tracing, and the parent session could not reach either primary text either | Both demoted in place: Turkish to `[S — UNVERIFIED]` with an explicit "do not promote without the primary text", Hirsimäki's token count hedged while its OOV direction and WER result stay `[A]`. **Neither is load-bearing** — F8's argument rests on Inuktitut, which is confirmed verbatim. The value here is that the ⚠ box no longer *looks* better-sourced than it is. |
| **F28** | **Several citations came back stronger, not weaker** — recorded because a review that only ever finds problems is not calibrated. Merialdo's crossover (~5,000 tagged seed sentences) and Elworthy's spread (95.96% best / 89.22% baseline / 66.51% worst) now have concrete anchors; Zhang & Chiang's motivating-application framing confirmed verbatim; gzip+kNN's reversal numbers confirmed exactly; TabPFN confirmed; **Zarma and Filipino confirmed at primary source with real ACL Anthology IDs**, where before they were cited only via internal report line numbers. | 24 | Report 24's own verification, spot-checked | **This is why D2 survives F26.** Its second and third legs — Zarma's synthetic-corruption result where the non-neural baseline won, and the Filipino result — got *stronger* in the same pass that broke its first leg. A decision resting on three independent citations lost one and kept two. |
| **F29** | **D18 still permits the harm it was written to prevent.** A parse fails on a *correctly-spelled* word whenever the grammar has a coverage gap, and D18's mechanism 1 flags on a failed parse. It closed the cache-miss route to a false accusation and left the coverage-gap route wide open. | 26 (cross-check B) | **Yes — verified: D18 contains no mention of coverage or grammar incompleteness at all.** | **The best single finding of the whole campaign, and it lands on the parent session's own decision.** D18 treated *grammar coverage* as ground truth about *the language* — the D16 error in a new place. D13 makes it rarer, not safe: at 95% recall, 1 word in 20 is a candidate false accusation, concentrated in the words users are least sure about. Ledger row **C13**; the honest fix ("not a word *and not near one*") reintroduces error-tolerant search and much of D14's budget problem. **Strengthens the suggest-only-first recommendation.** |
| **F30** | **"Option A is cheaper" was not established.** The parent inferred it from bounded input, leaning on an uncited "most optimised path in the repo", and glossed the 9.81%/12.42% tail measured **on the confirm path itself**. | 26 | **Yes — the criticism is correct** | Softened in place. What survives: the cost is *structurally different* from D14's, so one number should not govern both. What does not: "therefore cheaper". Bounded input does not imply a bounded tail. **N8 exists to characterise it** — and C13's fix would reintroduce the unbounded search anyway, so the two questions are less separable than the parent implied. |
| **F31** | **The "every fixed-size resource shrinks with morphology" pattern is two findings, not three.** The cache and the Keyman window are fixed-size resources in morphology-blind units; F18's training-corpus bias is *selection bias, not exhaustion*, forced into the same slogan. | 26 | **Yes** | Narrowed in place. **What survives is weaker but truer:** all three err in the direction that makes the design look adequate on simple languages. That is a reason to distrust favourable numbers, not a unified defect with a unified fix. A tidy generalization was doing work the evidence didn't support. |
| **F32** | **The correction-log-as-error-corpus argument was overstated.** No verified ground truth on the "corrected" side (users change their minds as well as fix typos); inherits the same complex-form bias for the third time; and a suggest-only stage 1 with tier 2 shelved generates few classic accept-the-fix events. | 26 | **Yes on all three** | Substantially qualified. The log is a **lead source, not a labelled corpus** — still transformative against a baseline of zero, still the argument for shipping prediction first, but **D2's synthetic corruption does not become unnecessary.** New ledger entry: whether logged pairs are *training data* or a *validation set* for the synthetic distribution (**C5** candidate d). Produced instrumentation item 6. |
| **F33** | **Instrumentation items 1 and 4 assume a Keyman accept/learn hook that does not exist.** D8a's own verified reading: *"no learn/persist/accept hook anywhere on the `LexicalModel` interface — the model is never told that a suggestion was accepted."* | 26 | **Yes — confirmed in D8a** | Reclassified from "one bit, no reason not to" to **inference, not observation**, using D8b's own mechanism (match what we offered against what later appears in context). Records a **confidence, not a bit**, and the inference quality is itself open. Items 2, 3, 5 unaffected. **Added item 6 — backspace-and-retype** — which needs no hook and is the likeliest real source of error pairs in stage 1. |
| **F34** | **N8 committed the exact D16 rule-1 violation this campaign caught once already at D14** — it measures on the four unrepresentative grammars and the parent called it "the deciding measurement". N6's "measure what cache size is needed" framing was validation-shaped despite its elimination label. | 26 | **Yes, and this one stings** | Both restated. N8 **motivates and finds shape; R1 decides** — its legitimate use is elimination (if the tail is unacceptable on grammars this small, it will not improve on complete ones). N6 reduced to a single yes/no falsification. Track N's heading corrected from "no real data at all" to "no *representative* data" — the four grammars are real, just not representative, which is what D16 actually says. |
| **F35** | **Prediction is not orthographically neutral.** "Needs no settled orthography" is true of correctness and false of effect: a predictor trained on inconsistently-spelled text offers the variants, and F5 establishes that suggestions bias what people write. | 26 | **Yes** — follows from the plan's own F5 citation | **A prediction-only product deployed into an unsettled orthography quietly becomes a standardising force.** Invisible to every metric in this plan. Not a reason to hold stage 1 back; it is a reason stage 1 is not the ethically neutral option it appears to be. Belongs in the D7/D12 conversation. |
| **F36** | **Thirteen more line-number citations had rotted — the repair pass that adopted the "cite by heading" rule did not sweep the citations that predated it.** Six share one root cause: they point ~125 lines short, landing in D1's LibLCM discussion instead of D9 or D4. One (`D14 § "Which reading is assumed"`) was a *section* reference to a heading that has never existed. | 25 (cross-check A) | **Yes — every one confirmed against the committed blob**, and the audit correctly re-verified against `git show` rather than the working tree, having detected that cross-check B was live-editing the same files | **All 13 converted to section references**, which is the durable fix — repointing to new line numbers would rot again within one editing session, as these did. Pass 0's surviving bare line numbers are now explicitly scoped as historical. **The lesson is not "we made 13 mistakes" — it is that adopting a convention does not retroactively apply it, and nobody had swept the back-catalogue.** Also verified clean by the same audit: every repo file/line citation, all IDs (D1-D18, C1-C13, N1-N9, R0-R4), every table's column count, and markdown well-formedness. |
| F7 | **Hierarchical Pitman-Yor / HPYLM appears nowhere in the plan** (confirmed by grep), despite being a leading candidate for exactly the small-data, fractional-count, uncertainty-aware-backoff problem D4 has. Also absent: CRF/MaxEnt as the *class predictor*, and copy/pointer mechanisms for the unseen-word job D9 exists to do. | 19 | **Not yet** — pass 3 | **D17 work item.** D4 is a "leading candidate" with no named live alternative. HPYLM is the obvious second entry in its ledger row. |

---

## Cross-check pass — auditing the reviewers, and the parent

Added after the campaign closed, on John's instruction to cross-check the result. Two agents:
`25-integrity-audit.md` (mechanical: rebase integrity, citation and ID consistency, table
well-formedness) and `26-audit-of-the-parent.md`.

**Report 25's outcome (F36):** 13 more rotted line citations, all now converted to section
references; everything else — repo-path citations, every decision/ledger/programme ID, every table,
markdown well-formedness, and the squash/rebase itself — verified clean. Two cosmetic items are
recorded and **not** fixed, deliberately: `### Consequences` appears three times and `### The
decision` twice in `PLAN.md`, which would collide as URL anchors but are unambiguous as written
because every citation into them names the parent decision (`D9 § "Consequences"`); and
`22-review-evaluation-validity.md` carries one stale line citation of its own. **Reports are
historical records and are not retro-edited** — only `PLAN.md` and `REVIEW-LOG.md` are living
documents. That rule is why reports 19, 20 and 23 carry parent-session correction sections appended
at the end rather than corrections applied in place.

Report 25 also did something worth noting: it detected mid-audit that report 26 was live-editing the
same files, and re-verified every finding against `git show HEAD:<path>` instead of the working
tree. Both cross-checks were dispatched simultaneously against a repository one of them was
changing, which was a dispatch error on the parent session's part; the audit caught it and worked
around it correctly.

**Report 26 exists because of an asymmetry nobody had noticed: the parent session verified every
one of the six reviewers, and nothing verified the parent.** By the end of the campaign a
substantial share of the plan's newest content was the parent's own reasoning — corrections it made
to reviewers, syntheses none of them stated, and two whole decisions. All of it unreviewed.

It was the highest-yield pass in the campaign. Of seven audited claims, **one was correct as
written, one was correct with a wrong sub-argument, four were overstated, and one was wrong** — and
the wrong one (**F29**) is arguably the most important finding in the entire body of work, because
it is a hole in D18, which the parent wrote, and which exists specifically to prevent the harm it
still permits.

**The lesson generalises past this project.** The parent session's failure mode was not sloppiness
— every individual claim was defensible in isolation. It was **tidiness**: a three-part pattern
where the evidence supported two (F31), a clean circularity-resolution that ignored three sources of
noise (F32), a "cheaper" that followed from "bounded" only if you did not look at the tail (F30).
Reviewing makes you confident, and confidence is what produces slogans. **Whoever integrates a
review needs their own reviewer**, and it should not be the same kind of reader.

## Campaign close-out — 2026-07-25

Three passes, six reviewers (19-24), plus the parent session's own pass-0 read. **28 findings**;
every load-bearing one re-verified at source by the parent session before it was allowed to change
`PLAN.md`. That check was not ceremonial: it corrected a venue, narrowed a circular inference,
reversed the sign of the campaign's sharpest finding, and caught a two-paper conflation behind the
number D2 leans on hardest.

### What the campaign changed

| | |
|---|---|
| **Decisions created** | D17 (the ledger discipline), D2 (error model — existed as a table row for the plan's whole life), D18 (when to flag — nobody had ever specified it) |
| **Decisions materially challenged** | D14 (the 1% is off by 1-2 orders of magnitude), D16 (its point 5 was false, and it exempted D14 from its own rule), D13 (the gate it inherited had already been retired) |
| **Decisions that survived attack** | D8's `.zhfst` impossibility, D8a's "must ship the engine", D1, the anytime contract, the core intra-word-recurrence insight, D5's shape |
| **Ledger rows** | C1-C12, from zero |
| **New sections** | the research programme (track N / track R), the instrumentation contract, the D17 re-read, the amendment convention |

### Three things worth carrying to the next campaign

1. **Run mechanical and argumentative audits both, and never let one substitute for the other.**
   Scored in § "Pass 2" above: the reviewer found every broken reference and missing row; the close
   read found every unrepealed claim and the one compositional bias. Neither found the other's.
2. **Verify at source before folding.** Six of the campaign's findings needed correction at the
   verification step, and two of those corrections *reversed the finding's direction*. A reviewer's
   verdict is a hypothesis about the literature, not a reading of it.
3. **The recurring defect was structural, not factual.** Amendments written at the amending site;
   a meta-rule exempting what it had not audited; a citation to an artifact renamed that morning; a
   cost number governing two different operations. None of these is a mistake about the world —
   each is a document losing track of itself as it grew. The conventions in D16 § "Amendments are
   written at the amended site" exist to make that class cheaper to avoid than to commit.

### The one finding that changes the product, not the document

D18's option A is not implementable as specified, so the plan ships as "suggest, never accuse"
whether or not anyone chooses it (F21). Combined with the three-product split in § "The research
programme", this is not a defect to fix so much as **a shipping order to adopt**: prediction first,
correction second, flagging last and only on R4 evidence — because the prediction product's
correction log is the error corpus that every later stage needs and no language in scope will ever
otherwise have.

---

## Post-campaign addendum — 2026-07-30 (report 27)

Not part of the campaign; recorded here because this file is where the series tracks **how a claim
came to be believed**, and report 27 is the first entry that produced its own evidence rather than
auditing someone else's.

**One finding belongs in this file rather than only in `PLAN.md`.** Report 27's measurement harness
produced *plausible* numbers while broken, twice, in opposite directions:

1. A depth-first walk truncated an arbitrary deep branch instead of the ranking tail, which
   manufactured a "20-50ms per confirm" figure. **That figure was reported to John before it was
   caught.** Real cost: 0.3-1.2ms.
2. A missing candidate dedupe let the confirm descent spend its whole budget inside rank 1, producing
   a clean, consistent, entirely false "0% accepted in top-3 on every grammar".

Neither looked like a bug. Both looked like findings — and one of them was pessimistic while the other
was optimistic, so no amount of "does this smell right" would have caught them both.

**What did catch them** is worth carrying as a standing requirement, alongside the campaign's three
lessons above: **a measurement harness in this series must self-check against the production path on
the same inputs, every run, and print the agreement rate.** Report 27's check runs
`FomaProposer::propose` + `confirm_all` against the harness's own walk + `confirm_all` over the same
held-out words; it agreed exactly, and its agreement rate independently reproduced report 13's Sena
coverage figure — an external corroboration that came free.

This generalises the campaign's cardinal rule. "Fabrication is the cardinal sin" was written for
citations, where the failure mode is a number with no source. For measurement the failure mode is a
number with an *impeccable* source that measures the instrument instead of the world, and the defence
is different: not verification at source, but **agreement with an independent path to the same
answer.**
