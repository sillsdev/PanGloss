# PanGloss interfaces: one inventory

Every command, serve op, output format, environment variable and WASM item PanGloss exposes, with its version and known callers.
`pangloss --describe` is the machine twin and wins where the two disagree; fix this table in the same PR that changes surface.
Design rules for changing any of it: `.claude/skills/pangloss-api-design/SKILL.md`. Drift not yet fixed:
`.claude/skills/pangloss-api-design/references/known-debt.md`.

Snapshot: PanGloss v0.35.4, 2026-11-27. "Added" is the oldest CHANGELOG mention (approximate).
**Callers** are per Motif caller map, 2026-11-27 (source: Motif origin/main `src/`, non-test; Motif pins PanGloss 0.35.0, with a move to 0.35.2 or later in motif#573): M = Motif calls it, L = lane scripts only, D = dev tooling,
none = no caller and no plan. No caller alone is not grounds for removal; retirement is an owner decision per surface, and a retired surface (**removed**) is deleted outright with no notice or alias.

**Level** (owner decision 2026-11-27): **experimental** = may change without coordinating with Motif; **stable** = changes only additively, or with a version bump coordinated with Motif; **frozen** = never grows. `--without` and `--retire` are stable; `--rule-costs`, `--query-fields` and `--include-selectable` are frozen. Other rows carry a provisional level (stable if Motif or a lane calls it, experimental for dev tooling) until the owner sets it. Showing the level in `--describe` is code: see the follow-up issue.

## CLI commands

| Command | What | Added | Version / output | Callers | Level |
|---|---|---|---|---|---|
| `batch` | parse a word list to TSV, about 35 flags | 0.3.x | batchTsv 5 ([batch-tsv.md](batch-tsv.md)) | M, L | stable |
| `parse` | one word: trace, gloss, realize | 0.3.x | trace-details v5 (v6 opt-in), legacy `--trace` JSON | M | stable |
| `serve` | resident parser, JSONL on stdin and stdout | 0.3.1 | protocol 2 ([serve-protocol.md](serve-protocol.md)) | M | stable |
| `import` | fwdata or fwbackup to snapshot | 0.3.0 | snapshot version 1, schemaVersion 2 | M | stable |
| `facts` | snapshot to facts.sqlite | 0.7.0 | facts schema 9, context 1, stats-manifest 2 | M | stable |
| `grammar-health` | health report JSON | 0.3.0 | schema_version 5 | M (CLI) | stable |
| `stats` | read the stats cache | 0.14.0 | cache 13/14, counters 8 ([stats-cache.md](stats-cache.md)) | M, L | stable |
| `stats-cache` | cache maintenance (not in USAGE) | 0.21.0 | | none | stable |
| `phonology-examples` | request, save, check phonology examples | 0.8.5 | | M | stable |
| `generate` | HC XML ids; `--request`, `--requests` JSONL (0.35.3) | 0.29.0 | request schemaVersion 1 (`docs/generation.md`) | M (via serve), L; `--requests`: Motif caller planned (motif#278) | stable |
| `why-not` | first failing constraint | 0.28.0 | `pangloss.whynot.v1` ([why-not.md](why-not.md)) | M | stable |
| `analyze-text` | segment text, Language Pack evidence | 0.17.0 | `docs/text-analysis-format.md` | none | stable |
| `cost-compare`, `readings-histogram` | compare batch TSVs | 0.13.0 | | D | experimental |
| `fst-health`, `fst-verify`, `coverage`, `plan-diagram`, `make-report`, `recipe-optimize` | foma-tools build commands | 0.5.0 to 0.24.0 | fst-health 10; fst-verify ([fst-verify.md](fst-verify.md)) | D | experimental |
| `describe` / `--describe` | machine-readable surface | 0.3.0 | schema_version 1 | M (capability gate) | stable |
| `--version`, `-V` | version | 0.1.0 | text | M | stable |
| `cost-calibrate`, `__recipe-optimize-child`, `__compile-worker-child` | hidden, internal | | | D | experimental |

### Flag groups worth knowing

| Group | Flags | Added | Callers | Level |
|---|---|---|---|---|
| batch caps | `--work-cap`, `--word-timeout-ms`, `--threads`, `--work-columns`, `--word-mem-cap-mib`, `--capture-cap-mib` | 0.9.0 to 0.2x | M | stable |
| batch analyses | `--analyses`, `--morph-spans` (0.34.0), `--query-fields` (0.35.0) | 0.10.0 | M for `--analyses`; `--query-fields`: frozen | stable (`--query-fields` frozen) |
| batch stats | `--stats`, `--cache`, `--stats-manifest`, `--stats-profile`, `--stats-compact`, `--cache-pack` | 0.14.0+ | M (first three) | stable |
| batch retire | `--retire`, `--retire-out` (`pangloss-retire/v1`), `--without` | 0.35.0, 0.35.3 | none; supported (stable, future capability) | stable |
| batch result cache | `--result-cache`, `--result-cache-max-mib` | 0.35.0 | none | stable |
| batch engine | `--engine hc/fst`, `--prefilter`, `--partials`, `--start` | 0.18 to 0.24 | L, D | experimental |
| batch diagnostics | `--cost-column`, `--no-flush`, `--phase-marks`, `--word-phase-marks` | 0.13+ | D, L | experimental |
| rejection trace | `--trace-rejections`, `-max-paths`, `-max-events`, `-max-bytes`, `-max-depth` | 0.3.0 | M (parse) | stable |
| parse | `--trace[=file]`, `--trace-format`, `--trace-details`, `--rule-costs[=N]` (0.35.4), `--gloss`, `--natural-gloss`, `--realize-map`, `--guess`, `--without` | | M for the trace trio; `--rule-costs`: frozen; `--without`: supported; rest none | stable (`--rule-costs` frozen) |
| serve | `--project`, `--work-cap`, `--word-timeout-ms`, `--word-mem-cap-mib`, `--guess`, `--timing` | 0.3.1 | M | stable |
| grammar-health | `--fw-project`, `--stored-analyses`, `--snapshot-out`, `--facts-out/-context/-light`, `--shadowed`, `--include-selectable` (0.35.1), `--fst*`, `--log-guids` | | M for the first group; `--shadowed` planned (motif#438); `--include-selectable`: frozen | stable (`--include-selectable` frozen) |
| why-not | `--expect`, `--expect-guid`, `--expect-json`, `--allomorphs`, `--json` | 0.28.0 | M | stable |
| stats | `--group`, `--kind`, `--object`, `--stratum`, `--direction`, `--word`, `--top`, `--sort`, `--exclude-censored`, `--wide`, `--by-kind`, `--format`, `--cache`, `--out` | 0.14.0 | M (`--group`, `--format`, `--cache`, `--out`) | stable |
| facts | `--out`, `--context`, `--stats`, `--stats-manifest`, `--light`, `--json` | | M | stable |

## serve ops (protocol 2; `interfaces.serveOps` in `--describe`)

| Op | Added | CLI twin | Callers | Level |
|---|---|---|---|---|
| `parse` (`traceDetails`, `ruleTrace`, `ruleCosts`) | 0.3.1 | `parse` | M | stable |
| `generate` | 0.29.0 | `generate --request(s)` | M | stable |
| `inventory` | 0.3.0 | none | M | stable |
| `match` (`boundaryMarkers` 0.35.3) | 0.29.0 | none | M | stable |
| `effects` (`outputMatchPattern` 0.35.0) | 0.29.0 | none | M | stable |
| `shadowedAllomorphs` | 0.35.0 | `grammar-health --shadowed` | removed outright (PR in progress); the CLI `--shadowed` (planned, motif#438) is the one route | removed |
| `health` | 0.35.3 | `grammar-health` | removed outright (PR in progress); CLI `grammar-health` is the one route | removed |
| `shutdown` | 0.3.1 | n/a | none; keep | stable |

## Formats and schemas alive

| Format | Version | Doc |
|---|---|---|
| batch TSV | 5 | [batch-tsv.md](batch-tsv.md) |
| serve protocol | 2 | [serve-protocol.md](serve-protocol.md) |
| trace-details | v5 default, v6 opt-in (`--rule-costs`) | [trace-details.md](trace-details.md) |
| legacy trace | unversioned | [trace-format.md](trace-format.md) |
| rejection diagnostics | `pangloss.diagnostic.v1` | [rejection-trace-v1.md](rejection-trace-v1.md) |
| why-not | `pangloss.whynot.v1`, expect-v1 | [why-not.md](why-not.md) |
| generate request | schemaVersion 1 | `docs/generation.md` |
| retire | `pangloss-retire/v1` | `docs/retire-format.md` |
| parse-analysis | v1 | `docs/parse-analysis-format.md` |
| grammar-health | 5 | `docs/grammar-diagnostics.md` |
| stats cache | 13/14, counters 8 | [stats-cache.md](stats-cache.md) |
| facts | 9, context 1, stats-manifest 2 | `docs/grammar-facts-format.md` |
| snapshot | version 1, schemaVersion 2 | `docs/snapshot-format.md` |
| text analysis | | `docs/text-analysis-format.md` |
| compile error | schema 1 | `docs/compile-errors.md` |
| fst-health | 10 | |
| describe | schema_version 1 | this file |

## Which diagnostic output to use

| Question | Use |
|---|---|
| Why does this word not parse as these morphs? | `why-not` |
| What did ordinary parsing reject? | `parse` or `batch` with `--trace-rejections` |
| Everything one word did, structured | `parse --trace-details` |
| Which rule spent the work budget? | `parse --rule-costs` (trace-details v6) |
| Full unmerged derivation for reading | `parse --trace` |

## WASM / JS (`pg-wasm`)

`PanGlossGrammar` (constructor, `withoutWorkLimits`, `withWordMemCap`, `analyzeText`, `analyzeWord`, `classCatalog`,
`classificationMatrix`, and the supplied-entry, lexicon, gloss-language and authority methods) and `PgPack` (`grammarId`,
`signatureState`, and others). Analysis-only, on an XML grammar (`openspec/changes/make-wasm-analysis-only`). Callers: none confirmed.

## Environment variables

About 25 `HC_*`, `PANGLOSS_*` and `PG_*` variables are read, plus `XDG_*`, `LOCALAPPDATA` and `HOME` for cache paths: for example
`HC_WORD_STATS`, `HC_FRONTIER_STATS`, `HC_ALT_YIELD`, `HC_RSL`, `HC_FST_PROFILE`, `HC_PREEXPAND_*`, `HC_*_BUDGET`,
`HC_DEBUG_CANDIDATES`, `PG_ROOT_CACHE_STATS`, `PANGLOSS_CORPUS_ROOT`, `PANGLOSS_CGROUP_DELEGATED_ROOT`,
`PANGLOSS_TRACE_EMIT_STAGES`. All are developer knobs, none is in `--describe`, none is a contract, and a few are undocumented.
