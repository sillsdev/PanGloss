# XAmple / AMPLE: primary-source findings

Research scope: XAmple (AMPLE successor), item-and-arrangement morphological parser used by
FieldWorks Language Explorer (FLEx). All claims below are cited to a primary source. Items that
could not be verified are marked UNVERIFIED.

## 1. Data files and control-file sections

XAmple = AMPLE (the morphological parser) + an embedded PC-PATR word-grammar parser. "When we
generate the source files for XAMPLE, in addition to the lexicon and analysis control files
traditionally required by AMPLE, we create a word grammar file for the embedded PC-PATR parser."
["Parser and interlinear text", Black/Simons/Zook, 2007](https://software.sil.org/fieldworks/wp-content/uploads/sites/38/2016/10/Parser-and-interlinear-text.pdf)
(pp. 4-5). FLEx generates, per project, an XAmple analysis control file (`adctl.txt`), an XAmple
lexicon file (`lex.txt`), and an XAmple word-grammar file (`gram.txt`), plus `cd.tab` (the
dictionary code table) — all fed into the XAmple DLL to initialize it (same source, p. 5).

The **AMPLE Reference Manual** (source: `pc-parse/doc/ample.txi` in the
[sillsdev/CarlaLegacy](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/doc/ample.txi)
repo) documents the mandatory **Analysis Data File** (one file, called `xxAD01.CTL` in prompts)
containing these field codes (all confirmed present, lines 427-461 of `ample.txi`):
`\ancc` (allomorphs-never-co-occur constraint, XAmple-only), `\ap` (allomorph properties, max 255
combined with `\mp`), `\ca` (categories, max 255), `\cat` (category output control: prefix/
suffix/computed), `\catcr` (category for compound-root-only words: left/right), `\ccl` (category
class), `\cr` (compound-root category pair), **`\dicdecap`** (dictionary decapitalization control
— confirmed real, contrary to the prompt's caveat that it might not exist), `\ft` (final test),
`\iah`/`\nah`/`\pah`/`\rah`/`\sah` (ad hoc pairs for infix/interfix/prefix/root/suffix), `\it`/
`\nt`/`\pt`/`\rt`/`\st` (successor tests per affix type), `\maxi`/`\maxn`/`\maxnull`/`\maxp`/
`\maxprops`/`\maxr`/`\maxs` (limits: infixes, interfixes, null allomorphs [default 10], prefixes,
properties [default 255, extendable via `\maxprops` up to 65535], roots [default 1], suffixes
[default 100]), `\mcc` (morpheme co-occurrence constraint), `\mcl` (morpheme class), `\mp`
(morpheme properties), `\patr` (PC-PATR/XAmple-only control settings — `CheckCycles`,
`TimeLimit`, `TopDownFilter`, `Unification`, etc.), `\pcl` (punctuation class), `\rd` (root
delimiter chars, default `< >`), `\scl` (string class), `\strcheck` (valid characters for
allomorphs/string environments).

**Dictionary files** (prefix/infix/suffix/root, or unified with `-u`) use internal codes mapped
via the **dictionary code table file** (`\dicdecap`-adjacent chapter is separate: "Dictionary Code
Table File"). Confirmed dictionary-entry field codes, from
[`dictfile.txi`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/doc/dictfile.txi):
**A** allomorph, **C** category (from/to pairs for affixes, one-or-more categories for roots),
**E** elsewhere allomorph (fallback + default underlying form), **F** feature descriptor, **G**
root gloss, **L** infix location, **M** morphname, **O** order class (can be a min/max pair since
v3.6.0), **P** morpheme property, **T** morpheme type (unified dictionaries only), **U**
underlying form, **Z** morpheme co-occurrence constraint (dictionary-local variant of `\mcc`), and
`!` "do not load." The prompt's guessed `\mp` (morpheme properties) and `\ap` (allomorph
properties) field-name pairing is correct for the *analysis data file*; inside dictionary entries
these are simply the **P** and (a plain property list following the allomorph, BNF rule 3)
positions of the allomorph field — dictfile.txi does not name a separate `\ap`-coded dictionary
field distinct from the property list attached to `\a`.

`MaxMorphnameLength` is not a field name; it is the `-n number` **command-line option**
("sets the maximum recommended morphname length... truncated with a warning") — ample.txi,
"AMPLE Command Options" section.

## 2. String/morpheme/allomorph environment constraint semantics (SEC / MEC / PEC / ANCC)

Two independent BNF grammars exist, both in the AMPLE manual, and they are **not** the same
syntax:

- **Successor/final test syntax** (`\pt`, `\it`, `\nt`, `\rt`, `\st`, `\ft` bodies) — a boolean
  expression language with clauses like `left allomorph is "X"`, `left surface matches "Y"`,
  `left morphname is member [CLASS]`, `orderclass > constant`, `left type is prefix`, joined by
  `AND`/`OR`/`XOR`/`IFF`/`NOT` — [`usertest.txi`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/doc/usertest.txi),
  rules 1-23. Per rule 5d-o: `allomorph`/`morphname` clauses test the dictionary allomorph string
  itself; `surface` clauses test the surface string *after orthography changes have been applied*
  — these are the two explicitly distinguished string domains the prompt asked about. `left`/
  `LEFT`/`INITIAL` test a string suffix (does the neighbor's string *end* with X); `current`/
  `right`/`RIGHT`/`FINAL` test a string prefix.

- **Allomorph-field environment-constraint syntax** (attached directly after an allomorph string
  in a dictionary entry, or in `\mcc`/`\ancc`) — a distinct BNF in
  [`dictfile.txi`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/doc/dictfile.txi)
  rules 6-25 and `ample.txi`'s "MCC syntax"/"ANCC syntax" sections. Three markers distinguish the
  three constraint kinds by what they match against:
  - **`/`** = **string environment constraint (SEC)** — matched against the contiguous surface
    character string built up so far (left) and yet to be matched (right); e.g.
    `/ [Vowel] _ #` (dictfile.txi rule 6).
  - **`+/`** = **morpheme environment constraint (MEC)** — matched against the *sequence of
    already-parsed morphemes* (morphnames, classes, or property/category/type keywords in
    `{root|prefix|infix|suffix}` braces), not characters (rule 12; also usable for infix-location
    fields per dictfile.txi rule 11 comment).
  - **`./`** = **punctuation environment constraint (PEC)** — matches punctuation immediately
    before/after the *whole word*, ignoring the conditioned allomorph's actual position; no
    ellipsis or cross-word conditions allowed (rule 18-23, added in AMPLE v3.3, "Introduction",
    item 3 of `ample.txi`).
  - A **negative string constraint**, `~/ ...`, differs from `/ ...` in two ways: it negates its
    result, and multiple `~/` constraints on one allomorph are ANDed (vs. multiple plain `/`
    constraints, which are ORed) — dictfile.txi, comment on rule 6d.

  Shared conventions across all three kinds: **`#`** = word boundary (a boundary marker preceded
  by `~`, i.e. `~#`, means "must NOT be a word boundary"); **`...`** between items = "a possible
  break in contiguity" (skip); **`(X)`** = optional item; **`[X]`** = a natural/string class
  (defined by `\scl`) or, in morpheme contexts, a morpheme class (`\mcl`); a bare **`~`** prefixed
  to a piece "reverses the desirability of an element, causing the constraint to fail if it is
  found rather than fail if it is not found"; and `~` attached to the environment bar itself
  (`~_`) "inverts the sense of the constraint as a whole" (dictfile.txi rules 9a/11a/24, ample.txi
  MCC-syntax comments on rules 9a/11).

  `\mcc`'s environment marker is `/` or `+/` (the field can use either a string or morpheme
  environment); `\ancc`'s marker is `/` or `~/` (ample.txi "MCC syntax"/"ANCC syntax" sections,
  rule 10 in each).

Runtime confirmation from source: `checkAmpleStringEnviron(pszSurfaceForm_m, ...)` in
[`pc-parse/ample/anal.c`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/ample/anal.c)
is called against `pszSurfaceForm_m`, confirming SEC tests the built surface string, while
`checkAmpleMorphEnviron(left, current->pRight, ...)` (same file) takes `AmpleHeadList *` morpheme
lists, confirming MEC tests the parsed-morpheme sequence, not characters.

## 3. No phonological rules or feature system — purely item-and-arrangement

Confirmed explicitly: "AMPLE... employs an 'item and arrangement' approach to morphological
description and cannot directly handle nonconcatenative phenomena" —
[software.sil.org/ample/](https://software.sil.org/ample/). The full AMPLE reference manual
(`ample.txi`, `dictfile.txi`, `usertest.txi`; ~4200 lines total) contains **no** section on
phonological rewrite rules, ordered rule application, or feature matrices — searched and found no
occurrence of "phonolog", "rewrite rule", or "feature matrix" anywhere in those files. All
allomorphy is handled by **listing every allomorph explicitly** as a separate `\a` field per
dictionary entry, each with its own environment constraints (dictfile.txi, "Allomorph (internal
code A)" section: "If an affix/root has multiple allomorphs, each one must be entered in its own
allomorph field"). The only generative mechanisms are **partial reduplication** (indexed string
classes, `[X^n]`, v3.2) and **full reduplication** (the literal token `<...>`, v3.12) — both
pattern-matching over already-listed strings, not rule-driven derivation (dictfile.txi rules 26-31;
`ample.txi` "New features" list). `\fd` (feature descriptor) fields exist only as opaque labels
"written verbatim to the `\fd` field of the output analysis file. It is not otherwise used by
AMPLE" (dictfile.txi, "Feature descriptor" section) — i.e. not a unification feature system for
phonology, only inflectional/syntactic labeling consumed downstream by PC-PATR in XAmple.

## 4. How XAmple decides an analysis is complete/valid

Order of operations, drawn from `ample.txi` and the FLEx parser paper:

1. **Successor tests** are run at each affix-type boundary as morphemes are assembled: prefix
   (`\pt`), infix (`\it`), interfix (`\nt`), root (`\rt`), suffix (`\st`). If not explicitly
   listed, the built-in tests `SEC_ST` (string env.), `ADHOC_ST` (ad hoc pairs), and `PEC_ST`
   (punctuation env.) are applied automatically after any user-defined ones; roots additionally
   get the built-in `ROOTS_ST` (dictfile/ample.txi \pt/\it/\nt/\rt/\st sections).
2. **Order-class and category constraints** are available to user-written tests via the
   `orderclass`/`orderclassmin`/`orderclassmax` and `fromcategory`/`tocategory` clauses
   (`usertest.txi` rules 7 and 9), and via `\cr` compound-root category pairs (ample.txi \cr
   section) — but AMPLE applies these only if the analysis-data-file author writes a test that
   uses them; there is no separate hard-coded "orderclass check" pass.
3. **Final tests** (`\ft`) run once a complete word candidate is assembled; the built-ins
   `MEC_FT` (morpheme-environment final test) and `MCC_FT` (morpheme co-occurrence final test)
   are always applied even if `\ft` never appears (ample.txi, "\ft" section: "If no `\ft` fields
   appear... AMPLE still applies the built-in final tests MEC_FT and MCC_FT"). `\ancc`
   (allomorphs-never-co-occur) is enforced by an `ANCC_FT` test, XAmple-only (ample.txi \ancc
   section).
4. **XAmple only:** after AMPLE produces a candidate morpheme sequence, "this preliminary result
   is passed to the word grammar" — the embedded PC-PATR parser — which does unification-based
   constituent-structure and feature-percolation checking (inflectional templates, inflection
   classes, feature percolation, circumfixes, clitic attachment, etc.); an AMPLE-legal sequence
   that PC-PATR's word grammar cannot unify is rejected ("Parser and interlinear text", pp. 4,
   listing eleven responsibilities of the word grammar).

## 5. Source location, license, and environment-matching code

Source **is** on GitHub: [`sillsdev/CarlaLegacy`](https://github.com/sillsdev/CarlaLegacy),
directory `pc-parse/ample/` (the `pc-parse` repo also bundles PC-PATR, STAMP, and related tools;
`software.sil.org/ample/` explicitly names "the **pc-parse** folder of the CarlaLegacy GitHub
repository" as AMPLE's source location). License: dual GPLv2 / Common Public License v0.5, per
[`pc-parse/LICENSE`](https://raw.githubusercontent.com/sillsdev/CarlaLegacy/master/pc-parse/LICENSE)
("pc-parse, Copyright 2001-2011, SIL International... free; you can redistribute it and/or modify
it under the terms of either: a) the GNU General Public License version 2... or b) the Common
Public License Version 0.5").

Environment-matching lives in
[`pc-parse/ample/envchk.c`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/ample/envchk.c)
(1736 lines) and
[`pc-parse/ample/envpar.c`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/ample/envpar.c)
(3044 lines, the environment-field *parser*). `envchk.c`'s header comment names its three public
entry points directly: `checkAmpleStringEnviron(...)` (SEC), `checkAmpleMorphEnviron(...)` (MEC),
and `checkAmplePunctEnviron(...)` (PEC), backed by static helpers `senv_left`/`senv_right`,
`menv_left`/`menv_right`, `penv_left`/`penv_right`, and `nenv_left`/`nenv_right` (interfix
variants). `\mcc` parsing lives in `mccpar.c`; `\ancc` parsing in `anccpar.c`; general
non-environment field validation in `validch.c` and `categ.c`. All confirmed by directory listing
via the GitHub API and by reading `envchk.c`'s function bodies (e.g.
`checkAmpleStringEnviron` calls `senv_left`/`senv_right` per constraint in its linked list,
inverting the result when `ec->bNot` is set — matching the documented `~/` negation semantics).

## 6. SIL/FieldWorks documentation

- **AMPLE Reference Manual**, version 3.12, by Stephen McConnel, H. Andrew Black, and Marius
  Doornenbal, June 2006 — the primary technical reference used throughout this document, at
  [`pc-parse/doc/ample.txi`](https://github.com/sillsdev/CarlaLegacy/blob/master/pc-parse/doc/ample.txi)
  (texinfo source; no rendered HTML/PDF found at a stable SIL URL — an old mirror at
  [ai.mit.edu/courses/6.863/doc/ample.html](http://www.ai.mit.edu/courses/6.863/doc/ample.html)
  returned a connection error during this research and could not be fetched — UNVERIFIED whether
  it still serves the same content).
- **Bibliographic citation for the founding AMPLE report**, confirmed from `ample.txi`'s own
  Bibliography chapter: Weber, David J., H. Andrew Black, and Stephen R. McConnel. 1988.
  *AMPLE: a tool for exploring morphology*. Occasional Publications in Academic Computing No. 12.
  Dallas, TX: Summer Institute of Linguistics. (Companion: Weber, Black, McConnel, and Buseman.
  1990. *STAMP: a tool for dialect adaptation*. OPAC No. 15.) Full text was not independently
  fetched/paginated in this research — UNVERIFIED beyond the citation itself and the material
  `dictfile.txi` states is adapted from its chapter 11.
- David J. Weber also authored a related SIL Work Papers article, **"A morphological parser for
  linguistic exploration"**, listed at
  [commons.und.edu/sil-work-papers/vol33/iss1/7](https://commons.und.edu/sil-work-papers/vol33/iss1/7/);
  the PDF full text returned HTTP 403 when fetched directly in this session — UNVERIFIED content,
  confirmed only to exist via its landing page.
- **FieldWorks "Parser and interlinear text"** (Andy Black, Gary Simons, Ken Zook, Nov 3 2007;
  updated 2013), fetched in full as a PDF from
  [software.sil.org/fieldworks/.../Parser-and-interlinear-text.pdf](https://software.sil.org/fieldworks/wp-content/uploads/sites/38/2016/10/Parser-and-interlinear-text.pdf) —
  this is the source for sections 1 and 4 above (file flow diagrams, XAmple/word-grammar division
  of labor). It references a fuller paper, `BlackSimonsFLExParser.doc` (also cited in academic
  form as a Stanford TLS10-2006 paper by Black & Simons,
  [cslipublications.stanford.edu/TLS/TLS10-2006/TLS10_Black_Simons.pdf](http://cslipublications.stanford.edu/TLS/TLS10-2006/TLS10_Black_Simons.pdf))
  — not fetched in this session; UNVERIFIED beyond the summary in the PDF actually read.
- No standalone "About the Parser: XAmple vs. HermitCrab" FLEx help page was located by search;
  the closest primary equivalent found is the "Parser and interlinear text" document above, which
  describes XAmple's architecture but does not name HermitCrab as an alternative (HermitCrab was
  added to FLEx later). UNVERIFIED that a dedicated XAmple-vs-HermitCrab comparison page exists.

## Summary of sourcing strength

**Well-sourced** (direct primary-source text, much of it exact BNF/field-by-field manual
content): Topics 1, 2, 3, 4, and 5. **Partially UNVERIFIED**: Topic 6 — the manual itself and the
FLEx architecture PDF are solid, but the 1988 AMPLE OPAC report, the Weber SIL Work Papers article,
and the Black/Simons Stanford paper were confirmed only to exist (bibliographic/landing-page
level), not read in full, and no dedicated XAmple-vs-HermitCrab FLEx help page was found.
