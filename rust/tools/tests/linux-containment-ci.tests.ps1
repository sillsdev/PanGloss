<#
  .DESCRIPTION
  Declarative contract for the required hosted-Linux containment proof. This test never starts
  Cargo, systemd, or a process tree. It pins the load-bearing workflow and service-script semantics
  that make a green CI result evidence of delegated cgroup-v2 containment rather than merely a
  successful Linux compile.
#>
. "$PSScriptRoot\_test-harness.ps1"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$workflowPath = Join-Path $repoRoot '.github\workflows\rust-ci.yml'
$serviceScriptPath = Join-Path $repoRoot 'rust\tools\linux-containment-ci.sh'
$workflowText = Get-Content -LiteralPath $workflowPath -Raw
$serviceScriptText = if (Test-Path -LiteralPath $serviceScriptPath) {
    Get-Content -LiteralPath $serviceScriptPath -Raw
} else {
    ''
}

function Assert-Matches {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$Text,
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][string]$Message
    )
    if ($Text -notmatch $Pattern) { throw $Message }
}

function Assert-DoesNotMatch {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$Text,
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][string]$Message
    )
    if ($Text -match $Pattern) { throw $Message }
}

function Get-LinuxContainmentJob {
    $jobPattern = '(?ms)^  (?<id>[A-Za-z0-9_-]+):\s*\r?\n(?<body>.*?)(?=^  [A-Za-z0-9_-]+:\s*\r?\n|\z)'
    foreach ($match in [regex]::Matches($workflowText, $jobPattern)) {
        $job = "$($match.Groups['id'].Value):`n$($match.Groups['body'].Value)"
        if ($job -match '(?im)^\s*name:\s*.*linux.*containment' -or $match.Groups['id'].Value -match 'linux[-_]containment') {
            return $job
        }
    }
    return ''
}

$linuxJob = Get-LinuxContainmentJob

Test-Case 'workflow has a dedicated required containment job pinned to Ubuntu 24.04' {
    Assert-True ($linuxJob.Length -gt 0) 'rust-ci.yml must define a dedicated Linux containment job'
    Assert-Matches $linuxJob '(?im)^\s*name:\s*.*linux.*containment.*required' `
        'the containment check name must explicitly identify it as required'
    Assert-Matches $linuxJob '(?im)^\s*runs-on:\s*ubuntu-24\.04\s*(?:#.*)?$' `
        'the containment job must be pinned to ubuntu-24.04, not an unpinned ubuntu-latest image'
    Assert-DoesNotMatch $linuxJob '(?im)^\s*continue-on-error:\s*true\s*(?:#.*)?$' `
        'the required containment proof must not be allowed to fail'
}

Test-Case 'containment job has least privilege and checks out submodules' {
    Assert-Matches $linuxJob '(?ims)^\s*permissions:\s*\r?\n(?:\s+[^\r\n]+\r?\n)*?\s+contents:\s*read\s*(?:#.*)?$' `
        'the containment job must grant only read access to repository contents'
    Assert-Matches $linuxJob '(?ims)uses:\s*actions/checkout@v4.*?\r?\n\s*with:\s*\r?\n(?:\s+[^\r\n]+\r?\n)*?\s+submodules:\s*(?:true|recursive)\s*(?:#.*)?$' `
        'the containment job checkout must initialize the repository submodules'
}

Test-Case 'containment job requires the cgroup proof and delegates lifecycle work to the reviewed script' {
    Assert-Matches $linuxJob '(?im)^\s*PANGLOSS_CGROUP_TEST_REQUIRED:\s*["'']?1["'']?\s*(?:#.*)?$' `
        'the Linux target must fail closed unless PANGLOSS_CGROUP_TEST_REQUIRED=1'
    Assert-Matches $linuxJob '(?ims)^\s*run:\s*(?:[>|]-?\s*\r?\n\s*)?(?:bash\s+)?(?:\./)?tools/linux-containment-ci\.sh\s*(?:#.*)?$' `
        'the job must invoke rust/tools/linux-containment-ci.sh from the rust working directory'
}

Test-Case 'service script creates a bounded transient service for the runner identity' {
    Assert-True ($serviceScriptText.Length -gt 0) 'rust/tools/linux-containment-ci.sh must exist'
    Assert-Matches $serviceScriptText '(?m)\bsystemd-run\b' 'the proof must run inside a transient systemd service'
    Assert-Matches $serviceScriptText '(?m)\bid\s+-u\b' 'the script must derive the GitHub runner UID'
    Assert-Matches $serviceScriptText '(?m)\bid\s+-g\b' 'the script must derive the GitHub runner GID'
    Assert-Matches $serviceScriptText '(?m)--uid(?:=|\s+)' 'the transient service must run as the runner UID'
    Assert-Matches $serviceScriptText '(?m)--gid(?:=|\s+)' 'the transient service must run as the runner GID'
    Assert-Matches $serviceScriptText '(?m)(?:--property(?:=|\s+)|-p\s+)["'']?Delegate=(?:memory|yes)' `
        'the service must delegate the memory controller'
    Assert-Matches $serviceScriptText '(?m)(?:--property(?:=|\s+)|-p\s+)["'']?DelegateSubgroup=pangloss-supervisor\b' `
        'the service must create the pangloss-supervisor delegated subgroup'
}

Test-Case 'transient service has finite resource and lifecycle bounds' {
    Assert-Matches $serviceScriptText '(?m)\bMemoryMax=[1-9][0-9]*(?:[KMGT](?:i?B)?)?\b' `
        'the service must have a finite nonzero MemoryMax'
    Assert-Matches $serviceScriptText '(?m)\bTasksMax=[1-9][0-9]*\b' 'the service must have a finite nonzero TasksMax'
    Assert-Matches $serviceScriptText '(?m)\bKillMode=control-group\b' 'service teardown must kill the entire control group'
    Assert-Matches $serviceScriptText '(?m)\bRuntimeMaxSec=[1-9][0-9]*(?:ms|s|min|h)?\b' `
        'the service must have a finite nonzero RuntimeMaxSec'
}

Test-Case 'service script proves the delegated root is empty, memory-enabled, and finitely capped' {
    Assert-Matches $serviceScriptText '(?m)\bsystemctl\s+show\b[^\r\n]*\bControlGroup\b' `
        'the script must derive the unit cgroup from systemd rather than guess its path'
    Assert-Matches $serviceScriptText '(?m)pangloss-supervisor' 'the derived path must select the delegated subgroup'
    Assert-Matches $serviceScriptText '(?m)cgroup\.procs' 'the script must inspect membership in the delegated root'
    Assert-Matches $serviceScriptText '(?m)(?:!\s+-s|-s[^\r\n]*(?:exit|return|false)|(?:-z|wc\s+-l|read)[^\r\n]*cgroup\.procs)' `
        'the script must reject a delegated root whose cgroup.procs is non-empty'
    Assert-Matches $serviceScriptText '(?m)cgroup\.controllers' 'the script must inspect available cgroup controllers'
    Assert-Matches $serviceScriptText '(?m)cgroup\.subtree_control' 'the script must inspect enabled subtree controllers'
    Assert-Matches $serviceScriptText '(?m)(?:grep|case|=~)[^\r\n]*\bmemory\b' `
        'the script must positively verify that the memory controller is available and enabled'
    Assert-Matches $serviceScriptText '(?m)memory\.max' 'the script must read the effective memory cap'
    Assert-Matches $serviceScriptText '(?m)(?:memory\.max|memory_max|memoryMax)[^\r\n]*(?:==|!=|=~|case)[^\r\n]*\bmax\b|\bmax\b[^\r\n]*(?:exit|return|false)' `
        'the script must reject an unlimited memory.max value'
    Assert-Matches $serviceScriptText '(?m)(?:memory\.max|memory_max|memoryMax)[^\r\n]*(?:[0-9]|\^\[0-9)' `
        'the script must validate that the finite memory cap is numeric'
}

Test-Case 'service script does not require delegated root cgroup.kill writability' {
    Assert-DoesNotMatch $serviceScriptText '(?m)^\s*\[\[\s+-w\s+"\$root_path/cgroup\.kill"\s+\]\]' `
        'the delegated service root must not require cgroup.kill writability; worker-created child cgroups are the kill surface'
}

Test-Case 'service script invokes exactly the managed Linux containment target' {
    $normalized = (($serviceScriptText -replace '\\\s+', ' ') -replace '\s+', ' ').Trim()
    $expected = './tools/pg.ps1 -Mode test -Package pg-worker-containment -TestTarget linux_containment -NoNextest -MaxConcurrent 1 -Jobs 2 -TestThreads 1'
    Assert-True ($normalized.Contains($expected)) "expected exact managed test invocation: $expected"
}

Test-Case 'service-main death probe proves a stubborn descendant and the unit cgroup disappear' {
    Assert-Matches $serviceScriptText '(?m)trap\s+["'']{2}\s+(?:TERM|SIGTERM)' `
        'the lifecycle probe must create a descendant that ignores TERM'
    Assert-Matches $serviceScriptText '(?m)(?:systemctl\s+kill[^\r\n]*(?=(?:[^\r\n]*--kill-who(?:m)?=main))(?=(?:[^\r\n]*--signal(?:=|\s+)KILL))|kill\s+-KILL[^\r\n]*(?:main|MAIN))' `
        'the probe must deliberately kill only the service main process'
    Assert-Matches $serviceScriptText '(?m)kill\s+-0[^\r\n]*(?:stubborn|descendant|child)' `
        'the probe must check the stubborn descendant PID after service-main death'
    Assert-Matches $serviceScriptText '(?m)(?:!\s+-[de]\s+[^\r\n]*(?:unit_cgroup|unitCgroup|cgroup_path)|(?:unit_cgroup|unitCgroup|cgroup_path)[^\r\n]*-[de])' `
        'the probe must assert that the unit cgroup path disappears'
}

Write-TestSummary
