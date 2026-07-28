# Aweti completion evidence

This directory is the durable evidence index for the nine-task Aweti plan.
Historical evidence reconstructed from the exported 2026-07-27 session is
explicitly labelled retrospective; fresh command captures retain their command,
revision, toolchain, denominator, and limitations.

## Baseline and diagnosis

- `baseline-retrospective.log` records the measured pre-fix 68/106 baseline
  from the exported session, including network size and provenance. It replaces
  the plan's stale 68/104 denominator; it is not represented as an original raw
  redirected log.
- `bare-root-diagnostic.md` records the Task 2 boundary isolation and the
  evidence that rejected the combining-mark hypothesis.

## Performance and release evidence

- `aweti-profile-before.md` and `aweti-profile-after.md`: bounded Task 4/5
  measurements and the shipped outgoing-arc preparation result.
- `{sena,indonesian,amharic,aweti}-release.log`: Task 7 language captures.
- `four-language-results.md`: cross-language matrix and Task 9 audit.

The current executable Aweti regression boundary is exactly 100/106 with the
six documented misses in `p6_templated_morphotactics_gate.rs`. This is progress,
not 100% proposer recall.
