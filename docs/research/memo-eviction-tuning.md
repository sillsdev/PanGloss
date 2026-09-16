# Memo eviction tuning: policy, weight, and cap

Measured 2026-09-15 on `research/mem-tamp` at `d7f33d43`, release build of `pg-cli` with
`--features alloc-trace` (`cargo build --release -p pg-cli --features alloc-trace`).

## Method

Workload: the 44-word Aweti list (`samples/data/aweti-words.txt`, first 44 lines) against
`samples/data/aweti.fwdata`, `--step-cap 200000 --threads 1`, one process per configuration.

**Benefit metric is the capped-word count, not wall clock.** A word that hits the step cap returned a
partial result; a word that completes did not. Memo benefit on this workload is exactly the words the
memo rescues from the cap:

```
benefit_preserved = (CAP_off - CAP_config) / (CAP_off - CAP_nomemo_cap)
                  = (24 - CAP_config) / 8
```

24 words cap with the memo off, 16 with the memo on and no byte pressure, so the memo is worth
exactly 8 words here — the same 8 the `memo_corpus_gate` reports as `completed only on`.
`benefit_preserved >= 0.90` therefore requires `CAP == 16`: losing even one word drops it to 0.875.

`ALLOC peak` is the counting allocator's largest per-word peak across the batch (`HC_ALLOC_STATS=1`,
reset per word), so it is a max over words, not a sum.

**Determinism checked, not assumed**: `gdsize` w=1 and freeze at 4 MiB were each run twice.
`CAP` and `memo_evictions` were identical (358,818 both runs); `ALLOC peak` differed by 2 bytes.

The earlier attempt at this sweep produced no usable numbers for two reasons worth recording:
`-Mode run` puts the child on an inherited console handle, so neither `*>` nor `-RunCaptureStdout`
captures its stderr diagnostics; and PowerShell's `$env:X = ''` leaves a variable **set but empty**,
which `std::env::var().is_ok()` reports as present — so an "unset" written that way silently turns
the knob on. Both are why this sweep starts each child from an explicitly rebuilt environment block.

## Controls

| config | cap | CAP | benefit | ALLOC peak | evictions |
|---|---|---:|---:|---:|---:|
| memo off | — | 24 | 0% | 43.4 MB | 0 |
| memo on, no eviction | 256 MiB | 16 | 100% | 362.7 MB | 0 |
| memo on, no eviction | 32 MiB | 16 | 100% | 239.0 MB | 0 |

Lowering the existing cap from 256 to 32 MiB costs nothing and saves 124 MB. No eviction required.

## S1: policy

At 32 MiB the cap is not tight enough to discriminate — every policy holds `CAP=16`, and the three
peaks agree to within one byte across a 20x spread in eviction count, so the peak is not memo-bound
there. Eviction costs ~14 MB of pure overhead for no benefit:

| policy | cap | CAP | ALLOC peak | evictions |
|---|---|---:|---:|---:|
| freeze | 32 MiB | 16 | 239.0 MB | 0 |
| `lru` | 32 MiB | 16 | 253.2 MB | 228,073 |
| `size` | 32 MiB | 16 | 253.2 MB | 11,167 |
| `gdsize` w=1 | 32 MiB | 16 | 253.2 MB | 12,152 |

At 4 MiB the cap bites, and the ordering separates:

| policy | cap | CAP | benefit | ALLOC peak | evictions |
|---|---|---:|---:|---:|---:|
| freeze | 4 MiB | 18 | 75% | **84.5 MB** | 0 |
| `lru` | 4 MiB | 16 | 100% | 190.4 MB | 915,573 |
| **`size`** | 4 MiB | **16** | **100%** | **183.3 MB** | 340,605 |
| `gdsize` w=1 | 4 MiB | 16 | 100% | 210.8 MB | 358,818 |

**`size` wins, and `gdsize` loses to it.** The clock term is not merely inert at this timescale —
it costs 27.5 MB against plain `cost/size` ordering at identical benefit. The controls were not
throwaways: they are what showed the combination losing to one of its own components.

## S2: weight — the knob cannot do anything

| policy | weight | CAP | ALLOC peak | evictions |
|---|---:|---:|---:|---:|
| `gdsize` | 1 | 16 | 210.8 MB | 358,818 |
| `gdsize` | 16 | 16 | 210.8 MB | 358,818 |
| `gdsize` | 256 | 16 | 210.8 MB | 358,818 |

Byte-identical eviction counts across a 256x weight range. This is not a measurement artifact, it is
arithmetic: `pr = clock + W * SCALE / bytes` scales every priority by `W`, and `clock` is assigned
from an evicted entry's own `pr`, so `W` cancels out of every comparison the heap makes. A uniform
scale factor on a quantity used only for ordering is a no-op by construction.

A weight that actually trades recency against size has to scale exactly one term, e.g.
`W * clock + SCALE / bytes`. The knob as specified is unusable and should either be re-specified that
way or removed — but per S1 the clock term is losing to plain size anyway, so removing it is the
cheaper correction.

## S3: cap

`gdsize` shown for continuity with S1; `size` is the better policy and its cap sweep is a gap (below).

| cap | freeze CAP / peak | `gdsize` CAP / peak |
|---|---|---|
| 256 MiB | 16 / 362.7 MB | not run |
| 32 MiB | 16 / 239.0 MB | 16 / 253.2 MB |
| 4 MiB | 18 / 84.5 MB | 16 / 210.8 MB |
| 1 MiB | 21 / 72.0 MB | 17 / 105.6 MB |

Freeze at 1 MiB (72.0 MB, 37.5% benefit) is near this workload's non-memo floor: the live frontier
and the grammar's own loaded tables, which no memo policy can touch.

## Recommendation

**`size` at a 4 MiB byte budget: 183.3 MB peak at 100% benefit** — a 49.5% reduction against the
362.7 MB the current 256 MiB default produces, with every one of the 8 memo-only words still
completing.

The case for eviction is narrower than the design assumed. It is *worse* than simply refusing at a
loose cap, and only earns its place once the cap is tight enough that refusing starts losing words —
at 4 MiB, freeze gives up 2 words while `size` gives up none. What eviction actually buys is the
ability to run a much tighter budget at full benefit.

The competing option deserves stating plainly: **freeze at 4 MiB reaches 84.5 MB — 117% lower than
the recommendation — for the price of 2 of the 8 words (75% benefit).** If a 90% benefit floor is
the requirement, that option is out; if it is negotiable, it is by far the cheaper mechanism, needing
no eviction code at all.

50 MB is not reachable on this workload by any configuration measured. The floor with the memo
effectively disabled is ~72 MB.

## Gaps

- `size` was not swept across caps; the S3 column is `gdsize`, the weaker policy. The recommended
  setting's own cap curve is therefore interpolated from one point.
- One workload only (44 Aweti words). Sena, Mbugwe, Indonesian and Amharic are unmeasured, and the
  Sena control from the original brief was never run.
- `memo_corpus_gate` has not been run at the recommended policy and cap. Until it is, "every one of
  the 8 words still completes" rests on this batch's capped-word count, not on the gate's own
  four-way tally with its analysis-identity assertions.
- No `parse_compare.py` release-binary comparison across the five reference grammars, which the
  oracle-alignment skill requires before any parity claim.
- Peak working set was collected but is not reported here; only allocator peak is.
