# Measuring parser changes

How PanGloss decides whether a parser change made parsing cheaper. The product language (wordforms,
backends, Dictionary packs and the rest) is in `CONTEXT.md`.

## Language

**Instruction comparison**:
Two builds of PanGloss parsing the same word list against the same grammar, compared by the
instructions each spends parsing.
_Avoid_: benchmark, timing run

**Parse-only count**:
The instructions a run spends parsing its word list, without what it spends loading the grammar.
_Avoid_: net count, total instructions

**Minimum effect**:
The smallest change in the parse-only count, as a percentage of the earlier build, that a verdict
treats as real; smaller changes count as no change.
_Avoid_: threshold, noise floor

**Verdict**:
The conclusion of an instruction comparison for one grammar: GO, NO-GO (no change),
NO-GO (regression) or INCONCLUSIVE.
_Avoid_: result, pass/fail

**GO**:
The verdict that the later build parses with fewer instructions, both beyond run-to-run variation
and by at least the minimum effect.
_Avoid_: improvement, win

**NO-GO (regression)**:
The verdict that the later build parses with more instructions, both beyond run-to-run variation
and by at least the minimum effect.
_Avoid_: slower

**NO-GO (no change)**:
The verdict that any difference between the two builds is smaller than the minimum effect, with
the whole confidence interval inside it.
_Avoid_: within noise, neutral

**INCONCLUSIVE**:
The verdict that the measurement cannot claim a change or its absence; more repeats are needed.
_Avoid_: noisy, NO-GO

**Instrument check**:
An instruction comparison of a build against itself, which must read neither GO nor
NO-GO (regression). It checks the measuring setup, not a change.
_Avoid_: no-change control, noise floor
