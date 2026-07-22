$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$library = if ($IsWindows) {
    Join-Path $root "target/release/pangloss_ffi.dll"
} elseif ($IsMacOS) {
    Join-Path $root "target/release/libpangloss_ffi.dylib"
} else {
    Join-Path $root "target/release/libpangloss_ffi.so"
}
$grammar = Join-Path $root "tools/fixtures/supplied-lexicon-host-smoke.xml"
$pythonSmoke = Join-Path $root "examples/native-abi/python_ctypes_smoke.py"
$csharpProject = Join-Path $root "examples/native-abi/csharp/PanGlossNativeSmoke.csproj"
$nugetConfig = Join-Path $root "examples/native-abi/NuGet.Config"

Push-Location $root
try {
    cargo build -p pg-ffi --release
    if ($LASTEXITCODE -ne 0) { throw "pg-ffi release build failed" }

    $python = Get-Command python3, python -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($python) {
        & $python.Source -m py_compile $pythonSmoke
        if ($LASTEXITCODE -ne 0) { throw "Python example did not compile" }
        & $python.Source $pythonSmoke $library $grammar
        if ($LASTEXITCODE -ne 0) { throw "Python native ABI smoke failed" }
    } else {
        Write-Host "SKIP Python smoke: no Python interpreter is installed"
    }

    $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($dotnet) {
        $dotnetHome = Join-Path $root "target/dotnet-home"
        New-Item -ItemType Directory -Force $dotnetHome | Out-Null
        $env:APPDATA = $dotnetHome
        $env:DOTNET_CLI_HOME = $dotnetHome
        $env:DOTNET_SKIP_FIRST_TIME_EXPERIENCE = "1"
        dotnet restore $csharpProject --configfile $nugetConfig
        if ($LASTEXITCODE -ne 0) { throw "C# example restore failed" }
        dotnet build $csharpProject --configuration Release --no-restore
        if ($LASTEXITCODE -ne 0) { throw "C# example did not compile" }
        dotnet run --project $csharpProject --configuration Release --no-build -- $library $grammar
        if ($LASTEXITCODE -ne 0) { throw "C# native ABI smoke failed" }
    } else {
        Write-Host "SKIP C# smoke: dotnet is not installed"
    }
} finally {
    Pop-Location
}
