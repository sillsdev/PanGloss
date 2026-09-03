# PanGloss acknowledgement severity decision

Date: 2026-08-11

## Decision

An acknowledgement may suppress an `Info` or `Warning` finding from the default human-facing build
warning stream. It does not change the finding's severity, the raw report, or any analytical result.

An acknowledgement may not suppress or bypass an `Error` or `Critical` finding. Build/admission
failures continue to require PanGloss' separate explicit override mechanism, including its stronger
authorization, rationale, provenance, and degraded-trust signal.

The two operations remain distinct in vocabulary and data:

| Operation | Applies to | Effect |
| --- | --- | --- |
| Acknowledge | `Info`, `Warning` | Changes the default presentation/attention state only |
| Override | `Error`, `Critical` admission failure | Explicitly permits otherwise-refused execution and records degraded trust |

An acknowledgement command refuses an `Error` or `Critical` target and directs the caller to the
override workflow. An override is never inferred from an acknowledgement, project profile, or broad
matching rule.

Machine-readable reports retain acknowledged findings and identify the acknowledgement event that
matched them. Human-facing summaries show the number of hidden acknowledged findings and provide a
way to display them.

## Remaining scope question

The default lifetime of an acknowledgement is not yet decided. Candidate scopes include an exact
model/profile/corpus observation, a stable construct across grammar revisions while the metric stays
within an accepted bound, or an acknowledgement with a mandatory review date.
