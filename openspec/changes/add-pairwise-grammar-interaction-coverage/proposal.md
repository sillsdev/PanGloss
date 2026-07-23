## Why

Single-construct fixtures do not cover emergent semantic interactions, while unconstrained fuzzing repeatedly rediscovers known cliffs and provides weak coverage accounting.

## What Changes

- Generate pairwise covering arrays over supported semantic knobs.
- Emit a manifest of covered variant pairs, witnesses, skips, truncations, and seed metadata.
- Run seeded composite fuzzing only after pairwise coverage, minimizing failures into named recipes.

## Impact

This adds interaction evidence without changing production parsing. It begins only after the relevant construct rows have stable dispositions.
