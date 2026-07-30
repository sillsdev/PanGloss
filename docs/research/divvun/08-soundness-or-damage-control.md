# Does Divvun/GiellaLT prove "FST-only" is sound, or does it just tolerate damage?

Research agent 8, PanGloss / Divvun-GiellaLT investigation. No code changed, no build run.

**Sources read**: prior reports `00`, `02`, `04` in this directory (synthesized, not re-derived).
Cloned/reused (shallow, `--depth 1`, reusing other agents' clones where identical, all under
`C:/Users/johnm/AppData/Local/Temp/claude/C--Users-johnm-Documents-repos-LCAtom/1b5e24e2-aeac-4668-b883-e199cfb811d9/scratchpad/divvun/`):
`a2/lang-sme`, `a2/giella-core` (primary sources for this report — read directly, in full, for
every file cited below); `a4/lang-fin`, `a4/lang-kal`, `a4/lang-crk` (read only to confirm the
shared test harness is byte-identical org-wide and to count YAML files); `a1/giella-core` (hash
cross-check only). External sources fetched live: `github.com/divvun/morph-test` README,
`giellalt.github.io/ling/docu-testing.html`.

All claims below are **VERIFIED** (I read the exact text at the cited path:line, or fetched the
cited URL myself) or **INFERRED** (my reasoning from VERIFIED evidence). Nothing is asserted from
memory of GiellaLT conventions in general — every quote below was re-read for this report.

---

## 0. Verdict, stated first

**(i), with one sharp exception carved out and correctly handled: (iii)-for-generation.**

GiellaLT's morphological **analyser** — the artifact that would be the FST-only replacement for
PanGloss's proposer+confirm split — is **not tested for soundness at all**, by explicit,
overwhelming, and largely self-admitted design. The test harness that every language in the org
shares (byte-identical file, confirmed across `lang-sme`, `lang-fin`, `lang-kal` clones) **hard-codes
a flag that unconditionally disables the one check that would catch over-generation**, and in
**zero of 1,300 YAML test files** scanned across four languages does any test author use the
tool's own (undocumented) negative-assertion syntax to compensate. GiellaLT's own historical
documentation states in so many words: *"When we test whether words are let through or not, we do
not test whether the parser actually gives correct analyses."* (`docs/docu-sme-testplan.md:138-139`,
quoted in full below). This is not an inference from absence — it is the project's own stated
testing philosophy, unchanged since it was written.

**But this is not a uniform "they never check" story, and the nuance is the second most important
finding of this report.** GiellaLT's shared test tool distinguishes **analysis** direction (surface
word → set of legal readings) from **generation** direction (lexical+tags → surface word), and the
soundness-disabling flag applies **only to the analysis direction**. Generation tests are held to a
strict **exact-set** standard by default, with no flag disabling it anywhere in the shared harness —
and GiellaLT's own public documentation records catching a real overgeneration bug (`klyhten`)
*because* the generation test enforced that standard. This exactly matches the theoretical
distinction the brief anticipated: the artifact that must never emit garbage (generation, which
feeds the speller) is tested for soundness; the artifact that is allowed to over-generate because a
downstream engine (Constraint Grammar) discards the excess (analysis, which feeds disambiguation) is
tested for recall only, on purpose, and the org's shared test infrastructure enforces that asymmetry
mechanically, not just by convention.

So, partitioned precisely:

| Artifact | What "sound" would mean | Tested for it? | Verdict |
|---|---|---|---|
| **Analyser** (`analyser-gt-desc.hfst`, feeds CG/disambiguator, feeds PanGloss's would-be replacement for HC confirm) | Every returned analysis is a well-formed reading, no spurious extras | **No** — the shared harness disables the check that would catch this, org-wide | **(i)**: tolerance by design, not a soundness proof |
| **Generator** (`generator-gt-norm.hfst`, feeds the speller) | Every surface form it emits for a given lexical input is a real, correct word — no garbage | **Yes** — exact-set by default, not disabled anywhere found | Sound-by-test for this artifact specifically, for the cases the YAML suite covers |

**What this means for the project owner's three framings**: it is **(i)** for the artifact that
matters to PanGloss (the analyser — the thing a proposer+pruner architecture would replace), and
something closer to **(iii)-already-solved** for the artifact PanGloss doesn't need (the generator —
PanGloss doesn't generate surface forms from an over-generating FST and prune them the same way; it
analyses). The clean one-line summary the project owner asked for: **GiellaLT's shipped-in-production
evidence that "FST-only works" is evidence about the generator, not the analyser — and the analyser
is the artifact that would actually replace HermitCrab's confirm step.** Their own test suite never
put the analyser to the soundness test PanGloss's confirm step exists to pass.

---

## 1. The test harness, exact semantics, quoted

### 1.1 The shared script every language invokes, byte-identical org-wide

`giella-core/scripts/run-morph-tester.sh.in` is the single script that ultimately runs every YAML
test for every giella-core-template language. Confirmed **byte-identical** (`md5sum`) across three
independent shallow clones of `giella-core` (`a1`, `a2`, `a4`), and confirmed by direct grep that
both `lang-fin/src/fst/test/run-gt-desc-anayaml-testcases.sh:20` and
`lang-kal/src/fst/test/run-gt-desc-anayaml-testcases.sh:20` delegate to
`giella-core/scripts/run-yaml-testcases.sh`, which in turn calls this file — VERIFIED. This is not
"a convention this report assumes is shared"; it is one file, checked out unmodified in every
language repo inspected.

**The decisive line**, `giella-core/scripts/run-morph-tester.sh.in:145-152` (VERIFIED, quoted in
full):

```bash
	$runtests \
		--colour \
		$outputformat \
		--ignore-extra-analyses \
		--section $section \
	    --app "$lookuptool" \
	    $fstoptions \
	    $halftestoption \
	    $testfile | tee -a $testtotalsfile
```

`--ignore-extra-analyses` is passed **unconditionally, every single invocation, for every
language**. There is no configure-time or per-test switch to turn it back on anywhere in this file
or in `run-yaml-testcases.sh.in` (read in full; VERIFIED it never sets or overrides this flag).

### 1.2 What the flag actually does — read from the tool's own source and its own documentation

The org's shared `giella-core/scripts/morph-test.py` is the (Python) implementation of this
contract — it is the fallback the `@GTMORPHTEST@` configure variable resolves to when the compiled
Rust `morph-test2` is not installed, and its semantics are what the tool's own public README
documents for both. Its CLI help text, `giella-core/scripts/morph-test.py:616-619` (VERIFIED,
quoted in full):

```python
self.add_argument("-i", "--ignore-extra-analyses",
    dest="ignore_analyses", action="store_true",
    help="""Ignore extra analyses when there are more than expected,
    will PASS if the expected one is found.""")
```

The upstream tool's own README (`github.com/divvun/morph-test`, fetched live) states the *default*
(flag absent) semantics explicitly — quoted verbatim from the fetch:

> "A test run succeeds if all and only the listed word forms are generated, and the word forms
> listed only get the specified analyses."

i.e. **the tool's default semantics are exact-set** — extra, unlisted analyses fail the test by
default. The same README gives the tool's own stated reason the flag exists:

> "All languages contain a certain amount of homonymy, which makes the `-i, --ignore-extra-analyses`
> option very useful: it makes the tests pass even if there are alternative analyses of a given word
> form."

This is the crux: the flag was built to let *legitimate* ambiguity (homonyms) through without
failing the test — but because it is applied **unconditionally**, it cannot distinguish a legitimate
homonym from a spurious, ill-formed extra analysis. Both are silently accepted. **The test suite as
actually run by every language in the org cannot detect over-generation in the analysis direction,
structurally, regardless of what any individual test author writes**, because the org-wide harness
suppresses the one check that would catch it before any test-author-level decision comes into play.

### 1.3 The exact code path, read directly, showing *which* direction is affected

`giella-core/scripts/morph-test.py:392-476` (VERIFIED, the pass/fail decision itself):

```python
def run_test(self, data, is_lexical):
    if is_lexical:
        desc = "Lexical/Generation"
        f = "gen"
        tests = self.config.surface_tests[data]
    else: #surface
        desc = "Surface/Analysis"
        f = "morph"
        tests = self.config.lexical_tests[data]
    ...
    for form in actual_results:
        if not form in expected_results:
            invalid.add(form)
    ...
    if len(invalid) > 0:
        if not is_lexical and self.args.ignore_analyses:
            invalid = set() # hide this for the final check
        elif not self.args.hide_fail:
            self.out.failure(n, caseslen, test, "Unexpected results", invalid)
    ...
    if len(detested) + len(missing) + len(invalid) > 0:
        self.count[d]["Fail"] += 1
```

Read closely: `is_lexical=True` is the **generation** direction (lexical input → surface output,
the direction a speller's word-form generator uses). `is_lexical=False` is the **analysis**
direction (surface input → set of lexical analyses, the direction a CG-fed disambiguator uses).
The suppression `if not is_lexical and self.args.ignore_analyses` fires **only when `is_lexical` is
`False`** — i.e. **only for analysis-direction tests**. For generation-direction tests, `invalid`
(an unexpected, extra generated surface form for a given lexical input) is **never** suppressed by
this flag, and it **does** increment the fail counter at the final `if` above. This is not an
accident of naming — the flag is literally called "ignore-extra-**analyses**," not
"ignore-extra-results," and the code honors that scope precisely.

**Net semantics, stated plainly**:
- **Generation tests** (surface-form output, speller-relevant): exact-set by default, everywhere,
  with no override in the shared harness. If the FST generates a wrong or unlisted surface form for
  a listed lexical input, the test fails.
- **Analysis tests** (reading-set output, CG/disambiguator-relevant): **subset-only**, org-wide,
  unconditionally, because the shared harness passes `--ignore-extra-analyses` on every invocation.
  A spurious, ill-formed extra analysis is indistinguishable, at the test level, from a legitimate
  homonym reading — both pass silently.

### 1.4 A negative-assertion capability exists in the tool — and is never used

`morph-test.py` also implements an **undocumented** (not mentioned in the upstream README, per the
live fetch) tilde-prefix syntax for explicitly forbidden results, `giella-core/scripts/morph-test.py:377-390`
(VERIFIED):

```python
def get_forms(self, test, forms):
    if test.startswith('~'):
        ...
    else:
        detested = set([i.lstrip('~') for i in forms if i.startswith('~')])
        expected = set([i.lstrip('~') for i in forms if not i.startswith('~')])
    return test, detested, expected
```

and, critically, this "detested" (forbidden) check is reported as `"BROKEN!" ... "Negative results"`
(`morph-test.py:478-485`, VERIFIED) and is **not** suppressed by `--ignore-extra-analyses` (that
flag only clears `invalid`, a structurally separate set from `detested`). **This is exactly the
negative/forbidden-case mechanism the brief asked me to look for, and it exists in the tooling.**

But it is **never used**. I grepped every YAML file in every clone available to this investigation
for a tilde inside a test value (excluding filename globs, which also use `~` for a different
purpose — fst-type exclusion — and are syntactically distinguishable):

```
find <all 5 clones> -iname "*.yaml" → 1,300 files
grep -P '(?<!\d)~\S' <every file> → 0 hits
```

Zero, across `lang-sme` (244 YAML files), `lang-fin` (14), `lang-kal` (20), `lang-crk` (277), and
`giella-core`'s own bundled test data — VERIFIED, this is an exhaustive scan of every YAML file this
investigation had access to, not a sample. **The capability to write "this analysis must NOT appear"
exists in the shared tool and is used by nobody in the org, in any language, anywhere in these
clones.** Whether it is used in some language repo not cloned by this investigation is possible but
unconfirmed — see §5.

### 1.5 First-hand corroboration: GiellaLT says this about itself, in its own words

`lang-sme/docs/docu-sme-testplan.md:136-153` (VERIFIED, quoted in full — this is the single most
important quote in this report):

> "When we test whether words are let through or not, we do not test whether the parser actually
> gives correct analyses. A word may thus be misanalysed, in two ways:
>
> 1. It is misspelled, but still given an (erroneous) analysis
> 2. It is correctly spelled, but given a grammatical analysis that it should not have had
>
> The first issue is of major concern to the spell checker project, and will not be dealt with
> here.
>
> The second issue has great importance to the disambiguator, and to the form generator isme.fst.
> Errors of this type pop up in two contexts: When the parser is used as input to the disambiguator
> (and the correct reading is missing from the input), and as a result of regularly reading through
> the analysis of a shorter, non-disambiguated text."

This is a first-person admission, from the developers of the deepest and most mature grammar in the
org, that **case (i)** — "a correctly-spelled word given a grammatical analysis it should not have
had" (textbook over-generation) — is a **known, accepted, uncorrected-for property of the shipped
analyser**, discovered by manual reading of corpus output ("regularly reading through the analysis
of a shorter, non-disambiguated text"), not by an automated soundness test. The remedy named is
downstream: the disambiguator (CG) is expected to sort it out.

The same document (`docu-sme-testplan.md:155-208`) then defines Precision/Recall — but **for the CG
disambiguator's cohort-selection accuracy**, not for the FST analyser's well-formedness:

> "Precision = #Tokens Correctly disambiguated / #Parses = TP/(TP+FP) ... A recall of less than 100%
> indicates that some correct analyses were removed, and a precision of less than 100% indicates
> that some wrong analyses were not removed."

This precision figure asks "did CG correctly discard the wrong readings from the cohort the FST
handed it" — it presupposes the FST's cohort may already contain wrong readings and measures whether
CG cleans them up. It is not, and does not purport to be, a measure of the FST's own soundness. No
comparable precision metric for the **analyser** itself (question A, not B, in report `03`'s
framing) is defined anywhere in this document or found anywhere else in this investigation.

---

## 2. Corroborating evidence: `hfst-lookup`, `lookup2cg`, and the pipeline's own assumptions

### 2.1 The pipe treats multiple analyses as normal input, not a defect

`lang-sme/src/cg3/disambiguator.cg3:1` (re-verified from report `03`, VERIFIED):

```
# -*- cg-pre-pipe: "$GTHOME/giella-core/scripts/preprocess ... | hfst-optimised-lookup
$GTHOME/langs/sme/tools/preprocess/analyser-disamb-gt-desc.hfstol | ...lookup2cg" -*-
```

`giella-core/scripts/lookup2cg` (Perl, read in full, VERIFIED) is the converter between
`hfst-optimised-lookup`'s raw multi-line tab-separated output and CG-2's cohort format. Its own
header comment states its job:

```perl
# lookup2cg
# - Rates and removes compound analyses according to
#   the number of word boundaries.
# - Reformats compound analyses and base forms, removes duplicates
```

It does **heuristic re-ranking** (`rate_compounds`: prefer the analysis with the fewest compound
boundaries; `select_lexicalized`: prefer a lexicalized compound reading over a dynamically-derived
one when both exist) and **deduplication**, then hands *every surviving reading* to CG as a cohort
line. There is no rejection of an analysis for being ill-formed — every analysis `hfst-lookup`
returns is packaged as a legitimate candidate reading for CG to choose among. This matches report
`03`'s finding exactly and independently confirms it: the analyser's output is treated as "the set
of legal readings," full stop, by every downstream consumer in this pipeline; nothing between the
FST and CG re-checks well-formedness.

### 2.2 The org's own public documentation independently records an over-generation bug caught only by the generation-direction test

`giellalt.github.io/ling/docu-testing.html` (fetched live) documents a generation-test failure
where the transducer produced an extra, wrong form:

> "there is one extra form (klyhten), which is incorrect and most likely a result of too loose
> two-level rules"

This is independent, public, first-party confirmation of two things at once: (a) GiellaLT's own
two-level phonological rules do over-generate in practice ("too loose two-level rules" is their own
diagnosis, not this report's inference), and (b) the mechanism that caught it was a
**generation**-direction test, consistent with §1.3's finding that generation tests are held to an
exact-set standard the analysis tests are not.

### 2.3 The historical bug log: overgeneration named explicitly, left unfixed, for years

`lang-sme/docs/docu-sme-bugs.md:70-74` (VERIFIED, re-confirmed independently in this report's own
clone, first found by report `02`):

> "The parser gives bealkálas from bealkit, which is correct, but it overgenerates to joavdálas for
> joavdit, where the correct form should be jovdelas. Look into this."

The file's own header states it is "kept here for nostalgic reasons," superseded by a Bugzilla
tracker this investigation did not have access to — so whether this specific bug was ever fixed is
**unknown**, but the document is explicit, first-party evidence that over-generation is a
recognized, named category of defect the team tracks (or tracked) as ordinary bugs, not as
soundness violations requiring an architectural fix.

---

## 3. The `Err/` and `Use/` tag families — deliberately admitting non-normative forms, tagged, not rejected

The brief specifically asked whether these tag families are "machinery for deliberately admitting
ill-formed words into the network and marking them." **Yes, unambiguously, for `Err/`.**
`lang-sme/src/fst/morphology/root.lexc:258-279` (VERIFIED, quoted representative lines):

```
 +Err/Orth         !!≈ * **@CODE@** substandard, not in normative fst
 +Err/Orth-a-á     !!≈ * **@CODE@** substandard, not in normative fst
 +Err/Orth-nom-gen !!≈ * **@CODE@** substandard, not in normative fst
 +Err/Orth-nom-acc !!≈ * **@CODE@** substandard, not in normative fst
 +Err/Lex          !!≈ * **@CODE@** substandard, not in normative fst, no normative lemma
 +Err/DerSub       !!≈ * **@CODE@** substandard for derivation, not in normative fst
 +Err/CmpSub       !!≈ * **@CODE@** substandard for compounding, not in normative fst
 +Err/MissingSpace !!≈ * **@CODE@** indicates that there is a missing space, causing an orthographic error
 +Err/MissingHyph  !!≈ * **@CODE@** when there is no hyphen where it should have been
 +Err/Hyph         !!≈ * **@CODE@** when there is a hyphen where none should have been
 +Err/Spellrelax   !!≈ * **@CODE@** used to tag spellrelaxed typos (tag is inserted via flag diacritics)
 +Err/Confused      !!≈ * **@CODE@** grammarcheking rela word error confusion pairs
```

(plus nine more `+Err/Confused-*` variants for specific confusable inflectional patterns, all at
the same location). Each is a self-declared *non-normative* branch of the lexicon, deliberately
compiled in and reachable by design (not an accident of over-generation), and tagged so downstream
consumers can tell it apart from a normative analysis. The live YAML test data confirms the FST
does return these in practice — `lang-sme/src/fst/test/gt-desc-yamls/noun-even_gt-desc.ana.yaml`
(VERIFIED, read directly) has entries like `bietna+N+Err/Orth+Sg+Com+PxSg1: bienainam` sitting
alongside the normative `bietna+N+Sg+Com+PxSg1: bienainan` for the *same* surface-adjacent forms —
the analyser's cohort for a given word is expected, by the test data itself, to mix normative and
`Err/`-tagged readings.

**Only one of these tags is ever filtered out of any build target.** The `filters/` directory
(VERIFIED, full listing) contains exactly one Err-related filter,
`remove-Err_SpaceCmp-strings.regex`, and even that one is explicitly **not** applied to every
target — `lang-sme/src/fst/Makefile.am:479` (VERIFIED): `# The HFST Grammar Checker analyser (keep
the Err/SpaceCmp strings):` — the grammar-checker build target deliberately retains it. None of
`Err/Orth`, `Err/Lex`, `Err/DerSub`, `Err/CmpSub`, `Err/MissingSpace`, `Err/MissingHyph`,
`Err/Hyph`, `Err/Spellrelax`, or any `Err/Confused-*` variant is removed by any filter found in this
repo. **The main analyser is not a well-formedness oracle; it is a superset acceptor whose output
must be post-filtered by the tag string if a consumer wants only normative analyses**, exactly the
pattern the brief's item 5 hypothesized.

`+Use/…` tags are a *different* mechanism, product-routing rather than error-tagging — worth being
precise about the distinction. `root.lexc:284-299` (VERIFIED):

```
 +Use/-Spell       !!≈ * **@CODE@** Orthographically correct, typically perifer words, excluded in speller because they cause trouble for frequent words
 +Use/SpellNoSugg  !!≈ * **@CODE@** recognized but not suggested in speller
 +Use/GC           !!≈ * **@CODE@** – only retained in the HFST Grammar Checker disambiguation analyser
 +Use/-GC          !!≈ * **@CODE@** – never retained in the HFST Grammar Checker disambiguation analyser
 +Use/TTS          !!≈ * **@CODE@** – only retained in the HFST Text-To-Speech disambiguation tokeniser
 +Use/PMatch       !!≈ * **@CODE@** means that the following is only used in the analyser feeding the disambiguator
```

`Use/` tags are correct-word product filtering (e.g. "this real word is excluded from the speller's
suggestion list for UX reasons, not because it's wrong") — a business-logic partition, not a
soundness mechanism. `Err/` tags are the soundness-relevant family: a deliberately admitted,
explicitly labeled non-normative superset.

---

## 4. Per-language verification table

| Language | Analysis-direction soundness test (exact-set)? | Negative/`~` cases used? | Generation-direction exact-set test? | Published precision (not just coverage) for the analyser? | Known-overgeneration admission found |
|---|---|---|---|---|---|
| **North Sámi** (`lang-sme`) | **No** — org-wide `--ignore-extra-analyses` suppresses it (§1.1–1.3) | **No**, 0/244 YAML files | Yes, by tool default, no override found; `Err/`-superset design confirms analyser is not meant to be sound anyway (§3) | **No** — `coverage-etc.bash` computes only `1 − OOV/tokens` (recall), no precision term (VERIFIED, re-read directly, matches report `04`'s independent finding); `docu-sme-testplan.md`'s only precision metric is CG's cohort-selection accuracy, not the FST's (§1.5) | Yes, explicit, twice: `docu-sme-testplan.md:138-144` (general admission) and `docu-sme-bugs.md:70-74` (named case, `joavdálas`), plus `Err/` tag family itself (§3) is an overgeneration-by-design admission |
| **Finnish** (`lang-fin`) | **No** — same shared harness, byte-identical script confirmed (§1.1) | **No**, 0/14 YAML files | Presumed yes (same tool, no override found in `lang-fin`'s Makefiles) — **not individually re-verified beyond confirming the shared script is used**, INFERRED from §1.1 | Unknown — no coverage/precision badge found in-repo (report `04`'s independent finding, re-cited) | Not searched specifically; INFERRED likely present given the shared architecture, not confirmed |
| **Greenlandic** (`lang-kal`) | **No** — same shared harness (§1.1) | **No**, 0/20 YAML files | Presumed yes, same caveat as Finnish | Unknown, same as Finnish | Not searched specifically |
| **Plains Cree** (`lang-crk`) | **Unknown** — uses a *different* test tool (`fsttest`, TOML fixtures, per report `04` §6.3), which this report did not clone or read; its pass/fail semantics were not established here | 0/277 YAML files show `~`-syntax, but `lang-crk`'s primary test format is TOML via `fsttest`, not YAML via `morph-test` — the YAML-file scan is not evidence about `fsttest`'s own semantics | Unknown | Unknown | Not searched |

---

## 5. What this settles, and what it does not

**Settled, VERIFIED**: for the analyser artifact — the one directly comparable to PanGloss's
proposer-FST-plus-confirm architecture — GiellaLT's shared, org-wide, byte-identical test harness
structurally cannot detect over-generation, because it unconditionally disables the one check
(`--ignore-extra-analyses`) that would catch it, and the negative-assertion escape hatch that exists
in the tooling is used by literally none of the 555 YAML files across `lang-sme`+`lang-fin`+`lang-kal`
inspected. This is corroborated by first-party admission (`docu-sme-testplan.md`), a first-party bug
log entry (`docu-sme-bugs.md`), a public documentation example (`docu-testing.html`'s `klyhten`
case), and by a lexicon design (`Err/*` tags) that is only coherent if the analyser is understood as
a deliberately over-inclusive, tagged superset rather than a well-formedness oracle.

**Also settled**: the soundness bar genuinely differs by artifact, mechanically, in the test tooling
itself, exactly along the line the brief's item 6 predicted — generation (speller-facing) is
exact-set by default with no override found; analysis (CG-facing) is subset-only, org-wide, by a
hard-coded flag. This is a real, load-bearing design decision, not an oversight limited to one
sloppy language: it is baked into the one shared script every language in the org runs.

**Not settled / unknown**:
- Whether any GiellaLT language *outside* the four clones read here (56 `lang-*` repos exist in the
  org per report `04`) uses the `~`-negative-case syntax, invokes `morph-test`/`morph-test2` with a
  locally-overridden (non-default) flag set, or maintains a separate precision-measuring test suite
  this investigation didn't find. The scan of 1,300 YAML files is exhaustive **for the repos
  cloned**, not for the org.
- Plains Cree's `fsttest`/TOML testing semantics — a structurally different tool this report did not
  read. Report `04` flagged it as institution-specific (ALTLab, not giella-core); it may or may not
  test for soundness differently. Genuinely unknown, not inferred either way.
- Whether GiellaLT maintains any *external*, non-YAML precision benchmark (a held-out annotated
  corpus scored for both recall and precision at the analyser level) outside the repos and org
  documentation surfaces checked. `coverage-etc.bash` is the only coverage/precision-adjacent tool
  found anywhere in this investigation (here and in report `04`), and it computes recall only.
- Whether the historical `joavdálas` bug (`docu-sme-bugs.md:70-74`) was ever fixed — the file itself
  says it is superseded by a Bugzilla tracker not accessed by this investigation.
- Whether `hfst-optimised-lookup`'s behavior differs from `hfst-lookup`'s in any way relevant to
  soundness (both were treated as equivalent for this report's purposes, matching how
  `run-morph-tester.sh.in:194-197` treats them as interchangeable lookup tools selected by file
  suffix) — not independently stress-tested here.

---

## 6. The implication for PanGloss, stated bluntly

**GiellaLT's production deployment is not evidence that FST-only analysis is sound.** It is evidence
that FST-only analysis *ships*, at scale, for languages much larger than PanGloss's FLEx-scale
target, **while explicitly not being tested for the property PanGloss's `confirm` step exists to
guarantee**, and while containing a first-party admission that it over-generates in exactly the way
the confirm step is designed to catch (case (i): "correctly spelled, but given a grammatical
analysis it should not have had"). Their tolerance for this is not evidence that the tolerance is
harmless in general — it is evidence that **their architecture has a downstream consumer (CG) that
can absorb the excess for their specific use cases (disambiguation feeding rule-based grammar
checking, not analysis as an end product)**, plus a deliberate tagging convention (`Err/*`) that
lets other consumers filter the excess back out by string-matching if they need to. Neither of those
remedies is "the FST is actually sound"; both are "we built a downstream filter instead of proving
the upstream is sound," which is precisely what PanGloss's `confirm` step also is — just implemented
as a real grammar-execution engine rather than a sentence-context disambiguator or a tag-substring
filter.

**This reframes the comparison the project owner asked about.** The question was never "does
FST-only work — look, Divvun ships it." The honest reframing, licensed directly by the evidence
above, is: **Divvun ships an over-generating FST as the front half of a two-stage architecture too**
— FST proposer, then either (a) Constraint Grammar discarding wrong readings by sentence context
(for disambiguation products) or (b) a human/consumer filtering by `Err/`/`Use/` tag string (for
spellers and grammar checkers) — and neither of those back ends is a general-purpose,
morphology-and-phonology-aware well-formedness verifier of the kind HermitCrab's confirm step is.
**PanGloss's confirm step is doing a job GiellaLT's architecture assigns to no single component at
all**; GiellaLT gets away without it because (a) CG only needs to be right about which reading is
contextually correct, not whether the FST's whole cohort was well-formed, and (b) their downstream
products (spellers, keyboards) are built from the *generation* direction, which — per §1.3 — *is*
tested to a real exact-set standard, sidestepping the analyser's looseness entirely for that
product.

**What, if anything, transfers to PanGloss (the (ii)/(iii) sliver)**: the one piece of this
investigation that *is* a positive, reusable result is not about testing philosophy at all — it is
report `03`'s independently-verified finding that GiellaLT's flag-diacritic filter cascade
(`filters/remove-illegal-derivation-strings-flagbased.regex` etc.) really does perform
grammar-internal well-formedness pruning (report `03`'s class-A pruning) entirely inside the FST, for
bounded, propagate-forward, single-valued constraints (derivation ordering, compound legality). That
finding stands on its own math (Kaplan & Kay regularity, empirically demonstrated by GiellaLT's own
shipped filters) and is unaffected by this report's finding that their *tests* don't check for
residual over-generation — a mechanism can be genuinely sound for the construct class it targets
even if the project's test suite never verifies that fact for the *whole* analyser. What this
report adds is the caution that **"it ships in production" cannot be used as evidence that the
*whole* analyser is sound, because it was never tested for that**, only that specific,
narrow-purpose filters (which happen to be sound by construction, per finite-state theory, not by
test) are doing well-defined jobs inside it. Any PanGloss design that borrows GiellaLT's
flag-diacritic technique for a specific bounded construct class should be validated by PanGloss's own
exact-set tests (which HC's confirm step already gives us, for free, today) — not by an appeal to
"Divvun does this and it works," because Divvun's own test suite would not have caught it if it
didn't.

---

## Sources

- `lang-sme` (North Sámi): `https://github.com/giellalt/lang-sme`, clone reused from
  `.../scratchpad/divvun/a2/lang-sme`.
- `giella-core` (shared build/test infra): `https://github.com/giellalt/giella-core`, clone reused
  from `.../scratchpad/divvun/a2/giella-core` (cross-checked byte-identical against `a1`, `a4`).
- `lang-fin`, `lang-kal`, `lang-crk`: clones reused from `.../scratchpad/divvun/a4/` (report `04`'s
  clone), read here only for YAML-file counts and harness-invocation confirmation.
- `divvun/morph-test` README: `https://github.com/divvun/morph-test`, fetched live.
- GiellaLT testing documentation: `https://giellalt.github.io/ling/docu-testing.html`, fetched live.
- All in-repo paths above are relative to the clones named; prefix with the path shown or the
  corresponding GitHub repo at `main` HEAD (shallow clone taken 2026-07-30, no pinned commit hash
  captured, consistent with reports `02`/`04`'s caveat about this checkout).
