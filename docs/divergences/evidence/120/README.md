# Evidence for divergence 120 (`DoNotUseForParsing`)

Run on 2026-11-02 against the installed FieldWorks 9.3.10.1452 (`C:\Program Files\SIL\FieldWorks 9`),
HermitCrab engine 3.8.2.0, XAMPLE engine 3.12.23.21 (the pins in `tools/xample-projector/FieldWorksPins.cs`),
`tools/xample-projector` and `pangloss` both built from PanGloss `f4ed795c` (v0.14.0).

## Result

Word list `words.txt`: `pat`. One stem entry `pat` (one `MoStemAllomorph`, one `MoStemMsa`, one part of
speech), no affixes, no rules. The two projects differ in one line of `project.fwdata`,
`<DoNotUseForParsing val="False" />` against `val="True"`.

| Engine | flag false (`unflagged/`) | flag true (`flagged/`) |
|---|---|---|
| FieldWorks HermitCrab (`HCLoader.Load`, `Morpher.ParseWord`) | 1 analysis | 1 analysis |
| FieldWorks XAMPLE (`XAmpleManagedWrapper`, real `xample.dll`) | 1 analysis | 1 analysis |
| PanGloss (`pangloss import`, then `parse`) | 1 analysis (`\|pat`) | 0 analyses (`-`), 1 entry excluded |

FieldWorks parses `pat` through the flagged entry, with the same allomorph and MSA. All five files the
`project` command generates (HC XML, XAMPLE `adctl`, `gram`, `lex`, debugger XSL) have identical SHA-256
for both projects (`fieldworks/projection-sha256.txt` lists the four committed ones; only `response.json`,
which records the source hash, differs): the flag does not reach the HermitCrab grammar or the XAMPLE lexicon. In the flagged
project `lex.txt` still lists `\a pat`.

## Files

- `do-not-use-for-parsing.grammar.xml`: a one-entry HermitCrab grammar with synthetic segments (`p`, `a`, `t`).
- `unflagged/`, `flagged/`: the FieldWorks projects. `project.fwdata` is the output of
  `XampleProjector author` on that grammar (`fieldworks/author-response.json`) with the flag set to `True`
  by editing that one line in `flagged/` (`diff unflagged/project.fwdata flagged/project.fwdata`). The flag
  is an ordinary LCM boolean; FieldWorks loaded the edited file without complaint. `WritingSystemStore/`
  holds the four pruned LDML files copied from `conformance-staging/underdefined/01-root-letter/fieldworks-authored/`
  (the authoring run's own LDML files are 1.4 MB each).
- `fieldworks/`: `<project>.parse-hc.json` and `<project>.parse-xample.json` for `unflagged` and `flagged`,
  `author-response.json` from the authoring run, and `out/`, the HC XML and XAMPLE `adctl`, `gram` and `lex` files
  that the `project` command generated (byte-identical for both projects; `projection-sha256.txt` lists their
  SHA-256). The XAMPLE debugger stylesheet (86 KB, SHA-256 `9dc070a515ad...`) was identical too and is not committed.
- `pangloss/`: the captured `pangloss` output for each project.

## Commands

From a PanGloss checkout of `f4ed795c` (v0.14.0), Windows PowerShell, `POWERSHELL_TELEMETRY_OPTOUT=1`,
FieldWorks 9.3.10 installed at `C:\Program Files\SIL\FieldWorks 9`. The runs use copies in `$work`, because
FieldWorks writes next to the project.

```powershell
$ev   = "$PWD\docs\divergences\evidence\120"
$work = "$env:TEMP\dnp"
$xp   = "$PWD\tools\xample-projector\bin\Debug\XampleProjector.exe"

# build the helper (finds Visual Studio 2022 MSBuild)
& .\tools\xample-projector\build.ps1 -Mode check

# run FieldWorks on both committed projects
foreach ($k in 'unflagged', 'flagged') {
  Copy-Item -Recurse "$ev\$k" "$work\$k"
  & $xp project  --project "$work\$k\project.fwdata" --out-dir "$work\$k\out" --database DoNotUseForParsing
  & $xp parse-hc --project "$work\$k\project.fwdata" --hc-xml "$work\$k\out\DoNotUseForParsing.hc.xml" --words "$ev\words.txt" --out "$work\$k.parse-hc.json"
  & $xp parse    --project "$work\$k\project.fwdata" --project-dir "$work\$k\out" --database DoNotUseForParsing --words "$ev\words.txt" --out "$work\$k.parse-xample.json"
}
# compare with the committed results: $ev\fieldworks\<project>.parse-hc.json, <project>.parse-xample.json, out\* (SHA-256 in projection-sha256.txt)
Get-FileHash "$work\flagged\out\*", "$work\unflagged\out\*" -Algorithm SHA256
```

PanGloss (any platform), with `pangloss` built from the same checkout (`pg.ps1 -Mode build -Package pg-cli`):

```
for k in unflagged flagged; do
  pangloss import $ev/$k/project.fwdata $work/$k.snapshot.json
  pangloss parse  $work/$k.snapshot.json pat
  pangloss batch  $work/$k.snapshot.json $ev/words.txt $work/$k.batch.tsv
done
```

The captured output is in `pangloss/`. The JSON files record `sourceSha256` of the file FieldWorks read; see Line endings.
`parse-hc.json` rows: `"word": "pat"`, one entry in `analyses` (allomorph `8528cc4a-...`, MSA `f3c5a310-...`),
`engineError: null`, `projectionAgrees: true`, identical in both files apart from `sourceSha256`.

### How the projects were made

An authoring run generates fresh GUIDs and timestamps, so it does not reproduce the committed bytes; the
steps are recorded for provenance.

```powershell
& $xp author --grammar "$ev\do-not-use-for-parsing.grammar.xml" --out-dir "$work\authored" --name DoNotUseForParsing
# the run's own response is fieldworks\author-response.json; the project is authored\DoNotUseForParsing\DoNotUseForParsing.fwdata
foreach ($k in 'unflagged', 'flagged') {
  New-Item -ItemType Directory "$work\made\$k\WritingSystemStore" | Out-Null
  Copy-Item "$work\authored\DoNotUseForParsing\DoNotUseForParsing.fwdata" "$work\made\$k\project.fwdata"
  # pruned LDML in place of the authoring run's 1.4 MB files
  Copy-Item conformance-staging\underdefined\01-root-letter\fieldworks-authored\WritingSystemStore\*.ldml "$work\made\$k\WritingSystemStore"
}
# the only difference between the two projects
(Get-Content -Raw "$work\made\flagged\project.fwdata") -replace '<DoNotUseForParsing val="False" />', '<DoNotUseForParsing val="True" />' |
  Set-Content -NoNewline "$work\made\flagged\project.fwdata"
```

## Line endings

The authoring run wrote CRLF; the repository stores LF (`.gitattributes`), so the committed files differ
in bytes from the ones the JSON hashes (`sourceSha256`) describe. The committed LF files were run again
through `project` and `parse-hc` on Windows (same HC XML hash, `pat` = 1 analysis for both) and through
`pangloss import`/`parse` (1 and 0), with the same results.

## Limits

One word, one entry. This shows that FieldWorks' two parsers load a flagged stem entry; it does not
re-test every entry kind (affixes, variants, complex forms), which the C# source reading in the entry covers.
