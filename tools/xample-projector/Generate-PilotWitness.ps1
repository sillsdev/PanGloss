param(
	[string]$MachineDir,
	[string]$ProjectorExe,
	[string]$ProjectName = 'DeepOptionalAffixNesting'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($MachineDir)) {
	$MachineDir = if ([string]::IsNullOrWhiteSpace($env:PANGLOSS_MACHINE_DIR)) {
		'C:\Users\johnm\Documents\repos\machine'
	} else {
		$env:PANGLOSS_MACHINE_DIR
	}
}
if ([string]::IsNullOrWhiteSpace($ProjectorExe)) {
	$ProjectorExe = if ([string]::IsNullOrWhiteSpace($env:PANGLOSS_XAMPLE_PROJECTOR_EXE)) {
		Join-Path $PSScriptRoot 'bin\Debug\XampleProjector.exe'
	} else {
		$env:PANGLOSS_XAMPLE_PROJECTOR_EXE
	}
}
if ([string]::IsNullOrWhiteSpace($ProjectName) -or
	$ProjectName -ne [System.IO.Path]::GetFileName($ProjectName) -or
	$ProjectName.IndexOfAny([System.IO.Path]::GetInvalidFileNameChars()) -ge 0) {
	throw "ProjectName must be one valid filename segment"
}

$fixtureRoot = Join-Path $MachineDir 'conformance\edge-cases\deep-optional-affix-nesting'
$grammarPath = Join-Path $fixtureRoot 'grammar.xml'
$witnessDir = Join-Path $fixtureRoot 'fieldworks'
if (-not (Test-Path -LiteralPath $ProjectorExe -PathType Leaf)) {
	throw "XampleProjector.exe was not found at $ProjectorExe; run build.ps1 -Mode check first."
}
if (-not (Test-Path -LiteralPath $grammarPath -PathType Leaf)) {
	throw "the source grammar was not found at $grammarPath"
}
if (Test-Path -LiteralPath $witnessDir) {
	throw "refusing to overwrite an existing witness at $witnessDir"
}

$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
if (-not $tempRoot.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
	$tempRoot += [System.IO.Path]::DirectorySeparatorChar
}
$staging = Join-Path $tempRoot (
	'xample-projector-witness-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $staging -Force | Out-Null
try {
	& $ProjectorExe author --grammar $grammarPath --out-dir $staging --name $ProjectName
	if ($LASTEXITCODE -ne 0) {
		throw "author failed with exit code $LASTEXITCODE"
	}

	$authorResponsePath = Join-Path $staging 'author-response.json'
	$authorResponse = Get-Content -Raw -LiteralPath $authorResponsePath | ConvertFrom-Json
	$kGuidProperty = $authorResponse.guidMap.PSObject.Properties |
		Where-Object Name -eq 'cK' | Select-Object -First 1
	if ($null -eq $kGuidProperty -or $kGuidProperty.Value -notmatch '^[0-9a-fA-F-]{36}$') {
		throw "author response has no usable guidMap.cK value"
	}

	$authoredProject = Join-Path $staging "$ProjectName\$ProjectName.fwdata"
	$projectedDir = Join-Path $staging 'projected'
	New-Item -ItemType Directory -Path $projectedDir -Force | Out-Null
	& $ProjectorExe project --project $authoredProject --out-dir $projectedDir --database $ProjectName
	if ($LASTEXITCODE -ne 0) {
		throw "project failed with exit code $LASTEXITCODE"
	}
	$hcXml = Join-Path $projectedDir "$ProjectName.hc.xml"
	& $ProjectorExe verify-parity --grammar $grammarPath --hc-xml $hcXml --guid-map $authorResponsePath `
		--expect k=1 --expect xxxxxxk=924 --expect xxxxxxxxxxxxk=1
	if ($LASTEXITCODE -ne 0) {
		throw "verify-parity failed with exit code $LASTEXITCODE"
	}

	$sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $authoredProject).Hash.ToLowerInvariant()
	$manifest = @"
version: 1
base_sha256: $sourceHash
cases:
  - id: empty-phoneme-inventory
    operations:
      - remove_all_phonemes:
          require_unreferenced: true
    expect:
      xample_projection: same_as_base
      hc_analyses: same_as_base
      inferred_segments: [x, k]
  - id: remove-k-only
    operations:
      - remove_phoneme:
          guid: $($kGuidProperty.Value)
          assert_representations: [k]
          require_unreferenced: true
    expect:
      xample_projection: same_as_base
      hc_analyses: same_as_base
      inferred_segments: [k]
"@

	$stagedWitness = Join-Path $staging 'fieldworks'
	New-Item -ItemType Directory -Path $stagedWitness -Force | Out-Null
	Copy-Item -LiteralPath $authoredProject -Destination (Join-Path $stagedWitness 'project.fwdata')
	$sourceWs = Join-Path (Split-Path $authoredProject) 'WritingSystemStore'
	$targetWs = Join-Path $stagedWitness 'WritingSystemStore'
	New-Item -ItemType Directory -Path $targetWs -Force | Out-Null
	$ldml = @(Get-ChildItem -LiteralPath $sourceWs -Filter '*.ldml' -File)
	if ($ldml.Count -eq 0) {
		throw "author produced no WritingSystemStore/*.ldml files"
	}
	$ldml | Copy-Item -Destination $targetWs
	Set-Content -LiteralPath (Join-Path $stagedWitness 'phonology-mutations.yaml') -Value $manifest -Encoding utf8
	Move-Item -LiteralPath $stagedWitness -Destination $witnessDir

	[pscustomobject]@{
		project = (Join-Path $witnessDir 'project.fwdata')
		sha256 = $sourceHash
		removeKGuid = $kGuidProperty.Value
		writingSystemFiles = $ldml.Count
	} | ConvertTo-Json -Compress
}
finally {
	$stagingFull = [System.IO.Path]::GetFullPath($staging)
	if ($stagingFull.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and
		$stagingFull -ne $tempRoot -and (Test-Path -LiteralPath $stagingFull)) {
		Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
	}
}
