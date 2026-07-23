## ADDED Requirements

### Requirement: Pattern and environment lowering has one semantic boundary
The FST compiler SHALL lower frozen grammar patterns and environments through one internal IR that
preserves anchors, polarity, grouping, alternation, character-table identity, and quantifier metadata.

#### Scenario: A node is not supported by a consumer
- **WHEN** lowering encounters a represented variant that the active compiler cannot consume
- **THEN** it returns a typed unsupported disposition and does not omit or weaken the node

### Requirement: Extracting the seam does not change existing compilation
Rules supported before the seam extraction SHALL retain equivalent networks and exact
proposer-to-confirm analysis results.

#### Scenario: Existing replacement fixtures run
- **WHEN** the multi-table, RTL, and Simultaneous gates compile through the shared seam
- **THEN** their network fingerprints and confirmed analysis multisets remain unchanged
