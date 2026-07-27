#!/usr/bin/env bash
# Runs the per-word timing harness for `openspec/changes/certify-language-readiness` section 1
# (rust/crates/pg-foma/tests/typology_speedup.rs) over the synthetic-language conformance suite --
# both discovery roots (`machine/conformance/**` and `conformance-staging/**`) -- in both engine
# modes (complete Rust HermitCrab, and the compiled proposer + confirm path), and writes:
#   - typology-speedup.csv  (the canonical per-word/per-outcome data)
#   - typology-speedup.md   (a rendered Markdown table, grouped per fixture/construct/typology)
#
# Replaces the hand-run recipe in docs/benchmark-matrix.md (manual `pangloss batch` invocations
# piped through `awk`) with a single runnable command. Drives both engines IN-PROCESS via the test
# binary (pg_parse::Morpher / pg_foma::composite::FomaAnalyzer directly) -- no `pangloss` binary is
# built or invoked by this script, and no CLI integer-millisecond floor is involved anywhere in the
# measurement (see the harness's own module doc for the floor treatment).
#
# Usage: rust/tools/typology-speedup.sh [--out-dir DIR] [--repeats N]
#
# Env vars the harness itself reads (set by this script from the flags above, or export them
# yourself before calling this script directly):
#   PG_TYPOLOGY_OUT_DIR  -- output directory (default: rust/target/typology-speedup)
#   PG_TYPOLOGY_REPEATS  -- timed samples per word per engine, after 1 discarded warmup (default: 7)
#
# The harness test itself is `#[ignore]`d (it compiles a foma network for every non-refused
# fixture and times every word repeatedly in both engines -- categorically slower than a unit test,
# same precedent as tests/f3_parity.rs and tests/p6_gate_parity.rs), so this script passes
# `--ignored` through to `cargo test`.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

out_dir="$repo_root/rust/target/typology-speedup"
repeats=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir)
      out_dir="$2"
      shift 2
      ;;
    --repeats)
      repeats="$2"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      echo "usage: $0 [--out-dir DIR] [--repeats N]" >&2
      exit 2
      ;;
  esac
done

if [ ! -f "$repo_root/machine/conformance/PROTOCOL.md" ]; then
  echo "warning: $repo_root/machine/conformance is empty -- only conformance-staging/ fixtures \
will be measured (run 'git submodule update --init machine' for the full corpus)" >&2
fi

mkdir -p "$out_dir"
export PG_TYPOLOGY_OUT_DIR="$out_dir"
if [ -n "$repeats" ]; then
  export PG_TYPOLOGY_REPEATS="$repeats"
fi

echo "[typology-speedup] running the timing harness (release build)..." >&2
(cd "$repo_root/rust" && cargo test --release -p pg-foma --test typology_speedup -- \
    --ignored --nocapture full_corpus_report)

echo "[typology-speedup] CSV:      $out_dir/typology-speedup.csv" >&2
echo "[typology-speedup] Markdown: $out_dir/typology-speedup.md" >&2
