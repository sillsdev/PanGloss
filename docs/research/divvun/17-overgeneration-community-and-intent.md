# 17 — Over-generation: community, quality philosophy, and intent

**Research agent 17, PanGloss/Divvun investigation.** Builds on report `08` (over-generation is
structurally untested at the analyser level: `--ignore-extra-analyses` hardcoded org-wide, 0/1,572
YAML files use negative assertions, `lang-sme/docs/docu-sme-testplan.md:138-153` states they don't
test analysis correctness). This report investigates the *softer* question the project owner asked:
is the current state good enough by the team's own standard, and do they want something better?
All dates below are retrieval dates unless stated otherwise; today's date is 2026-07-30.

Everything marked **VERIFIED** was read directly (primary source fetched and quoted, or a raw
GitHub API / raw-markdown response inspected). Anything marked **INFERRED** is a reasonable reading
that is not itself a quote. Nothing below is invented; where a source class produced nothing, that
is stated explicitly as a negative result.

---

## 1. Their stated quality philosophy — and the crucial split

**VERIFIED.** The single most direct piece of evidence is GiellaLT's own maturity classification
page, which defines what "production-quality" means for each resource type they build.
Source: `https://giellalt.github.io/MaturityClassification.html`, cross-checked against the raw
source at `https://raw.githubusercontent.com/giellalt/giellalt.github.io/main/MaturityClassification.md`
(retrieved 2026-07-30).

For **Spell checkers**, precision/false-positives is a named, numeric criterion at every maturity
level:

> Alpha: "Coverage at least 60% of running text / false positives less than 40%"
> Beta: "coverage at least 80% / false positives is below 20%"
> Production: "Coverage at least 95% of running text / false positives less than 5%"

For **Grammar checkers**, precision is likewise named at Beta and Production:

> Beta: "the targeted errors are captured and corrected with a precision of at least" 60%
> Production: "...with a precision of at least 80%"

For **Language model** (the entry under which the morphological analyser itself is classified — the
FST that does analysis/generation) the criteria at every level are exclusively about lexicon size,
grammar completeness, and running-text *coverage* — recall-shaped measures. The word "precision"
does not occur anywhere in the Language model criteria at any of the four maturity levels
(Experiment/Alpha/Beta/Production). A resource can be certified **Production** — the highest tier,
"visible on the front page," installable via the one-click installer — while its analyser has never
had a false-positive or over-generation ceiling applied to it at all.

This is a direct, institutional answer to the owner's question about whether precision is on their
radar for the analyser: **it is not part of their own definition of "done" for that layer.** It is
explicitly part of their definition of "done" for the two layers that face the user directly
(speller, grammar checker).

This split is not accidental or merely an oversight — it is confirmed as deliberate, stated
engineering philosophy in the team's own papers (all VERIFIED by reading the full PDFs):

- Wiechetek, Pirinen, Hämäläinen & Argese, *"Rules Ruling Neural Networks"* (RANLP 2021,
  `https://aclanthology.org/2021.ranlp-1.171` / full PDF at `acl-bg.org`): *"Developing a reliable
  grammar checker with a high precision that at the same time covers a lot of errors has therefore
  been our main focus. Good precision (i.e. avoiding false alarms) is a priority because users get
  easily frustrated if a grammar checker gives false alarms and underlines correct sentences."* And
  in the conclusion: *"A higher precision, even at the cost of a lower recall, is in line with our
  objective of keeping false alarms low, so users will be comfortable using our language tools."*
- Wiechetek, Pirinen, Gaup & Omma, *"No more fumbling in the dark – Quality assurance of high-level
  NLP tools in a multi-lingual infrastructure"* (IWCLUL 2021,
  `https://aclanthology.org/2021.iwclul-1.6`): defines an explicit fp1/fp2/fn1/fn2 taxonomy for the
  grammar checker and states *"There should be a balance of correct and erroneous sentences covering
  the same phenomena so that one can test for false positives and false negatives."* Their
  regression suite (17,800 hand-marked sentences as of August 2021) exists specifically to hold the
  grammar checker's precision to a stated target: *"the overall aim for these grammar tests is to
  keep the correctness at 100%."* They also state plainly: *"Stricter rules typically lower recall to
  ensure stable precision."*
- Wiechetek, Hiovain-Asikainen, Mikkelsen, Moshagen, Pirinen, Trosterud & Gaup, *"Unmasking the Myth
  of Effortless Big Data"* (LREC 2022, `https://aclanthology.org/2022.lrec-1.125`): reports
  precision/recall for **morphological disambiguation** (the Constraint Grammar step that consumes
  the analyser's output) — 0.99/0.99 for North Sámi PoS tagging, 0.93/0.95 for morphological
  disambiguation — i.e. precision is measured and published for the *downstream, disambiguated*
  output, not the raw analyser lattice.

So the philosophy is not "over-generation is a non-issue" in a blanket sense — it is a **layered**
philosophy: the FST is allowed, by design and by omission of any tested criterion, to over-generate;
the Constraint Grammar and speller layers that a user actually sees are held to explicit, published,
regression-tested precision targets, because that is where a false positive becomes user-visible
harm.

## 2. The "nobody would type that" argument — searched specifically, and what was actually found

**Verbatim phrase: not found.** I searched the GiellaLT/Divvun documentation site, the team's own
papers (RANLP 2021, LREC 2022, IWCLUL 2021, CGMTA 2025), the `morph-test` README, and public GitHub
issues for language resembling "nobody would type that," "no one would write that," or an explicit
claim that spurious analyser output is harmless because the input string is unrealistic. **No such
statement exists in any source I could access.** This absence is itself informative and is reported
as a documented negative result per the brief's instructions.

The closest textual analogue, and it is a materially different argument, is in the `morph-test`
README (`https://github.com/divvun/morph-test`, raw README fetched 2026-07-30):

> "All languages contain a certain amount of homonymy, which makes the `-i, --ignore-extra-analyses`
> option very useful: it makes the tests pass even if there are alternative analyses of a given word
> form. That is, homonym analyses won't destroy the test results."

This is a **homonymy** argument (the word form is real and the extra analysis is a real, attested
grammatical reading of it), not a **junk-string** argument (a chance concatenation that only a
generative machine would produce). The team's own compound-error papers make clear why the two get
conflated in practice, and why the "nobody would type it" framing does not actually fit their
central failure mode:

> "Two adjacent words can either be syntactically related or erroneous compounds, depending on the
> syntax." — Wiechetek et al., RANLP 2021.

Compounding in North Sámi (and the other Sámi/Finnic languages in the infrastructure) is genuinely,
massively productive: almost any noun+noun sequence is a *structurally valid* compound candidate,
and the analyser cannot distinguish a novel-but-real compound a person actually wrote (e.g. a fresh
coinage in a news article) from an accidental adjacency of two unrelated words. This is the opposite
of "junk nobody would type" — real users type exactly these strings constantly, because natural
compounding is one of the most common ways new words enter the language. The GitHub issue evidence
in §5 below shows this directly: every reported "false positive compound" is a real sentence from a
real corpus of published North Sámi text.

**Applying the question to speller vs. analyser, as the brief asked:** the "it's rare/tolerable"
argument is far weaker for the speller than for the analyser, and the team's own maturity
classification (§1) reflects exactly that asymmetry — the speller is held to a numeric false-positive
ceiling (<5% at Production) precisely because accepting a misspelling as correct is direct,
user-visible harm (a wrong word silently endorsed). The analyser's over-generation, by contrast, is
mostly invisible to an end user because the CG layer sits between the analyser and any human, and
discards spurious readings by context before they are ever displayed — this was report 08's finding
and is confirmed architecturally in every pipeline diagram in the papers read for this report
(RANLP 2021 Figure 1; IWCLUL 2021 Figure 1; CGMTA 2025 Figure 4 all show the FST analyser feeding
several CG disambiguation/filtering stages before any suggestion reaches the user). The over-generation
argument's plausibility is real but is a property of the *pipeline architecture*, not of a claim that
the junk strings are individually implausible.

## 3. What they say they want next — roadmaps, and where the effort actually goes

**VERIFIED**, most recent primary source found: Wiechetek & Unhammer, *"Drawing Blue Lines – What can
Constraint Grammar do for GEC?"* (CGMTA 2025, `https://aclanthology.org/2025.cgmta-1.3`, dated
March 5, 2025 — the most recent dated primary source located in this investigation). Its closing
roadmap statement:

> "Looking forward, we plan to explore additional languages, cover more error types and streamline
> the rule writing process."

That is the team's own, dated, stated priority list: **more languages, more grammar-checker error
types, better rule-authoring tooling.** Tightening the FST analyser's precision does not appear on
this list. The same paper also documents a live, explicit precision-over-recall trade-off decision at
the grammar-checker level:

> "A third of the false negatives in our test are numeral phrases including the word åvtå/avta... We
> decided against performing grammar checking on this word due to its polysemy... which would lead to
> a lot of false positives."

This is the clearest instance found anywhere in this investigation of the "we accept a cost here to
avoid a worse cost there" reasoning the owner asked about — but again, it is made about the
user-facing grammar checker, where false positives are known to be seen and to frustrate users, not
about the analyser's raw over-generation, which is not discussed as a trade-off at all because it is
essentially invisible downstream.

**Where the product effort visibly goes (VERIFIED via `https://divvun.org/`, retrieved 2026-07-30):**
the site's own front page lists four product lines — Divvun Manager (install/update), the grammar
checker (MS Word, Google Docs web app), mobile keyboards, and an online speller for ~30 circumpolar
languages. All four are downstream, distribution-facing products built on top of the analyser, not
analyser-precision work itself. No blog or news section was found at divvun.org or divvun.no (searched
explicitly; only a link out to `borealium.org` was found, not itself explored further within scope).

**A 2024 paper reveals where their public argumentative energy is spent, and it is not analyser
precision.** Moshagen, Wiechetek et al., *"Indigenous language technology in the age of machine
learning"* (2024, `https://www.tandfonline.com/doi/full/10.1080/08003831.2024.2410124`; fetched and
HTML-stripped directly since the WebFetch tool could not render it, retrieved 2026-07-30) frames
their 20-year rule-based investment as an ethical and epistemic alternative to LLMs for Indigenous
languages, explicitly warning that unsupervised neural generation for Sámi produces text that "will
look Sámi to an unknowing person, but in reality often is just gibberish," with "little to no quality
assurance of the generated output" (citing Wiechetek et al. 2024). The paper is built around the CARE
and FAIR principles (data ownership/ethics), not around a self-critique of their own FST's precision.
**INFERRED:** the 2024-2025 public/academic energy of this group is directed at (a) defending the
rule-based paradigm against neural competitors on ethical and reliability grounds, and (b) expanding
language and error-type coverage — not at re-engineering the analyser's over-generation.

## 4. Has anyone proposed negative/precision testing for the analyser? — searched directly, weak signal only

Searched: GitHub issue/discussion full-text search across the `giellalt` and `divvun` organizations
for "false positive," "over-generation," and "negative test," via the GitHub API (retrieved
2026-07-30). Results:

- `giellalt/giella-core#93`, **"Overgeneration in adjectives - boundary differences"** (Bugzilla Bug
  388, opened 2007-04-12 by `@snomos` i.e. Sjur Moshagen, closed, labeled `bug` + `high priority`).
  **VERIFIED full body read via `gh issue view`.** This is a *different kind* of over-generation than
  the one central to this investigation: it is about the FST's intermediate xfst representation
  producing 32 spurious duplicate strings that differ only in boundary-marker characters (`#` vs
  `^`), a pure combinatorial/performance problem ("it takes a lot of disk space and processing") —
  not a linguistic false-analysis problem. It was treated as a high-priority bug and closed. Not
  relevant to precision/over-generation of *linguistic* analyses, but shows the team does treat some
  classes of "over-generation" as urgent when the cost is concrete (disk/CPU) rather than abstract
  (unmeasured precision).
- `giellalt/lang-sme#447`, **"false positive compound"** (Bugzilla Bug 2686) — see §5, the single most
  relevant item found.

No GitHub Discussion, issue, or paper proposing a systematic negative/precision-testing regime for
the raw analyser (of the kind report 08 found absent from all 1,572 YAML test files) was found. This
is a documented negative result: the absence of such a proposal, after 19+ years of active
development on this infrastructure and an explicit, well-resourced quality-assurance research
program at the grammar-checker level (§1, §3), is itself informative. **INFERRED:** the team has the
tooling, the corpus, the regression-testing culture (`morph-test`, the Yaml-based grammar-checker
regression suite, `divvunspell accuracy`) and the institutional appetite to build a negative-testing
regime for the analyser if they judged it worthwhile; the fact that in 19 years none of that
apparatus has been pointed at the analyser's own generation step suggests it is a considered
omission, not an oversight.

## 5. How downstream consumers cope — a live, dated, practitioner-level case study

**VERIFIED**, read in full via `gh issue view --json comments`: `giellalt/lang-sme#447`, **"false
positive compound"** (Bugzilla Bug 2686), opened 2020-09-30 by Thomas Omma, still **OPEN** as of
last update 2024-09-03 (and confirmed still open on 2026-07-30), labeled `enhancement` +
`low priority`, 31 comments.

This issue is a running log, over more than a year (Sept 2020 – Jan 2021 in the visible comments), of
individual real sentences from the North Sámi corpus where GramDivvun's compound-error module
(`msyn-compound`) wrongly suggested merging two legitimately separate words into one — i.e. exactly
the false-positive failure mode this whole investigation concerns, reported by a native-speaker
practitioner (Thomas Omma) directly against real corpus text, not synthetic examples:

> "Dan dihte lea giella ja oahpahus guovddáš fáddat mu politihkkalaš barggus... > oahpahusguovddášfáddat / should be > oahpahus guovddášfáddat" (2020-09-30)

Linda Wiechetek (lead grammar-checker researcher) responds to each report individually, sometimes
fixing it immediately (adding a missing lexicon entry, adjusting a rule), sometimes diagnosing it as
a harder "real word error" requiring more context modeling:

> "The problem here is that 'orrun' is a real word error. We could work with phonological sets... and
> then make a rule later to catch them." (2020-10-08)
> "fixed! The real word error is not corrected, but it is left as it is. So no false alarm for the
> compound anymore." (2020-10-08)
> "this is weird, it is analyzed as two words but not split up. I'll make a bug." (2020-11-03)

By January 2021 the same reports have migrated from ad hoc issue comments into the formal regression
test output format described in the IWCLUL 2021 paper (§1) — entries tagged `FAIL fp2` (a false
positive with no corresponding manual mark-up), e.g.:

> "[ 155/1323][FAIL fp2] : () => Lasáhus čuoggái:[Lasáhusčuoggái] (msyn-compound)" (2021-01-14)

**What this shows, directly and concretely:** compounding false positives are not dismissed as
implausible edge cases — they are recognized, individually triaged, frequently fixed, and eventually
folded into the automated regression suite. But the *general class* of the problem — the fact that
the compound-detection rule can always find a new real sentence to misfire on — is not solved, has
not been closed after 4+ years, and is explicitly labeled **low priority**. This is about as clean an
answer as exists anywhere in the public record to "do they know, and is it fine": **they know, in
granular and repeated detail, from a real user testing real text; they patch what they can; and they
have formally deprioritized eliminating the residual class.** That is a considered "good enough for
now," not "we didn't notice" and not "it will never come up."

**Downstream CG authors complaining about noisy analyser input specifically:** no instance was found
of a CG author, inside or outside UiT, complaining in public that the *raw analyser* is too noisy to
work with. This is consistent with the architecture: CG disambiguation absorbs the noise as a matter
of course (that is its designed job), so the cost shows up as ordinary rule-writing effort, not as a
distinct complaint. The closest thing to friction is the issue above, which is friction at the CG
*output* (grammar-checker suggestion) level, not at the analyser-input level.

## 6. Rust, foma, and interest in replacing the toolchain

**VERIFIED via GitHub API** (`api.github.com/repos/divvun/foma-rs`, retrieved 2026-07-30):
`divvun/foma-rs` is a real, active-until-recently repository. Facts directly from the API response:

- Created 2026-07-12; last push 2026-07-19T21:37:06Z; license Apache-2.0; language Rust; single
  active contributor (`bbqsrc` / Brendan Molloy, a long-standing Divvun engineer, also credited in
  the IWCLUL 2021 "fumbling in the dark" paper's acknowledgements for the original test-tooling
  setup).
- As of retrieval (2026-07-30), the repository has had **no commits in the ~11 days since 2026-07-19**
  — consistent with the brief's note that the port's activity stopped after 2026-07-17.
- The commit history is a security/robustness hardening sweep, not a precision or over-generation
  initiative: commit messages describe fixing "malloc-style crashes," buffer overflows and panics
  when loading untrusted `.foma`/`.att` binary images (`io_net_read`, `cmatrix_init`,
  `fsm_completes`), releasing patch versions 0.4.1 and 0.4.2 for these fixes, and explicitly aiming
  for behavior-preserving compatibility ("no legit file is rejected... 558 tests, including all
  save/load round-trips").

**INFERRED:** this is a memory-safety rewrite/port of foma's C internals into Rust, motivated by
robustness against malformed/untrusted input files, not by a desire to change what the toolchain
accepts as valid output. If anything, a faithful, bug-for-bug-compatible port would *preserve* the
analyser's existing over-generation behavior rather than address it — the stated goal is
compatibility, not tightening. I found no statement anywhere (commit messages, repo description — the
repo has no description field set — or elsewhere) connecting `foma-rs` to precision or over-generation
concerns. Its relevance to PanGloss is real (PanGloss depends on this exact port), but the evidence
does not support reading it as a sign that Divvun is using the rewrite as an opportunity to fix
over-generation.

## 7. Verifying the coordinator's specific GramDivvun lead

The mid-task brief asked me to verify a specific hypothesis: that Divvun recognized single-word
spellers miss context/compounding errors and built GramDivvun as a downstream, context-aware fix
rather than tightening the FST — and to check whether they explicitly frame over-acceptance of
dynamically-formed compounds as an accepted trade-off.

**Confirmed, with one correction to the hypothesis's framing.** The "downstream fix, not upstream
tightening" architecture is confirmed directly and repeatedly (§1, §3, and the pipeline diagrams in
RANLP 2021 / IWCLUL 2021 / CGMTA 2025 all show the same shape: FST analyser → several CG
disambiguation/filtering stages → suggestions). GramDivvun's own stated motivation, read directly
from its papers, is exactly what the lead describes: a grammar checker is needed because *"[it] goes
beyond spell checking by detecting context-dependent errors, agreement violations, and syntactic
problems that spellers cannot catch"* (RANLP 2021, quoted in full in §1) — i.e. an explicit statement
that the speller alone is architecturally incapable of these corrections, and CG is the intentional
answer.

Where the hypothesis needs correction: I found **no statement** that the team deliberately
*over-accepts* compounds to avoid flagging valid creative ones. What I found is closer to the
opposite default: *"All possible compounds written apart are considered to be errors by default,
unless the lexicon specifies a two or several word compound or a syntactic rule removes the error
reading"* (RANLP 2021) — i.e. GramDivvun's default stance on split compounds is aggressive
(flag first), and the false positives that result (§5, and RANLP 2021's own worked examples of
"suggestions [that] still count as false positives" for homonymous genitive/nominative compound
candidates) are a *side effect* of that aggressive default colliding with the genuine, structural
ambiguity of compounding, not a deliberate leniency. The one explicit "we chose not to flag this to
avoid false positives" statement I found (CGMTA 2025, the `åvtå/avta` polysemy case, §3) is about
excluding a specific word from a check entirely, not about tolerating compound over-acceptance in
general. The "real-word error vs. unavoidable typo" distinction the lead asked me to watch for is
explicit in their own materials — IWCLUL 2021 defines "real word errors" as a category precisely
because they are undetectable by "a traditional speller," and the team does not, anywhere I found,
conflate that unavoidable-for-any-speller category with the compounding-specific false-positive
problem that is particular to their generative FST approach. They keep the two conceptually distinct
in their own error taxonomy (orthographic / real word / morpho-syntactic / syntactic / lexical /
formatting / foreign-language / unclassified, per IWCLUL 2021 and CGMTA 2025).

## 8. Source classes that yielded nothing — documented negatives

- **Divvun/Giellatekno blog:** searched explicitly (`divvun.no`, `divvun.org`, "blog," "news").
  No blog or news section was found at either domain as of 2026-07-30; only a link to
  `borealium.org` from the divvun.org front page, not explored further (out of the scope actually
  reached).
- **Personal blogs of Trosterud, Moshagen, Pirinen, Wiechetek, Rueter:** searched by name plus
  "blog"/"precision"/"grammar checker." Pirinen's personal site (`flammie.github.io`) surfaced only
  as a mirror host for the RANLP 2021 paper PDF, not as an independent blog commentary. No other
  personal blogs were located via search.
- **Mailing lists / forum archives (GiellaLT, Divvun, HFST, Apertium):** searched for Apertium
  wiki/mailing-list commentary on GiellaLT analyser precision specifically. Found only technical
  Apertium wiki pages describing the North Sámi–Norwegian language pair's use of GiellaLT's HFST/CG
  output as a pipeline component — no editorial or critical commentary on precision from the
  Apertium community was found in accessible results. No HFST mailing-list archive content was
  located.
- **Reddit / Mastodon / social media:** searched (r/linguistics, r/conlangs, r/Sapmi-adjacent terms,
  named researchers). No relevant discussion of GiellaLT/Divvun analyser or grammar-checker
  precision was found. This matches the brief's own expectation of low yield; reported as a
  genuine negative rather than an omission.
- **HFST/Apertium community's explicit outside view of GiellaLT as a toolchain:** not found within
  the search budget of this report; the LREC 2022 paper does compare GiellaLT to Apertium and
  Hugging Face from the *inside* (their own framing, favorable), but no reciprocal outside commentary
  from Apertium developers about GiellaLT's precision practices was located.

## 9. Direct answer to the owner's two questions

**Is it good enough, by their own standard?** Yes, for the standard they actually hold themselves to —
which is not "the analyser never over-generates," it is "the tools a user directly sees (speller,
grammar checker) meet published, numeric, regression-tested precision targets." By that
self-selected standard, evidenced in their own maturity classification, their own papers' explicit
precision-over-recall design choices, and 19+ years of regression-testing investment concentrated
exactly there, the system is performing as intended. The analyser's over-generation is not being
graded as "good enough" — it is not being graded at all, by design, because it is architecturally
absorbed downstream before it reaches a user. That is a real answer to "is it fine," but it is a
narrower and more specific answer than "yes, overall, it's fine": it is "fine at the layer we chose to
measure and defend; untested and unmeasured at the layer this investigation has been probing."

**Do they want something better?** For the layers they measure, yes, visibly and concretely — the
2025 roadmap (§3) commits to more error types, more languages, better rule-authoring tooling, and the
regression-testing paper documents active, continuing precision gains at the grammar-checker level.
For the analyser's raw over-generation specifically — the thing report 08 established as structurally
untested — I found **no evidence of appetite to change it**: no proposal, no issue, no roadmap
mention, no funding description targets it, in 19 years of public record. The clearest sign is
negative-space: an organization that built an elaborate, well-resourced regression-testing and
error-taxonomy apparatus for one layer of its own pipeline, and never once pointed that same
apparatus at the layer immediately upstream, in nearly two decades. Combined with the live,
low-priority-labeled, still-open compound false-positive tracker (§5) — which shows they *feel* the
downstream cost of upstream over-generation regularly, patch it piecemeal, and have explicitly
decided the residual class does not merit a structural fix — the most honest summary is: **this is a
considered, working trade-off they are comfortable with, not an oversight and not a stated
philosophical position that "junk would never be typed."** The evidence for "we know, we've decided
it's not worth fixing at the source, and we keep mopping up downstream" is much stronger and better
documented than any evidence for either "we haven't noticed" or "we've explicitly declared it a
non-issue."

---

### Sources consulted (with retrieval dates)

- `https://giellalt.github.io/MaturityClassification.html` and raw source, 2026-07-30
- `https://github.com/divvun/morph-test` (README), 2026-07-30
- `https://giellalt.github.io/proof/spelling/testing-suggestions.html` and raw source, 2026-07-30
- Wiechetek, Pirinen, Hämäläinen & Argese, "Rules Ruling Neural Networks," RANLP 2021 (full PDF), 2026-07-30
- Wiechetek, Hiovain-Asikainen, Mikkelsen, Moshagen, Pirinen, Trosterud & Gaup, "Unmasking the Myth of
  Effortless Big Data," LREC 2022 (full PDF), 2026-07-30
- Wiechetek, Pirinen, Gaup & Omma, "No more fumbling in the dark," IWCLUL 2021 (full PDF), 2026-07-30
- Wiechetek & Unhammer, "Drawing Blue Lines – What can Constraint Grammar do for GEC?," CGMTA 2025
  (full PDF), 2026-07-30
- Moshagen, Wiechetek et al., "Indigenous language technology in the age of machine learning," 2024
  (HTML, partial extraction), 2026-07-30
- `https://divvun.org/`, 2026-07-30
- `https://github.com/giellalt/giella-core/issues/93`, full thread via `gh issue view`, 2026-07-30
- `https://github.com/giellalt/lang-sme/issues/447`, full thread (31 comments) via `gh issue view`, 2026-07-30
- `https://api.github.com/repos/divvun/foma-rs` and commit history, 2026-07-30
- GitHub cross-org issue search (`giellalt`, `divvun`) for "false positive" / "overgeneration" / "negative test", 2026-07-30
- Web searches (see individual sections) for Divvun/Giellatekno blog, personal researcher blogs,
  Apertium wiki/mailing lists, Reddit/Mastodon — documented as negative results where applicable
