#!/usr/bin/env bash
# Run one already-built grammar differential under the owner's finite process envelope.
set -u
if [ "$#" -lt 5 ]; then
    printf 'usage: %s <binary> <grammar> <words.txt> <out.jsonl> <strategy> [auto-order] [closure-work]\n' "$0" >&2
    exit 2
fi
strategy_repo=$(cd "$(dirname "$0")/../.." && pwd)
strategy_binary=$(realpath "$1")
strategy_source=$(realpath "$2")
strategy_words=$(realpath "$3")
strategy_output=$(realpath -m "$4")
strategy_name=$5
strategy_order=${6:-surface-enum,factored}
# Freeze the baseline's optional work cap independently of the compile API's defaults.
strategy_closure_work=${7:-3000}
if ! [[ "$strategy_closure_work" =~ ^[0-9]+$ ]]; then
    printf 'closure-work must be a non-negative integer\n' >&2
    exit 2
fi
ulimit -v 8388608
export PANGLOSS_MACHINE_CHECKOUTS=/mnt/h/repos/machine
export POWERSHELL_TELEMETRY_OPTOUT=1
export DOTNET_GCHeapHardLimit=0x40000000
export PANGLOSS_TRACE_EMIT_STAGES=1
export PANGLOSS_FST_HANG_GUARD_SECONDS=3600
export RAYON_NUM_THREADS=4
# -File treats a bare -- as an ambiguous parameter; use the managed binder-proof channel.
for strategy_arg in "$strategy_source" "$strategy_words" "$strategy_output" "$strategy_name" "$strategy_order"; do
    if [[ "$strategy_arg" == *\"* ]]; then
        printf 'managed argument channel cannot encode a double quote: %s\n' "$strategy_arg" >&2
        exit 2
    fi
done
export PANGLOSS_EXTRA_ARGS="\"$strategy_source\" \"$strategy_words\" \"$strategy_output\" --fst-strategy \"$strategy_name\" --fst-auto-order \"$strategy_order\" --fst-closure-work $strategy_closure_work"
cd "$strategy_repo"
/usr/bin/time -v timeout 3600 pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Exe "$strategy_binary" -Jobs 4 \
    > "$strategy_output.log" 2>&1
strategy_exit=$?
python3 - "$strategy_output" "$strategy_source" "$strategy_words" "$strategy_name" "$strategy_exit" "$strategy_order" "$strategy_closure_work" <<'PY'
import hashlib
import json
from pathlib import Path
import subprocess
import sys
output, source, words, strategy, exit_code, auto_order, closure_work = sys.argv[1:]
path = Path(output + '.manifest.json')
if path.exists():
    manifest = json.loads(path.read_text())
else:
    manifest = {
        'schema_version': 1, 'source': source,
        'source_sha256': hashlib.sha256(Path(source).read_bytes()).hexdigest(),
        'words_sha256': hashlib.sha256(Path(words).read_bytes()).hexdigest(),
        'words': [word.strip() for word in Path(words).read_text().splitlines() if word.strip()],
        'machine_pin': subprocess.check_output(['git', 'rev-parse', 'HEAD:machine'], text=True).strip(),
        'config': {'strategy': strategy, 'auto_order': auto_order.split(','),
                   'bounds': {'closure_work': int(closure_work), 'closure_depth': None,
                              'compound_depth': None, 'derivation_depth': None}},
        'xml_sha256': hashlib.sha256(Path(source).read_bytes()).hexdigest() if source.endswith('.xml') else None,
        'phase': 'launch_failed',
    }
code = int(exit_code)
manifest['external_limits'] = {'virtual_address_space_kib': 8388608, 'hang_guard_seconds': 3600}
manifest['external_outcome'] = {
    'kind': 'complete' if code == 0 else 'machine_limit' if code == 124 else 'process_failure',
    'exit_code': code,
}
if code == 124:
    manifest['external_outcome'].update(measure='hang_guard_seconds', limit=3600)
pending = Path(str(path) + '.external-pending')
pending.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + '\n')
pending.replace(path)
PY
exit "$strategy_exit"
