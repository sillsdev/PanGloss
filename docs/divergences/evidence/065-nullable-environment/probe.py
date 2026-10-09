from pathlib import Path
import xml.etree.ElementTree as ET
import itertools
import json
import subprocess
import os

root = Path.cwd()
scratch = Path('/tmp/pangloss-lanes/disagree-065')
scratch.mkdir(exist_ok=True)
seed = root / 'docs/divergences/evidence/058-variant-lowering/ambiguous-disagree-parity-blocker/grammar.xml'
env = dict(os.environ, PATH='/home/johnm/.dotnet:' + os.environ['PATH'])
oracle = '/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
results = {}
shapes = [('plain', 'ltr', 'right')] + list(itertools.product(['bounded', 'unbounded'], ['ltr', 'rtl'], ['left', 'right']))
for bound, direction, side in shapes:
    name = f'nullable-disagree-{bound}-{direction}-{side}'
    folder = root / 'conformance-staging/edge-cases' / name
    folder.mkdir(exist_ok=True)
    tree = ET.parse(seed)
    tree.find('Language/Name').text = name
    rule = tree.find('Language/PhonologicalRuleDefinitions/PhonologicalRule')
    rule.set('multipleApplicationOrder', 'rightToLeftIterative' if direction == 'rtl' else 'leftToRightIterative')
    sub = rule.find('PhonologicalSubrules/PhonologicalSubrule')
    if bound == 'plain':
        sub.remove(sub.find('Environment'))
    else:
        quant = sub.find('.//OptionalSegmentSequence')
        quant.set('max', '-1' if bound == 'unbounded' else '1')
        if side == 'left':
            right = sub.find('Environment/RightEnvironment')
            right.tag = 'LeftEnvironment'
            right.find('PhoneticTemplate').attrib = {'initialBoundaryCondition': 'true'}
    ET.indent(tree, space='  ')
    tree.write(folder / 'grammar.xml', encoding='utf-8', xml_declaration=True)
    words = [''.join(w) for w in itertools.product('iyau', repeat=2)]
    txt = scratch / f'{name}.words.txt'
    txt.write_text('\n'.join(words) + '\n')
    tsv = scratch / f'{name}.oracle.tsv'
    proc = subprocess.run(['bash', str(root / 'machine/conformance/adapters/hc-dotnet-wrapper.sh'), 'batch', str(folder / 'grammar.xml'), str(txt), str(tsv), oracle], env=env, capture_output=True, timeout=30)
    if proc.returncode:
        raise RuntimeError((name, proc.returncode, proc.stderr))
    rows = [r.split('\t') for r in tsv.read_text().splitlines() if len(r.split('\t')) == 5]
    assert len(rows) == len(words) and all(r[3] == 'ok' for r in rows)
    lines = ['# oracle-provenance: founding-oracle', '# hc.dll, Machine 18cf242f4b114b0eb9bac304b4b171ca2f499a39.', f'language: {name}', 'inspired_by: ["synthetic feature disagreement at a word edge"]', 'sources: ["HCLoader.cs:2338-2344,2745-2770"]', 'requires: [phonology]', 'fieldworks_producible: true', 'words:']
    for row in rows:
        lines.append(f'  - word: {row[1]}')
        lines.extend(['    expect_fail: true'] if row[4] == '-' else ['    parses:', f'      - signature: "{row[4]}"'])
        if bound != 'plain':
            lines.extend(['    exercises:', f'      - "quantifier.{bound}-unlowerable"'])
            if direction == 'rtl':
                lines.append('      - "right-to-left-rewrite.unlowerable"')
    (folder / 'words.yaml').write_text('\n'.join(lines) + '\n')
    (folder / 'STAGING.md').write_text(f'# {name}\n\nSynthetic back/round disagreement over a four-member class. The unchanged\nfeature at each position distinguishes the required surface ia from other\ntwo-segment words; a nullable environment at the edge must permit the rewrite.\nOracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`; 16 complete rows.\nDivergence: docs/divergences/065-nullable-rewrite-environment.md.\nFieldWorks: HCLoader.cs:2338-2344,2745-2770.\nUpstream PR: none (Rust-only defect, network closed).\n')
    results[name] = rows
    print(name, [(r[1], r[4]) for r in rows if r[4] != '-'], flush=True)
(scratch / 'oracle-results.json').write_text(json.dumps(results, indent=2))
