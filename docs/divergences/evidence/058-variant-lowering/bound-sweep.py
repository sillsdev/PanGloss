from pathlib import Path
import xml.etree.ElementTree as ET
import subprocess
import os
import json

root = Path.cwd()
out = Path('/tmp/pangloss-lanes/extra-alpha')
out.mkdir(exist_ok=True)
oracle = '/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
env = dict(os.environ, PATH='/home/johnm/.dotnet:' + os.environ['PATH'])
results = {}
for bound in ['bounded', 'unbounded']:
    for side in ['left', 'right']:
        for minimum in [0, 1]:
            for bare in [False, True]:
                name = f'{bound}-{side}-min{minimum}-bare{bare}'
                folder = out / name
                folder.mkdir(exist_ok=True)
                tree = ET.parse(root / f'conformance-staging/edge-cases/quantified-alpha-{bound}-ltr-{side}/grammar.xml')
                rule = tree.find('Language/PhonologicalRuleDefinitions/PhonologicalRule')
                quant = rule.find('.//OptionalSegmentSequence')
                quant.set('min', str(minimum))
                if bare:
                    seq = rule.find(f'.//{side.title()}Environment/PhoneticTemplate/PhoneticSequence')
                    for child in list(seq):
                        if child.tag == 'SimpleContext':
                            seq.remove(child)
                lex = tree.find('Language/Strata/Stratum/LexicalEntries')
                for shape, mid in [('a', 'ZERO'), ('ac', 'ONE'), ('act', 'MIXEDONE'), ('at', 'TRIG')]:
                    entry = ET.SubElement(lex, 'LexicalEntry', {'id': 'extra' + mid})
                    allos = ET.SubElement(entry, 'Allomorphs')
                    allo = ET.SubElement(allos, 'Allomorph', {'id': 'allo' + mid})
                    ET.SubElement(allo, 'PhoneticShape').text = shape if side == 'right' else shape[::-1]
                    ET.SubElement(entry, 'MorphemeId').text = mid
                    ET.SubElement(entry, 'Gloss').text = mid
                ET.indent(tree, space='  ')
                tree.write(folder / 'grammar.xml', encoding='utf-8', xml_declaration=True)
                words = ['a', 'e', 'ac', 'ec', 'act', 'ect', 'at', 'et', 'actt', 'ectt', 'acct', 'ecct', 'accct', 'eccct', 'acat', 'ecat']
                if side == 'left':
                    words = [w[::-1] for w in words]
                (folder / 'words.txt').write_text('\n'.join(words) + '\n')
                proc = subprocess.run(['bash', str(root / 'machine/conformance/adapters/hc-dotnet-wrapper.sh'), 'batch', str(folder / 'grammar.xml'), str(folder / 'words.txt'), str(folder / 'oracle.tsv'), oracle], env=env, capture_output=True, timeout=30)
                if proc.returncode:
                    raise RuntimeError((name, proc.returncode, proc.stderr))
                rows = [r.split('\t') for r in (folder / 'oracle.tsv').read_text().splitlines() if len(r.split('\t')) == 5]
                assert len(rows) == len(words) and all(r[3] == 'ok' for r in rows)
                results[name] = rows
                print(name, [(r[1], r[4]) for r in rows if r[4] != '-'], flush=True)
(out / 'oracle-results.json').write_text(json.dumps(results, indent=2))
