# The cascade route: xfscript precedent and the two-level seam re-examined

Research agent 9, PanGloss / Divvun-GiellaLT investigation. No code changed, no build run, no
commit made.

**Sources.** Local shallow clones (`--depth 1`) under
`C:/Users/johnm/AppData/Local/Temp/claude/C--Users-johnm-Documents-repos-LCAtom/1b5e24e2-aeac-4668-b883-e199cfb811d9/scratchpad/divvun/`:
`a2/lang-sme`, `a4/giella-core`, `a4/lang-kal`, `a4/lang-crk`, plus newly cloned this session into
`a9/lang-amh` and `a9/lang-crk-full` (unused duplicate, `a4/lang-crk` was sufficient). Org-wide
surveys used `gh api search/code` and `gh api repos/.../commits|releases` against the live
`github.com/giellalt` API (not cloned; results are live-repository facts as of 2026-07-30, subject
to GitHub code-search's known incompleteness — it indexes only default branches and is not
guaranteed exhaustive). Literature was fetched live (ACL Anthology, arXiv) this session; two PDFs
(Ritchie 1992, Karttunen/Kaplan/Zaenen 1992) could not be extracted as readable text by the fetch
tool — those citations rest on the papers' indexed abstracts/search summaries, not a full read, and
are flagged accordingly below.

Claims are marked **VERIFIED** (read directly this session at the cited `path:line` or URL) or
**INFERRED** (reasoned from verified facts). "Unknown" is stated rather than guessed.

---

## 0. Direct verdict

**The "two-level vs cascade seam" is a real but narrow theoretical fact, and PanGloss can decline
it almost entirely.** GiellaLT itself provides the proof: roughly 30 of its ~155 `lang-*` repositories
write phonology as an ordered `xfst`/`foma` **replace-rule cascade** (`phonology.xfscript`), not
`twolc`, using the exact `replace`-calculus formalism `pg-foma` already emits, compiled through the
same shared build infrastructure, to the same `.foma`/`.hfst` targets, shipping in the same release
pipeline **[V]**. This is not a toy fallback: `lang-kal` (Kalaallisut/West Greenlandic — the single
most morphologically complex language discussed in this investigation's prior reports) has run its
*entire* phonology this way for years, is being hand-edited as recently as 2026-05-22, and ships a
183 MB compiled grammar bundle and a released, packaged speller **[V]**. GiellaLT's own comment on
the mechanism (`lang-sme/configure.ac:150-161`, quoted in full in §1) frames the twolc/xfscript
choice as a **per-language authoring decision with no architectural preference attached** — both
compile through parallel branches of the identical shared Makefile machinery.

Separately, direct inspection of `phonology.twolc`'s own development history (§5) found that the
single strongest *candidate* for a genuinely-simultaneous, cascade-resistant construct in this
grammar — diphthong simplification needing to "see" a stem's gradation class — was attempted three
times as a true two-level construct and abandoned each time as unworkably messy; the shipped
solution instead uses per-suffix enumerated trigger diacritics, the same device gradation itself
uses, and that device is confirmed to transfer to the cascade world unchanged (§4, with a
live, shipped Greenlandic construction to prove it, and a live Cree before/after example of the
identical translation being done by hand). **No live, shipped rule in the deepest twolc grammar
this investigation has examined requires true rule-to-rule mutual reference that an ordered cascade
cannot express.**

So: the seam is real in the sense that Kaplan & Kay's theorem gives composition (cascade) and
intersection (two-level) as two *different* closure operations on regular relations, not
interchangeable ones in general (§2) — but it is avoidable in the sense that matters for PanGloss,
because (a) PanGloss need never consume a twolc grammar, (b) a large, real fraction of GiellaLT's own
language portfolio already declines twolc in favor of exactly PanGloss's formalism, and (c) the one
attested hard case in the flagship twolc grammar was solved by falling back to a device that is
cascade-native anyway. The seam should be downgraded from "the blocker that decides whether HC can
go" (`00-synthesis-and-decision.md`, blocker #1) to: a fact about two specific tools' compiled output
that has no bearing on which formalism PanGloss authors in, because PanGloss was never going to author
in twolc to begin with.

---

## 1. The xfscript-phonology precedent: exists, is common, and includes a flagship mature/shipped case

### 1.1 The documented mechanism

`lang-sme/configure.ac:150-161` **[V]** (verified again this session, identical to report `00`'s
citation):

```
# LEXICON_IN_PHONOLOGY
# Set this to 'yes' IFF a) your phonology is formulated using rewrite rules,
# AND b) your phonology file contains a reference to the lexical transducer in
# the following form:
#
# load stack fst/lexicon.&FST&
#
# where &FST& will be automatically replaced with the relevant fst suffix
# (hfst, foma). When done like that, the phonology rules will be composed with
# the lexicon directly, which should lead to much faster compilaton of xfst
# rewrite rules.
AC_SUBST([LEXREF_IN_XFSCRIPT], [""])
AM_CONDITIONAL([LEXREF_IN_PHONOLOGY], [test "x$LEXREF_IN_XFSCRIPT" != "x"])
```

This is explicit, first-party confirmation that GiellaLT's own build system anticipates and names
"phonology formulated using rewrite rules" as a first-class alternative to twolc. **Important
correction to a naive reading of this flag**: `LEXREF_IN_XFSCRIPT` does **not** mark "this language
uses xfscript phonology" — it marks the narrower, optional condition (b), whether the `.xfscript`
file itself directly `load`s the lexicon transducer for a faster, more compact compose-at-source
build. Direct check of three languages that *do* write phonology as xfscript (`lang-kal`, `lang-crk`,
verified below) shows all three leave `LEXREF_IN_XFSCRIPT=""` (off) **[V]**,
`lang-kal/configure.ac:158`, `lang-crk/configure.ac:158`. `lang-kal`'s own file makes this legible —
`lang-kal/src/fst/morphology/phonology.xfscript:8-20` **[V]**:

```
!! Innkommenter de følgende linjer dersom man vil kompilere reglerne direkte mot
!! leksikon-fst-en. Husk også på at endre i configure.ac:
!! AC_SUBST([LEXREF_IN_XFSCRIPT], ["yes"])
!
! load stack morphology/lexicon.&FST&
! define Lexicon ;
```
("Uncomment the following lines if you want to compile the rules directly against the lexicon FST.
Remember to also change in configure.ac...") — the lexicon-reference lines are present but commented
out, i.e. this maintainer has the option available and has not (yet) taken it. I did not find any
`lang-*` repo with `LEXREF_IN_XFSCRIPT` actually set to `"yes"` in this session — GitHub code
search's tokenizer does not support exact-phrase matching on this string reliably, so this is stated
as **not found**, not **does not exist**.

### 1.2 Org-wide survey

`gh api search/code` for `filename:phonology.xfscript` under `org:giellalt`, restricted to the
canonical path `src/fst/morphology/` (matching where `lang-sme` keeps `phonology.twolc`), returned
**30 distinct repositories** (deduplicated across two paginated queries, run 2026-07-30):

```
lang-kal   lang-crk   lang-srs   lang-esu   lang-som   lang-ciw   lang-iku   lang-epo
lang-bla   lang-hin   lang-cwd   lang-oji   lang-amh   lang-grn   lang-ces   lang-cor
lang-moh   lang-tel   lang-ron   lang-zul-x-exp   lang-ess   lang-nso   lang-ipk   lang-gle
lang-mhr   lang-tkl   lang-gup   lang-rup
```
(`lang-gle` and `lang-mhr` also separately have leftover `phonology.twolc`/`phonology.xfscript-from-apertium`
paths — mixed-transition repos, not double-counted as "clean" xfscript cases.) The same query for
`filename:phonology.twolc` at the canonical path returned on the order of 130 raw hits across ~120
distinct repos, but a size check shows most of that count is **template boilerplate, not real
grammars**: `giellalt/template-lang-und`'s own stub `phonology.twolc` is 763 bytes **[V]**, and
several `lang-*` repos that appear in the twolc list are at or near that size — `lang-eng` 1,142 B,
`lang-tha` 754 B, `lang-non` (Old Norse) 759 B, `lang-got` (Gothic) 959 B **[V]**, i.e. essentially
untouched templates for languages with no active phonology work at all. By contrast, every xfscript
repo checked has to have been *deliberately* pointed at `phonology.xfscript` (§1.3 below shows this
requires an explicit `Makefile.modifications-phon.am` edit away from the template default), so the
30-repo xfscript count is a much less inflated signal of genuine, active choice than the raw twolc
count. Sizes across the 30, spot-checked this session (bytes, `phonology.xfscript`, **[V]**):

| Repo | Size | Note |
|---|---|---|
| `lang-bla` (Blackfoot) | 19,429 B | substantial |
| `lang-srs` | 18,526 B | substantial |
| `lang-moh` (Mohawk) | 16,444 B | substantial — a canonical polysynthetic/noun-incorporating language |
| `lang-ciw` | 14,973 B | substantial |
| `lang-cwd` | 14,011 B | substantial |
| `lang-crk` (Plains Cree) | 660 lines (~15 KB) | substantial, detailed below |
| `lang-kal` (Kalaallisut) | 512 lines | mature, actively shipped, detailed below |
| `lang-cor`, `lang-gup`, `lang-ess`, `lang-oji`, `lang-ipk`, `lang-tkl`, `lang-iku` | 2.5–6.8 KB | small-to-moderate, real content |
| `lang-amh`, `lang-som`, `lang-ces`, `lang-grn`, `lang-epo`, `lang-tel`, `lang-ron`, `lang-zul-x-exp`, `lang-rup`, `lang-hin`, `lang-nso` | 0.6–2.6 KB | small / early-stage |

So the precedent spans the full maturity range from stub to production-shipped, with several
languages (Blackfoot, Cree, Mohawk, Kalaallisut) at a genuinely substantial scale, and — crucially —
**Kalaallisut and Mohawk are both polysynthetic languages with heavy derivational morphology**, the
exact typological profile this investigation's prior reports (`00`, `02`) flagged as the hardest
case for any FST-based approach.

### 1.3 Build-system confirmation: this is not dead code, and it is Foma-compilable

`giella-core/am-shared/src-morphology-dir-include.am:23` **[V]**: the phonology source variable is
generic (`GT_PHONOLOGY_MAIN`), not twolc-specific. Confirmed directly in three repos'
`src/fst/Makefile.modifications-phon.am` **[V]**:

```
lang-kal:  GT_PHONOLOGY_MAIN=phonology.xfscript
lang-crk:  GT_PHONOLOGY_MAIN=phonology.xfscript
lang-amh:  GT_PHONOLOGY_MAIN=phonology.xfscript
lang-sme:  GT_PHONOLOGY_MAIN=phonology.twolc     ! contrast, same variable name
```
`GT_PHONOLOGY_SUPPLEMENTS=` is blank in all three xfscript repos, and a direct `find -iname
'*.twolc*'` in both `lang-kal` and `lang-amh`'s full checkouts returned **zero files** **[V]** — for
these languages, xfscript is not a supplement alongside twolc, it is the *entire* phonology.

The actual compile rule, `giella-core/am-shared/xfscript-include.am:23-29` **[V]** (quoted in full,
uncommented, live):

```
####### HFST build rules: ########
.generated/%.hfst: %.xfscript $(GENDIR)
	$(AM_V_HXFST)printf "\n\nsave stack $@\nquit\n" | cat $< - \
		| "$(HFST_XFST)" -p $(MORE_VERBOSITY) $(HFST_FORMAT)

####### Foma build rules: #######
.generated/%.foma: %.xfscript $(GENDIR)
	$(AM_V_FOMA)"$(FOMA)" $(VERBOSITY) -l $< -e "save stack $@" -s
```

Contrast this with `00-synthesis-and-decision.md` §2.1's finding that the equivalent
`twolc → foma` bridge (`giella-core/am-shared/twolc-include.am:42-57`) is **commented out**,
verbatim reason given: "interfere with proper Foma builds, and do not work for the Hfst + Foma
combo." **This is the exact asymmetry the whole investigation turns on, now nailed down precisely**:
the xfscript route has a live, uncommented, standalone-foma build target; the twolc route's
equivalent was tried and abandoned. `giella-core/am-shared/src-morphology-dir-include.am:193-200`
**[V]** further shows both `CAN_HFST` and `CAN_FOMA` branches select `GT_PHONOLOGY` targets
generically by suffix substitution (`.foma.xfscript`/`.compose.foma`/`.lookup.foma`), so the xfscript
route is wired into exactly the same conditional-build machinery lang-sme's twolc route uses for
HFST, with the Foma branch simply *also* present and functional.

### 1.4 The flagship case: `lang-kal` (Kalaallisut / West Greenlandic)

`lang-kal/src/fst/morphology/phonology.xfscript` is 512 lines, ~65 named `define` rules, compiled
via one explicit ordered `.o.` composition cascade at the end of the file (lines 364–507) **[V]**.
This is the exact `pg-foma`-native shape: named rewrite rules, each an ordinary `A -> B || L _ R`
(or a disjunction of such), composed left-to-right. Representative rules, quoted verbatim **[V]**:

```
! Line 65-67, feeding-conditioned allomorphy of a fusional trigger symbol:
define g2toX [ g2 -> %^FUS [ n n g ] || [k|p|t] %^CLIT _ ,,
               g2 -> g               || Uvular [%>|%^CLIT] _ ,,
               g2 -> %^FUS g         || [ p|k ] %> _ ] ;

! Line 236-238, gemination keyed on a trigger diacritic placed by an earlier
! (lexicon-side) step and consumed here, across skippable material:
define geminationEQ l -> [ l l ], s -> [ t s ] || [ Vow|%^T ] ( %> ) _ Vow Cns %^GEMEQ %^T %> NonUvular ;

! Line 352, cleanup pass removing a doubled morpheme-boundary symbol left by
! earlier deletions:
define DobbeltmorfemgraenseVaek [ %> -> 0 || %> _ ] ;
```

The composition cascade itself (`phonology.xfscript:364-507`, 65 `.o.`-joined named rules) carries
explicit hand-authored ordering commentary that is direct evidence of real feeding/bleeding
relationships being engineered by rule *placement*, e.g. at line 467: `ttaaqAssibilering ! ... flyttet
fra linie 320 hertil for at forbygge at` ("moved from line 320 to here in order to prevent...") —
i.e. a maintainer discovered an ordering-dependent interaction and fixed it by moving the rule's
position in the cascade, the textbook SPE-style debugging move. This is production evidence that
ordered-cascade phonology handles the same "which rule sees which output" problem two-level
avoids by declaration — by explicit, documented, human-managed rule placement, exactly as HC's own
strata do.

**Maturity signal, verified via the live GitHub API (not a clone)**: commit history on this exact
file shows edits on 2026-05-22, 2026-05-08, 2026-05-07, 2026-04-28, 2026-04-26 (×2), 2026-04-25 —
i.e. actively hand-tuned within the last ~10 weeks of this research date (2026-07-30), commit
messages in Danish ("simplificeringer" = simplifications, "reorganiseret sektioner ... for
overblik" = reorganized sections for overview) **[V]**, matching the same PL-initialed,
Danish-annotated maintenance pattern report `00` found in Greenlandic's *lexc* files (`derivations
file`, five-year TIP battle). Release assets confirm this compiles and ships: `grammar-kal`'s latest
build (`grammar-kal_0.2.0-dev.20260223T183826Z+build.475_noarch-all.drb`, 183 MB, build #475) and a
`speller-kal/v1.0.3` release with macOS `.pkg`, Windows `.exe`, and mobile `.pkt.tar.zst` installers
**[V]**, all fetched live via `gh api repos/giellalt/lang-kal/releases`. **This is not an
experimental or abandoned branch of GiellaLT's toolchain — it is a released consumer product, built
entirely on xfst/foma replace-rule phonology, with no twolc anywhere in the repository.**

### 1.5 A second precedent, with a directly relevant translation example: `lang-crk` (Plains Cree)

`lang-crk/src/fst/morphology/phonology.xfscript` is 660 lines, ~33 named rules, same `!!€`
deep/surface in-file regression-test convention report `02` documented for `phonology.twolc`
**[V]** — i.e. the same per-rule testing discipline GiellaLT applies to twolc phonology is applied
here too, not a lesser standard. `giella-core/devtools/ruletest/` **[V]** (`README.txt`,
`compile-rewrite-rules.sh`, `test-rewrite-rules.sh`, `extract-rule-test-cases.sh`) is *shared,
org-level tooling* purpose-built to compile and pair-test a `GiellaLT-style phonology.xfscript` file
rule-by-rule against its `!!€` test pairs, using either the Foma or HFST compiler
(`README.txt:15-19`) — this is dedicated infrastructure investment in the xfscript route, not an ad
hoc per-language script, and its own bundled fixture is a copy of `lang-crk`'s real file
(`giella-core/devtools/ruletest/crk.phonology.xfscript`, 14.5 KB) **[V]**.

`lang-crk` also carries a directly usable **before/after translation example** of exactly the
construct this brief's Q3 asks about (§3, §4 below). `lang-crk/src/fst/morphology/phonology.xfscript:477-483`
**[V]**, quoted in full:

```
! Matching weak/strong reduplication consonant placeholder d1 with stem-initial consonant
! "ReduplCRule1"
!! __@RULENAME@__
! d1:Cx <=> _ (0:i 0:y) [ a: [ y2: | ý2 ] | â: h ] (%^IC:0) ( %-: ) %<:0 Cx: ;
!    where Cx in ( c k m n p s t w y ) ;

define ReduplRule [ [ d1 | d2 ] -> c || _ [ \%< ]+ %< c ,,
                    [ d1 | d2 ] -> h || _ [ \%< ]+ %< h ,,
                    [ d1 | d2 ] -> k || _ [ \%< ]+ %< k ,,
                    [ d1 | d2 ] -> m || _ [ \%< ]+ %< m ,,
                    [ d1 | d2 ] -> n || _ [ \%< ]+ %< n ,,
                    [ d1 | d2 ] -> p || _ [ \%< ]+ %< p ,,
                    [ d1 | d2 ] -> s || _ [ \%< ]+ %< s ,,
                    [ d1 | d2 ] -> t || _ [ \%< ]+ %< t ,,
                    [ d1 | d2 ] -> w || _ [ \%< ]+ %< w ,,
                    [ d1 | d2 ] -> y || _ [ \%< ]+ %< [ y | ý ] ]
      .o. [ [ y2 | ý2 | y3 ] -> y || [ d1 | d2 ] ?* _ ]
      .o. [ [ d1 | d2 ] -> 0 ]
      .o. [ [ y2 | y3 ] -> 0 ] ;
```

The commented-out line is the **original twolc rule**, a genuine alpha-variable (`Cx`) matching a
reduplication placeholder diacritic `d1` against a stem-initial consonant it must copy, with `where
Cx in (c k m n p s t w y)` — a 9-member correspondence class. The live rule directly below is the
**hand-translated replace-calculus equivalent**: the single alpha-variable rule was expanded, by a
human, into 9 concrete disjuncts (one `->` arm per consonant), composed with 3 cleanup passes that
remove the placeholder and normalize a secondary glide diacritic. This is not this report's
inference about how the translation *would* work — it is GiellaLT's own maintainers doing exactly
that translation, left in the source as a paper trail. It directly answers Q3's "Where…matched"
question (§3) and is additionally notable as an in-network idiom for **bounded, template-shaped
partial reduplication** (a consonant-copying trigger diacritic, not unbounded copying) — relevant to,
though outside the scope of, report `05`'s finding that `foma-rs` lacks `compile-replace`.

### 1.6 A small counter-example, for calibration: `lang-amh` (Amharic)

`lang-amh/src/fst/morphology/phonology.xfscript` is 64 lines, ~19 named rules, composed in one `.o.`
chain (`phonology.xfscript:43-63`) **[V]**. This is a plain, unremarkable SPE-style cascade (vowel
deletion/epenthesis conditioned on a placeholder `X`, palatalization before a boundary, consonant
softening word-finally) with no gradation-scale sophistication — it reads as early-stage, not a
mature grammar. Its only commit in this session's query window is the repo-wide
"`[Template merge] src/fst reorg`" commit from 2024-01-11 **[V]**, i.e. it has not been
substantively hand-edited since a mechanical reorganization — in contrast to `lang-kal`'s weekly
2026 edits. **This matters for calibrating the precedent honestly**: xfscript phonology is
*available and used* across roughly 30 languages, but maturity varies enormously, from
production-shipped (`lang-kal`) to essentially a stub with a plausible skeleton (`lang-amh`). The
existence claim (Q1) is solid; a claim that *every* xfscript repo is production-grade would not be.

### 1.7 Answer to Q1

**Exists.** Not one language, but a real cluster of ~30 across the org, spanning from stub to
production-shipped, including at least one (`lang-kal`) that is mature, large, actively maintained
within the last ten weeks, and released as a shipped consumer product, and at least one more
(`lang-moh`, Mohawk, 16.4 KB, unread in detail this session but confirmed present and substantial)
representing the same "polysynthetic, heavy-derivation" typology this investigation's own reference
grammars worry about. `lang-crk` additionally preserves a first-party, in-repository example of a
twolc alpha-variable rule being hand-translated into the exact replace-calculus idiom PanGloss would
need to synthesize mechanically. **This is a direct, load-bearing precedent for PanGloss's entire
approach running on Divvun's own infrastructure, using Divvun's own shared build tooling, unmodified.**

---

## 2. Formal relationship between the two formalisms

**Both denote regular relations; the difference is the closure operation, not the relation class.**
Kaplan & Kay 1994, *"Regular Models of Phonological Rule Systems,"* Computational Linguistics
20(3):331–378 (ACL Anthology `J94-3001`) **[V, abstract and framing directly confirmed via ACL
Anthology this session]**: the paper's stated contribution is a single mathematical framework
(regular languages/relations) under which **both** an ordered cascade of context-sensitive rewrite
rules **and** Koskenniemi's two-level formalism are shown to denote regular relations, supporting
"efficient generation and recognition" for either. The two constructions used to get there are
different:

- **Cascade → composition.** A cascade of *n* rules, each individually a regular relation
  (Kaplan-Kay's own earlier `replace`-operator result licenses each rule's own regularity), composed
  sequentially (`R1 .o. R2 .o. … .o. Rn`) yields one regular relation, **provided each rule applies
  obligatorily, directionally, and non-recursively into its own unbounded output** — regular
  relations are closed under composition unconditionally; the caveat is about what counts as *one
  rule step* being itself regular, not about closure. (This precise caveat, and its practical
  failure mode for genuinely self-feeding `Iterative` rules, is report `05`'s §5, independently
  re-derived and not repeated here.)
- **Two-level → intersection.** A set of *m* two-level rules, each an **equal-length relation**
  over the alphabet of lexical:surface symbol pairs (because twolc's `0`s are ordinary symbols, not
  epsilons — this is the mechanism that makes intersection well-defined at all: two arbitrary
  finite-state transducers cannot in general be intersected, but Koskenniemi's same-length
  constraint transducers can), is intersected (`R1 ∩ R2 ∩ … ∩ Rm`) to yield the grammar's single
  regular relation. This equal-length/same-alphabet framing, and the point that general FST
  intersection is not guaranteed regular while twolc's restricted equal-length case is, is
  independently corroborated by this session's web search of the standard two-level-morphology
  literature summary (not itself a primary citation, but converging with Kaplan & Kay's own
  treatment) **[A]**.

**Composition ≠ intersection, and this has one concrete practical consequence, not several.**
Ritchie 1992, *"Languages Generated by Two-Level Morphological Rules,"* Computational Linguistics
18(1):41–59 (ACL Anthology `J92-1003`) formalizes the four twolc rule types this brief names —
**context restriction, surface coercion, composite, and exclusion rules** — matching `=>`, `<=`,
`<=>`, `/<=` respectively (terminology cross-checked via search-indexed summary of the paper; **the
PDF itself did not extract as readable text this session, so the paper's specific proofs are cited
by title/venue/summary only, marked [A], not independently re-derived**). The generative-power
result reported in secondary literature is that the languages generated by two-level rule systems
are **closed under intersection but not under union or complementation** — i.e. two-level's
declarative "all constraints hold at once" semantics is a genuinely narrower operation than what an
arbitrary rewrite cascade's output relation can express, in the specific sense that stacking more
two-level rules can only ever *shrink* the accepted-pairs set (pure conjunction), whereas a cascade
composed of a rule that both deletes and reintroduces material is not describable as any such
shrinking sequence.

**Is there a general mechanical translation in either direction? No — and GiellaLT's own build
history contains a real, abandoned attempt at exactly the twolc→foma direction.**
`giella-core/am-shared/twolc-include.am:42-57` **[V, re-confirmed this session, same text as report
`00`'s citation]** is the disabled code: split the twolc rule transducer into `RULE_PARTS`,
pairwise-`hfst-intersect` them together, dump to AT&T text, and `read att` that dump into standalone
foma. The comment states this "interfere[s] with proper Foma builds, and do not work for the Hfst +
Foma combo" — i.e. GiellaLT's own engineers tried the general mechanical bridge from an intersected
two-level rule set into a foma-loadable network and abandoned it as unreliable in their own build,
not merely as theoretically absent. This is concrete, first-party evidence, not an artifact of this
report's own reasoning, that the twolc→foma direction has no working general bridge in the tool
ecosystem PanGloss would actually use.

The reverse direction — cascade → two-level — has no claimed general construction anywhere this
session found either, and the theoretical obstruction is exactly Ritchie's closure asymmetry: a
cascade's composed relation can encode transformations (net insertion/deletion sequences, iterated
rewriting) that a pure intersection of same-length constraints cannot represent without
**introducing an intermediate level as a new same-length "tape"** — which is precisely what
Karttunen, Kaplan & Zaenen's *"Two-Level Morphology with Composition,"* COLING 1992:141–148 (ACL
Anthology `C92-1025`) is understood to address: an alternative **compilation strategy** that computes
the two-level intersection via a composition pipeline over multiple lexical-representation levels,
rather than via Koskenniemi's original single-pass parallel intersection. **This paper is
frequently miscited as "proof that two-level rules are secretly a cascade"; it is not that — it is a
different way to *compute* the same intersected relation, using composition as an implementation
technique, with the levels introduced as an engineering device rather than as linguistically
meaningful strata.** (**[A]**, based on the paper's search-indexed abstract and secondary summaries;
the PDF did not extract as readable text this session, so I could not independently verify the
precise formal claim beyond this framing — flagged rather than overclaimed.)

**Practical meaning for PanGloss.** Since PanGloss authors HC grammars (feature-structure,
alpha-variable, ordered-stratum — report `05`'s C1–C22 inventory) and lowers them to a foma
replace-calculus cascade, and never needs to *read* a twolc file, the composition-side of Kaplan &
Kay's theorem is the only one PanGloss's own compiler needs to rely on, and report `05`'s §5 already
established the conditions under which that holds for HC's own rule shapes (obligatory, directional,
non-self-recursive). The intersection side, and its non-translatability to/from composition in
general, is a fact about *twolc*, a formalism PanGloss does not use and need not consume.

---

## 3. Construct-coverage table: twolc facility → replace-calculus equivalent → verdict

Built against the real `lang-sme/src/fst/morphology/phonology.twolc` (1,781 lines, 112 named rules,
verified operator counts this session: **164** `<=>`, **23** `=>`, **1** `<=`, **0** `/<=`
(`grep -c`, this session) — i.e. the biconditional composite rule is overwhelmingly the dominant
operator in the one large twolc grammar this investigation has read line-by-line, and the pure
exclusion operator `/<=` is **entirely unused**).

| twolc facility | Replace-calculus equivalent | Verdict |
|---|---|---|
| `<=>` (composite: correspondence obligatory **and** restricted to given context) | Ordinary directional `->` rule: obligatory replacement already forces the correspondence in-context, and simply not adding any other rule producing that correspondence elsewhere achieves the restriction. This is the default shape of essentially every rule in `lang-kal/phonology.xfscript` and `lang-crk/phonology.xfscript` read this session. | **Direct, unremarkable equivalent.** 164/188 (87%) of lang-sme's live rule instances use this operator; it is the operator a plain `->` rule already models. |
| `=>` (context restriction: correspondence permitted only in context, not obligatory elsewhere) | `(->)` optional-replace notation, or simply omitting the rule (if nothing else could produce the correspondence, restriction is vacuous), or an explicit filter composed afterward that rejects any occurrence of the correspondence outside the licensed context. | **Direct equivalent, more verbose.** foma's own optional-replace syntax `a (->) b || L _ R` is documented in `pg-foma`'s own `rewrite.rs:1889` comment (`"Optional replacement `a (->) b`: both a and b are valid outputs"`) **[V]** — i.e. the Rust port already implements the needed primitive. |
| `<=` (surface/output coercion: input must correspond to given output whenever in context — i.e. a prohibition on the elsewhere-form) | Obligatory `->` rule that rewrites every other candidate correspondence to the required one in that context; equivalently, a rule targeting the *complement* set. | **Direct but asymmetric-effort equivalent** — coercion rules are naturally phrased as "this is the only value alternatives can take here," which a replace rule expresses as a positive statement about the required output, same mechanism as `<=>` from the other side. Used only **once** in lang-sme's shipped grammar (`grep -c ' <= ' phonology.twolc` this session), so this is a low-stakes translation question for this grammar specifically. |
| `/<=` (exclusion: correspondence forbidden in given context) | A negative-context `->` rule variant, or (equivalently) simply never writing a rule that produces the correspondence there. | **Unexercised in the reference grammar** (0 occurrences, verified this session) — no real-world instance to validate the translation against; theoretically straightforward as the mirror image of `=>`. |
| `Where … matched` alpha-variables (twolc's positional lockstep correspondence lists, 65 occurrences in `phonology.twolc`) | **No native equivalent** — replace calculus has no shared-variable binding construct. The resolution, demonstrated live in production (`lang-crk/phonology.xfscript:477-483`, §1.5), is **enumeration**: expand the variable's domain into one concrete disjunct per member, joined with `,,` inside a single `define`, exactly mirroring `pg-foma`'s own P6 tuple-enumeration strategy for HC alpha-variables (report `05` §3, Amharic's 312-tuple result). | **Requires enumeration; no shortcut.** This is the one facility in the table with a real cost, and it is the *same* cost PanGloss's own compiler already pays for HC alpha-variables — not a new problem the twolc route introduces. Domain sizes in `phonology.twolc`'s `matched` rules are small (observed: 2–10 members per variable, e.g. `Cx in (z m h p g b d)`, `Cx in (c k m n p s t w y)`), so the enumeration blow-up (report `05`'s alphabet-size concern) does not bite here — it bites when the *alphabet*, not the number of `matched` rules, is large (Amharic's 417-segment case), which is an orthogonal axis. |
| `Sets` declarations (named symbol-class abbreviations, e.g. `Vow`, `Cns`, `WeG`, `StemCns`) | foma/xfst `define Name [ … ] ;` — literally the same abbreviation mechanism, used identically in every xfscript file read this session (`define Vow [...]`, `define Cns [...]` in `lang-kal`, `lang-amh`, `lang-crk`). | **Lossless, trivial, already the native idiom.** Not a translation at all — both formalisms use the identical named-class convention; twolc's `Sets` block is textually renamed to a `define`, nothing more. |
| Deletable trigger diacritics mapped to zero (`X1:0` … `W9:0`, ~30 symbols) | Ordinary alphabet symbols, attached in the lexicon (or, in the cascade world, insertable/attachable by an earlier rule step), read as literal context by a later rule, then deleted by a final cleanup rule (`Trigger -> 0`). | **Confirmed to translate unchanged — this is not a hypothesis, it is observed, shipped code.** `lang-kal/phonology.xfscript` uses the *identical* pattern with its own trigger-diacritic family (`%^GEM`, `%^GEMS`, `%^GEMEQ`, `%^GEMC`, `%^Loan`, `%^T`, `%^ProgI`, etc. — `Dummy` set defined at `phonology.xfscript:49`), consumed by rules reaching across skippable material (`geminationS`, `phonology.xfscript:240-249`) and deleted by explicit cleanup rules at the end of the cascade (`InflBorderDel`, `DummyDeletion`, `ProgIVaek`, `phonology.xfscript:50,356-357`). Full construction in §4. |
| Archiphoneme convention (`º`, `¤`, `e7/i7/o7/u7`, `g8/h8/m8/n8`, `b9/d9/g9…`) | Ordinary lexical symbols, distinguished purely by spelling convention at lexicon-authoring time, read as left-context by rules, deleted or realized by the same rules. | **Lossless, zero-cost — this is just a naming convention for a segment, not a formal device.** Nothing about it is twolc-specific; any lexc-based lexicon (which `pg-foma` already emits) can spell a stem with an archiphoneme letter today. |
| Six-way boundary symbols (`« » %< %> # %^ ∑`) distinguishing prefix/suffix/inflectional/derivational/word/compound edges | Ordinary alphabet symbols inserted at lexc-compile time at the relevant morpheme boundary, visible to rule contexts exactly as in twolc. | **Lossless, and independently confirmed live**: `lang-kal/phonology.xfscript:48-51` defines its own `Border`/`Dummy` boundary-symbol sets (`%>`, `%<`, `%^ALTINF`, `%^CLIT`, etc.) used identically to lang-sme's `«`/`»`/`%<`/`%>` in rule contexts throughout the file. |
| `--resolve` and conflict resolution between rules (per HFST documentation found this session: right-arrow conflicts auto-resolved, left-arrow conflicts not, by default) | **No equivalent construct exists, because the situation it resolves cannot arise the same way in a cascade.** `--resolve` arbitrates the case where two *independent* two-level rules issue contradictory verdicts about the *same* lexical:surface correspondence in the *same* context — a genuine artifact of intersecting several authors-didn't-coordinate constraints simultaneously. In a cascade, only one rule ever owns the transformation of a given intermediate string at each step; there is no possibility of two rules disagreeing about "what happens now," because the author has already fixed the order. | **Not a gap — a difference in where the same authorial decision gets made.** Two-level's `--resolve` is an automatic fallback for authors who did not fully partition their rules' domains; a cascade requires the author to make that same partitioning decision explicitly, up front, as rule order. This is arguably *more* transparent for the cascade author (the order is visible in the file), not less expressive. |

**Overall Q3 verdict:** every named twolc facility either (a) has a direct, same-cost replace-calculus
idiom, (b) is a pure notational convention with zero formal content (Sets, archiphonemes, boundary
symbols), or (c) requires enumeration (alpha-variables) — and (c) is the *same* cost PanGloss's
compiler already pays for HC's own alpha-variables, not a new tax the twolc route would introduce.
Nothing in this table blocks a cascade from expressing what `phonology.twolc` expresses.

---

## 4. The trigger-diacritic construction, written out for the cascade world

This is the mechanism prior work (`02-north-sami-morphophonology.md` §3–4) identified as the closest
structural match to HC's MPR gating, and the one PanGloss most wants to adopt. It is **confirmed, not
merely inferred, to transfer to the ordered-cascade world**, because a mature GiellaLT xfscript
grammar already does it, in production.

**The construction (as done in `lang-kal/phonology.xfscript`, generalized):**

1. **Attachment.** At lexicon-authoring time (in the lexc source, or — for a synthesized/compiled
   grammar — at the point in the rule compiler that would otherwise emit the affix's surface form),
   a distinguished, otherwise-unused symbol is attached to the affix that requires a particular
   phonological effect on the stem. In `lang-kal`, this is symbols like `%^GEMS`, `%^GEMEQ`,
   `%^GEMC`, `%^Loan`, `%^T` — declared as ordinary alphabet members, part of the `Dummy` set
   (`phonology.xfscript:49`, `define Dummy [ «|»|%^|%^ALTINF|...%^Loan|...%^T|... ]`).
2. **Transport.** The symbol rides on the tape, unmodified, through however much intervening
   material (stem-final consonants, other morpheme boundaries) separates it from the site where it
   needs to have an effect — exactly as `lang-sme`'s `X4`/`Q7`/etc. ride from the suffix back to the
   gradation site across `%>:`, `StemCns:`, and boundary symbols.
3. **Consumption.** A later rule in the cascade writes its context to explicitly include the trigger
   symbol, at whatever distance the grammar needs, and performs the phonological change only when it
   is present. `lang-kal/phonology.xfscript:240-249` **[V]**:
   ```
   define geminationS (r) g -> [ k k ],
                          j -> [ t s ],
                          m -> [ m m ],
                          n -> [ n n ],
                    [ n g ] -> [ n n g ],
                          q -> [ q q ],
                          r -> [ q q ],
                          s -> [ t s ],
                          t -> [ t t ],
                          v -> [ p p ] || [ Vow|%^T ] ( %> ) _ Vow (Cns) %^GEMS %^T %< NonUvular ;
                          ! geminating stops
   ```
   Here `%^GEMS` sits several segments to the *right* of the gemination site in the rule's own
   right-context, exactly the "skip across intervening material to find the trigger" shape
   `phonology.twolc`'s `WeG`-conditioned gradation rules use.
4. **Deletion.** Once every rule that needed to consult the trigger has run, a cleanup rule near the
   end of the cascade deletes it, so it never reaches the surface. `lang-kal/phonology.xfscript:50,
   356-357` **[V]**:
   ```
   define InflBorderDel [ %^GEM|%^GEMC|%^GEMEQ|%^GEMS|%^Loan ] -> 0 ;
   ...
   define DummyDeletion [ Dummy -> 0 ] ;
   ```
   and these deletion rules are placed **late** in the `.o.`-chain (`phonology.xfscript:385, 507`),
   i.e. after every rule that needs to read the trigger has already had its turn — the cascade's
   ordering *is* the mechanism that guarantees "read before delete," a guarantee twolc's parallel
   model gets for free (nothing is ever "deleted before being read" in an intersection, since there
   is no time axis) but which a cascade author must simply get right by placement, and does, here.

**Applied to HC's MPR gating (the actual PanGloss use case):** an HC rule gated by an MPR feature
that a morpheme upstream sets and a rule downstream checks compiles to exactly this pattern: emit a
distinguished symbol at the gating morpheme's affix boundary in the lexc output, write the gated
phonological rule's context to require that symbol within its reachable window, and add a
cleanup/deletion pass after all gated rules have run. This requires **no new formal machinery
beyond what `pg-foma` already has** — ordinary alphabet symbols, `->` rules, and rule ordering — and
is now confirmed by a shipping example rather than resting on structural analogy alone.

**One caveat carried over from report `05` (`gate.rs`, `mpr-overwrite-encoding-research.md`):**
`00-synthesis-and-decision.md` §4 blocker 5 documents that a flag literal placed **inside** a `foma-rs`
`->` rule's own `||` context triggers a vendored-toolkit defect (nondeterministic apply or minimizer
crash). The construction above uses **ordinary symbols**, not flag diacritics, inside the rule
context — `%^GEMS` is a plain multichar symbol like any tag, not a `@U.../@D...@`-style flag — so it
is not the same code path as the documented `gate.rs` defect and should not be assumed to inherit
it. This distinction (trigger-diacritic-as-ordinary-symbol vs. trigger-diacritic-as-flag) was not
separately re-tested against `foma-rs` this session (no build was run, per instructions); Experiment
A/B from `00-synthesis-and-decision.md` §5 remain the right way to confirm it empirically.

---

## 5. Where the cascade route is genuinely worse

This section looked specifically for **live, shipped** rules in `phonology.twolc` that are mutually
conditioning (each referring to the other's output) and would need an awkward or impossible cascade
ordering. The search result is smaller than expected, and itself informative.

### 5.1 The one candidate case, examined directly — and it turned out to be a non-case

`phonology.twolc:1204-1291` **[V]**, quoted at length because the whole passage matters: this is a
run of **commented-out**, abandoned rule attempts, immediately preceding the *live* diphthong-
simplification rules, in which the developers explicitly wrestle with whether diphthong
simplification needs to consult a stem's **consonant-gradation grade** (whether it is grade
G1/G2/G3) — i.e. whether one phonological process's applicability depends on another's classification
of the same stem, the textbook shape of a "genuinely simultaneous, mutually-conditioning" pair:

```
! Two possible strategies for fixing bug 56, the oahpaheaddjiid bug:
! ------------------------------------------------------------------
! 1. lexical strategy: ...
! con: gives a lexical solution to a basically morphophonological problem

! 2. morphophonological strategy, the smj way.
! Define a G3 context, and integrate it into a further rule
! ...
! con: defining G3 is a mess, at best
! We have optional dipht. simpl. only in G3-nouns and no cons grad.

! Going for 2:
!"G12 Compulsatory Diphthong Simplification in i-Stems before Suffixes Beginning with j:"
!  Vx:0 <=> Vow _ G12 [ i | e7:e ] X5: ;
! ...
!"G3 Facultative Diphthong Simplification in i-Stems before Suffixes Beginning with j:"
!  Vx:0 => Vow _ G3 [ i | e7:e ] X5: ;
```

Three successive attempts at a **true two-level construct** referencing a shared `G3`
gradation-class definition are each abandoned (this matches `phonology.twolc:140-165`'s
independently-documented three-times-abandoned `G1`/`G2`/`G3` structural definition, report `02`
§7a). **The shipped rule that replaced all three** (`phonology.twolc:1246-1294`, live, not
commented) does not reference gradation class at all — it is keyed purely on **per-suffix trigger
diacritics** (`%^DISIMP`, `X5`, `X3`, `X2`, `X1`, `Q2`, `Q3`, `Q7`, `Y7`, `W9`), the *same* device
gradation itself uses, attached at the lexicon/affix side and read as ordinary right-context. This
means the one candidate in this grammar for "genuinely needs simultaneous evaluation of two rules'
outputs" was tried as such and **explicitly rejected as unworkable within twolc itself**, in favor of
a device (trigger diacritics) already shown in §3–4 to transfer to the cascade world unchanged.

Cross-checking the live, shipped gradation and diphthong-simplification rules against each other
directly: every `WeG`-conditioned gradation rule's context refers only to lexically-attached trigger
symbols (never to another rule's *output*), and every diphthong-simplification rule's context refers
only to its own separate trigger-diacritic family — **no live rule's context in this grammar depends
on another live rule's output segment.** I did not find a counter-example after checking the full
set of named rules and their `Sets`/context vocabulary.

### 5.2 Ranked list of what cascade genuinely cannot do that two-level can, given this evidence

1. **Nothing found in the shipped grammar.** After reading the operators' actual usage
   (§3), the trigger mechanism's actual cross-rule dependencies (§4), and the one candidate case for
   true mutual conditioning (§5.1), no currently-shipped rule in `phonology.twolc` requires an
   evaluation order a cascade cannot express. This is a stronger and more specific claim than
   "probably fine" — it is the result of checking the specific place a mutual-dependency problem
   would show up, in the deepest twolc grammar in the org, and not finding one.
2. **Author convenience, not expressive power, for the `Sets`/rule-schema case.** Ritchie's
   closure result (§2) is real: two-level's generated-language class is closed under intersection but
   not union/complementation. But this is a property of the *class of relations describable in one
   authoring pass*, not a claim that any specific shipped grammar exploits it — and §5.1 shows this
   grammar's own authors hit exactly this wall (an attempted structural G3 definition) and retreated
   to enumeration rather than pushing further into genuine intersection-only territory.
3. **Automatic conflict resolution across independently-authored rules (`--resolve`)** is a real
   convenience two-level has that a cascade does not — but per §3's last row, it is solving a problem
   (uncoordinated rule authors producing contradictory verdicts) that a cascade's explicit ordering
   requirement prevents from arising in the first place, so this is better read as "twolc needs an
   escape hatch for a problem cascades don't have," not "cascades can't do something twolc can."
4. **Self-feeding iterative rules within a single stratum** (report `05` §5 item 3) remain the one
   *bona fide* open gap this investigation has found anywhere — but it is a cascade-internal
   limitation (no general static detection of genuine self-feeding), not a two-level-vs-cascade
   comparison at all: twolc's declarative semantics do not have an "iterative rule" concept in the
   same sense, so this gap is orthogonal to the twolc/cascade seam this report was asked to test, not
   an instance of it.

**Bottom line for §5:** looked for the hard case specifically, in the grammar most likely to contain
one, and did not find a shipped instance. The theoretical asymmetry (Ritchie's closure result) is
real but this investigation could not locate a linguistic phenomenon in GiellaLT's own deepest
grammar that actually needs it.

---

## 6. Summary against the five deliverables

1. **Verdict:** avoidable, not a real blocker. PanGloss never needs to produce or consume twolc; a
   substantial, real fraction of GiellaLT's own portfolio (including a mature, actively-maintained,
   shipped polysynthetic grammar) already runs the identical cascade formalism PanGloss emits,
   through the identical shared build tooling, with a live standalone-foma output path the twolc
   route lacks. The one attested hard case in the flagship twolc grammar was abandoned as a
   two-level construct by GiellaLT's own developers, in favor of a mechanism already shown to
   transfer to the cascade world.
2. **xfscript precedent: exists.** ~30 `giellalt/lang-*` repos, canonical path
   `src/fst/morphology/phonology.xfscript`, spanning stub-scale to production-shipped. Flagship:
   `lang-kal` (Kalaallisut), 512 lines/~65 rules, edited as recently as 2026-05-22, released as a
   183 MB grammar bundle and a packaged, versioned speller. Second precedent with a directly
   relevant translation artifact: `lang-crk` (Plains Cree), 660 lines, preserving an in-source
   before/after of a twolc alpha-variable rule hand-translated into an enumerated replace-calculus
   rule. Counter-example for calibration: `lang-amh`, 64 lines, early-stage, last substantively
   touched by a template merge, not organic development.
3. **Construct-coverage table:** §3. Every twolc facility has either a direct replace-calculus
   idiom, is a pure notational convention (Sets, archiphonemes, boundary symbols — zero cost), or
   requires enumeration (alpha-variables — same cost PanGloss already pays for HC's own
   alpha-variables). `--resolve` conflict resolution has no cascade equivalent because a cascade's
   explicit ordering prevents the underlying problem from arising.
4. **Trigger-diacritic construction for the cascade world:** §4, written out step-by-step (attach →
   transport → consume-in-context → delete late in the cascade), confirmed against a live, shipped
   example (`lang-kal`'s `%^GEMS`/`%^GEMEQ`/`%^Loan` family) rather than resting on structural
   analogy alone. No new formal machinery needed beyond what `pg-foma` already has.
5. **Ranked list of what cascade genuinely cannot do:** §5.2 — nothing found in the shipped
   reference grammar; the theoretical asymmetries that exist either turn out to be convenience-only
   (`--resolve`) or were themselves abandoned by GiellaLT's own developers when they hit them
   (the G3/diphthong-simplification case). The one real open gap found anywhere in this
   investigation (self-feeding iterative rules) is orthogonal to the twolc/cascade seam, not an
   instance of it.

## 7. Open questions this session could not close

- Whether any `lang-*` repo actually sets `LEXREF_IN_XFSCRIPT=yes` (the faster, lexicon-composed
  build variant) — checked three repos directly, all leave it off; did not exhaustively check all
  ~30 xfscript repos, and GitHub code search's tokenizer could not reliably confirm a negative
  org-wide. **[?]**
- `lang-moh` (Mohawk), `lang-bla` (Blackfoot), `lang-srs`, `lang-ciw`, `lang-cwd` were confirmed
  present and substantial by file size only; their rule content was not read this session the way
  `lang-kal`/`lang-crk`/`lang-amh` were. A follow-up reading Mohawk's file specifically would
  strengthen the polysynthetic-precedent claim further, since Mohawk is a more extreme case of
  noun-incorporating polysynthesis than Kalaallisut. **[?]**
- Ritchie 1992 and Karttunen/Kaplan/Zaenen 1992's specific formal proofs were not read as extracted
  text this session (the PDF-fetch tool returned undecoded binary for both); the closure and
  composition-strategy claims attributed to them here rest on search-indexed abstracts/secondary
  summaries, marked **[A]**, not on an independent line-by-line reading of the papers' proofs.
- Whether the ordinary-symbol trigger-diacritic pattern (§4) is actually clean under `foma-rs`'s
  specific vendored version was not re-tested this session (no build run, per instructions) — this
  is exactly report `00`'s Experiment A/B, still the right next step to close this empirically.
