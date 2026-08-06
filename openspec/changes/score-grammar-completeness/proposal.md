# How complete and accurate is this grammar? A few numbers a casual user can read.

**Status: intent.** Successor to `certify-language-readiness` (archived 2026-08-06), rescoped from a
device-readiness certification to a grammar-completeness score. Design and tasks are unwritten; the
axes and the open questions are not.

## Why

Health asks whether the artifact is well-built. This asks something health structurally cannot:
**is this grammar an adequate description of the language?** Health is closed-world — it reasons about
the grammar it was handed. This is open-world, and the mechanical tell is that it needs data the
grammar was not built from. Nothing else in the system does.

The audience is a casual user, not an engineer. They want a small number of figures that go up as the
work improves, and eventually a headline that says *this grammar is ready for broad usage*. Morphology
and phonology completeness is assumed, not measured here — the interesting axis is coverage of the
language, and separately, accuracy.

## The two axes, which are not the same axis

**Breadth and depth over semantic domains.** The SIL/FieldWorks semantic-domain hierarchy is already
in the source data — `aweti.fwdata` alone carries **1,792 `SemanticDomain` elements** — and
`pg-fwdata`'s importer currently **drops every one of them**. So the first work is extraction, not
invention. Its top level gives roughly the 5–15 standard categories wanted (universe, person,
language and thought, social behaviour, daily life, work, physical actions, states, grammar); the
deeper hierarchy gives depth. **Breadth** is how many top-level domains carry real entries; **depth**
is how far down each one goes. Custom domains a project defines sit alongside the standard ones.

**Genre or register is a different axis.** Scripture, health, news, science are not semantic domains —
they are text types. They matter, but as *which corpus you evaluate against*, not as how the lexicon is
classified. A scripture corpus exercises certain semantic domains heavily and leaves others empty, and
conflating the two would make "covers the health domain" and "was tested on health text" the same
claim when they are independent. Both are wanted; they are separate columns.

**Accuracy is overall, not per domain.** Precision, recall and F1 characterise how completely the
affix mechanics are covered, and per-domain slicing would give thin, noisy denominators. One set of
numbers.

## Accuracy has a precondition, and without it the number must not be produced

**F1, precision and recall only mean anything against a held-out corpus that is all three of:
substantively sized, known to contain only good words, and believed to be fully within what this
grammar should cover.**

That is not a caveat on the metric. It is the metric's definition of a miss. On an uncurated corpus a
failed analysis is ambiguous between three unrelated causes — a real gap in the grammar, a typo or OCR
artifact, or a token the grammar was never meant to cover (a name, a loanword, a code-switch). Those
demand opposite responses: fix the grammar, fix the corpus, or do nothing. A single number that
averages them tells a project to work on whichever is loudest, which is not the same as whichever
matters.

So corpus curation is a **gate**, not preparation. Three properties, each recorded:

- **Substantively sized** — enough tokens that the figure is stable rather than a sample artifact. The
  threshold is a real question, not a formality.
- **Known clean** — the words are words. Somebody looked.
- **In scope** — the grammar *should* analyse every token in it. This is the strongest claim and the
  easiest to get wrong; a corpus with names and borrowings in it fails here while looking fine.

**When the corpus does not satisfy all three, the report states that accuracy is not computable and
why — it does not print a number with an asterisk.** This repo already holds the symmetric rule
elsewhere: "I could not look" must never read as "everything is fine". A precision figure computed on
an unvetted corpus is that failure in its most persuasive form, because it looks like evidence.

**And the gold standard still has to be named.** Agreement with the HC oracle is parser-versus-parser
parity — it shows the port is faithful, not that an analysis is linguistically right. Only
human-annotated text supports the second claim. Whether a corpus is representative is likewise not a
fact a tool can settle, which is why the predecessor modelled held-out status as an **attestation**
(attestor, date, stated as unverified). Keep that. A score whose provenance is a signed human claim is
worth more than one that hides the same claim inside a number.

## What Changes

- Extract semantic domains in `pg-fwdata` instead of discarding them.
- Breadth and depth measures over the domain hierarchy, plus per-project custom domains.
- Genre-scoped corpus evaluation as a separate column.
- Overall precision/recall/F1, with its gold standard named on the report, gated on a curated
  held-out corpus and **suppressed entirely** when that corpus does not qualify.
- A corpus qualification record — size, cleanliness, in-scope claim, attestor, date — carried with any
  accuracy figure, so a reader can see what the number rests on without leaving the report.
- A small headline set, stable over time so a project can watch it rise, composing to an overall
  readiness statement.
- Artifact thresholds (pack size, latency, device class) are **not** here — they move to
  `recipe-scoped-fst-health`, which judges artifacts per recipe.

## Non-goals

Certifying anything for a device. Measuring morphological or phonological completeness. Re-deriving
what `pg-assess` already computes.
