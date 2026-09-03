# PanGloss unmeasured-acknowledgement decision

Date: 2026-08-11

## Decision

When a run does not produce a comparable current measurement for a previously acknowledged
observation, the acknowledgement remains `active_unverified`. It does not become stale merely because
the current invocation omitted the workload, lacked instrumentation, failed measurement, or produced
censored evidence.

`active_unverified` means:

- the durable acknowledgement event remains valid for the next comparable observation;
- the current run did not revalidate, improve, or breach its accepted bound;
- human output may state why it was not evaluated;
- no zero, cleared, healthy, or unchanged value is inferred;
- the next comparable complete measurement evaluates the original accepted value and bound normally.

The state becomes stale only when comparison identity changes incompatibly, such as metric-definition,
typed subject/kind, pipeline/backend/profile, resource envelope, workload identity, or aggregation
semantics. Expiry and explicit supersession remain separate lifecycle events.

This closes the measurement-absence question raised in
`pangloss-health-measurement-coverage-decision.md`.
