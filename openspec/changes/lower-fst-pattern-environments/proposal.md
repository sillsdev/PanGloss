## Why

Quantifier and metathesis compilation currently risk duplicating pattern/environment lowering and
then disagreeing about anchors, polarity, groups, alternation, and character-table identity.

## What Changes

- Introduce one internal lowering seam from frozen grammar patterns/environments to FST compiler IR.
- Preserve anchors, polarity, grouping, alternation, table ownership, and unsupported detection.
- Migrate existing replacement callers without changing accepted analyses.

## Impact

This is compiler plumbing, not new HermitCrab behavior. It is the serialized predecessor of the
quantifier and metathesis changes and provides their common resource-accounting boundary.
