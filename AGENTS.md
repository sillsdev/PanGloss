# Agent workflow

- For substantial PanGloss work, use the `agent-handoff` skill at high reasoning effort and delegate concrete, bounded research or implementation tasks to Luna as the normal path. Up to two Luna agents may receive `Workspace` access under the standing repository disclosure authorization.
- Resolve architectural questions with a Luna research handoff before implementation. Before build-heavy delegation, measure live physical memory, commit headroom, CPU load, and active procgov/Cargo trees. Permit up to two concurrent managed builds when both 19 GB job caps fit with ample headroom; increase beyond that only from fresh measurements and keep all Rust work inside `rust/tools/pg.ps1`.
- The primary agent must personally inspect every delegated diff and claim, resolve integration issues, and rerun the authoritative verification. Never treat a Luna result as trusted without review.
