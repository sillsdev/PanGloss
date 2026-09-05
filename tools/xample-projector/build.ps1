param(
	[ValidateSet('check', 'test')]
	[string]$Mode = 'check'
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$csproj = Join-Path $root 'XampleProjector.csproj'

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

# --- author / verify-parity live tests (Task 3 slice B): the `machine` conformance submodule's
#     own fixtures, not Sena 3 -- independent of whether Sena3 is reachable above. ---
$conformanceRoot = Join-Path $root '..\..\machine\conformance'
$pilotGrammar = Join-Path $conformanceRoot 'edge-cases\deep-optional-affix-nesting\grammar.xml'
$mprRefusalGrammar = Join-Path $conformanceRoot 'languages\prefixal-discontinuous-slot-dependency\grammar.xml'
$requireRefusalGrammar = Join-Path $conformanceRoot 'languages\suffixing-evidential-adjacency-chain\grammar.xml'

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
	Write-Host "Running: $exePath verify-parity --grammar `"$pilotGrammar`" --hc-xml `"$pilotHcXml`" --guid-map `"$authorResponsePath`""
	$verifyOutput = & $exePath verify-parity --grammar $pilotGrammar --hc-xml $pilotHcXml --guid-map $authorResponsePath
	if ($LASTEXITCODE -ne 0) {
		Write-Error "'verify-parity' failed (exit $LASTEXITCODE). Output:`n$($verifyOutput | Out-String)"
		exit $LASTEXITCODE
	}
	$verifyJson = $verifyOutput | Out-String | ConvertFrom-Json
	if ($verifyJson.morphologicalRuleCount -ne 12 -or $verifyJson.lexicalEntryCount -ne 1 -or $verifyJson.segmentCount -ne 2 -or
		$verifyJson.slotCount -ne 12 -or $verifyJson.xampleLexEntryCount -ne 13 -or
		$verifyJson.engineAnalysisCountK -ne 1 -or $verifyJson.engineAnalysisCountXxxxxxK -ne 924) {
		Write-Error "verify-parity reported unexpected counts:`n$($verifyOutput | Out-String)"
		exit 1
	}
	Write-Host "verify-parity OK: 12 rules, 1 lex entry, 2 segments, 12 slots (order: $($verifyJson.slotOrderingRule)), 13 XAMPLE lex entries, HC engine 1/xxxxxxk=924 analyses."

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

exit 0
