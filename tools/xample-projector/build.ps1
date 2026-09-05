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
	Write-Host "SKIPPED (live run): sample project not found at $senaFwdata"
	exit 0
}

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
}
finally {
	Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

exit 0
