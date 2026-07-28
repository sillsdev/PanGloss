# Historical Task 1 baseline reproduction

Worktree: `C:\Users\johnm\Documents\repos\PanGloss\.claude\worktrees\p6-chain-restriction`
Commit: `fa81ec82916fc03e9acdc9dea394d8db5a0b0c53`
Toolchain: `rustc 1.96.1 (31fca3adb 2026-06-26)`; `cargo 1.96.1 (356927216 2026-06-26)`
Worktree status before/after: clean (no tracked or untracked changes reported by `git status --short`)

Each command has `<name>.meta.json`, `<name>.stdout.log`, and `<name>.stderr.log`. Metadata records exact command, commit, toolchain, cwd, UTC start/end, wall seconds, timeout, numeric exit code, and timeout status.

| Name | Result | Wall | Key observation |
|---|---:|---:|---|
| `pg-foma-lib` | exit 0, no timeout | 40.886 s | 66 passed, 13 ignored |
| `f1-sena-release` | exit 0, no timeout | 127.957 s | 326/326 engine analyses across 87 analyzed / 120 corpus words; full tier, uncovered 0 |
| `f2-indonesian-release` | exit 0, no timeout | 50.413 s | 97/97 analyses across 96 analyzed / 121 corpus words; 7 explicit reduplication exclusions; 3 unsupported reduplication rows |
| `f3-amharic-release` | exit 0, no timeout | 477.397 s | 30/30 analyses across 28 analyzed / 100 scanned; 10 zero-analysis 10-second engine timeouts; parity on 28; 1 unsupported process-morph row |
| `aweti-a-60s` | exit 0, no timeout | 51.165 s | final 14,806 states / 270,541 arcs; lexc 13,744 / 126,066; all rules composed, `skipped=[]`; 16 uncovered rows |
| `aweti-b-120s` | exit 0, no timeout | 17.864 s | `RECALL = 68/104 = 65.4%`; 36 misses; sweep 16.793601 s |
| `aweti-c-30s` | exit 0, no timeout | 1.058 s | `parua` covered, raw_n=1, 29.2 µs |

The historical revision's own denominator is 104, not the later 106-word corpus denominator. No killed or timed-out command was interpreted as a negative result.
