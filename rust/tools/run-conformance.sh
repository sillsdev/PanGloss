#!/usr/bin/env bash
# Runs the engine-agnostic morphological-parser conformance suite (the `machine` submodule,
# pinned to its `conformance-framework` branch — see docs/hermitcrab-rust-port-audit.md section 5
# and machine/conformance/PROTOCOL.md) against this repo's own `hc-rs` engine, via the suite's own
# C# driver in "adapter" mode (no fixture-parsing/comparison logic duplicated here — that's exactly
# what the driver already does, correctly, for every engine that implements the adapter contract).
#
# `hc-rs batch <grammar> <words> <output>` already speaks that exact contract (5-column
# idx/word/ms/status/signature TSV, `--start` resumption) — nothing engine-side needed to run this.
#
# Usage: rust/tools/run-conformance.sh [--include-pathological] [--skip-build] [--engine=default|foma]
#
# `--engine=foma` (P3, docs/fst-plan/foma-fst-plan.md 3b): passes `--engine=foma` through to every
# `hc-rs batch` invocation the driver makes, so the SAME script runs the conformance suite against
# either the default full-search engine (the pre-existing behavior, unchanged) or the
# `hc_foma::composite::FomaAnalyzer` propose->confirm path -- run it twice (once per engine) to
# compare pass/fail sets; the gate requires them identical (zero NEW divergences on the foma run
# beyond known-conformance-divergences.txt).
#
# Exit code: 0 = every attempted fixture passed OR every failure is a documented, known divergence
# (see known-conformance-divergences.txt, next to this script); 1 = at least one UNEXPECTED
# failure; 2 = a harness-level error (bad args, no fixtures found, malformed fixture metadata --
# the driver's own exit code, passed straight through).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
machine_dir="$repo_root/machine"
conformance_project="$machine_dir/src/SIL.Machine.Morphology.HermitCrab.Conformance"

include_pathological=""
skip_build=""
engine="default"
for arg in "$@"; do
  case "$arg" in
    --include-pathological) include_pathological="--include-pathological" ;;
    --skip-build) skip_build="1" ;;
    --engine=*) engine="${arg#--engine=}" ;;
    *)
      echo "unknown argument: $arg" >&2
      echo "usage: $0 [--include-pathological] [--skip-build] [--engine=default|foma]" >&2
      exit 2
      ;;
  esac
done
case "$engine" in
  default|foma) ;;
  *)
    echo "invalid --engine: $engine (expected default|foma)" >&2
    exit 2
    ;;
esac
engine_flag=""
if [ "$engine" = "foma" ]; then
  engine_flag="--engine=foma"
fi

if [ ! -f "$machine_dir/conformance/PROTOCOL.md" ]; then
  echo "error: $machine_dir/conformance is empty -- run 'git submodule update --init machine' first" >&2
  exit 2
fi

if [ "$skip_build" != "1" ]; then
  echo "[run-conformance] building hc-rs (release)..." >&2
  (cd "$repo_root/rust" && cargo build --release -p hc-cli)
  echo "[run-conformance] building the conformance driver..." >&2
  dotnet build "$conformance_project" -v quiet
fi

hc_rs="$repo_root/rust/target/release/hc-rs"
if [ ! -x "$hc_rs" ] && [ -x "$hc_rs.exe" ]; then
  hc_rs="$hc_rs.exe"
fi
if [ ! -x "$hc_rs" ]; then
  echo "error: hc-rs binary not found at $hc_rs(.exe) -- build it first or drop --skip-build" >&2
  exit 2
fi

# Capabilities hc-rs declares: every `requires:` value used anywhere under machine/conformance/ is
# "phonology" (or empty) as of the pinned commit (see docs/hermitcrab-rust-port-audit.md section 5)
# -- phonological rewrite rules (both Iterative and Simultaneous) are fully ported, so declaring it
# means no fixture is skipped for a capability PanGloss actually has.
out="$(mktemp)"
trap 'rm -f "$out"' EXIT
set +e
echo "[run-conformance] engine=$engine" >&2
dotnet run --no-build --project "$conformance_project" -- \
  --fixtures "$machine_dir/conformance" \
  --adapter "$hc_rs batch {grammar} {words} {output} $engine_flag" \
  --capabilities phonology \
  $include_pathological | tee "$out"
driver_exit="${PIPESTATUS[0]}"
set -e

if [ "$driver_exit" != "1" ]; then
  # 0 (all passed) or 2 (harness error) -- nothing for the known-divergences filter to do.
  exit "$driver_exit"
fi

known_file="$(dirname "${BASH_SOURCE[0]}")/known-conformance-divergences.txt"
known_ids="$(grep -v '^\s*#' "$known_file" | grep -v '^\s*$' || true)"

failed_ids="$(sed -n 's/^\[FAIL\] \([^[:space:]]*\).*/\1/p' "$out" || true)"
unexpected=""
for id in $failed_ids; do
  if ! grep -qxF "$id" <<<"$known_ids"; then
    unexpected="$unexpected $id"
  fi
done

if [ -z "$unexpected" ]; then
  echo
  echo "[run-conformance] every failure is a documented known divergence -- treating as pass." >&2
  exit 0
else
  echo
  echo "[run-conformance] UNEXPECTED failure(s), not in $known_file:$unexpected" >&2
  exit 1
fi
