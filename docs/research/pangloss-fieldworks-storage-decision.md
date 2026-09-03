# PanGloss FieldWorks storage decision

Date: 2026-08-11

## Accepted

The PanGloss project is a FieldWorks project, identified by its `.fwdata` file and FieldWorks project
GUID. PanGloss project-owned configuration lives outside `.fwdata` under:

```text
ConfigurationSettings/
  PanGloss/
    project.toml
    Acknowledgements/
      <event-id>.json
```

The configuration, immutable acknowledgement-event, neutral advice-option, evidence-context, and
derived acknowledgement-state shapes in the associated research notes are accepted as the current
design direction.

PanGloss will not inject arbitrary XML or invented objects into `.fwdata`. A sanctioned LibLCM
discovery marker remains a possible later addition, not the configuration or acknowledgement store.

## Deferred

Explicit FLExBridge/Chorus registration and merge handling for
`ConfigurationSettings/PanGloss/**` is required for the eventual synchronized-project contract, but
is deferred from the first implementation slice.

Until that work is implemented and verified through real two-replica Send/Receive tests:

- PanGloss describes these files as **project-local**, not Chorus-synchronized;
- no UI, report, or documentation claims that acknowledgements follow collaborators;
- generated analytical reports remain outside Chorus;
- FieldWorks backup includes the files only when configuration settings are included;
- the schemas and file layout must remain forward-compatible with later Chorus registration.

## Next unresolved lifecycle question

Whether ordinary read-only PanGloss commands may create missing project configuration, or whether
creation requires an explicit initialization action, remains undecided.
