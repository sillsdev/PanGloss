## ADDED Requirements

### Requirement: Compile profiles identify the compiled pipeline
Every compile profile SHALL name and fingerprint the constructor/network it measures. Metrics from
an experimental cascade SHALL NOT be labeled as production.

#### Scenario: Production still uses surface-prebaked emission
- **WHEN** diagnostics run before replacement-cascade production wiring
- **THEN** the report contains production emitter/lexc metrics and no production state-explosion curve

### Requirement: Production emitter metrics are available in Stage 1
The production profile SHALL report top-line compile time, emitter/probe stages, per-template lexc
line counts, lexc parse time, final states/arcs, and resource outcomes.

#### Scenario: One template dominates emission
- **WHEN** a template emits most lexc lines
- **THEN** its contribution is identifiable without claiming a replacement-rule net exists

### Requirement: Cascade curve requires production replacement wiring
Per-rule own-net and running composition metrics SHALL be emitted as production evidence only when
the lookup network is built by the production replacement-cascade constructor.

#### Scenario: Experimental cascade is profiled early
- **WHEN** a developer explicitly profiles the P6 prototype
- **THEN** the result is labeled `experimental_composition` and cannot satisfy production-profile gates

### Requirement: Observation does not mutate compilation
Profiling SHALL NOT add minimization, determinization, composition, cloning, or other automaton work
solely to calculate a metric.

#### Scenario: Sink is disabled
- **WHEN** the same grammar compiles without profiling
- **THEN** structural network identity and parse results match the profiled construction path
