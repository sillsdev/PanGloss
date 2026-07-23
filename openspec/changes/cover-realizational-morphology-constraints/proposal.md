## Why

Realizational features, stem names/families, blocking, maximum application, and co-occurrence rules
mostly constrain confirmation or proposal admission rather than spelling. Their coverage needs to be
classified and tested without duplicating the complete HermitCrab engine inside the FST.

## What Changes

- Inventory each constraint variant and its actual architectural boundary.
- Add safe FST admission filters only where overapproximation remains recall-preserving.
- Prove confirm-only and overapproximated paths end to end with negative witnesses.

## Impact

This is the final Stage-2 coverage classification lane. It changes proposer filtering only where
proven safe and does not add HermitCrab model features.
