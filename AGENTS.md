# Agent workflow

- For substantial PanGloss work, use the `agent-handoff` skill at high reasoning effort and delegate concrete, bounded research or implementation tasks to Luna as the normal path. Up to two Luna agents may receive `Workspace` access under the standing repository disclosure authorization.
- Resolve architectural questions with a Luna research handoff before implementation. Run at most one build-heavy Luna handoff at a time; use the second slot only for read-only research or review so managed Rust build slots are not starved.
- The primary agent must personally inspect every delegated diff and claim, resolve integration issues, and rerun the authoritative verification. Never treat a Luna result as trusted without review.
