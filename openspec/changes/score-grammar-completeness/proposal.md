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

## The open question this must answer before it is built

**F1 against what gold standard?** Agreement with the HC oracle is parser-versus-parser parity — it
says the port is faithful, not that the analysis is linguistically right. Real accuracy needs
human-annotated text, and whether a corpus is representative of a language is not a fact a tool can
settle. The predecessor was honest about this and modelled held-out corpus status as an
**attestation** — attestor, date, explicitly stated as unverified — rather than pretending to measure
it. Keep that. A score whose provenance is a signed human claim is more useful than one that hides the
same claim inside a number.

## What Changes

- Extract semantic domains in `pg-fwdata` instead of discarding them.
- Breadth and depth measures over the domain hierarchy, plus per-project custom domains.
- Genre-scoped corpus evaluation as a separate column.
- Overall precision/recall/F1, with its gold standard named on the report.
- A small headline set, stable over time so a project can watch it rise, composing to an overall
  readiness statement.
- Artifact thresholds (pack size, latency, device class) are **not** here — they move to
  `recipe-scoped-fst-health`, which judges artifacts per recipe.

## Non-goals

Certifying anything for a device. Measuring morphological or phonological completeness. Re-deriving
what `pg-assess` already computes.
