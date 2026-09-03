# PanGloss optional-configuration decision

Date: 2026-08-11

## Accepted behavior

PanGloss does not require project configuration. Every operation can run from a FieldWorks
`.fwdata` project plus command-line arguments and built-in defaults.

Configuration under `ConfigurationSettings/PanGloss/` is an optional durable layer for:

- named pipeline/backend and resource profiles;
- report-policy defaults;
- durable acknowledgement events such as “this high-cost grammar construct is intentional”;
- selection or extension of neutral advice catalogs.

Reading or analyzing an unconfigured `.fwdata` project does not create files. Project configuration
is created only by an explicit initialization or acknowledgement command. Existing files are never
silently overwritten.

## Resolution and provenance

Effective values resolve in this order:

```text
explicit command-line value
  > selected project profile
  > project default
  > PanGloss built-in default
```

The absence of configuration is a normal state, not a warning. Reports record the effective values,
their provenance (`cli`, `project-profile`, `project-default`, or `builtin`), and the project-config
digest when a project configuration participated.

## Acknowledged build findings

An explicit acknowledgement may remove a matching reviewed finding from the default human-facing
build warning stream. It does not delete the finding or rewrite the producing health/readiness/rule
profile artifact. Machine-readable output retains:

- the original finding and severity;
- the acknowledgement event and rationale;
- the match scope and accepted bound;
- the resulting attention state;
- whether the finding was hidden from the default presentation.

Commands provide a way to show acknowledged findings, and summary output reports their count. A
stale, expired, context-mismatched, or worsened finding resurfaces automatically.

## Still unresolved

The severity boundary is not yet decided: whether acknowledgements may suppress only non-gating
`Info`/`Warning` presentation, or may also bypass `Error`/`Critical` admission failures. PanGloss
already has a separate explicit capability/health override model for the latter, so the two actions
must not become ambiguous.
