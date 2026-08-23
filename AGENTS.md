# Agent workflow

> Scope: read by OpenAI Codex CLI only. Claude Code (and any other agent) follows `CLAUDE.md`
> instead -- the two files are not interchangeable and this one's "Luna"/"Workspace access" content
> does not apply outside Codex.

- For substantial PanGloss work, use the `agent-handoff` skill at high reasoning effort and delegate concrete, bounded research or implementation tasks to Luna as the normal path. Up to two Luna agents may receive `Workspace` access under the standing repository disclosure authorization.
- Resolve architectural questions with a Luna research handoff before implementation. Before build-heavy delegation, measure live physical memory, commit headroom, CPU load, and active procgov/Cargo trees. Permit up to two concurrent managed builds when both 19 GB job caps fit with ample headroom; increase beyond that only from fresh measurements and keep all Rust work inside `rust/tools/pg.ps1`.
- Before changing an FST threshold, refusal, retry, or containment mechanism, classify the evidence explicitly as **correctness/representability**, **production readiness**, or **resource containment**. Do not use a readiness Error to avoid a contained stress attempt, and never use a larger limit to excuse incomplete or unproven output. The controlling contract is `docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`.
- The primary agent must personally inspect every delegated diff and claim, resolve integration issues, and rerun the authoritative verification. Never treat a Luna result as trusted without review.
