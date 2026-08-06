## Why

Quantified pattern nodes have incomplete FST dispositions. Optional and bounded regular forms need
exact witnesses; unbounded or unsafe combinations must remain honestly unsupported rather than be
silently approximated.

## What Changes

- Compile optional and explicitly bounded regular quantifiers through the shared lowering IR.
- Account for expansion/composition before allocation.
- Retain typed unsupported outcomes for unbounded or non-regular combinations.

## Impact

This changes only FST proposal coverage. The complete Rust HermitCrab engine remains the oracle and
confirmation implementation.
