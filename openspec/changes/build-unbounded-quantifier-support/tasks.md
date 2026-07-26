## 1. Lowering

- [x] 1.1 Widen the repetition slot's upper bound to an optional value; accept an absent bound in the
      pattern-node walk instead of bailing
- [x] 1.2 Skip the inverted-bound and finite-ceiling checks entirely when no upper bound is present —
      not merely leave them untripped
- [x] 1.3 Render an absent upper bound through the backend's native unbounded operators, with the
      minimum-occurrence off-by-one pinned at the compiled-automaton level rather than by string
      comparison
- [x] 1.4 Audit every reader of the upper bound for an absent-bound path; confirm the candidate
      enumerator still refuses repetition slots (it enumerates concrete alternatives, so a repetition
      is genuinely outside its remit)

## 2. Capability

- [x] 2.1 Widen the quantifier predicate so an absent upper bound is no longer disqualifying
- [x] 2.2 Correct the stale doc claiming the shape check excludes quantifiers, at every site

## 3. Right-to-left mirror

- [x] 3.1 REPRODUCE the shallow-reversal defect on a repetition group with non-palindromic contents
      before changing anything
- [x] 3.2 Recurse into a repetition group's contents when building the mirror; pin the bounded and
      unbounded variants as regression tests, since the two changes interact

## 4. Evidence

- [x] 4.1 Conformance fixture for the unbounded case, ground truth derived by running the engine
- [x] 4.2 Update the coverage-ledger containment citation and note so they reflect that unbounded is
      now covered, and regenerate the golden from the test's own computation

## 5. Verification

- [x] 5.1 Full workspace green in debug; `pg-foma` green in debug and release
- [x] 5.2 All five coverage gates green (conformance cross-check, plan-interaction, citation liveness,
      tag liveness, structural witness)
- [x] 5.3 Conformance suite at its established baseline; `p6_gate_parity` including ignored
- [ ] 5.4 `f3_parity --include-ignored` fully observed — the Indonesian and Sena legs passed; the
      Amharic leg (~20 min) was started but not observed to completion. Re-run:
      `cargo test -p pg-foma --release --test f3_parity -- --include-ignored`
