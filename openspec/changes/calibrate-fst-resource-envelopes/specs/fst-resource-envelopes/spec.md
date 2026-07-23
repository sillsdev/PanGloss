## ADDED Requirements

### Requirement: Resource envelopes are evidence-calibrated
Default compile and apply envelopes SHALL be derived from reproducible supervised scale sweeps and SHALL identify platform, build profile, recipe, seed, and measured headroom.

Default logical-work limits SHALL be calibrated as the primary machine-independent early cutoffs.
Wall-time values SHALL be calibrated as outer watchdog limits and SHALL NOT be presented as a
cross-machine promise that every excessive grammar fails within the same elapsed duration.

#### Scenario: Default changes
- **WHEN** a default limit is raised or lowered
- **THEN** the change references reproducible calibration data and a versioned policy revision

### Requirement: Calibration sets defaults below fixed emergency ceilings
Calibration SHALL propose evidence-backed defaults and named envelopes without exceeding the
versioned hard-coded absolute ceilings. The ceiling values SHALL be reviewed as emergency containment
bounds and SHALL remain distinct from recommended operating envelopes.

#### Scenario: Scale evidence suggests raising a normal envelope
- **WHEN** measured headroom supports a larger default or retry envelope
- **THEN** that envelope may be revised without silently changing or exceeding the separately
  versioned absolute ceiling

### Requirement: Calibration combines real and synthetic evidence
Final policy calibration SHALL include representative real-language grammars and words, generated
one-factor scale sweeps, selected pairwise stress interactions, and long or ambiguous runtime words.
Every workload SHALL retain its applicable semantic correctness gate while resource measurements are
collected. Neither real-language evidence nor synthetic stress evidence alone is sufficient for the
final policy recommendation.

#### Scenario: Real grammars are small but a construct pair explodes
- **WHEN** real-language runs remain healthy and a generated pairwise case reaches a typed cliff
- **THEN** defaults use real-language headroom while diagnostics and ceilings account for the measured cliff

#### Scenario: A synthetic case is expensive but semantically wrong
- **WHEN** its oracle or proposer-to-confirm correctness gate fails
- **THEN** its performance numbers are not used as valid calibration evidence until correctness is fixed

### Requirement: Platform evidence is honest and does not fork runtime policy
Calibration SHALL record platform, hardware, toolchain, and build profile for every measurement.
Windows is the currently available calibration platform. Until a Linux calibration run is available,
Linux SHALL be recorded as `not_run`; its absence SHALL NOT block implementation or create a separate
runtime policy. Later credible cross-platform evidence MAY revise the one portable policy.

#### Scenario: Final calibration currently runs only on Windows
- **WHEN** the policy recommendation is produced
- **THEN** it cites the Windows environment, records Linux as `not_run`, and does not imply Linux was measured

#### Scenario: Later Linux evidence shows a tighter safe range
- **WHEN** a reproducible Linux run produces a credible worse result
- **THEN** the portable policy is reviewed conservatively rather than creating Linux-only limits

### Requirement: Calibration recommends but never mutates policy
The calibration tooling SHALL emit raw measurements, reproducible recipes, proposed values,
headroom calculations, and a diff against the current versioned policy. It SHALL NOT rewrite,
activate, or adapt production constants automatically. Changing defaults or absolute ceilings SHALL
require explicit human review and a committed policy-version change.

#### Scenario: A calibration run recommends larger defaults
- **WHEN** the run completes successfully
- **THEN** current production limits remain unchanged until a reviewer commits the proposed policy revision

#### Scenario: Calibration runs on a different machine
- **WHEN** its measurements differ from the previous evidence
- **THEN** runtime behavior does not adapt to that machine without an explicit reviewed policy change

### Requirement: Memory evidence uses process high-water measurement
Scale reports SHALL measure sampled worker RSS, record the sampling interval and observed maximum,
and SHALL NOT label either that observation or state/arc counts as an exact hard memory ceiling.

#### Scenario: Small final net has large transient allocation
- **WHEN** transient RSS spikes during minimization
- **THEN** the report captures the RSS high-water mark even if the final state count is small
