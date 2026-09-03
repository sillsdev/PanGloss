# PanGloss storage in a FieldWorks project

Status: design-grill addendum. This supersedes the location proposed in
`pangloss-project-config-and-acknowledgements.md`; the separation of configuration,
acknowledgements, advice, and immutable reports remains current.

## Recommendation

The project boundary is the FieldWorks project directory containing `<name>.fwdata`, not a source
repository. Use FieldWorks' existing configuration area:

```text
<FieldWorks project>/
  <name>.fwdata
  ConfigurationSettings/
    PanGloss/
      project.toml
      Acknowledgements/
        <event-id>.json
```

Discover this directory from the `.fwdata` path. `project.toml` should carry the FieldWorks project
GUID and fail on a mismatched copied sidecar. Machine-local state belongs elsewhere and must not
silently alter the shared semantic pipeline.

## Do not hand-edit `.fwdata`

`.fwdata` is LibLCM persistence, not an extensible application manifest. PanGloss must not add raw
XML elements, attributes, or invented object classes. Any in-project marker would have to be written
through a sanctioned LibLCM model API and transaction, then proven across FieldWorks save, migration,
backup/restore, and real FLExBridge three-way merge.

Motif's `CmResource` precedent is deliberately narrow: the model provides only a Unicode `Name` and
GUID `Version`, suitable for a small external-resource/version marker. It is not a general document
store for PanGloss profiles or acknowledgement evidence.

## Chorus and backup contract

`ConfigurationSettings` is a first-class FieldWorks project folder. FieldWorks backup can include
all files below it when configuration settings are selected. This still does not prove Send/Receive:
Chorus applications explicitly configure file include patterns, and safe concurrent edits require a
file-type-specific merge strategy.

Before PanGloss claims project configuration is shared, the FieldWorks/FLExBridge integration must:

1. include `ConfigurationSettings/PanGloss/**` in the Chorus project configuration;
2. register and version the PanGloss file formats;
3. define conflict behavior for `project.toml` rather than silently choosing one whole file;
4. exercise two real replicas adding and editing the files through Send/Receive;
5. verify FieldWorks backup and restore with configuration settings included.

## Acknowledgements as immutable events

Do not use one mutable `acknowledgements.json` file. Store one immutable file per acknowledgement.
Independent replicas then add distinct files, which Chorus can union without merging two edits to
one JSON object. Revocation, expiry changes, and supersession are new events pointing to the earlier
event; history is not rewritten.

Each event retains its stable ID, FieldWorks project GUID, finding/construct identity, evidence
context, accepted value/severity, author, time, rationale, scope, and revisit conditions. Applying
events changes only the derived attention view, never the source reports or their verdicts.

## Remaining decision

The recommended product contract is “FieldWorks project sidecars, explicitly integrated with
FLExBridge/Chorus,” not “opaque data embedded inside `.fwdata`.” A small LibLCM marker could be added
later for discovery only if testing demonstrates a concrete need; it should not become the data
store.
