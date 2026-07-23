## Why

Simultaneous rewrite rules are honestly skipped and account for another major Aweti coverage gap. Iterative compilation is not semantically equivalent.

## What Changes

- Specify and compile simultaneous match selection and replacement semantics.
- Cover overlapping/non-overlapping matches, deletion, epenthesis, feature changes, and subrule priority.
- Re-run Aweti at analysis level without weakening resource budgets.

## Impact

This is a distinct semantic compiler change and must not be combined with RTL implementation in one worktree.
