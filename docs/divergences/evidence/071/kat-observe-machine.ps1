$ErrorActionPreference = 'Stop'
$dir = '/tmp/pangloss-lanes/kat'
Copy-Item /home/johnm/work/machine.worktrees/strrep-unapply/.review/strrep-tool/src/SIL.Machine.Morphology.HermitCrab.Tool/SignatureFormat.cs "$dir/observer/SignatureFormat.cs" -Force
foreach ($rev in @('b9e7db44', 'a873d60d')) {
    $engine = "$dir/machine-$rev"
    dotnet restore "$dir/observer/Observer.csproj" "-p:EngineRoot=$engine" --source /home/johnm/.nuget/packages -p:NuGetAudit=false
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    dotnet build "$dir/observer/Observer.csproj" "-p:EngineRoot=$engine" -c Release --no-restore
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    dotnet "$dir/observer/bin/Release/net10.0/Observer.dll" "$dir/grammar.xml" "$dir/words.txt" | Tee-Object "$dir/csharp-$rev.tsv"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
