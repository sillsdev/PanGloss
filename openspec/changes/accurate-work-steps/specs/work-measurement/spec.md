## ADDED Requirements

### Requirement: Hardware counters never affect a parse

No cap, deadline or decision that can change a parse SHALL read a hardware counter. Work units SHALL
be a pure function of grammar, word, options and PanGloss version.

#### Scenario: Same word on different machines

- **WHEN** one PanGloss version parses one word with one grammar and the same options on two machines
- **THEN** the work total and the `CAP` verdict are identical

#### Scenario: A timed-out word

- **WHEN** a word stops on its wall-clock deadline
- **THEN** the result is marked `TIMEOUT`, stays incomplete and non-certifying, and is excluded from
  work comparisons

### Requirement: Instructions are the ground truth for cost decisions

A claim that a change makes parsing cheaper SHALL be decided on instructions from one named tool,
compared within that tool, with its noise floor stated.

#### Scenario: A memory-affecting change

- **WHEN** a change touches memoization, deduplication, cloning or allocation
- **THEN** wall time, peak memory and allocation count are reported separately beside instructions

### Requirement: Work is a reliable direction signal

Before `work_steps` replaces `steps`, a maintained set of parse-identical before/after pairs SHALL
pass the acceptance test in design decision D5.

#### Scenario: No-change control

- **WHEN** a pair differs by nothing that affects execution
- **THEN** `work_steps` differs by exactly zero

#### Scenario: A disagreement

- **WHEN** the signs of the changes in total work and total instructions disagree
- **THEN** the pair is listed with a cause, and an unexplained disagreement fails the test

### Requirement: Retiring steps cannot lose recall

The attempt cap SHALL NOT be removed until no word that completes today newly caps under the
recalibrated default, on conformance and on all local real grammars.

#### Scenario: A word newly caps

- **WHEN** a word completed by the old limits reaches `CAP` under the new default
- **THEN** the retirement task fails and the default is not published

### Requirement: Hosts compare work only under stated conditions

A host before/after figure SHALL use the same word list and PanGloss version, under `--work-cap`
with no deadline.

#### Scenario: Capped words in a comparison

- **WHEN** either side contains a capped word
- **THEN** the figure is shown as a lower bound, not as an exact percentage

#### Scenario: A PanGloss upgrade

- **WHEN** the PanGloss version differs from the stored baseline
- **THEN** the old grammar is re-run on the new version before any comparison
