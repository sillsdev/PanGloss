from pathlib import Path
import re
import itertools
import xml.etree.ElementTree as ET
import subprocess
import os

root = Path.cwd()
text = (root / 'rust/crates/pg-foma/src/replace/owning_table_tests.rs').read_text()
xml = re.search(r'const TWO_VAR_AMBIGUOUS_DISAGREE_XML: &str = r#"(.*?)"#;', text, re.S).group(1)
env = dict(os.environ, PATH='/home/johnm/.dotnet:' + os.environ['PATH'])
oracle = '/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
for label, minimum, maximum, direction in [('plain', None, None, 'leftToRightIterative'), ('bounded', '0', '1', 'leftToRightIterative'), ('unbounded', '0', '-1', 'leftToRightIterative'), ('rtl', '0', '1', 'rightToLeftIterative')]:
    folder = Path('/tmp/pangloss-lanes/ambiguous-witness') / label
    folder.mkdir(parents=True, exist_ok=True)
    tree = ET.ElementTree(ET.fromstring(xml))
    rule = tree.find('Language/PhonologicalRuleDefinitions/PhonologicalRule')
    rule.set('multipleApplicationOrder', direction)
    if minimum is not None:
        sub = rule.find('PhonologicalSubrules/PhonologicalSubrule')
        envnode = ET.SubElement(sub, 'Environment')
        right = ET.SubElement(envnode, 'RightEnvironment')
        template = ET.SubElement(right, 'PhoneticTemplate', {'finalBoundaryCondition': 'true'})
        seq = ET.SubElement(template, 'PhoneticSequence')
        repeat = ET.SubElement(seq, 'OptionalSegmentSequence', {'min': minimum, 'max': maximum})
        ET.SubElement(repeat, 'SimpleContext', {'naturalClass': 'ncVowel'})
    ET.indent(tree, space='  ')
    tree.write(folder / 'grammar.xml', encoding='utf-8', xml_declaration=True)
    words = [''.join(w) for w in itertools.product('iyau', repeat=2)]
    (folder / 'words.txt').write_text('\n'.join(words) + '\n')
    proc = subprocess.run(['bash', str(root / 'machine/conformance/adapters/hc-dotnet-wrapper.sh'), 'batch', str(folder / 'grammar.xml'), str(folder / 'words.txt'), str(folder / 'oracle.tsv'), oracle], env=env, capture_output=True, timeout=30)
    print(label, proc.returncode, proc.stdout.decode(), proc.stderr.decode(), flush=True)
    if proc.returncode:
        raise RuntimeError('oracle failed')
    print((folder / 'oracle.tsv').read_text(), flush=True)
