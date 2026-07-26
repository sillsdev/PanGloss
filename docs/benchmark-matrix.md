# Benchmark matrix — reference corpora, per-word latency and build time

Closes gap G6 (`docs/hermitcrab-rust-port-audit.md` §3 item 5: "No aggregated p50/p95/build-time
benchmark matrix has been published across all three reference corpora — informal timing
measurements exist per-item, not a single authoritative table").

Measured 2026-07-26 at `85f25dc`, release build, `--threads 1`, on Windows 11 / this workstation.
Numbers come from `pangloss batch`'s own per-word `elapsed_ms` column (column 3 of the 5-column
parity TSV) — the same measurement the parity path already takes, so this adds no instrumentation
and does not touch a parity-sensitive format.

**Resolution floor is real and is reported as such.** `elapsed_ms` is integer milliseconds, so a word
completing in under 1 ms lands in the 0 bucket; those are shown as `<1`, never as `0`.

---

## The headline is not a latency number

**All three reference grammars are REFUSED by the `--engine=foma` (optimized) path under default
capability enforcement.** Each for a different, specific reason:

> **CORRECTION 2026-07-26.** The first version of this table listed ONE refusal for Indonesian. That
> was wrong — an artifact of reading only the tail of the diagnostic output instead of counting every
> `capability-refuse` line. Indonesian has **three**. The corrected table is below, and it makes the
> conclusion *stronger*, not weaker: `mpr-group.overwrite-output` is present in **all three**
> reference grammars, not two.

| Grammar | Refusing predicates (complete) | Status of each gap |
|---|---|---|
| Indonesian | `quantifier.bounded-expansion` (`prule 2`), `compounding.non-recursive` (`mrule 0`), `mpr-group.overwrite-output` (`MprGroup 0`) | quantifier **CLOSED 2026-07-26** (tasks.md 4.5); compounding **PROVABLE** (4.1); MPR **PERMANENT CARVE-OUT** |
| Amharic | `mpr-group.overwrite-output` (`MprGroup 0`) | **PERMANENT CARVE-OUT** — structurally unsound for a monotone admission filter; the ADR 0005 override is the only on-ramp |
| Sena | `compounding.non-recursive` (`mrule 0`), `mpr-group.overwrite-output` (`MprGroup 0`) | compounding **PROVABLE** (4.1); MPR the same permanent carve-out |

**So no reference grammar can EVER clear the `--engine=foma` capability gate**, at any point in the
future, without `--allow-unproven`. All three carry `MprGroupOverwrite`, whose refusal is not an
unfinished construction but a structural impossibility for a monotone-accumulation admission filter
(`pg_grammar::model::mpr_add_output`'s own doc; ADR 0001's worked confirm-only trap). Closing 4.5 and
4.1 reduces each grammar's refusal set but cannot empty any of them.

That is a designed-in property of the honest-boundary architecture, not a defect and not a roadmap
item. What it means practically: the FST path ships for grammars that avoid `Overwrite` MPR groups,
and for the reference corpora it is exercised under the ADR 0005 override with a `trust=unproven`
stamp — which is exactly what that override exists for.

Two consequences worth stating plainly, because they are easy to mis-read:

1. **The coverage cross-check reporting 20/20 Covered does not mean the optimized path runs on the
   reference grammars.** It does not, on any of the three. Those are different claims: coverage is
   about whether each *construct* has evidence at its own disposition; this is about whether a
   *particular grammar* clears the capability gate. Both statements are true simultaneously and
   neither implies the other. (`docs/conformance/shared-construct-id-analysis.md` says the same thing
   about row-level vs configuration-level completeness.)
2. **All three reference grammars contain a permanently carved-out construct**
   (`MprGroupOverwrite`). So "the FST path runs the reference grammars with capability enforcement on"
   is **not reachable by design**, now or later — only via the documented ADR 0005 override, which
   stamps the result `trust=unproven`. That is the honest boundary, not a defect to be fixed.

This also settles a question the unbounded-quantifier decision left open. That record argued the
construct mattered because `max=-1` is the DTD/loader **default** and so likely common; it turns out
to be load-bearing in a *reference grammar* (Indonesian's `prule 2`), which is stronger evidence than
the DTD-default argument alone. It was **not** Indonesian's only blocker — see the correction above —
but it was a real one, and it is now closed.

---

## Default engine (`--engine=default`) — the oracle / fallback path

This is the full-HermitCrab parser: correct by construction, and explicitly **not** the path being
optimized (`docs/fst-plan/foma-fst-plan.md`; the optimized path is foma-propose + HC-confirm). Read
these as oracle costs, not as product latency.

| Corpus | words | ok | p50 | p95 | p99 | max | total |
|---|---|---|---|---|---|---|---|
| Indonesian | 121 | 120 | `<1` ms | 5 ms | 16 ms | 42 ms | 0.23 s |
| Amharic | 673 | 669 | 131 ms | 18,995 ms | 105,595 ms | 454,499 ms | 3,134 s |
| Sena | **5,365 of 7,121** | 5,141 | 31 ms | 2,104 ms | 12,323 ms | 617,327 ms | 4,533 s |

**Sena is partial and must not be quoted as complete**: the run was killed at 5,365 of 7,121 words.
Because the corpus is not sorted by difficulty, the percentiles are indicative rather than final. The
`ok` counts likewise exclude words the run never reached.

The tail is the story: Amharic's p99 is **105 seconds** and its worst word takes **7.6 minutes**;
Sena's worst takes **10.3 minutes**. Against the project's own sub-10 ms/word target
(`build-for-full-scale-grammars`), the oracle path misses by four to five orders of magnitude at the
tail while being entirely acceptable at the median (31–131 ms). This is precisely why the FST
propose-and-confirm path exists, and why the worst-word pinning in
`.claude/skills/dead-end-census` is a standing lever rather than a one-off.

---

## Optimized engine (`--engine=foma`) — one force-compiled data point

Since the gate refuses all three, the only way to measure the optimized path on a reference corpus
today is `--allow-unproven` (ADR 0005), which force-compiles and stamps the result
`trust=unproven`. **Reported as force-compiled; not a certified configuration.**

Indonesian, same corpus, same binary, same run conditions:

| Engine | words | ok | p50 | p95 | p99 | max | total |
|---|---|---|---|---|---|---|---|
| default (oracle) | 121 | 120 | `<1` ms | 5 ms | 16 ms | 42 ms | 0.23 s |
| foma (`--allow-unproven`) | 121 | 120 | `<1` ms | **1 ms** | **1 ms** | **8 ms** | **0.02 s** |

**~11× faster end-to-end, 5× better p95, 5× better worst case — with byte-identical signatures for
every one of the 121 words** (diffed on the `(word, signature)` projection; zero differences). So
where the optimized path can run, it is both faster and exactly agreeing with the oracle, which is
the propose-and-confirm contract working as designed.

Amharic and Sena were **not** measured on the foma path. They were not attempted under
`--allow-unproven` because force-compiling a permanently-carved-out construct produces a result whose
correctness is not underwritten by anything, and a latency number for it would invite exactly the
over-reading this document is trying to prevent.

---

## Build / compile time

Not separately instrumented in this pass. `pangloss batch` measures `grammar_load_ms`,
`compile_ms`, and `morpher_build_ms` internally (`pg-cli/src/main.rs:963-1117`) but does not emit
them to the TSV, so capturing them means either parsing progress output or adding a reporting flag.
Since the three grammars that matter are refused before compilation, a build-time column would only
have one honest row today anyway. Deferred rather than half-filled.

Per-grammar compile figures from earlier optimization passes are recorded in
`docs/fst-plan/foma-fst-plan.md` and the memory record on FST optimization; those are the numbers to
consult until this column is filled properly.

---

## Reproducing

```
cargo build --release -p pg-cli
pangloss batch samples/data/<g>-hc.xml samples/data/<g>-words.txt out.tsv \
    --threads 1 --engine=default
# percentiles from column 3, skipping the STARTED progress rows that --threads 1 emits
awk -F'\t' '$3!="STARTED"{print $3}' out.tsv | sort -n | ...
```

The `--threads 1` mode writes a `STARTED` row per word before its result row; any analysis of this
TSV must filter those out or every count doubles.

## What would make this table complete

1. **tasks.md 4.5** (unbounded quantifier) — unblocks Indonesian on the foma path with enforcement ON,
   turning the force-compiled row above into a certified one.
2. **tasks.md 4.1** (recursive compounding) — one of Sena's two refusals.
3. A finished Sena default run (~75+ min) for final rather than indicative percentiles.
4. A build-time column, once the CLI reports its existing `compile_ms`/`morpher_build_ms` measurements.
5. Amharic and Sena foma-path numbers are gated on the ADR 0005 override by design and should be
   published only alongside the `trust=unproven` stamp, if at all.
