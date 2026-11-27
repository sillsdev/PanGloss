---
name: pangloss-api-design
description: >-
  Use before adding or changing a CLI command, flag, serve op or request field, output field, format or schema version,
  env var, error code, exit code or `--describe` entry, and when reviewing a PR that does. Also for "should this be a
  flag, a subcommand or a serve op", removing or renaming surface, and what Motif pins. Holds the surface model,
  principles, placement guide, versioning rules and the PR "Surface" checklist.
---

# PanGloss API design

The surface is small per item and grew fast: 15 minor releases in 13 days, a good part of it with no caller in code.
The risk is drift, not size. This skill says how to add, change and retire surface cleanly. The rules below are owner decisions of 2026-11-27.

## The surface in five lines

1. One parser engine, two hosts: `batch` (files in, TSV out; `parse` is its one-word form) and `serve` (resident JSONL).
2. Grammar-analysis family: `grammar-health`, `stats`, `facts`, `why-not`, `analyze-text`; each output has its own schema version.
3. Grammar queries (`generate`, serve `match` / `effects` / `inventory`): serve-first, `generate --request(s)` is the CLI twin.
4. Dev tooling (`fst-*`, `cost-*`, `recipe-optimize`, env knobs, hidden commands) is not a contract.
5. `pangloss --describe` is the contract. Which diagnostic output? Why a word fails to parse here: `why-not`. Rejections seen
   during an ordinary parse: `--trace-rejections` (`pangloss.diagnostic.v1`). Everything one word did, for a UI: `--trace-details`
   (`trace-details` v5, v6 opt-in). Full unmerged derivation for a human or model: legacy `--trace` JSON. Slow word: `--rule-costs`.

Full table with versions and callers: `docs/formats/INTERFACES.md`.

## Principles

1. **No named caller, no new surface.** The PR names a Motif issue, or a grammar lane and its use, and a wiring plan.
   Why: every item carries permanent docs, tests and refusal rows. Existing surface without a caller is not affected: no
   caller alone is not grounds for removal; retiring a surface is an owner decision, one surface at a time.
2. **Additive within a version.** New optional field, enum value, error code, op: same version. Removed, renamed, retyped or
   re-meant field, new required field, changed row status: new version. Why: Motif pins versions and refuses unknown ones.
3. **Absent flag means old bytes.** New output is reachable only through a new flag or request field, and a golden pins the
   default bytes. Why: #521 nearly made v6 the default; review caught it, no test would have.
4. **Refuse unknown input.** Unknown flag, field, guid or conflicting flags are a named refusal, never silently ignored.
   Why: a silent ignore reads as success. A flag that needs more than about 4 refusal rows is a mode: split it.
5. **Refusal, error and non-answer are three typed things.** Refusal: the request did not run (`bad_request`). Error: it ran
   and failed (`internal`). Non-answer: it ran and honestly does not know (`unknown`, `complete:false`, `stopReason`), exit 0,
   in the document. Branch on `code`, never on message. Why: callers retry, fix input or accept, respectively.
6. **stdout is data, stderr is logs.** One JSON document (`--json`) or JSONL with a `type` per line for streams. Why: hosts
   read stdout only. (Batch sideband lines on stderr are a documented exception; do not add more.)
7. **Bound the output.** Every JSON-emitting path states a size or row bound, sets `truncated: true` and says which flag lifts
   it. Why: callers, especially models, cannot digest unbounded output.
8. **CLI/serve parity by sharing code.** The CLI and serve call one function and a test asserts equal output (`generate
   --requests` equals the `serve` rows). A serve-only or CLI-only capability is declared so in `--describe`.
9. **Agent-friendly.** Errors name the fix ("pass `--x`", the valid list), not just the fault; no prompts; every input has a flag.
10. **One naming convention.** Kebab-case flags, `--flag=value` and `--flag value` both accepted, `--json` is a boolean,
    `--out <path>`, camelCase JSON keys for new documents. Match the existing command before inventing a spelling.
11. **Reuse, do not re-derive.** Call the function that already decides it (AGENTS.md "Never re-derive"). One reduction, one
    computation, two entry points.
12. **Docs and tests ship with the surface**: `--describe` entry, a test that declared equals emitted, `docs/formats/` text
    written for outside readers, a `changes/unreleased/` fragment when user-visible (see its README), the INTERFACES row.

## Where does a new capability go?

Ask in order and stop at the first yes:
1. Can the caller compose it from existing output (post-process `batch --analyses`)? Add nothing.
2. Is it the same document with more or less in it? An option or request field on the existing command or op. Narrowing or
   decorating flags are options; a flag that changes the meaning of others is a mode, so use a subcommand.
3. Is the response shape new, and the work depends on a loaded grammar and many requests? A serve op, with a CLI twin or a
   `--describe` declaration that none exists.
4. Own inputs, own lifecycle, own output document? A subcommand (verb-first, same flag names as its siblings).
5. Does it change bytes a pinned reader sees? A new format version, opt-in by request field or flag; never the default.

Say no when: no named caller; it duplicates an entry point (`grammar-health` and a serve op for the same report would be two routes; keep one); the
only reason is "might be handy"; it needs a new error shape instead of the existing codes; or it breaks Principle 3.

## Versioning, stability, retirement

- One scheme: every JSON document carries `schema: "pangloss.<name>"` plus an integer `schemaVersion`. Existing formats switch
  on their next version bump, coordinated with Motif, with no compatibility layer (the follow-up issue lists which differ today). Writers never emit a new version unless asked.
- Readers refuse an unknown version, may ignore unknown fields, and degrade on an unknown enum value only where the doc says so.
- Default output stays the oldest version Motif reads. The default moves only after Motif's pin moves, in a release listed under
  "Breaking and format changes".
- Stability levels, shown in `--describe` and INTERFACES.md (the `--describe` part is a follow-up): experimental (may change
  without coordinating with Motif; it still needs a named caller), stable (changes only additively, or with a version bump
  coordinated with Motif), frozen (never grows).
- Retirement (owner policy 2026-11-27): when the owner retires a surface it is removed outright in the next release. No alias
  windows, deprecation notices, migration layers or legacy flags kept for compatibility. The only gates: Motif confirms it does
  not call the surface (or changes in step), and the owner decides per surface. Renaming or normalising a flag likewise happens
  with no alias, coordinated with Motif in one release window. List the removal under "Breaking and format changes".

## Surface checklist (copy into the PR body under "Surface")

- [ ] Need: named caller (Motif issue, or lane and use) and the wiring plan; stability level stated.
- [ ] Placement: why not compose, option, serve op, subcommand (see guide above).
- [ ] Default bytes: unchanged with the new surface absent; golden test named.
- [ ] Version: additive in version N, or new version M opt-in; pinned versions untouched.
- [ ] Refusals: every conflicting flag or field is a tested refusal naming its reason.
- [ ] Errors: stable codes listed in docs and `--describe`; no new free-text-only failure for a host.
- [ ] Bounds: size bound, `truncated`, the lifting flag.
- [ ] Discoverable: `--describe` entry and declared-equals-emitted test, `docs/formats/` text, `docs/formats/INTERFACES.md` row.
- [ ] Reuse: names the existing function it calls.
- [ ] Motif: pinned surface unchanged, or the Motif issue and pin move are linked.
- [ ] Retirement: what removes it later.

## The Motif contract

Motif pins a PanGloss tag (`SourceTag`) and each format in `src/SIL.Motif.Host/PanGloss/PanGlossInterfaceVersions.cs`
(in the Motif repo): trace-details id, grammar-health, facts, batch-TSV columns. Its `FakePanGloss` imitates the pinned binary
and has its own surface test, so it can drift from both. Motif fails its build on a format mismatch. Treat anything Motif parses
as frozen except additive keys until its pin moves. The missing piece is a contract test that runs the built binary and pins
default output bytes (trace-details v5, diagnostic.v1, batch TSV, `--describe` keys) against Motif's fixtures. Until it exists,
Principle 3 is enforced by review only. Never read Motif's pins or callers from the WSL clone (stale; Motif lands on Windows): ask the motif-cli lead or read the Windows clone via `herdr-x run win`. Details and the #521 near-miss: `references/motif-contract.md`.

## The inventory

`docs/formats/INTERFACES.md` is the human table of every command, serve op, format, env var and WASM item with version and known
callers; `--describe` is the machine twin. A change that touches surface updates both in the same PR. When the table and the
code disagree, the code wins and the table gets fixed. A gate test that enforces this is a follow-up, not yet code.

## Known drift

This skill describes the target. The code does not yet meet all of it: `--json` and `--out` mean different things in different
commands, CLI errors are untyped free text, there is no `--help`, and docs trail code. See `references/known-debt.md` and do not
copy a debt item into new surface. New commands follow the principles above, not the nearest existing violation.
