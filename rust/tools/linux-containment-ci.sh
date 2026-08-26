#!/usr/bin/env bash
# Hosted-Linux proof for the delegated cgroup-v2 contract. The outer process owns only two exact,
# run-scoped transient units; the build itself runs as the unprivileged Actions runner.
set -Eeuo pipefail

fail() {
    printf 'linux containment CI: %s\n' "$*" >&2
    exit 1
}

require_numeric() {
    local name=$1 value=$2
    [[ "$value" =~ ^[0-9]+$ ]] || fail "$name must contain only decimal digits"
}

unit_control_group() {
    local unit=$1 value
    value=$(systemctl show "$unit.service" --property=ControlGroup --value) ||
        fail "could not read ControlGroup for $unit.service"
    [[ "$value" == /* && "$value" != *'//'* && "$value" != *'/./'* && "$value" != *'/../'* ]] ||
        fail "systemd returned a non-canonical ControlGroup for $unit.service: $value"
    printf '%s\n' "$value"
}

wait_for_file() {
    local path=$1 label=$2
    for _ in $(seq 1 300); do
        [[ -s "$path" ]] && return 0
        sleep 0.1
    done
    fail "timed out waiting for $label"
}

cleanup_exact_unit() {
    local unit=$1
    sudo --non-interactive systemctl stop "$unit.service" >/dev/null 2>&1 || true
    sudo --non-interactive systemctl reset-failed "$unit.service" >/dev/null 2>&1 || true
}

run_gate_child() {
    local unit=$1 membership_line self_leaf unit_root root_path memory_max

    [[ "${PANGLOSS_CGROUP_TEST_REQUIRED:-}" == 1 ]] ||
        fail 'PANGLOSS_CGROUP_TEST_REQUIRED=1 is required'
    [[ "$(stat -fc %T /sys/fs/cgroup)" == cgroup2fs ]] || fail 'the host is not using cgroup v2'

    mapfile -t memberships < <(grep '^0::' /proc/self/cgroup)
    [[ ${#memberships[@]} -eq 1 ]] || fail 'expected exactly one unified /proc/self/cgroup membership'
    membership_line=${memberships[0]}
    self_leaf=${membership_line#0::}
    [[ "$self_leaf" == /* && "$self_leaf" != *'//'* && "$self_leaf" != *'/./'* && "$self_leaf" != *'/../'* ]] ||
        fail "non-canonical self cgroup membership: $self_leaf"

    unit_root=$(unit_control_group "$unit")
    [[ "$self_leaf" == "$unit_root/pangloss-supervisor" ]] ||
        fail "self cgroup $self_leaf is not the delegated supervisor leaf below $unit_root"
    root_path="/sys/fs/cgroup$unit_root"
    [[ -d "$root_path" ]] || fail "delegated root does not exist: $root_path"

    # DelegateSubgroup keeps this root empty; the service process lives in pangloss-supervisor.
    [[ ! -s "$root_path/cgroup.procs" ]] || fail 'delegated root cgroup.procs is not empty'
    grep -qw memory "$root_path/cgroup.controllers" || fail 'memory is unavailable in cgroup.controllers'
    printf '+memory\n' >"$root_path/cgroup.subtree_control" ||
        fail 'could not enable memory in cgroup.subtree_control'
    # A readback contains "memory" (without the enabling write's plus sign).
    grep -qw memory "$root_path/cgroup.subtree_control" || fail '+memory is not enabled in cgroup.subtree_control'

    memory_max=$(<"$root_path/memory.max")
    [[ "$memory_max" != max ]] || fail 'delegated root memory.max is unlimited'
    [[ "$memory_max" =~ ^[0-9]+$ ]] || fail "delegated root memory.max is not numeric: $memory_max"
    (( memory_max > 0 )) || fail 'delegated root memory.max must be positive'
    [[ -w "$root_path/cgroup.kill" ]] || fail 'delegated root cgroup.kill is not writable'

    export PANGLOSS_CGROUP_DELEGATED_ROOT="$unit_root"
    exec pwsh -NoProfile -File ./tools/pg.ps1 -Mode test -Package pg-worker-containment -TestTarget linux_containment -NoNextest -MaxConcurrent 1 -Jobs 2 -TestThreads 1
}

run_probe_main() {
    local ready_path=$1 pid_path=$2
    (
        trap '' TERM
        printf '%s\n' "$BASHPID" >"$pid_path"
        while :; do sleep 1; done
    ) &
    local stubborn_child=$!
    kill -0 "$stubborn_child" || fail 'stubborn descendant did not start'
    printf 'ready\n' >"$ready_path"
    wait "$stubborn_child"
}

case "${1:-}" in
    --gate)
        [[ $# -eq 2 ]] || fail 'invalid gate-child arguments'
        run_gate_child "$2"
        ;;
    --probe-main)
        [[ $# -eq 3 ]] || fail 'invalid lifecycle-probe arguments'
        run_probe_main "$2" "$3"
        ;;
    '') ;;
    *) fail "unknown mode: $1" ;;
esac

[[ $# -eq 0 ]] || exit 0
[[ "$(uname -s)" == Linux ]] || fail 'this proof requires Linux'
command -v systemd-run >/dev/null || fail 'systemd-run is unavailable'
command -v systemctl >/dev/null || fail 'systemctl is unavailable'
command -v pwsh >/dev/null || fail 'pwsh is unavailable'
sudo --non-interactive true || fail 'passwordless sudo is required to create transient system units'

run_id=${GITHUB_RUN_ID:-}
run_attempt=${GITHUB_RUN_ATTEMPT:-}
require_numeric GITHUB_RUN_ID "$run_id"
require_numeric GITHUB_RUN_ATTEMPT "$run_attempt"

runner_uid=$(id -u)
runner_gid=$(id -g)
require_numeric runner_uid "$runner_uid"
require_numeric runner_gid "$runner_gid"

gate_unit="pangloss-containment-${run_id}-${run_attempt}"
probe_unit="pangloss-containment-probe-${run_id}-${run_attempt}"
workdir=$(pwd -P)
script_path=$(realpath "$0")
temp_parent=${RUNNER_TEMP:-/tmp}
probe_dir=$(mktemp -d "$temp_parent/pangloss-containment-${run_id}-${run_attempt}.XXXXXX")
probe_ready="$probe_dir/ready"
probe_pid="$probe_dir/stubborn.pid"

cleanup() {
    cleanup_exact_unit "$probe_unit"
    cleanup_exact_unit "$gate_unit"
    rm -f -- "$probe_ready" "$probe_pid"
    rmdir -- "$probe_dir" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

common_properties=(
    --property=Type=exec
    --property=MemoryMax=6G
    --property=MemorySwapMax=0
    --property=TasksMax=4096
    --property=KillMode=control-group
    --property=TimeoutStopSec=30s
    --property=RuntimeMaxSec=20min
)

# First prove the host's service-main-death behavior with a descendant that cannot be stopped by TERM.
sudo --non-interactive systemd-run --quiet \
    --unit="$probe_unit" \
    --uid="$runner_uid" \
    --gid="$runner_gid" \
    --working-directory="$workdir" \
    "${common_properties[@]}" \
    /usr/bin/bash "$script_path" --probe-main "$probe_ready" "$probe_pid"

wait_for_file "$probe_ready" 'lifecycle probe readiness'
wait_for_file "$probe_pid" 'stubborn descendant PID'
stubborn_child=$(<"$probe_pid")
require_numeric stubborn_child "$stubborn_child"
kill -0 "$stubborn_child" || fail 'stubborn descendant exited before service-main death'

probe_control_group=$(unit_control_group "$probe_unit")
unit_cgroup_path="/sys/fs/cgroup$probe_control_group"
[[ -d "$unit_cgroup_path" ]] || fail "probe unit cgroup does not exist: $unit_cgroup_path"

sudo --non-interactive systemctl kill --kill-whom=main --signal=KILL "$probe_unit.service"
for _ in $(seq 1 300); do
    if ! kill -0 "$stubborn_child" 2>/dev/null && [[ ! -d "$unit_cgroup_path" ]]; then
        break
    fi
    sleep 0.1
done
if kill -0 "$stubborn_child" 2>/dev/null; then
    fail "stubborn descendant $stubborn_child survived service-main death"
fi
[[ ! -d "$unit_cgroup_path" ]] || fail "probe unit cgroup survived service-main death: $unit_cgroup_path"
sudo --non-interactive systemctl reset-failed "$probe_unit.service" >/dev/null 2>&1 || true

# Then run the required Rust target inside a separately bounded, memory-delegated service.
sudo --non-interactive systemd-run --quiet --wait --pipe --collect \
    --unit="$gate_unit" \
    --uid="$runner_uid" \
    --gid="$runner_gid" \
    --working-directory="$workdir" \
    --setenv=HOME="$HOME" \
    --setenv=PATH="$PATH" \
    --setenv=PANGLOSS_CGROUP_TEST_REQUIRED=1 \
    --setenv=GITHUB_ACTIONS="${GITHUB_ACTIONS:-true}" \
    --setenv=CI="${CI:-true}" \
    --property=Delegate=memory \
    --property=DelegateSubgroup=pangloss-supervisor \
    "${common_properties[@]}" \
    /usr/bin/bash "$script_path" --gate "$gate_unit"
