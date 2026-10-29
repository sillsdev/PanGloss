. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function New-Pkg([string]$Name, [string[]]$Deps = @(), [bool]$Lib = $true) {
    [PSCustomObject]@{ Name = $Name; Dir = "rust/crates/$Name"; HasLib = $Lib; Deps = $Deps }
}
$pkgs = @(
    (New-Pkg 'leaf'),
    (New-Pkg 'mid' @('leaf')),
    (New-Pkg 'top' @('mid') $false),
    (New-Pkg 'other')
)

Test-Case 'a source change maps to its crate and the crates depending on it' {
    $s = Get-AffectedScope -ChangedPaths @('rust/crates/mid/src/lib.rs') -Packages $pkgs
    Assert-False $s.Global 'not global'
    Assert-Equal 'mid' ($s.Changed -join ',') 'changed'
    Assert-Equal 'mid,top' ($s.Closure -join ',') 'closure includes dependents only'
}

Test-Case 'a longer crate directory wins over a shorter prefix' {
    $nested = @($pkgs) + @([PSCustomObject]@{ Name = 'leaf-sub'; Dir = 'rust/crates/leaf/sub'; HasLib = $true; Deps = @('leaf') })
    $s = Get-AffectedScope -ChangedPaths @('rust/crates/leaf/sub/src/x.rs') -Packages $nested
    Assert-Equal 'leaf-sub' ($s.Changed -join ',') 'nested owner'
}

Test-Case 'docs, CI and tooling changes map to no crate' {
    $s = Get-AffectedScope -ChangedPaths @('docs/a.md', '.github/workflows/x.yml', 'rust/tools/pg.ps1', 'rust/README.md') -Packages $pkgs
    Assert-False $s.Global 'inert paths are not global'
    Assert-Equal 0 $s.Changed.Count 'nothing to run'
}

Test-Case 'shared paths fall back to the full suite' {
    foreach ($path in @('rust/Cargo.toml', 'rust/Cargo.lock', 'rust/.config/nextest.toml', 'rust/rust-toolchain.toml',
            'conformance-staging/a/words.yaml', 'machine/conformance/x', 'rust/stray.rs')) {
        $s = Get-AffectedScope -ChangedPaths @('rust/crates/leaf/src/lib.rs', $path) -Packages $pkgs
        Assert-True $s.Global "$path must be global"
    }
}

Test-Case 'two changed crates union their dependents' {
    $s = Get-AffectedScope -ChangedPaths @('rust/crates/other/src/a.rs', 'rust/crates/leaf/Cargo.toml') -Packages $pkgs
    Assert-Equal 'leaf,other' ($s.Changed -join ',') 'changed'
    Assert-Equal 'leaf,mid,other,top' ($s.Closure -join ',') 'closure'
}

Test-Case 'doctest scope takes the package list' {
    $doc = Get-DoctestInvocation -DebugProfile:$false -TestThreads 2 -Packages @('a', 'b') -ExtraArgs @('-E', 'rdeps(a)')
    Assert-Equal 'test|--doc|--locked|--profile|pg-test-opt|-p|a|-p|b|--no-fail-fast|--|--test-threads|2' ($doc -join '|') 'args'
}

Write-TestSummary
