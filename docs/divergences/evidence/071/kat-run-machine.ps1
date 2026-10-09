$ErrorActionPreference = 'Stop'
$dir = '/tmp/pangloss-lanes/kat'
$words = @('kat', 'kad', 'kak', 'dat', 'tat', 'kta', 'ka')
$words | Set-Content "$dir/words.txt"
$words | ForEach-Object { "parse $_" } | Set-Content "$dir/parse-script.txt"
foreach ($rev in @('b9e7db44', 'a873d60d')) {
    dotnet "$dir/machine-$rev/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll" -i "$dir/grammar.xml" -s "$dir/parse-script.txt" -o "$dir/hc-$rev.txt"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Get-Content "$dir/hc-$rev.txt"
}
