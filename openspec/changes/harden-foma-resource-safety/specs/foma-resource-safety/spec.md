## ADDED Requirements

### Requirement: Compile and apply work have explicit resource boundaries
Potentially adversarial foma compilation and application SHALL execute under separately reported
`worker_watchdog_limits` and `logical_work_budgets`. The watchdog SHALL enforce wall time and I/O
sizes in the parent and SHALL sample worker RSS at a reported interval. Sampled RSS SHALL NOT be
described as a kernel-enforced or exact memory ceiling.

#### Scenario: One opaque compose grows beyond sampled RSS policy
- **WHEN** a sample observes the worker above its configured RSS guardrail
- **THEN** the parent kills the worker and reports the sampled value, interval, and typed outcome

### Requirement: Resource configuration cannot disable absolute ceilings
Compile and apply SHALL have versioned, hard-coded, deliberately high absolute ceilings for every
enforced logical, byte, and wall-time dimension. Defaults, named envelopes, host policy, and caller-
selected limits SHALL resolve at or below those ceilings. No configuration value SHALL mean
unlimited or raise an effective limit above its absolute ceiling.
The runtime application dimensions and absolute values SHALL be one portable set shared by Windows,
Linux, and WASM. Applications MAY choose different lower effective limits but SHALL NOT redefine or
raise the shared absolute values.

#### Scenario: A caller requests an excessive apply budget
- **WHEN** the requested value exceeds the hard-coded ceiling
- **THEN** configuration is rejected or clamped according to the public configuration contract and
  the reported effective value never exceeds the ceiling

#### Scenario: Normal work uses the default envelope
- **WHEN** it reaches a calibrated early logical limit
- **THEN** it stops with actionable diagnostics well before the absolute emergency ceiling

#### Scenario: Native and WASM analyze under the same requested limits
- **WHEN** the request and analysis artifact are equivalent
- **THEN** both runtimes expose the same budget dimensions, absolute ceilings, and typed exhaustion contract

### Requirement: Every compilation path is checked
Newly compiled lexc, regex, first-rule, single-rule, and no-rule networks SHALL be size-checked before further use.

#### Scenario: Huge single network avoids a fold
- **WHEN** one compiled network already exceeds the envelope
- **THEN** compilation fails before final minimization or application

### Requirement: Windows and Linux are equal worker targets
Windows and Linux SHALL expose the same versioned worker protocol, wall-time/I/O enforcement,
sampled-RSS semantics, and typed outcomes. Neither target SHALL be CI-only or subordinate.

#### Scenario: Equivalent work breaches the watchdog on either platform
- **WHEN** adversarial work exceeds an enforced or sampled watchdog bound on Windows or Linux
- **THEN** the parent survives and receives the same typed outcome contract

#### Scenario: One platform runner is unavailable during development
- **WHEN** verification lacks a Windows or Linux runner
- **THEN** that evidence is `not_run` and independent implementation and tests continue

### Requirement: The watchdog is scoped to one non-descendant worker
The production compilation worker SHALL NOT launch descendant processes. The watchdog SHALL use
standard child waiting/killing plus sampled worker telemetry and SHALL NOT introduce general
process-tree, Job Object, cgroup, async-runtime, CPU-quota, or process-count infrastructure.

#### Scenario: Future compiler work requires a descendant
- **WHEN** a proposed production compiler path would launch another process
- **THEN** it requires a new bounded containment design and cannot silently rely on this watchdog

### Requirement: Worker communication is bounded
Worker request bytes, result bytes, stdout, stderr, and diagnostic payloads SHALL have versioned
limits enforced by the parent.

#### Scenario: Worker floods diagnostic output
- **WHEN** captured output reaches its configured byte limit
- **THEN** the parent terminates the worker and returns a typed output-limit outcome

### Requirement: Resource failure never starts another strategy automatically
A terminal resource failure SHALL return its typed outcome immediately. It SHALL NOT retry, invoke
another engine, or start another compilation or analysis strategy. Only a new explicit caller
request MAY select different limits or a different public strategy.

#### Scenario: A strategy reaches its wall limit
- **WHEN** the foma strategy times out
- **THEN** the system records and returns the reason without starting any other strategy

### Requirement: Analysis pipelines are explicit and stable for a request
The runtime SHALL support explicitly named FST-propose-plus-HermitCrab-confirm and HermitCrab-only
pipelines. The selected pipeline SHALL be recorded in every word and batch outcome and SHALL NOT
change during the request. Both pipelines SHALL use the same portable budget dimensions, atomic word
outcomes, and caller-controlled retry contract.

#### Scenario: An engine requests HermitCrab parse diagnostics
- **WHEN** it explicitly selects the HermitCrab-only pipeline
- **THEN** the runtime runs that pipeline under the effective apply budgets and returns its detailed
  parse-failure diagnostics with the named pipeline

#### Scenario: The combined pipeline exhausts a budget
- **WHEN** FST-propose-plus-HermitCrab-confirm returns incomplete
- **THEN** HermitCrab-only is not started unless the caller submits a new request selecting it

### Requirement: Larger-limit retries are explicit new requests
The compiler SHALL NOT automatically increase a limit or retry a resource-terminated compilation.
A caller MAY start a new attempt by explicitly selecting a named, versioned resource envelope with
larger limits. The new attempt SHALL reference or preserve the prior terminal finding.

#### Scenario: An AI caller reaches the default work budget
- **WHEN** compilation terminates at that budget
- **THEN** the caller promptly receives the reached counter, effective envelope, partial measurements,
  and applicable remedies without an automatic retry

#### Scenario: A caller intentionally retries with more resources
- **WHEN** it selects a larger named envelope in a new request
- **THEN** the compiler runs once under that envelope and records both the selected limits and the
  prior failure context

### Requirement: Deterministic work budgets fail before the wall watchdog
Logical counters for states, arcs, emitted lines, products, paths, candidates, and other owned work
SHALL be the primary explosion controls. The parent wall-time limit SHALL remain an outer host-safety
watchdog for uninstrumented work, stalls, and machine-dependent delays rather than the normal
compiler-health cutoff.

#### Scenario: A grammar causes a rapidly growing alternatives product
- **WHEN** its deterministic work counter reaches the effective envelope limit
- **THEN** compilation stops with the counter, responsible constructs, factors, and partial
  measurements without waiting for the wall-time watchdog

#### Scenario: Native compilation stops making observable progress
- **WHEN** no logical counter can stop the stalled worker before its wall limit
- **THEN** the parent kills the worker and returns a typed watchdog outcome

### Requirement: Proven bounds reserve work before allocation
Before a material allocation or expansion, the compiler SHALL compare any exact value or proven
conservative lower bound with the remaining cumulative logical budget. If the proven minimum cannot
fit, it SHALL stop before starting the operation. A heuristic or otherwise uncertain estimate SHALL
NOT reject compilation; it MAY emit a finding, after which actual work is counted under the envelope.

#### Scenario: A product provably cannot fit
- **WHEN** exact alternative factors prove that the operation requires more work than remains
- **THEN** the compiler avoids the allocation and reports the factors, responsible constructs,
  required minimum, remaining budget, and effective envelope

#### Scenario: Growth is only estimated
- **WHEN** the compiler has a concerning estimate but no trustworthy lower bound
- **THEN** it reports the estimate and attempts the operation while charging actual work

### Requirement: Word-analysis results are atomic and independently retryable
Reaching an application budget SHALL produce a typed incomplete outcome for that word and SHALL NOT
present analyses found so far as a complete result. A batch SHALL preserve complete results for
words that finished. The caller MAY submit only incomplete words again with explicitly selected
larger apply budgets; the runtime SHALL NOT retry them automatically.

#### Scenario: One difficult word reaches its candidate budget
- **WHEN** easier words in the batch have already completed
- **THEN** their complete results remain valid, the difficult word carries an incomplete outcome and
  diagnostic counts, and no partial analyses for that word are presented as definitive

#### Scenario: The caller retries incomplete words
- **WHEN** it submits those words in a new request with larger caller-selected limits
- **THEN** each retry runs once under those effective limits and preserves the prior failure context

### Requirement: Batches have cumulative caller-selected budgets
Application SHALL support both per-word budgets and an optional cumulative batch budget. Reaching
the batch budget SHALL preserve completed word results, mark a word whose work began but did not
finish as incomplete, and mark remaining unstarted words as not attempted. Incomplete and not-
attempted SHALL be distinct typed outcomes and MAY be submitted in a later caller-requested batch.

#### Scenario: The cumulative batch budget is exhausted between words
- **WHEN** several words completed and the next word has not started
- **THEN** completed results remain valid and every remaining word is marked not attempted

#### Scenario: The cumulative batch budget is exhausted during a word
- **WHEN** analysis of that word has consumed work but has not completed
- **THEN** that word is incomplete, later words are not attempted, and earlier complete results remain valid

### Requirement: WASM is not a compilation environment
The WASM target SHALL load precompiled analysis artifacts and perform bounded application only. Its
dependency graph and exported API SHALL NOT contain FST compiler construction.

#### Scenario: A browser receives grammar source
- **WHEN** a caller supplies grammar source instead of a compatible analysis artifact
- **THEN** the WASM API rejects the input without attempting FST compilation
