# Sena memo-value bisect

## Result

The memo paid materially immediately before the upstream analysis port, then collapsed at
`5f06e428` (`fix(pg-rules): port upstream HC analysis fixes -- PriorityUnion and state-keyed
analysis merge`). The direct release-binary bracket is:

| revision | memo-on mean | memo-off mean | `t_off / t_on` | pair spread | pairs |
|---|---:|---:|---:|---:|---:|
| `b37e907e` (parent, pre-port) | 2,987.4 ms | 4,392.0 ms | **1.47064** | 1.41233–1.54870 | 5 |
| `5f06e428` (upstream port) | 2,049.8 ms | 2,052.0 ms | **1.00141** | 0.93164–1.03601 | 5 |

Thus the collapse is at the upstream port, not at a later memo-cap or representation change.
The relevant runtime source is unchanged from `1e216020` to `f3386685` (the intervening change is
test-only), and the current-head five-pair result was
1.01334, with pair spread 0.98715–1.03518.

## Method

Each point used a fresh release `pg-cli` binary built through
`rust/tools/pg.ps1 -Mode release -Package pg-cli -BaseMode off`. The benchmark used the absolute
`C:\Users\johnm\Documents\repos\PanGloss\samples\data\sena.fwdata`, the first 300 words of
the ignored Sena word corpus, `--threads 1`, and `--step-cap 2000000000`. Memo on/off runs were
paired and interleaved in the same binary. The ratio uses the sum of per-word `elapsed_ms` in the
batch TSV, as opposed to process-launch or grammar-load time. Every retained run produced 300
rows, with zero caps and zero timeouts (238 parsed and 62 corpus-invalid words).

The child was invoked through `System.Diagnostics.ProcessStartInfo` with stdout/stderr redirected;
memo and diagnostic environment variables were removed rather than set to empty. The first
five-pair runs are the reported values; earlier two/three-pair pilots were superseded where a
five-pair rerun existed.

## Candidate sweep

The table gives the arithmetic mean of paired ratios and the min–max pair spread. Points with two
pairs are exploratory checks of later candidates; the five-pair boundary above is the causal
bracket.

| revision | change | pairs | `t_on` mean | `t_off` mean | ratio mean | spread |
|---|---|---:|---:|---:|---:|---:|
| `b37e907e` | pre-port baseline | 5 | 2,987.4 | 4,392.0 | 1.47064 | 1.41233–1.54870 |
| `5f06e428` | upstream PR 494/493 port | 5 | 2,049.8 | 2,052.0 | 1.00141 | 0.93164–1.03601 |
| `b9ff6365` | retained-word memo cap | 5 | 2,081.4 | 2,084.8 | 1.00443 | 0.91986–1.05666 |
| `d3f227ab` | saturate memo-key rule counts | 5 | 2,202.2 | 2,219.8 | 1.01222 | 0.88981–1.10723 |
| `bd7ed6f7` | avoid second replay clone | 2 | 2,031.5 | 2,033.5 | 1.00144 | 0.96548–1.03739 |
| `24aacbb7` | per-table byte cap | 2 | 1,963.5 | 2,005.5 | 1.02341 | 0.97706–1.06976 |
| `0bbbe03c` | raise byte cap to 256 MiB | 2 | 2,049.0 | 2,091.5 | 1.02074 | 1.02058–1.02090 |
| `20755dd2` | stream `apply_mrules`/`apply_templates` | 2 | 1,958.0 | 2,036.0 | 1.04042 | 1.01890–1.06194 |
| `4b545fdc` | share alternatives through `Rc` | 2 | 2,091.5 | 2,127.0 | 1.01690 | 1.00240–1.03141 |
| `e4cd9fa7` | push-time alternative prune | 2 | 1,996.5 | 2,094.0 | 1.04876 | 1.02513–1.07239 |
| `149f88df` | exact inverse analysis FS | 2 | 1,273.5 | 1,292.5 | 1.01494 | 1.00470–1.02518 |
| `1e216020` | current relevant runtime | 5 | 1,787.8 | 1,811.0 | 1.01334 | 0.98715–1.03518 |

## Why the upstream port explains the collapse

`5f06e428` changes two related analysis-path behaviors:

* In `pg-rules/src/morph.rs`, analysis syntactic-FS accumulation changes from `Add` to
  `PriorityUnion` (upstream HC PR 494).
* In `pg-rules/src/stratum.rs`, `MergeEquivalentAnalyses` changes from a shape-only fold to an
  `AnalysisStateKey` fold, with the required identity fallback and FS generalization (PR 493).

The paired step probe makes the mechanism visible. On the pre-port binary, memo on/off consumed
`712,547` / `1,485,331` steps respectively: memo avoided `772,784` steps. On the port, the same
probe consumed `436,608` / `502,150`: only `65,542` steps remained memo-sensitive. That is a
91.5% reduction in the memo-on versus memo-off work gap. The memo still saves analysis work, but
the upstream transition/merge changes remove almost all of the redundant work that made caching
valuable; hash/key lookup, retained-result handling, and replay overhead then erase the remaining
wall-clock saving.

The best-supported sub-attribution is PR 494's `PriorityUnion`, not PR 493's state-keyed merge.
The repository's independent five-grammar attribution note
(`docs/research/2026-09-10-hc-analysis-fs-port-measurements.md`) reports Sena attempts falling
from 50,971 to 35,476 on the port, while its shape-keyed attribution variant found the
state-keyed-merge difference only on Amharic. Its Indonesian control likewise attributes the
attempt change to `PriorityUnion`. This is consistent with the step-count bracket here.

## Why the other candidates are not the collapse

The word/byte caps only refuse future memo stores and do not change the cascade; their ratios stay
near parity. Key-count saturation (`d3f227ab`) is itself a memo-key optimization and landed after
the ratio had already collapsed. Streaming and `Rc` alternatives are live-frontier/representation
changes, not memo-subtree elimination. The Sena alternative-prune experiment found zero literal
duplicates to remove, and the exact-inverse analysis-FS commit records Sena as unchanged. None
reproduced the pre-port 47% memo effect; later two-pair ratios remain small/noisy by comparison.

## Could not establish

This sweep establishes the commit-level collapse at `5f06e428`; it does not, by itself, isolate
PR 494 from PR 493 in the same five-pair release benchmark. The finer PR 494 attribution relies on
the committed variant experiment above, which used a different five-grammar/debug measurement
setup. I therefore cannot claim that `PriorityUnion` alone has a release-wall ratio independently
reproduced here, only that it is the strongest supported mechanism inside the collapsing commit.

The measurements cover the first 300 Sena words, not the full 6,146-word corpus. Peak memory was
not independently measured in this sweep, so the reported “zero memory” observation was not
verified here. Finally, the later two-pair points are intentionally weak evidence; their spreads
are shown so they are not mistaken for causal proof.

No repository commit was created.
