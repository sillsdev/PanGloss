$ErrorActionPreference = 'Stop'
foreach ($rev in @('b9e7db44', 'a873d60d')) {
    $dest = "/tmp/pangloss-lanes/kat/machine-$rev"
    New-Item -ItemType Directory -Force $dest | Out-Null
    git archive --format=tar "--output=$dest/source.tar" $rev src/SIL.Machine src/SIL.Machine.Morphology.HermitCrab src/SIL.Machine.Morphology.HermitCrab.Tool src/AssemblyInfo.props
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    tar -xf "$dest/source.tar" -C $dest
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    dotnet restore "$dest/src/SIL.Machine.Morphology.HermitCrab.Tool" --source /home/johnm/.nuget/packages -p:NuGetAudit=false
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    dotnet build "$dest/src/SIL.Machine.Morphology.HermitCrab.Tool" -c Release --no-restore | Tee-Object "$dest/build.log"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
