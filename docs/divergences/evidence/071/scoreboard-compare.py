from collections import Counter
from pathlib import Path
root = Path('/tmp/pangloss-lanes')
evidence = Path('docs/divergences/evidence/071')
records = {}
for version in ['before', 'after']:
    log = root / f'strrep-scoreboard-cells-{version}.log'
    rows = [line.split('\t')[1:] for line in log.read_text().splitlines() if line.startswith('SCOREBOARD_CELL\t')]
    assert all(len(row) == 3 for row in rows), rows
    cells = {(fixture, strategy): state for fixture, strategy, state in rows}
    assert len(cells) == len(rows), 'duplicate cell'
    records[version] = cells
    (evidence / f'scoreboard-{version}.tsv').write_text('fixture\tstrategy\tstate\n' + ''.join('\t'.join((*key, cells[key])) + '\n' for key in sorted(cells)))
before, after = records['before'], records['after']
changed = [(key, state, after.get(key)) for key, state in before.items() if after.get(key) != state]
added = {key: after[key] for key in after.keys() - before.keys()}
assert len(before) == 219 and len(after) == 222, (len(before), len(after))
assert not changed, changed
fixture = 'staging:edge-cases/strrep-rewrite-unapplication'
assert added == {(fixture, 'PlanComposed'): 'refused', (fixture, 'TunedSurfaceProbed'): 'oracle_exact', (fixture, 'TemplatedUnderlyingTokens'): 'oracle_exact'}, added
lines = ['Original: 73 scored fixtures, 219 cells.', 'Proposed: 74 scored fixtures, 222 cells.', 'Existing cells checked: 219.', 'Existing cells changed state: 0.', 'Existing cells removed: 0.', 'Added cells: 3.']
lines += ['Added: ' + '\t'.join((*key, added[key])) for key in sorted(added)]
for strategy in ['TunedSurfaceProbed', 'TemplatedUnderlyingTokens', 'PlanComposed']:
    for version, cells in records.items():
        buckets = Counter(state for (_, s), state in cells.items() if s == strategy)
        lines.append(f'{strategy} {version}: ' + ', '.join(f'{kind}={buckets[kind]}' for kind in ['oracle_exact', 'compiles_but_misses', 'refused', 'unmeasurable']))
lines += ['Original focused gate exit: 0.', 'Proposed focused gate exit with original ratchet retained: 101 (approved inventory addition).', 'Both temporary diagnostic checks exit: 0.', 'Measurement emits every owner-measured cell; --nocapture preserves passing-test output.', 'Original measurement restores aa59f3d0 bridge.rs/rewrite.rs and removes only the new fixture.', 'Proposed measurement uses the port and the new fixture. All sources and the fixture restored in finally.', 'No expectation changed before this comparison.']
text = '\n'.join(lines) + '\n'
(evidence / 'scoreboard-comparison.txt').write_text(text)
print(text)