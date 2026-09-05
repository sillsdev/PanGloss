param(
	[ValidateSet('check', 'test')]
	[string]$Mode = 'check'
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$csproj = Join-Path $root 'XampleProjector.csproj'

# Task 5 (author determinism, strengthened): a .fwdata is plain XML with random guids and
# timestamps sprinkled through it, so a byte diff across two independent 'author' runs is
# meaningless until both are normalized the same way -- known guids become their fixture id
# (readable AND still catches an id resolving to the wrong object), everything else collapses to
# fixed placeholders so only real structural differences survive the diff.
function Get-GuidToFixtureIdMap {
	param($AuthorResponse)
	$map = @{}
	foreach ($prop in $AuthorResponse.guidMap.PSObject.Properties) {
		$map[$prop.Value] = $prop.Name
	}
	return $map
}

# Container elements the .fwdata format writes for an unordered LCM collection (OC/RC) rather
# than an ordered sequence (OS/RS) -- measured against the pilot fixture's own PartOfSpeech
# (AffixSlotsOC), PhPhonemeSet (PhonemesOC, BoundaryMarkersOC), and MoInflAffMsa (SlotsRC) records.
# PrefixSlotsRS/SuffixSlotsRS/AlternateFormsOS are deliberately absent: their order is the exact
# thing this project's "Slot ordering" section and allomorph-ordering rule depend on, so sorting
# them here would hide a real regression instead of removing an artifact.
$script:UnorderedFwdataContainers = 'AffixSlots', 'Phonemes', 'BoundaryMarkers', 'Slots'

function Sort-UnorderedContainerChildren {
	param([string]$RecordText)
	foreach ($container in $script:UnorderedFwdataContainers) {
		$RecordText = [regex]::Replace($RecordText, "(?s)<$container>(.*?)</$container>", {
			param($m)
			$items = [regex]::Matches($m.Groups[1].Value, '<objsur[^/]*/>') | ForEach-Object { $_.Value } | Sort-Object
			"<$container>" + ($items -join '') + "</$container>"
		})
	}
	return $RecordText
}

function Get-NormalizedFwdataText {
	param([string]$Path, [hashtable]$GuidToFixtureId)
	$text = Get-Content -Raw -Path $Path
	foreach ($guid in $GuidToFixtureId.Keys) {
		$text = $text -replace [regex]::Escape($guid), $GuidToFixtureId[$guid]
	}
	$text = $text -replace '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', 'GUID'
	$text = $text -replace '\d{4}-\d{1,2}-\d{1,2} \d{1,2}:\d{2}:\d{2}(\.\d+)?', 'TIMESTAMP'

	# Measured: a .fwdata's top-level <rt> records are written sorted by the object's own guid --
	# sorting the pilot's 151 records by their pre-normalization guid reproduces the on-disk order
	# exactly. Blanking that guid removes the only key the order ever depended on, so two
	# independent runs keep the SAME records in a DIFFERENT order (and the same is true, one level
	# down, of each unordered collection's own <objsur> children); re-sorting both before comparing
	# removes that artifact instead of chasing it as a false failure.
	$firstRtIndex = $text.IndexOf('<rt ')
	if ($firstRtIndex -lt 0) { return $text }
	$header = $text.Substring(0, $firstRtIndex)
	$body = $text.Substring($firstRtIndex)
	$singleLine = [System.Text.RegularExpressions.RegexOptions]::Singleline
	# <rt .../> (self-closing, e.g. FsFeatureSystem) and <rt ...>...</rt> are both valid record
	# shapes; matching only the latter silently glues a self-closing record onto the NEXT record.
	$records = [regex]::Matches($body, '<rt\b[^>]*/>|<rt\b.*?</rt>', $singleLine) |
		ForEach-Object { Sort-UnorderedContainerChildren $_.Value } | Sort-Object
	return $header + ($records -join "`n")
}

# Task 3 slice C part 1 (mutate/parse live proof) helpers.

function Get-DigitBlindLines {
	param([string]$Path)
	(Get-Content -Path $Path) | ForEach-Object { [regex]::Replace($_, '\d+', '#') }
}

# Deleting an unreferenced phoneme record shifts every LATER object's hvo within the same load
# session (bMorphnameIsMsaId=y bakes each MSA's own hvo into adctl.txt/gram.txt/lex.txt as literal
# text -- e.g. "RootPOS109" / "\lx 105") -- measured directly: diffing a base project's XAMPLE
# files against the SAME project's phoneme-deleted clone shows every differing line differs ONLY in
# digit runs, never in surrounding text. So "unaffected by the phoneme deletion" is verified as
# byte-identical OR identical-after-blinding-digits, not raw byte-identity alone.
function Test-XampleFilesEquivalentIgnoringHvoRenumbering {
	param([string]$BaseDir, [string]$CloneDir, [string]$Database, [string]$Label)
	foreach ($name in @('adctl.txt', 'gram.txt', 'lex.txt')) {
		$baseFile = Join-Path $BaseDir "$Database$name"
		$cloneFile = Join-Path $CloneDir "$Database$name"
		$baseRaw = Get-Content -Raw -Path $baseFile
		$cloneRaw = Get-Content -Raw -Path $cloneFile
		if ($baseRaw -ceq $cloneRaw) {
			Write-Host "  $name byte-identical (base vs $Label clone)."
			continue
		}
		$baseBlind = Get-DigitBlindLines -Path $baseFile
		$cloneBlind = Get-DigitBlindLines -Path $cloneFile
		if ($baseBlind.Count -ne $cloneBlind.Count) {
			Write-Error "$Label`: $name line count differs after digit-blinding: base=$($baseBlind.Count) clone=$($cloneBlind.Count)"
			exit 1
		}
		for ($i = 0; $i -lt $baseBlind.Count; $i++) {
			if ($baseBlind[$i] -cne $cloneBlind[$i]) {
				Write-Error "$Label`: $name differs at line $($i + 1) beyond hvo renumbering.`n  base:  $($baseBlind[$i])`n  clone: $($cloneBlind[$i])"
				exit 1
			}
		}
		Write-Host "  $name identical to base except for hvo renumbering (base vs $Label clone; digit-blind compare)."
	}
}

function Assert-HcXmlLacksSegmentDefinition {
	param([string]$BaseHcXmlPath, [string]$CloneHcXmlPath, [string]$Representation, [string]$Label)
	$pattern = "(?s)<SegmentDefinition[^>]*>.*?<Representation>$([regex]::Escape($Representation))</Representation>.*?</SegmentDefinition>"
	$baseText = Get-Content -Raw -Path $BaseHcXmlPath
	$cloneText = Get-Content -Raw -Path $CloneHcXmlPath
	if ($baseText -notmatch $pattern) {
		Write-Error "$Label`: base hc.xml unexpectedly has no SegmentDefinition for '$Representation' -- cannot prove the clone dropped it."
		exit 1
	}
	if ($cloneText -match $pattern) {
		Write-Error "$Label`: clone hc.xml still contains a SegmentDefinition for '$Representation'."
		exit 1
	}
	Write-Host "  clone hc.xml correctly lacks the '$Representation' SegmentDefinition present in the base hc.xml ($Label)."
}

# msaGuid is a real LCM guid (ParseCommand resolves it via a live LcmCache), stable across a
# file-copy clone even though the raw hvo XAmple's XML embeds is only stable within one cache
# session -- so a per-analysis signature built from msaGuids is comparable between the base
# project and a phoneme-deleted clone even though their two parse runs open separate sessions.
function Get-AnalysisSignatures {
	param($ParseResponse, [string]$Word)
	$wordEntry = $ParseResponse.words | Where-Object { $_.word -eq $Word }
	if (-not $wordEntry) { return @() }
	return $wordEntry.analyses | ForEach-Object { ($_.morphemes | ForEach-Object { $_.msaGuid }) -join '+' } | Sort-Object
}

function Find-MSBuild {
	$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
	if (Test-Path $vswhere) {
		$vsPath = & $vswhere -latest -requires Microsoft.Component.MSBuild -property installationPath 2>$null
		if ($vsPath) {
			$msbuild = Join-Path $vsPath 'MSBuild\Current\Bin\MSBuild.exe'
			if (Test-Path $msbuild) { return $msbuild }
		}
	}
	return $null
}

function Invoke-Build {
	param([string[]]$ExtraArgs = @())
	$msbuild = Find-MSBuild
	if ($msbuild) {
		Write-Host "Building with MSBuild: $msbuild"
		& $msbuild $csproj /restore /nologo /verbosity:minimal @ExtraArgs | Out-Host
		return $LASTEXITCODE
	}
	Write-Host 'MSBuild (vswhere) not found; falling back to dotnet build.'
	if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
		Write-Error 'Neither MSBuild nor the dotnet SDK is available. Install one of them.'
		return 1
	}
	& dotnet build $csproj --nologo @ExtraArgs | Out-Host
	return $LASTEXITCODE
}

$exePath = Join-Path $root 'bin\Debug\XampleProjector.exe'

$buildExit = Invoke-Build
if ($buildExit -ne 0) {
	Write-Error "Build failed (exit $buildExit)."
	exit $buildExit
}
Write-Host 'Build OK.'

if ($Mode -eq 'check') {
	exit 0
}

# --- Mode 'test' ---

if (-not (Test-Path $exePath)) {
	Write-Error "Build reported success but $exePath is missing."
	exit 1
}

$capturedResponse = Join-Path $root 'testdata\captured-response.json'
Write-Host "Running --validate-capture against $capturedResponse"
& $exePath --validate-capture $capturedResponse
if ($LASTEXITCODE -ne 0) {
	Write-Error "--validate-capture failed on the checked-in capture (exit $LASTEXITCODE)."
	exit $LASTEXITCODE
}
Write-Host '--validate-capture OK.'

$capturedInspectResponse = Join-Path $root 'testdata\captured-inspect-response.json'
Write-Host "Running --validate-capture against $capturedInspectResponse"
& $exePath --validate-capture $capturedInspectResponse
if ($LASTEXITCODE -ne 0) {
	Write-Error "--validate-capture failed on the checked-in inspect capture (exit $LASTEXITCODE)."
	exit $LASTEXITCODE
}
Write-Host '--validate-capture (inspect) OK.'

$capturedAuthorResponse = Join-Path $root 'testdata\captured-author-response.json'
Write-Host "Running --validate-capture against $capturedAuthorResponse"
& $exePath --validate-capture $capturedAuthorResponse
if ($LASTEXITCODE -ne 0) {
	Write-Error "--validate-capture failed on the checked-in author capture (exit $LASTEXITCODE)."
	exit $LASTEXITCODE
}
Write-Host '--validate-capture (author) OK.'

$fieldWorksDir = $env:PANGLOSS_FIELDWORKS_DIR
if ([string]::IsNullOrEmpty($fieldWorksDir)) { $fieldWorksDir = 'C:\Program Files\SIL\FieldWorks 9' }
$fwProjectsDir = $env:PANGLOSS_FW_PROJECTS_DIR
if ([string]::IsNullOrEmpty($fwProjectsDir)) { $fwProjectsDir = 'C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects' }
$senaSource = Join-Path $fwProjectsDir 'Sena 3'
$senaFwdata = Join-Path $senaSource 'Sena 3.fwdata'

if (-not (Test-Path $fieldWorksDir)) {
	Write-Host "SKIPPED (live run): FieldWorks install not found at $fieldWorksDir"
	exit 0
}

if (-not (Test-Path $senaFwdata)) {
	Write-Host "SKIPPED (Sena3 project/inspect live tests): sample project not found at $senaFwdata"
}
else {
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-test-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
try {
	$projectCopy = Join-Path $tempRoot 'Sena 3'
	Write-Host "Copying $senaSource -> $projectCopy"
	Copy-Item -Path $senaSource -Destination $projectCopy -Recurse -Force

	$outDir = Join-Path $tempRoot 'out'
	New-Item -ItemType Directory -Path $outDir -Force | Out-Null

	$projectFwdata = Join-Path $projectCopy 'Sena 3.fwdata'
	Write-Host "Running: $exePath project --project `"$projectFwdata`" --out-dir `"$outDir`" --database Sena3"
	& $exePath project --project $projectFwdata --out-dir $outDir --database Sena3
	$liveExit = $LASTEXITCODE
	if ($liveExit -ne 0) {
		Write-Error "Live 'project' run failed (exit $liveExit)."
		exit $liveExit
	}

	$expectedFiles = @('Sena3.hc.xml', 'Sena3adctl.txt', 'Sena3gram.txt', 'Sena3lex.txt', 'Sena3XAmpleWordGrammarDebugger.xsl')
	foreach ($name in $expectedFiles) {
		$path = Join-Path $outDir $name
		if (-not (Test-Path $path)) {
			Write-Error "Live run did not produce expected file: $path"
			exit 1
		}
		$len = (Get-Item $path).Length
		if ($len -le 0) {
			Write-Error "Live run produced an empty file: $path"
			exit 1
		}
		Write-Host ("  {0} ({1} bytes)" -f $name, $len)
	}

	$responseJson = Join-Path $outDir 'response.json'
	if (-not (Test-Path $responseJson)) {
		Write-Error "Live run did not produce response.json at $responseJson"
		exit 1
	}

	Write-Host "Validating the live-run capture: $responseJson"
	& $exePath --validate-capture $responseJson
	if ($LASTEXITCODE -ne 0) {
		Write-Error "--validate-capture failed on the live-run response.json (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	Write-Host 'Live run OK.'

	# --- Probe: 'inspect' produces at least one phoneme, each with a real guid ---
	$inspectResponse = Join-Path $tempRoot 'inspect-response.json'
	Write-Host "Running: $exePath inspect --project `"$projectFwdata`" --out `"$inspectResponse`""
	& $exePath inspect --project $projectFwdata --out $inspectResponse
	$inspectExit = $LASTEXITCODE
	if ($inspectExit -ne 0) {
		Write-Error "Live 'inspect' run failed (exit $inspectExit)."
		exit $inspectExit
	}
	$inspectJson = Get-Content $inspectResponse -Raw | ConvertFrom-Json
	$phonemeCount = $inspectJson.phonemes.Count
	if ($phonemeCount -le 0) {
		Write-Error "Live 'inspect' run reported zero phonemes."
		exit 1
	}
	foreach ($phoneme in $inspectJson.phonemes) {
		if ($null -eq $phoneme.guid -or $phoneme.guid.Length -ne 36) {
			Write-Error "Live 'inspect' run reported a phoneme with a malformed guid: '$($phoneme.guid)'"
			exit 1
		}
	}
	Write-Host "Live 'inspect' probe OK: $phonemeCount phoneme(s), all with 36-char guids."

	# --- Probe: two consecutive 'inspect' runs over the same project produce byte-identical JSON
	#     (pins InspectCommand's guid sort as the thing that makes this true) ---
	$inspectResponse2 = Join-Path $tempRoot 'inspect-response-2.json'
	& $exePath inspect --project $projectFwdata --out $inspectResponse2
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Second live 'inspect' run (determinism probe) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	$inspectText1 = Get-Content $inspectResponse -Raw
	$inspectText2 = Get-Content $inspectResponse2 -Raw
	if ($inspectText1 -ne $inspectText2) {
		Write-Error "Determinism probe: two consecutive 'inspect' runs produced different JSON."
		exit 1
	}
	Write-Host "Live 'inspect' determinism probe OK: two runs produced byte-identical JSON ($($inspectText1.Length) bytes)."

	# --- Probe: two consecutive 'project' runs over the same copy agree exactly on every entry
	#     marked deterministic:true, and the sole deterministic:false entry is the GAFAWS OUT file
	#     (pins determinism by EFFECT: same sha256 across independent runs, not by the label alone) ---
	$outDir2 = Join-Path $tempRoot 'out2'
	New-Item -ItemType Directory -Path $outDir2 -Force | Out-Null
	Write-Host "Running second 'project' pass for determinism comparison: $exePath project --project `"$projectFwdata`" --out-dir `"$outDir2`" --database Sena3"
	& $exePath project --project $projectFwdata --out-dir $outDir2 --database Sena3 | Out-Null
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Second 'project' run (determinism probe) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}

	$response1 = Get-Content $responseJson -Raw | ConvertFrom-Json
	$response2 = Get-Content (Join-Path $outDir2 'response.json') -Raw | ConvertFrom-Json

	if ($response1.generated.Count -ne $response2.generated.Count) {
		Write-Error "Determinism probe: run 1 produced $($response1.generated.Count) generated file(s), run 2 produced $($response2.generated.Count)."
		exit 1
	}
	$nonDeterministicCount = 0
	foreach ($entry1 in $response1.generated) {
		$entry2 = $response2.generated | Where-Object { $_.path -eq $entry1.path }
		if (-not $entry2) {
			Write-Error "Determinism probe: entry '$($entry1.path)' present in run 1 but not run 2."
			exit 1
		}
		if ($entry1.deterministic) {
			if ($entry1.sha256 -ne $entry2.sha256) {
				Write-Error "Determinism probe: '$($entry1.path)' is marked deterministic but sha256 differs between runs ($($entry1.sha256) vs $($entry2.sha256))."
				exit 1
			}
		}
		else {
			$nonDeterministicCount++
			if ($entry1.path -notmatch '^OUT.*gafawsData\.xml$') {
				Write-Error "Determinism probe: the entry marked non-deterministic ('$($entry1.path)') is not the GAFAWS OUT file."
				exit 1
			}
		}
	}
	if ($nonDeterministicCount -ne 1) {
		Write-Error "Determinism probe: expected exactly 1 entry marked deterministic:false, found $nonDeterministicCount."
		exit 1
	}
	Write-Host "Determinism probe OK: $($response1.generated.Count) generated file(s) compared across two runs; exactly 1 correctly marked non-deterministic (the GAFAWS OUT file); every other entry byte-identical."

	# --- Probe: an output-dir collision surfaces as ProjectionException -> exit 5, naming the step ---
	$collisionOutDir = Join-Path $tempRoot 'out-collision'
	New-Item -ItemType Directory -Path $collisionOutDir -Force | Out-Null
	$collisionPath = Join-Path $collisionOutDir 'Sena3adctl.txt'
	New-Item -ItemType Directory -Path $collisionPath -Force | Out-Null
	Write-Host "Running (expected to fail): $exePath project --project `"$projectFwdata`" --out-dir `"$collisionOutDir`" --database Sena3 (Sena3adctl.txt pre-created as a directory)"
	$collisionOutput = & $exePath project --project $projectFwdata --out-dir $collisionOutDir --database Sena3 2>&1
	$collisionExit = $LASTEXITCODE
	$collisionText = ($collisionOutput | Out-String)
	if ($collisionExit -ne 5) {
		Write-Error "Output-dir collision probe: expected exit 5, got $collisionExit. Output:`n$collisionText"
		exit 1
	}
	if ($collisionText -notmatch 'adctl') {
		Write-Error "Output-dir collision probe: exit was 5 but the output does not name 'adctl'. Output:`n$collisionText"
		exit 1
	}
	Write-Host "Output-dir collision probe OK: exit 5, output names 'adctl'."
}
finally {
	Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
}

# --- refusal-before-project-exists probes: GrammarParser.Parse runs before AuthorSession.Run ever
#     creates a .fwdata, so these need FieldWorks (for the pin check in Program.Main) but NEITHER
#     the machine submodule NOR Sena 3 -- both grammars are tiny inline fixtures. ---
$parserRefusalTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-parser-refusal-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $parserRefusalTempRoot -Force | Out-Null
try {
	# Finding 1: MorphemeCoOccurrenceRule referencing an unknown id used to refuse only from
	# inside GrammarAuthor.CreateCoOccurrenceRules, after AuthorSession.Run had already created and
	# locked a real .fwdata. GrammarParser.Parse now refuses it before any project exists.
	$unknownRefGrammarXml = @'
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>UnknownCoOccurrenceRefProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
    <MorphemeCoOccurrenceRules>
      <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="eK" otherMorphemes="doesNotExist" adjacency="anywhere" />
    </MorphemeCoOccurrenceRules>
  </Language>
</HermitCrabInput>
'@
	$unknownRefGrammarPath = Join-Path $parserRefusalTempRoot 'unknown-ref.grammar.xml'
	Set-Content -Path $unknownRefGrammarPath -Value $unknownRefGrammarXml -Encoding utf8
	$unknownRefOutDir = Join-Path $parserRefusalTempRoot 'unknown-ref-out'
	$unknownRefOutput = & $exePath author --grammar $unknownRefGrammarPath --out-dir $unknownRefOutDir --name UnknownRef 2>&1
	$unknownRefExit = $LASTEXITCODE
	$unknownRefText = ($unknownRefOutput | Out-String)
	if ($unknownRefExit -ne 7) {
		Write-Error "Unknown co-occurrence id refusal probe: expected exit 7, got $unknownRefExit. Output:`n$unknownRefText"
		exit 1
	}
	if ($unknownRefText -notmatch 'doesNotExist') {
		Write-Error "Unknown co-occurrence id refusal probe: exit was 7 but output does not name 'doesNotExist'. Output:`n$unknownRefText"
		exit 1
	}
	$unknownRefFwdata = Join-Path $unknownRefOutDir 'UnknownRef\UnknownRef.fwdata'
	if (Test-Path $unknownRefFwdata) {
		Write-Error "Unknown co-occurrence id refusal probe: exit was 7 but a .fwdata was left on disk at $unknownRefFwdata."
		exit 1
	}
	Write-Host "Unknown co-occurrence id refusal probe OK: exit 7, output names 'doesNotExist', no .fwdata on disk."

	# Finding 3: ids are document-global per the DTD (XML ID type), but DtdProcessing.Ignore means
	# nothing enforced that -- a PartOfSpeech and a LexicalEntry sharing one id used to silently
	# collide in AuthorResult.GuidMap. GrammarParser.Parse now refuses the reused id up front.
	$duplicateIdGrammarXml = @'
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>DuplicateIdProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="dup1"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="dup1" partOfSpeech="dup1">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
'@
	$duplicateIdGrammarPath = Join-Path $parserRefusalTempRoot 'duplicate-id.grammar.xml'
	Set-Content -Path $duplicateIdGrammarPath -Value $duplicateIdGrammarXml -Encoding utf8
	$duplicateIdOutDir = Join-Path $parserRefusalTempRoot 'duplicate-id-out'
	$duplicateIdOutput = & $exePath author --grammar $duplicateIdGrammarPath --out-dir $duplicateIdOutDir --name DuplicateId 2>&1
	$duplicateIdExit = $LASTEXITCODE
	$duplicateIdText = ($duplicateIdOutput | Out-String)
	if ($duplicateIdExit -ne 7) {
		Write-Error "Duplicate id refusal probe: expected exit 7, got $duplicateIdExit. Output:`n$duplicateIdText"
		exit 1
	}
	if ($duplicateIdText -notmatch 'dup1') {
		Write-Error "Duplicate id refusal probe: exit was 7 but output does not name 'dup1'. Output:`n$duplicateIdText"
		exit 1
	}
	$duplicateIdFwdata = Join-Path $duplicateIdOutDir 'DuplicateId\DuplicateId.fwdata'
	if (Test-Path $duplicateIdFwdata) {
		Write-Error "Duplicate id refusal probe: exit was 7 but a .fwdata was left on disk at $duplicateIdFwdata."
		exit 1
	}
	Write-Host "Duplicate id refusal probe OK: exit 7, output names 'dup1', no .fwdata on disk."

	# Critical re-review finding: GrammarParser.RegisterId used to track only DTD `id` attributes,
	# but AuthorResult.GuidMap is also keyed by AffixTemplate/Slot Name text -- two AffixTemplates
	# each with a slot named "Root" passed Parse and only then threw mid-Author, after a real
	# .fwdata already existed. GrammarParser.Parse now registers Name text in the same seenIds
	# space, so this refuses before any project exists.
	$duplicateSlotNameGrammarXml = @'
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>DuplicateSlotNameProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrP1">
            <MorphemeId>P1</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrP1s1">
                <MorphologicalInput>
                  <PhoneticSequence><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                  <CopyFromInput />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrP2">
            <MorphemeId>P2</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrP2s1">
                <MorphologicalInput>
                  <PhoneticSequence><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments>
                  <CopyFromInput />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>TemplateA</Name>
            <Slot optional="true" morphologicalRules="mrP1"><Name>Root</Name></Slot>
          </AffixTemplate>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>TemplateB</Name>
            <Slot optional="true" morphologicalRules="mrP2"><Name>Root</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
'@
	$duplicateSlotNameGrammarPath = Join-Path $parserRefusalTempRoot 'duplicate-slot-name.grammar.xml'
	Set-Content -Path $duplicateSlotNameGrammarPath -Value $duplicateSlotNameGrammarXml -Encoding utf8
	$duplicateSlotNameOutDir = Join-Path $parserRefusalTempRoot 'duplicate-slot-name-out'
	$duplicateSlotNameOutput = & $exePath author --grammar $duplicateSlotNameGrammarPath --out-dir $duplicateSlotNameOutDir --name DuplicateSlotName 2>&1
	$duplicateSlotNameExit = $LASTEXITCODE
	$duplicateSlotNameText = ($duplicateSlotNameOutput | Out-String)
	if ($duplicateSlotNameExit -ne 7) {
		Write-Error "Duplicate slot-name refusal probe: expected exit 7, got $duplicateSlotNameExit. Output:`n$duplicateSlotNameText"
		exit 1
	}
	if ($duplicateSlotNameText -notmatch 'Root') {
		Write-Error "Duplicate slot-name refusal probe: exit was 7 but output does not name 'Root'. Output:`n$duplicateSlotNameText"
		exit 1
	}
	$duplicateSlotNameFwdata = Join-Path $duplicateSlotNameOutDir 'DuplicateSlotName\DuplicateSlotName.fwdata'
	if (Test-Path $duplicateSlotNameFwdata) {
		Write-Error "Duplicate slot-name refusal probe: exit was 7 but a .fwdata was left on disk at $duplicateSlotNameFwdata."
		exit 1
	}
	Write-Host "Duplicate slot-name refusal probe OK: exit 7, output names 'Root', no .fwdata on disk."
}
finally {
	Remove-Item -Path $parserRefusalTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# --- author / verify-parity live tests (Task 3 slice B): the `machine` conformance submodule's
#     own fixtures, not Sena 3 -- independent of whether Sena3 is reachable above. ---
$conformanceRoot = Join-Path $root '..\..\machine\conformance'
$pilotGrammar = Join-Path $conformanceRoot 'edge-cases\deep-optional-affix-nesting\grammar.xml'
$mprRefusalGrammar = Join-Path $conformanceRoot 'languages\prefixal-discontinuous-slot-dependency\grammar.xml'
$requireRefusalGrammar = Join-Path $conformanceRoot 'languages\suffixing-evidential-adjacency-chain\grammar.xml'

# --- AllomorphCoOccurrenceRule authoring probe (Finding 6: moved above the machine-submodule gate
#     below so it always runs when FieldWorks is present -- this fixture is this tool's own
#     testdata, not part of the machine submodule, and needs no submodule at all): pins the fix for
#     the silent-drop defect (a type="exclude" AllomorphCoOccurrenceRule used to author with no
#     IMoAlloAdhocProhib created and no refusal at all -- see GrammarAuthor.CreateCoOccurrenceRules),
#     and now also proves the exclusion actually binds in the live HC engine, not just that an
#     object was created. ---
$alloCoOccurGrammar = Join-Path $root 'testdata\allomorph-cooccurrence-probe.grammar.xml'
$alloCoOccurTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-allo-cooccur-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $alloCoOccurTempRoot -Force | Out-Null
try {
	$alloCoOccurOutDir = Join-Path $alloCoOccurTempRoot 'allo-cooccur-out'
	New-Item -ItemType Directory -Path $alloCoOccurOutDir -Force | Out-Null
	& $exePath author --grammar $alloCoOccurGrammar --out-dir $alloCoOccurOutDir --name AlloCoOccur
	if ($LASTEXITCODE -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: 'author' failed (exit $LASTEXITCODE)."
		exit 1
	}
	$alloCoOccurResponsePath = Join-Path $alloCoOccurOutDir 'author-response.json'
	$alloCoOccurResponse = Get-Content $alloCoOccurResponsePath -Raw | ConvertFrom-Json
	if ($alloCoOccurResponse.authored.MoAlloAdhocProhib -ne 1) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: expected authored.MoAlloAdhocProhib == 1, got '$($alloCoOccurResponse.authored.MoAlloAdhocProhib)'."
		exit 1
	}
	if (-not ($alloCoOccurResponse.guidMap.PSObject.Properties.Name -contains 'allomorphCoOccurrence[0]')) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: guidMap is missing 'allomorphCoOccurrence[0]'."
		exit 1
	}
	Write-Host "AllomorphCoOccurrenceRule authoring probe OK: exit 0, authored.MoAlloAdhocProhib == 1, guidMap has 'allomorphCoOccurrence[0]'."

	$alloCoOccurFwdata = Join-Path $alloCoOccurOutDir 'AlloCoOccur\AlloCoOccur.fwdata'
	$alloCoOccurProjectedOutDir = Join-Path $alloCoOccurTempRoot 'projected'
	New-Item -ItemType Directory -Path $alloCoOccurProjectedOutDir -Force | Out-Null
	& $exePath project --project $alloCoOccurFwdata --out-dir $alloCoOccurProjectedOutDir --database AlloCoOccur
	if ($LASTEXITCODE -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: 'project' (on the authored project) failed (exit $LASTEXITCODE)."
		exit 1
	}
	$alloCoOccurHcXml = Join-Path $alloCoOccurProjectedOutDir 'AlloCoOccur.hc.xml'

	# The probe grammar has one optional slot (mrP1, prefix "x") over one lexical entry ("k"), and
	# excludes mrP1's own subrule from co-occurring with that entry's allomorph "anywhere" in a
	# word -- the ONLY way to produce "xk" at all is that exact combination, so a correct exclusion
	# makes "xk" unparseable (0 analyses) while the affixless "k" still parses (1 analysis). This is
	# the probe's own designed semantics, not an oracle-confirmed count (this fixture is not a
	# `machine` conformance fixture) -- unlike the pilot fixture's counts below.
	$alloCoOccurVerifyOutput = & $exePath verify-parity --grammar $alloCoOccurGrammar --hc-xml $alloCoOccurHcXml --guid-map $alloCoOccurResponsePath --expect k=1 --expect xk=0
	if ($LASTEXITCODE -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: 'verify-parity' failed (exit $LASTEXITCODE). Output:`n$($alloCoOccurVerifyOutput | Out-String)"
		exit 1
	}
	$alloCoOccurVerifyJson = $alloCoOccurVerifyOutput | Out-String | ConvertFrom-Json
	if ($alloCoOccurVerifyJson.engineAnalysisCounts.k -ne 1 -or $alloCoOccurVerifyJson.engineAnalysisCounts.xk -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: verify-parity reported unexpected engine counts:`n$($alloCoOccurVerifyOutput | Out-String)"
		exit 1
	}
	Write-Host "AllomorphCoOccurrenceRule authoring probe OK: verify-parity confirms the exclusion binds in the live HC engine (k=1, xk=0)."
}
finally {
	Remove-Item -Path $alloCoOccurTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not (Test-Path $pilotGrammar)) {
	Write-Host "SKIPPED (author/verify-parity live tests): machine submodule fixture not found at $pilotGrammar"
	exit 0
}

$authorTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-author-test-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $authorTempRoot -Force | Out-Null
try {
	# --- author the pilot fixture, project it, and verify-parity against the fixture it came from ---
	$authorOutDir = Join-Path $authorTempRoot 'author-out'
	New-Item -ItemType Directory -Path $authorOutDir -Force | Out-Null
	Write-Host "Running: $exePath author --grammar `"$pilotGrammar`" --out-dir `"$authorOutDir`" --name Pilot"
	& $exePath author --grammar $pilotGrammar --out-dir $authorOutDir --name Pilot
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Live 'author' run failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	$authorResponsePath = Join-Path $authorOutDir 'author-response.json'
	& $exePath --validate-capture $authorResponsePath
	if ($LASTEXITCODE -ne 0) {
		Write-Error "--validate-capture failed on the live 'author' response.json (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	Write-Host 'Live author run OK.'

	$pilotFwdata = Join-Path $authorOutDir 'Pilot\Pilot.fwdata'
	$projectedOutDir = Join-Path $authorTempRoot 'projected'
	New-Item -ItemType Directory -Path $projectedOutDir -Force | Out-Null
	Write-Host "Running: $exePath project --project `"$pilotFwdata`" --out-dir `"$projectedOutDir`" --database Pilot"
	& $exePath project --project $pilotFwdata --out-dir $projectedOutDir --database Pilot
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Live 'project' run (on the authored pilot project) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	Write-Host 'Live project run (on the authored pilot project) OK.'

	$pilotHcXml = Join-Path $projectedOutDir 'Pilot.hc.xml'
	# The pilot's own oracle-confirmed words.yaml is where 1/924 come from (see README) -- the only
	# two counts this build.ps1 asserts are ones a real oracle run backs, per this repo's oracle
	# discipline; verify-parity itself derives every other expectation from grammar.xml.
	Write-Host "Running: $exePath verify-parity --grammar `"$pilotGrammar`" --hc-xml `"$pilotHcXml`" --guid-map `"$authorResponsePath`" --expect k=1 --expect xxxxxxk=924"
	$verifyOutput = & $exePath verify-parity --grammar $pilotGrammar --hc-xml $pilotHcXml --guid-map $authorResponsePath --expect k=1 --expect xxxxxxk=924
	if ($LASTEXITCODE -ne 0) {
		Write-Error "'verify-parity' failed (exit $LASTEXITCODE). Output:`n$($verifyOutput | Out-String)"
		exit $LASTEXITCODE
	}
	$verifyJson = $verifyOutput | Out-String | ConvertFrom-Json
	if ($verifyJson.morphologicalRuleCount -ne 12 -or $verifyJson.lexicalEntryCount -ne 1 -or $verifyJson.segmentCount -ne 2 -or
		$verifyJson.slotCount -ne 12 -or $verifyJson.xampleLexEntryCount -ne 13 -or
		$verifyJson.engineAnalysisCounts.k -ne 1 -or $verifyJson.engineAnalysisCounts.xxxxxxk -ne 924) {
		Write-Error "verify-parity reported unexpected counts:`n$($verifyOutput | Out-String)"
		exit 1
	}
	Write-Host "verify-parity OK: 12 rules, 1 lex entry, 2 segments, 12 slots (order: $($verifyJson.slotOrderingRule)), 13 XAMPLE lex entries, HC engine 1/xxxxxxk=924 analyses."
	if ($verifyJson.guidMapVerifiedCount -le 0) {
		Write-Error "verify-parity reported guidMapVerifiedCount <= 0 -- the guid-map binding check did not run."
		exit 1
	}
	Write-Host "verify-parity guid-map binding OK: $($verifyJson.guidMapVerifiedCount) guidMap entries checked against the live authored project."

	# --- verify-parity guid-map binding probes: a swapped or emptied guid map must be refused
	#     (exit 8), not silently accepted -- both copies live beside the real author-response.json
	#     so its "projectPath" (relative to that directory) still resolves. ---
	$swappedResponse = Get-Content $authorResponsePath -Raw | ConvertFrom-Json
	$slot1Guid = $swappedResponse.guidMap.slot1
	$slot2Guid = $swappedResponse.guidMap.slot2
	$swappedResponse.guidMap.slot1 = $slot2Guid
	$swappedResponse.guidMap.slot2 = $slot1Guid
	$swappedResponsePath = Join-Path $authorOutDir 'author-response-swapped-slots.json'
	$swappedResponse | ConvertTo-Json -Depth 10 | Set-Content -Path $swappedResponsePath -Encoding utf8
	$swappedOutput = & $exePath verify-parity --grammar $pilotGrammar --hc-xml $pilotHcXml --guid-map $swappedResponsePath --expect k=1 --expect xxxxxxk=924 2>&1
	$swappedExit = $LASTEXITCODE
	$swappedText = ($swappedOutput | Out-String)
	if ($swappedExit -ne 8) {
		Write-Error "verify-parity guid-map binding probe (swapped slots): expected exit 8, got $swappedExit. Output:`n$swappedText"
		exit 1
	}
	if ($swappedText -notmatch 'order') {
		Write-Error "verify-parity guid-map binding probe (swapped slots): exit was 8 but output does not name an order difference. Output:`n$swappedText"
		exit 1
	}
	Write-Host "verify-parity guid-map binding probe (swapped slot1/slot2 guids) OK: exit 8, output names the order difference."

	$emptyResponse = Get-Content $authorResponsePath -Raw | ConvertFrom-Json
	$emptyResponse.guidMap = New-Object PSObject
	$emptyResponsePath = Join-Path $authorOutDir 'author-response-empty-guidmap.json'
	$emptyResponse | ConvertTo-Json -Depth 10 | Set-Content -Path $emptyResponsePath -Encoding utf8
	$emptyOutput = & $exePath verify-parity --grammar $pilotGrammar --hc-xml $pilotHcXml --guid-map $emptyResponsePath --expect k=1 --expect xxxxxxk=924 2>&1
	$emptyExit = $LASTEXITCODE
	$emptyText = ($emptyOutput | Out-String)
	if ($emptyExit -ne 8) {
		Write-Error "verify-parity guid-map binding probe (empty guidMap): expected exit 8, got $emptyExit. Output:`n$emptyText"
		exit 1
	}
	if ($emptyText -notmatch 'missing fixture id') {
		Write-Error "verify-parity guid-map binding probe (empty guidMap): exit was 8 but output does not name the missing key(s). Output:`n$emptyText"
		exit 1
	}
	Write-Host "verify-parity guid-map binding probe (emptied guidMap) OK: exit 8, output names the missing key(s)."

	# --- determinism: author the SAME fixture into a second directory; structure (authored counts,
	#     guidMap key set) must match exactly even though GUIDs/timestamps make the .fwdata bytes differ ---
	$authorOutDir2 = Join-Path $authorTempRoot 'author-out-2'
	New-Item -ItemType Directory -Path $authorOutDir2 -Force | Out-Null
	& $exePath author --grammar $pilotGrammar --out-dir $authorOutDir2 --name Pilot | Out-Null
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Second live 'author' run (determinism probe) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	$authorResponse1 = Get-Content $authorResponsePath -Raw | ConvertFrom-Json
	$authorResponse2 = Get-Content (Join-Path $authorOutDir2 'author-response.json') -Raw | ConvertFrom-Json
	$keys1 = ($authorResponse1.guidMap.PSObject.Properties.Name | Sort-Object) -join ','
	$keys2 = ($authorResponse2.guidMap.PSObject.Properties.Name | Sort-Object) -join ','
	if ($keys1 -ne $keys2) {
		Write-Error "Determinism probe: guidMap key sets differ between two 'author' runs over the same fixture."
		exit 1
	}
	$counts1 = ($authorResponse1.authored.PSObject.Properties | ForEach-Object { "$($_.Name)=$($_.Value)" } | Sort-Object) -join ','
	$counts2 = ($authorResponse2.authored.PSObject.Properties | ForEach-Object { "$($_.Name)=$($_.Value)" } | Sort-Object) -join ','
	if ($counts1 -ne $counts2) {
		Write-Error "Determinism probe: authored counts differ between two 'author' runs over the same fixture."
		exit 1
	}
	if ($authorResponse1.projectSha256 -eq $authorResponse2.projectSha256) {
		Write-Host "Determinism probe note: projectSha256 happened to match across two runs (LibLCM guids are random, so this is not required to differ, only permitted to)."
	}
	Write-Host "Determinism probe OK: $($authorResponse1.guidMap.PSObject.Properties.Name.Count) guidMap keys and every authored count match across two independent 'author' runs (sha256 legitimately differs by run, since LCM guids are assigned randomly)."

	# --- determinism, strengthened: normalize both .fwdata files (known guids -> fixture id,
	#     every remaining guid/timestamp -> a fixed placeholder) and diff them directly, rather
	#     than trusting guidMap key sets and authored counts as a proxy for the project itself ---
	$pilotFwdata2 = Join-Path $authorOutDir2 'Pilot\Pilot.fwdata'
	$normalizedFwdata1 = Get-NormalizedFwdataText -Path $pilotFwdata -GuidToFixtureId (Get-GuidToFixtureIdMap $authorResponse1)
	$normalizedFwdata2 = Get-NormalizedFwdataText -Path $pilotFwdata2 -GuidToFixtureId (Get-GuidToFixtureIdMap $authorResponse2)
	if ($normalizedFwdata1 -ne $normalizedFwdata2) {
		$diffLines = Compare-Object -ReferenceObject ($normalizedFwdata1 -split "`r?`n") -DifferenceObject ($normalizedFwdata2 -split "`r?`n")
		Write-Error "Normalized .fwdata determinism probe: normalized files differ ($($diffLines.Count) differing line(s)). First 10:`n$(($diffLines | Select-Object -First 10 | Out-String))"
		exit 1
	}
	Write-Host "Normalized .fwdata determinism probe OK: two independent 'author' runs produce byte-identical .fwdata text after mapping guidMap guids to fixture ids and blanking every remaining guid/timestamp ($($normalizedFwdata1.Length) chars)."

	# --- refusal probes: exit 7, naming the unsupported construct ---
	if (Test-Path $mprRefusalGrammar) {
		$mprOutDir = Join-Path $authorTempRoot 'mpr-refusal-out'
		New-Item -ItemType Directory -Path $mprOutDir -Force | Out-Null
		$mprOutput = & $exePath author --grammar $mprRefusalGrammar --out-dir $mprOutDir --name MprRefusal 2>&1
		$mprExit = $LASTEXITCODE
		$mprText = ($mprOutput | Out-String)
		if ($mprExit -ne 7) {
			Write-Error "MPRFeatures refusal probe: expected exit 7, got $mprExit. Output:`n$mprText"
			exit 1
		}
		if ($mprText -notmatch 'mrModeTrans' -or $mprText -notmatch 'MPRFeatures') {
			Write-Error "MPRFeatures refusal probe: exit was 7 but output does not name mrModeTrans/MPRFeatures. Output:`n$mprText"
			exit 1
		}
		Write-Host "MPRFeatures refusal probe OK: exit 7, output names mrModeTrans's MPRFeatures."
	}
	else {
		Write-Host "SKIPPED (MPRFeatures refusal probe): fixture not found at $mprRefusalGrammar"
	}

	if (Test-Path $requireRefusalGrammar) {
		$requireOutDir = Join-Path $authorTempRoot 'require-refusal-out'
		New-Item -ItemType Directory -Path $requireOutDir -Force | Out-Null
		$requireOutput = & $exePath author --grammar $requireRefusalGrammar --out-dir $requireOutDir --name RequireRefusal 2>&1
		$requireExit = $LASTEXITCODE
		$requireText = ($requireOutput | Out-String)
		if ($requireExit -ne 7) {
			Write-Error "type=`"require`" refusal probe: expected exit 7, got $requireExit. Output:`n$requireText"
			exit 1
		}
		if ($requireText -notmatch 'type="require"') {
			Write-Error "type=`"require`" refusal probe: exit was 7 but output does not name type=`"require`". Output:`n$requireText"
			exit 1
		}
		Write-Host "type=`"require`" refusal probe OK: exit 7, output names type=`"require`"."
	}
	else {
		Write-Host "SKIPPED (type=`"require`" refusal probe): fixture not found at $requireRefusalGrammar"
	}
}
finally {
	Remove-Item -Path $authorTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Task 3 slice C part 1 live proof: 'mutate' + 'parse' against a freshly authored pilot
#     project. Skipped (with reason) only when FieldWorks itself is absent -- already checked at
#     the top of this script, so reaching here means FieldWorks is present. The Machine grammar
#     root defaults to the path named in this slice's own task brief and is independently
#     overridable via PANGLOSS_MACHINE_DIR, distinct from this repo's own `machine` submodule
#     ($conformanceRoot above) -- Part 2 swaps in the checked-in copy of this same fixture data. ---
$machineRootForMutateParse = $env:PANGLOSS_MACHINE_DIR
if ([string]::IsNullOrEmpty($machineRootForMutateParse)) { $machineRootForMutateParse = 'C:\Users\johnm\Documents\repos\machine' }
$mutateParseConformanceDir = Join-Path $machineRootForMutateParse 'conformance'
$mutateParseGrammar = Join-Path $mutateParseConformanceDir 'edge-cases\deep-optional-affix-nesting\grammar.xml'

if (-not (Test-Path $mutateParseGrammar)) {
	Write-Host "SKIPPED (mutate/parse live proof): grammar not found at $mutateParseGrammar"
	exit 0
}

$mpTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-mutate-parse-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $mpTempRoot -Force | Out-Null
try {
	# --- 1. Author the pilot fixture as the base project for this part. ---
	$baseAuthorOut = Join-Path $mpTempRoot 'base-author'
	New-Item -ItemType Directory -Path $baseAuthorOut -Force | Out-Null
	& $exePath author --grammar $mutateParseGrammar --out-dir $baseAuthorOut --name MutateParseBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'author' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseFwdata = Join-Path $baseAuthorOut 'MutateParseBase\MutateParseBase.fwdata'
	$baseSha256Before = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
	Write-Host "mutate/parse live proof: base project authored, sha256 $baseSha256Before"

	# --- Record the "k" phoneme guid via inspect. ---
	$baseInspectPath = Join-Path $mpTempRoot 'base-inspect.json'
	& $exePath inspect --project $baseFwdata --out $baseInspectPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'inspect' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseInspect = Get-Content $baseInspectPath -Raw | ConvertFrom-Json
	$kPhoneme = $baseInspect.phonemes | Where-Object { $_.representations -contains 'k' } | Select-Object -First 1
	if (-not $kPhoneme) { Write-Error "mutate/parse live proof: no phoneme with representation 'k' found in $baseInspectPath"; exit 1 }
	$kGuid = $kPhoneme.guid
	Write-Host "mutate/parse live proof: 'k' phoneme guid = $kGuid"

	# --- project the base project once: gives the byte-identity/hc.xml baseline AND parse's own adctl/gram/lex. ---
	$baseProjectedOut = Join-Path $mpTempRoot 'base-projected'
	New-Item -ItemType Directory -Path $baseProjectedOut -Force | Out-Null
	& $exePath project --project $baseFwdata --out-dir $baseProjectedOut --database MPBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'project' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }

	# --- Case A: remove-k-only ---
	$removeKRequestObj = @{
		schemaVersion = 1
		caseId        = 'remove-k-only'
		baseSha256    = $baseSha256Before
		operations    = @(@{ op = 'remove_phoneme'; guid = $kGuid; assertRepresentations = @('k'); requireUnreferenced = $true })
	}
	$removeKRequestPath = Join-Path $mpTempRoot 'remove-k-only-request.json'
	($removeKRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $removeKRequestPath -Encoding utf8
	$removeKOutDir = Join-Path $mpTempRoot 'remove-k-only-out'
	& $exePath mutate --project $baseFwdata --request $removeKRequestPath --out-dir $removeKOutDir
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'remove-k-only' mutate failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$removeKResponsePath = Join-Path $removeKOutDir 'mutation-response.json'
	& $exePath --validate-capture $removeKResponsePath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: --validate-capture failed on remove-k-only's mutation-response.json (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$removeKResponse = Get-Content $removeKResponsePath -Raw | ConvertFrom-Json
	if ($removeKResponse.deletedCount -ne 1) { Write-Error "remove-k-only: expected deletedCount 1, got $($removeKResponse.deletedCount)"; exit 1 }
	if ($removeKResponse.removed.Count -ne 1 -or $removeKResponse.removed[0].representations.Count -ne 1 -or $removeKResponse.removed[0].representations[0] -ne 'k') {
		Write-Error "remove-k-only: unexpected removed[]:`n$($removeKResponse.removed | ConvertTo-Json -Depth 5)"
		exit 1
	}
	if (-not $removeKResponse.reopened) { Write-Error "remove-k-only: reopened was not true"; exit 1 }
	$baseSha256AfterA = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
	if ($baseSha256AfterA -ne $baseSha256Before) { Write-Error "remove-k-only: SOURCE PROJECT WAS MODIFIED (sha256 $baseSha256Before -> $baseSha256AfterA)"; exit 1 }
	Write-Host "mutate 'remove-k-only' OK: deletedCount=1, removed=['k'], reopened=true, source sha256 unchanged ($baseSha256Before)."

	$removeKClonedFwdata = Join-Path $removeKOutDir $removeKResponse.materializedProjectPath
	$removeKProjectedOut = Join-Path $mpTempRoot 'remove-k-only-projected'
	New-Item -ItemType Directory -Path $removeKProjectedOut -Force | Out-Null
	& $exePath project --project $removeKClonedFwdata --out-dir $removeKProjectedOut --database MPBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'project' on the remove-k-only clone failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	Test-XampleFilesEquivalentIgnoringHvoRenumbering -BaseDir $baseProjectedOut -CloneDir $removeKProjectedOut -Database 'MPBase' -Label 'remove-k-only'
	Assert-HcXmlLacksSegmentDefinition -BaseHcXmlPath (Join-Path $baseProjectedOut 'MPBase.hc.xml') -CloneHcXmlPath (Join-Path $removeKProjectedOut 'MPBase.hc.xml') -Representation 'k' -Label 'remove-k-only'

	# --- Case B: empty-phoneme-inventory ---
	$emptyRequestObj = @{
		schemaVersion = 1
		caseId        = 'empty-phoneme-inventory'
		baseSha256    = $baseSha256Before
		operations    = @(@{ op = 'remove_all_phonemes'; requireUnreferenced = $true })
	}
	$emptyRequestPath = Join-Path $mpTempRoot 'empty-phoneme-inventory-request.json'
	($emptyRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $emptyRequestPath -Encoding utf8
	$emptyOutDir = Join-Path $mpTempRoot 'empty-phoneme-inventory-out'
	& $exePath mutate --project $baseFwdata --request $emptyRequestPath --out-dir $emptyOutDir
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'empty-phoneme-inventory' mutate failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$emptyResponsePath = Join-Path $emptyOutDir 'mutation-response.json'
	& $exePath --validate-capture $emptyResponsePath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: --validate-capture failed on empty-phoneme-inventory's mutation-response.json (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$emptyResponse = Get-Content $emptyResponsePath -Raw | ConvertFrom-Json
	if ($emptyResponse.deletedCount -ne 2) { Write-Error "empty-phoneme-inventory: expected deletedCount 2, got $($emptyResponse.deletedCount)"; exit 1 }
	$emptyReps = ($emptyResponse.removed | ForEach-Object { $_.representations[0] } | Sort-Object) -join ','
	if ($emptyReps -ne 'k,x') { Write-Error "empty-phoneme-inventory: expected removed representations k,x -- got $emptyReps"; exit 1 }
	if (-not $emptyResponse.reopened) { Write-Error "empty-phoneme-inventory: reopened was not true"; exit 1 }
	$baseSha256AfterB = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
	if ($baseSha256AfterB -ne $baseSha256Before) { Write-Error "empty-phoneme-inventory: SOURCE PROJECT WAS MODIFIED (sha256 $baseSha256Before -> $baseSha256AfterB)"; exit 1 }
	Write-Host "mutate 'empty-phoneme-inventory' OK: deletedCount=2, removed=[k,x], reopened=true, source sha256 unchanged ($baseSha256Before)."

	$emptyClonedFwdata = Join-Path $emptyOutDir $emptyResponse.materializedProjectPath
	$emptyProjectedOut = Join-Path $mpTempRoot 'empty-phoneme-inventory-projected'
	New-Item -ItemType Directory -Path $emptyProjectedOut -Force | Out-Null
	& $exePath project --project $emptyClonedFwdata --out-dir $emptyProjectedOut --database MPBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'project' on the empty-phoneme-inventory clone failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	Test-XampleFilesEquivalentIgnoringHvoRenumbering -BaseDir $baseProjectedOut -CloneDir $emptyProjectedOut -Database 'MPBase' -Label 'empty-phoneme-inventory'
	Assert-HcXmlLacksSegmentDefinition -BaseHcXmlPath (Join-Path $baseProjectedOut 'MPBase.hc.xml') -CloneHcXmlPath (Join-Path $emptyProjectedOut 'MPBase.hc.xml') -Representation 'k' -Label 'empty-phoneme-inventory'

	# --- Negative: a fixture with a SegmentNaturalClass, requireUnreferenced must refuse and delete nothing. ---
	$negativeCandidates = @('disjunctive-recheck', 'free-fluctuating-allomorph-pair', 'strrep-identity', 'diacritic-segments', 'loader-pattern-shapes')
	$negativeFixtureUsed = $null
	$negativeFwdata = $null
	foreach ($candidate in $negativeCandidates) {
		$candidateGrammar = Join-Path $mutateParseConformanceDir "edge-cases\$candidate\grammar.xml"
		if (-not (Test-Path $candidateGrammar)) {
			Write-Host "  negative-probe candidate '$candidate': fixture not found at $candidateGrammar, skipping."
			continue
		}
		$candidateOutDir = Join-Path $mpTempRoot "negative-author-$candidate"
		$candidateOutput = & $exePath author --grammar $candidateGrammar --out-dir $candidateOutDir --name NegProbe 2>&1
		if ($LASTEXITCODE -eq 0) {
			$negativeFixtureUsed = $candidate
			$negativeFwdata = Join-Path $candidateOutDir 'NegProbe\NegProbe.fwdata'
			Write-Host "  negative-probe candidate '$candidate': authored successfully, using it."
			break
		}
		Write-Host "  negative-probe candidate '$candidate': refused ($(($candidateOutput | Out-String).Trim())), trying next."
	}
	if (-not $negativeFixtureUsed) {
		Write-Error "mutate/parse live proof: none of the negative-probe candidate fixtures ($($negativeCandidates -join ', ')) could be authored."
		exit 1
	}

	$negativeInspectPath = Join-Path $mpTempRoot 'negative-inspect-before.json'
	& $exePath inspect --project $negativeFwdata --out $negativeInspectPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: negative-probe 'inspect' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$negativePhonemeCountBefore = (Get-Content $negativeInspectPath -Raw | ConvertFrom-Json).phonemes.Count

	$negativeSha256 = (Get-FileHash -Algorithm SHA256 -Path $negativeFwdata).Hash.ToLowerInvariant()
	$negativeRequestObj = @{
		schemaVersion = 1
		caseId        = 'negative-referenced-phoneme'
		baseSha256    = $negativeSha256
		operations    = @(@{ op = 'remove_all_phonemes'; requireUnreferenced = $true })
	}
	$negativeRequestPath = Join-Path $mpTempRoot 'negative-request.json'
	($negativeRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $negativeRequestPath -Encoding utf8
	$negativeOutDir = Join-Path $mpTempRoot 'negative-out'
	$negativeOutput = & $exePath mutate --project $negativeFwdata --request $negativeRequestPath --out-dir $negativeOutDir 2>&1
	$negativeExit = $LASTEXITCODE
	$negativeText = ($negativeOutput | Out-String)
	if ($negativeExit -ne 9) { Write-Error "negative probe ($negativeFixtureUsed): expected exit 9, got $negativeExit. Output:`n$negativeText"; exit 1 }
	if ($negativeText -notmatch 'mutation\.referenced-phoneme' -or $negativeText -notmatch 'PhNCSegments') {
		Write-Error "negative probe ($negativeFixtureUsed): exit was 9 but output does not name mutation.referenced-phoneme/PhNCSegments. Output:`n$negativeText"
		exit 1
	}
	$negativeClonedFwdata = Join-Path $negativeOutDir 'NegProbe\NegProbe.fwdata'
	if (-not (Test-Path $negativeClonedFwdata)) { Write-Error "negative probe ($negativeFixtureUsed): refused but no clone was left at $negativeClonedFwdata"; exit 1 }
	$negativeInspectAfterPath = Join-Path $mpTempRoot 'negative-inspect-after.json'
	& $exePath inspect --project $negativeClonedFwdata --out $negativeInspectAfterPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: negative-probe clone 'inspect' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$negativePhonemeCountAfter = (Get-Content $negativeInspectAfterPath -Raw | ConvertFrom-Json).phonemes.Count
	if ($negativePhonemeCountAfter -ne $negativePhonemeCountBefore) {
		Write-Error "negative probe ($negativeFixtureUsed): phoneme count changed ($negativePhonemeCountBefore -> $negativePhonemeCountAfter) despite the refusal"
		exit 1
	}
	Write-Host "negative referenced-phoneme probe OK ($negativeFixtureUsed): exit 9, names mutation.referenced-phoneme + PhNCSegments, phoneme count unchanged ($negativePhonemeCountBefore)."

	# --- parse: the real XAMPLE engine over the base project's own generated files ---
	$mpWordsPath = Join-Path $mpTempRoot 'words.txt'
	Set-Content -Path $mpWordsPath -Value @('k', 'xxxxxxk') -Encoding utf8
	$baseParseOutPath = Join-Path $mpTempRoot 'base-parse.json'
	& $exePath parse --project $baseFwdata --project-dir $baseProjectedOut --database MPBase --words $mpWordsPath --out $baseParseOutPath --max-analyses 2000 --max-prefixes 12
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'parse' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseParse = Get-Content $baseParseOutPath -Raw | ConvertFrom-Json
	$kParse = $baseParse.words | Where-Object { $_.word -eq 'k' }
	$xxParse = $baseParse.words | Where-Object { $_.word -eq 'xxxxxxk' }
	if ($kParse.analyses.Count -ne 1) { Write-Error "parse: expected exactly 1 analysis for 'k', got $($kParse.analyses.Count)"; exit 1 }
	Write-Host "parse OK: engineVersion=$($baseParse.engineVersion), effective parameters: $($baseParse.parameters | ConvertTo-Json -Compress)"
	Write-Host "parse OK: 'k' = $($kParse.analyses.Count) analysis (analyses), reachedMaxAnalyses=$($kParse.reachedMaxAnalyses), engineError=$($kParse.engineError)"
	Write-Host "parse (reported as-is, never massaged): 'xxxxxxk' = $($xxParse.analyses.Count) analyses, reachedMaxAnalyses=$($xxParse.reachedMaxAnalyses), engineError=$($xxParse.engineError) (the HC oracle's own count for this word is 924 -- XAMPLE is a different engine and is not expected to match it)."

	# --- parse the empty-phoneme-inventory clone's own files; multiset must match the base's ---
	$cloneParseOutPath = Join-Path $mpTempRoot 'clone-parse.json'
	& $exePath parse --project $emptyClonedFwdata --project-dir $emptyProjectedOut --database MPBase --words $mpWordsPath --out $cloneParseOutPath --max-analyses 2000 --max-prefixes 12
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: clone 'parse' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$cloneParse = Get-Content $cloneParseOutPath -Raw | ConvertFrom-Json
	foreach ($word in @('k', 'xxxxxxk')) {
		$baseSigs = Get-AnalysisSignatures -ParseResponse $baseParse -Word $word
		$cloneSigs = Get-AnalysisSignatures -ParseResponse $cloneParse -Word $word
		if (($baseSigs -join '|') -ne ($cloneSigs -join '|')) {
			Write-Error "parse multiset comparison: '$word' differs between base ($($baseSigs.Count) analyses) and empty-phoneme-inventory clone ($($cloneSigs.Count) analyses)."
			exit 1
		}
	}
	Write-Host "parse multiset comparison OK: base and empty-phoneme-inventory clone produce identical analysis multisets for 'k' ($($kParse.analyses.Count)) and 'xxxxxxk' ($($xxParse.analyses.Count))."
}
finally {
	Remove-Item -Path $mpTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

exit 0
