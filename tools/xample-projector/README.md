# XampleProjector

A version-pinned managed helper that opens a FieldWorks 9 project via LibLCM and produces the
same two real FieldWorks projections of it that PanGloss needs to measure XAMPLE-to-HC migration
differences: the HC XML (`HCLoader` + `XmlLanguageWriter`) and the XAMPLE control/dictionary/
grammar files (the same XSL transforms and GAFAWS step `M3ToXAmpleTransformer`/`XAmpleParser`
drive internally, replicated here because that class is `internal`). Both sides read the same
opened `LcmCache`, so they can never diverge on which project state they saw.

This slice (Task 3, slice A) ships the tool scaffold plus three subcommands: `inspect` (read-only
phonology survey), `project` (full HC + XAMPLE projection), and `--validate-capture` (portable,
FieldWorks-free schema check). A later slice adds LibLCM-driven phoneme mutation and direct
`xample64.dll` parsing.

## Contract

Every producing mode writes a JSON response with `schemaVersion: 1`.

`inspect --project <path-to-.fwdata> --out <response.json>`:
```
{
  "schemaVersion": 1,
  "fieldWorksVersion": "9.3.10.26161",
  "sourcePath": "...", "sourceSha256": "<64 hex>",
  "projectName": "...", "activeParser": "HC" | "XAmple",
  "phonemes": [{ "guid", "representations": ["..."], "inboundReferences": [{ "guid", "class" }] }],
  "boundaryMarkers": [ ... same shape as phonemes ... ],
  "naturalClasses": [{ "guid", "name", "kind": "segments" | "features", "memberGuids": [...] }],
  "diagnostics": ["..."]
}
```

`project --project <path-to-.fwdata> --out-dir <dir> --database <name>` writes, into `<dir>`
(never `%TEMP%`, unlike the FieldWorks internals this replicates): `<database>.hc.xml`,
`<database>adctl.txt`, `<database>gram.txt`, `<database>lex.txt`,
`<database>XAmpleWordGrammarDebugger.xsl`, any GAFAWS intermediate files a POS-with-templates
pass produced, and `response.json`:
```
{
  "schemaVersion": 1, "mode": "project",
  "fieldWorksVersion": "...", "assemblyVersions": { "<dll>": "<file version>", ... },
  "sourcePath": "...", "sourceSha256": "<64 hex>", "database": "...",
  "generated": [{ "path", "sha256": "<64 hex>", "bytes" }],
  "hcLoadDiagnostics": [{ "kind", "message" }],
  "diagnostics": []
}
```
Every file in `generated` is described from its FINAL on-disk state after all writes complete
(some paths, e.g. the GAFAWS intermediate, are legitimately overwritten once per part-of-speech
that has slotted affix templates, so an early snapshot would go stale by the time the response is
written).

`--validate-capture <response.json>`: schema validation only, no FieldWorks install required.
Checks required fields are present, `schemaVersion == 1`, every `sha256`-shaped field is 64
lowercase hex, and `assemblyVersions`'s keys are exactly the pinned set. This is the portable-CI
path.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | ok |
| 2 | usage |
| 3 | pin mismatch (an assembly/native DLL under the FieldWorks install does not match the pinned file version) |
| 4 | project open failure (missing file, locked by another application, or needs FLEx migration) |
| 5 | projection failure (HCLoader, XmlLanguageWriter, or an XAMPLE transform failed) |
| 6 | capture validation failure |

## Build and run

```powershell
& .\tools\xample-projector\build.ps1 -Mode check   # restore + build only
& .\tools\xample-projector\build.ps1 -Mode test    # build, --validate-capture the checked-in
                                                    # testdata capture, and (if a FieldWorks
                                                    # install and the Sena 3 sample project are
                                                    # both reachable) a live 'project' run against
                                                    # a throwaway copy of Sena 3
```
`build.ps1` locates MSBuild via `vswhere.exe` (preferring the Visual Studio toolchain this project
was built against) and falls back to `dotnet build` if MSBuild is unavailable.

FieldWorks install directory: `$env:PANGLOSS_FIELDWORKS_DIR`, default
`C:\Program Files\SIL\FieldWorks 9`. Sample-project directory for the live test:
`$env:PANGLOSS_FW_PROJECTS_DIR`, default `<FieldWorks source checkout>\DistFiles\Projects`.

## Pinned versions

This tool was built and verified against FieldWorks 9.3.10 (net48, x64). At startup (every mode
except `--validate-capture`) it checks the following file versions under the FieldWorks
directory and refuses (exit 3) on any mismatch, printing both versions:

| File | Expected file version |
|---|---|
| `ParserCore.dll` | 9.3.10.26161 |
| `SIL.LCModel.dll` | 11.0.0.55167 |
| `SIL.Machine.dll` | 3.8.2.0 |
| `SIL.Machine.Morphology.HermitCrab.dll` | 3.7.4.0 |
| `XAmpleManagedWrapper.dll` | 9.3.10.26161 |
| `xample64.dll` | 3.12.23.21 |

## Runtime probing

This tool references FieldWorks assemblies by `HintPath` with `Private=false` (it never copies
FieldWorks's 100+ DLLs into its own output). At runtime it widens the native DLL search path with
`SetDllDirectory` (so ICU, and in a later slice `xample.dll`, resolve) and registers an
`AppDomain.AssemblyResolve` handler that loads a missing managed assembly by simple name from the
FieldWorks directory. Both are set up in `Program.Main` before any FieldWorks type is touched.

## Provenance

`ProjectIdentifier.cs`, `NullFdoDirectories.cs`, `NullThreadedProgress.cs`, and
`DiagnosticLogger.cs` are adapted from FieldWorks's own headless tool,
`Src\GenerateHCConfig\{ProjectIdentifier,NullFdoDirectories,NullThreadedProgress,ConsoleLogger}.cs`.
`XampleProjection.cs` replicates the XSL/GAFAWS driving sequence from
`Src\LexText\ParserCore\M3ToXAmpleTransformer.cs` and `XAmpleParser.cs` (both drive an `internal`
class this tool cannot reference directly), parameterized to write into a caller-chosen directory
instead of the hardcoded `%TEMP%` the original always uses.
