## Why

Multiple CharacterDefinitionTables are currently detected by a gate that demonstrates wrong-root rewriting. Detection prevents a silent claim but does not provide construct support.

## What Changes

- Thread the correct table identity through rewrite rendering and alpha-tuple resolution.
- Add positive and negative multi-table witnesses.
- Add the high-risk `table × alpha × strata` interaction recipe.

## Impact

This closes one known semantic correctness gap. It is intentionally separate from directional and simultaneous rewrite compilation.
