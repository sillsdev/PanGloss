## Context

`plan.rs` holds five closed node kinds — `Leaf`, `Compose{children,strategy}`, `Union`,
`Gate{partition,children}`, `Replace{cascade,children}` — with `NodeId` a stable FNV-1a content
address over the node's own semantic content (a Merkle address, so a node's id changes exactly when
its meaning does). `enumerate.rs` builds plans; `build_controllable` executes them.

## Goals / Non-Goals

- **Goal:** a grammar author can look at one picture and see how their language is decomposed, and
  where a construct was refused.
- **Goal:** the diagram is faithful by construction — generated from the same `Plan` the compiler
  executes, never a parallel hand-maintained drawing that can drift.
- **Non-Goal:** an interactive explorer. Mermaid in markdown, because that renders in the report, in
  a PR, and in a plain file viewer with no tooling.
- **Non-Goal:** rendering the compiled automata. Node counts and arc counts belong in the report's
  timing/size section; drawing an FST with a million arcs helps nobody.

## Decisions

- **JSON first, mermaid second.** Two steps, not one, so the JSON is available for anything else
  (diffing two grammar revisions, feeding a future viewer, machine checks) and the mermaid renderer
  stays a pure function over a documented shape. This mirrors `crate::health`'s established
  "canonical JSON is the source artifact; the rendering is a view" convention.
- **`NodeId` is the diagram's node identity.** Content addressing means two runs over an unchanged
  grammar produce an identical diagram, and a diff between two revisions highlights exactly the
  subtrees whose meaning changed. That property is free and worth preserving deliberately.
- **Label by linguistic role, with the node kind secondary.** A reader needs "prefix layer, stratum
  2" before they need `Compose[Static]`. The mapping from node to linguistic description has to come
  from the plan's own payload (partition keys, cascade rule ids, leaf detail) rather than a second
  source of truth.
- **Collapse by default above a size threshold.** A plan over a realistic lexicon has far too many
  leaves to draw. Default to summarizing sibling leaf groups as a single labelled node carrying a
  count, with an opt-in full rendering. State the threshold in the output, so a reader always knows
  whether they are seeing everything.

## Risks / Trade-offs

- **A diagram is a claim.** If a node's label says a construct is handled when it is refused, the
  picture lies more persuasively than prose would. So capability outcomes must be rendered from the
  real `compose_envelope`/predicate verdicts, not inferred from the node's existence — a node exists
  in the plan whether or not it was admitted.
- Mermaid has practical size limits; very large graphs fail to render rather than degrading. The
  collapsing default exists to stay inside that, and the renderer should report the node count it
  emitted so a failed render is diagnosable.
- The JSON shape becomes a compatibility surface the moment anything consumes it. Version it from the
  start, the same way `COVERAGE_LEDGER_SCHEMA_VERSION` is versioned.
