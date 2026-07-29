<#
  Covers: Get-CorpusManifest / Get-CorpusRoot / Test-CorpusPresent (rust/tools/_common.ps1) --
  the PowerShell-side mirror of pg_conformance_fixtures::corpus's Rust reader. Uses a synthetic
  manifest + synthetic corpus files under a temp directory; never reads the real
  rust/tools/corpus-manifest.json or samples/data/.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$fakeRepo = New-TestTempDir -Prefix 'pg-corpus-manifest-repo'
$toolsDir = Join-Path $fakeRepo 'rust\tools'
New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null

$manifestJson = @'
{
  "schema_version": 1,
  "corpus_root": "samples/data",
  "corpora": [
    {
      "logical_name": "synthetic",
      "purpose": "a synthetic corpus used only to exercise the manifest reader in isolation",
      "files": [
        { "path": "present.txt", "role": "corpus", "required": true },
        { "path": "absent.txt", "role": "grammar", "required": true },
        { "path": "optional.txt", "role": "extra", "required": false }
      ],
      "requiring_tests": ["synthetic-suite"]
    }
  ]
}
'@
Set-Content -Path (Join-Path $toolsDir 'corpus-manifest.json') -Value $manifestJson

$corpusRoot = New-TestTempDir -Prefix 'pg-corpus-manifest-data'
# -NoNewline: plain Set-Content appends the host's line ending, which would make the byte-count
# assertion below depend on Windows CRLF vs some other host's convention rather than the string.
Set-Content -Path (Join-Path $corpusRoot 'present.txt') -Value 'hello' -NoNewline
# absent.txt and optional.txt deliberately never created.

Test-Case 'Get-CorpusManifest parses the synthetic manifest' {
    $m = Get-CorpusManifest -RepoRoot $fakeRepo
    Assert-Equal 1 $m.schema_version
    Assert-Equal 1 $m.corpora.Count
    Assert-Equal 'synthetic' $m.corpora[0].logical_name
}

Test-Case 'Test-CorpusPresent reports the missing REQUIRED file, ignoring the missing optional one' {
    $m = Get-CorpusManifest -RepoRoot $fakeRepo
    $r = Test-CorpusPresent -RepoRoot $fakeRepo -Manifest $m -CorpusRoot $corpusRoot
    Assert-False $r.Ok
    Assert-Equal 1 $r.Missing.Count
    Assert-Contains $r.Missing 'synthetic:absent.txt'
}

Test-Case 'present files are reported with a name, byte size, and a 12-char sha256 prefix' {
    $m = Get-CorpusManifest -RepoRoot $fakeRepo
    $r = Test-CorpusPresent -RepoRoot $fakeRepo -Manifest $m -CorpusRoot $corpusRoot
    $present = $r.Present | Where-Object { $_.Path -eq 'present.txt' }
    Assert-True ($null -ne $present) 'present.txt must be reported as present'
    Assert-Equal 5 $present.Bytes # "hello" is 5 bytes
    Assert-Equal 12 $present.Sha256Short.Length
}

Test-Case 'once every required file exists, Test-CorpusPresent reports Ok with an empty Missing list' {
    Set-Content -Path (Join-Path $corpusRoot 'absent.txt') -Value 'now present'
    $m = Get-CorpusManifest -RepoRoot $fakeRepo
    $r = Test-CorpusPresent -RepoRoot $fakeRepo -Manifest $m -CorpusRoot $corpusRoot
    Assert-True $r.Ok
    Assert-Equal 0 $r.Missing.Count
}

Test-Case 'PANGLOSS_CORPUS_ROOT overrides the manifest-declared corpus_root' {
    $restore = $env:PANGLOSS_CORPUS_ROOT
    try {
        $env:PANGLOSS_CORPUS_ROOT = $corpusRoot
        $root = Get-CorpusRoot -RepoRoot $fakeRepo
        Assert-Equal $corpusRoot $root
    } finally {
        $env:PANGLOSS_CORPUS_ROOT = $restore
    }
}

Remove-Item -Recurse -Force $fakeRepo -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $corpusRoot -ErrorAction SilentlyContinue

Write-TestSummary
