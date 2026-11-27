# Known API debt

Where the surface departs from `SKILL.md`. Recorded 2026-11-27 against v0.35.4 from the API review. Do not copy these into new
surface; fix them in one release window with Motif, with no aliases or legacy spellings kept (owner policy 2026-11-27). Each has a follow-up issue where one exists.

## Flag conventions
- `--json` is a boolean on why-not, coverage, facts, plan-diagram, cost-compare and readings-histogram, but takes a PATH on fst-verify.
- `stats` uses `--format text|jsonl`; `parse` uses `--trace --trace-format=json`; three spellings for "machine output".
- `--out` is a flag on facts, stats and fst-verify, a positional on batch, grammar-health, make-report and import.
- `--flag=value` (`--trace-format=json`, `--guess=false`) and `--flag value` are handled per command by hand-rolled parsers.
- Id lists: comma-separated (`--without`, `--retire`), repeated flag (why-not `--expect`), space-separated positionals (generate).
- Casing: kebab-case flags; serve and `--describe` `interfaces` keys camelCase; `--describe` top level snake_case.

## Errors and exit codes
- CLI errors are free text on stderr, `pangloss <cmd>: ...`, exit 1. Only serve, why-not and compile failures are typed.
- Exit codes: everything is 1 except `stats` (3, missing object facts). Usage errors are not distinguished (should be 2).
- Three error shapes: serve `{type:error,code}`, diagnostic `PANGLOSS_DIAGNOSTIC_ERROR`, compile-error schema 1.
- Batch stderr lines (`TIMEOUT`, `WORK_STEPS`) are machine-parsed: stderr is not only for humans there.

## Discovery
- No `--help` or `-h`; `pangloss --help` prints usage to stderr and exits 1.
- `--describe` flags carry only name, takes-value and summary: no value type, default, output schema id, exit codes or examples.
- About 25 `HC_*`, `PANGLOSS_*`, `PG_*` env vars are read; none is in `--describe`; some are undocumented
  (`HC_DEBUG_CANDIDATES`, `HC_APPLY_PATH_BUDGET`, `PG_ROOT_CACHE_STATS`).
- Env vars and the `--trace-*` rejection flags are invisible to a host that gates on `--describe`.

## Doc and code mismatches
1. `docs/formats/README.md` indexed 9 documents and omitted the generation, retire, parse-analysis, grammar-diagnostics,
   text-analysis and snapshot formats (now indexed; the files stay in `docs/`).

## Versions and parity
- Version schemes mix: string ids in the document (trace-details), integers (grammar-health, facts, stats), a protocol integer
  plus "additive, no bump" (serve), `schema_version: 1` plus added keys (describe).
- Diagnostic output has three names: `rejection-trace-v1.md`, `pangloss.diagnostic.v1`, `--trace-rejections`.
- serve-only ops (inventory, match, effects) cannot be scripted from the CLI; CLI-only features (why-not, stats, analyze-text,
  `--without`, `--gloss`) have no serve op; serve `parse` has no per-request caps, guess or `without`.
- `--without` object kinds widened in #531 after #509 said entries and allomorphs only, with no capability bit to probe.
- No Motif caller and no plan (caller map, 2026-11-27), with the owner's decisions of the same day: `--without` and `--retire` are
  supported API (stable, kept for future capability); `--rule-costs`, `--query-fields`, `--include-selectable` are frozen (no growth);
  serve `health` and `shadowedAllomorphs` are removed outright (PR in progress; `grammar-health` and `--shadowed` are the one route); serve `shutdown` stays. Planned: `--shadowed` (motif#438), `generate --requests` (motif#278).
