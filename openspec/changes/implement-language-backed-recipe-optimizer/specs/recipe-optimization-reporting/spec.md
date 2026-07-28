## ADDED Requirements

### Requirement: Replayable optimization report
Each run SHALL emit schema-versioned JSON and readable Markdown containing input hashes, registry
version, tool version, seed, budgets, search strategy, space counts, pruning reasons, evaluated
candidates, measurements, confirmation status, Pareto frontier, selected recipe, and termination
reason.

#### Scenario: Approximate run completes
- **WHEN** search terminates at its budget
- **THEN** the report states that optimality is unproven and quantifies the explored and unexplored space

### Requirement: Report Plan artifacts
The selected candidate and baseline SHALL each have canonical Plan JSON and a Mermaid diagram
generated from the executable Plan and real capability verdicts.

#### Scenario: Compare selected recipe with baseline
- **WHEN** a run selects a non-baseline recipe
- **THEN** the report links both diagrams and provides a node- and metric-level comparison

### Requirement: Four-grammar promoted experiment
The implementation evidence SHALL run the optimizer on four promoted synthetic conformance
grammars, using at least two distinct realizable recipes for each grammar that admits them, and
SHALL publish one detailed case study selected for the strongest combination of pruning,
performance difference, and construct interaction.

#### Scenario: Promoted experiment completes
- **WHEN** all four grammar runs finish
- **THEN** their full-HC status, candidate counts, algorithms, timings, sizes, and winners are recorded

### Requirement: Evidence boundary is explicit
Reports and fixtures SHALL identify actual-language research only as design provenance. Conformance
fixtures SHALL use synthetic data and SHALL NOT encode actual language names or data in fixture
identifiers, features, or modules.

#### Scenario: Linguistically inspired interaction fixture is added
- **WHEN** a fixture models deletion, reduplication repair, and lexical exceptions
- **THEN** its implementation is synthetic and its external-language inspiration appears only in documentation comments

