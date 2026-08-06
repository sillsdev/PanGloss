# OpenSpec here: changes and tasks only. No specs.

This project uses OpenSpec as a **change and task tracker**. It does not use the spec half, and that
is a decision, not an omission.

## What defines behaviour

The code, its tests, `README.md`, `CONTEXT.md`, `CLAUDE.md`, and `docs/`. Every one of those is
either executed or read routinely, so it is either verified or noticed when wrong.

A spec is none of those. It would be a fourth description of behaviour that nothing compiles, nothing
runs, and nobody re-reads — the exact shape this repo has repeatedly had to clean up. A requirement
that matters should be stated where it can be enforced: **name the test**. Durable external knowledge
(a paper, an algorithm, a measurement) goes in `docs/research/`.

## The rules

- **Never create a `specs/` directory inside a change.** The `code-defined` schema
  (`openspec/schemas/code-defined/schema.yaml`) has no specs artifact, so `openspec status` reports
  3/3 on proposal + design + tasks alone.
- **Never create or populate `openspec/specs/`.** There is no canonical spec tree and there should
  not be one. Do not run `openspec archive`'s spec-sync step, and do not use the
  `openspec-sync-specs` skill — it has been removed from `.claude/skills/` and `.codex/skills/` for
  that reason.
- **Archive by moving**, to `openspec/changes/archive/YYYY-MM-DD-<name>/`. See that directory's own
  README.

## `openspec validate` does not work here, by design

It fails every change with *"Change must have at least one delta. No deltas found."* That check is
hardcoded in the CLI rather than driven by the schema, so it cannot be switched off — and the only
thing it validates is the well-formedness of the artifact this project has deliberately abolished.

**28 red lines from `openspec validate` are expected. Do not fix them by adding specs.** What still
works, and is what we actually use: `openspec list`, `show`, `status`, `doctor`, and task-checkbox
progress tracking.
