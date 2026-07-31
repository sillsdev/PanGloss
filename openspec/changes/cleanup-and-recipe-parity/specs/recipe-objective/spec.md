## ADDED Requirements

### Requirement: Propose-side work is measured deterministically
The per-candidate evaluation SHALL record `raw_paths` — the count of raw paths yielded by the
proposer's `apply_up` traversal across the corpus, before tag-decode and dedup — as a
deterministic score field. The field SHALL be serialized with a backward-compatible default so
older reports still deserialize.

#### Scenario: raw_paths survives to the report
- **WHEN** a candidate is evaluated over a corpus
- **THEN** its score carries the summed `raw_paths`, equal to the sum of the per-word proposal
  diagnostics the analyzer already computes, and the value is identical across repeated runs on
  the same inputs

### Requirement: The ranking key prices both propose-side and confirm-side work
The winner-selection key SHALL be a deterministic lexicographic key whose leading term reflects
total adjudication work including propose-side traversal (`raw_paths`) and confirm-side oracle
work (`confirmation_steps`), with wall-clock excluded from ranking. The exact composition is
fixed in design.md and validated against the four-corpus oracle below before it is committed.

#### Scenario: Sena-shaped divergence selects the lower-total-work candidate
- **WHEN** two candidates certify identically and one does several-fold more propose-side work
  for a marginally lower confirm-step count (the measured Sena shape)
- **THEN** the key selects the candidate with lower combined work, and a pinned synthetic
  fixture test encodes this preference

#### Scenario: Dominant-on-all-metrics winners are unaffected
- **WHEN** one candidate is better on every deterministic metric (the measured Indonesian shape)
- **THEN** the key selects it, unchanged from the previous key

### Requirement: Four-corpus winner correctness is the acceptance oracle
Before the key change lands, the optimizer SHALL be run on the available real-corpus slices
(out-of-band, gitignored data, via the managed build entry points), and the selected winner on
each SHALL be the candidate that is empirically better-or-equal on the deterministic work
metrics. These runs are recorded as observations in the evidence doc; they are not certification
claims and never enter committed fixtures.

#### Scenario: No corpus regresses to a dominated winner
- **WHEN** the ranking-key change is evaluated on the corpus slices
- **THEN** no corpus's selected winner is strictly dominated (worse or equal on all deterministic
  work metrics, worse on at least one) by another certified candidate in the same report
