import os
from pathlib import Path
import subprocess
import json
import xml.etree.ElementTree as ET

root = Path.cwd()
out = Path('/tmp/pangloss-lanes/variant-probes')
out.mkdir(parents=True, exist_ok=True)
oracle = '/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
env = dict(os.environ, PATH='/home/johnm/.dotnet:' + os.environ['PATH'])
base = ET.parse(root / 'conformance-staging/edge-cases/right-to-left-bounded-quantifier-rewrite/grammar.xml')

def sc(cls, polarity=None):
    extra = '' if polarity is None else f'<AlphaVariables><AlphaVariable variableFeature="varVoice" polarity="{polarity}" /></AlphaVariables>'
    return f'<SimpleContext naturalClass="{cls}">{extra}</SimpleContext>'

def q(child, lo=0, hi=2):
    return f'<OptionalSegmentSequence min="{lo}" max="{hi}">{child}</OptionalSegmentSequence>'

probes = {}
for direction in ['rtl', 'bounded', 'unbounded']:
    maximum = -1 if direction == 'unbounded' else 2
    tail = q(sc('ncCons'), hi=maximum) + sc('ncTrig')
    probes[f'{direction}-ambiguous-disagree'] = (sc('ncAny', 'plus'), sc('ncAny', 'minus'), tail)
    probes[f'{direction}-alpha-in-repeat'] = (sc('ncVowel'), '<Segment segment="ce" />', q(sc('ncAny', 'plus'), lo=2, hi=maximum) + sc('ncTrig'))
    probes[f'{direction}-empty-repeat'] = (sc('ncVowel'), '<Segment segment="ce" />', q('', hi=maximum) + sc('ncTrig'))
    probes[f'{direction}-nested-repeat-control'] = (sc('ncVowel'), '<Segment segment="ce" />', q(q(sc('ncCons'), hi=maximum), hi=maximum) + sc('ncTrig'))
    probes[f'{direction}-inverted-repeat'] = (sc('ncVowel'), '<Segment segment="ce" />', q(sc('ncCons'), lo=3, hi=2) + tail if direction == 'unbounded' else q(sc('ncCons'), lo=3, hi=2) + sc('ncTrig'))
    probes[f'{direction}-quantified-focus-control'] = (q(sc('ncVowel'), lo=1, hi=maximum), '<Segment segment="ce" />', tail)
    probes[f'{direction}-quantified-replacement-control'] = (sc('ncVowel'), q('<Segment segment="ce" />', lo=1, hi=maximum), tail)
    probes[f'{direction}-no-owning-table'] = (sc('ncVowel'), '<Segment segment="ce" />', tail)

results = {}
for name, (lhs, rhs, right) in probes.items():
    tree = ET.ElementTree(ET.fromstring(ET.tostring(base.getroot())))
    lang = tree.find('Language')
    lang.find('Name').text = name
    natural = lang.find('NaturalClasses')
    natural.append(ET.fromstring('<FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>'))
    rule = lang.find('PhonologicalRuleDefinitions/PhonologicalRule')
    rule.set('multipleApplicationOrder', 'rightToLeftIterative' if name.startswith('rtl-') else 'leftToRightIterative')
    rule.insert(1, ET.fromstring('<VariableFeatures><VariableFeature id="varVoice" name="a" phonologicalFeature="featVoice" /></VariableFeatures>'))
    rule.find('PhoneticInput').clear()
    rule.find('PhoneticInput').append(ET.fromstring(f'<PhoneticSequence>{lhs}</PhoneticSequence>'))
    sr = rule.find('PhonologicalSubrules/PhonologicalSubrule')
    sr.find('PhoneticOutput').clear()
    sr.find('PhoneticOutput').append(ET.fromstring(f'<PhoneticSequence>{rhs}</PhoneticSequence>'))
    sr.find('Environment/RightEnvironment/PhoneticTemplate').clear()
    sr.find('Environment/RightEnvironment/PhoneticTemplate').append(ET.fromstring(f'<PhoneticSequence>{right}</PhoneticSequence>'))
    if name.endswith('alpha-in-repeat'):
        sr.find('Environment/RightEnvironment/PhoneticTemplate').set('finalBoundaryCondition', 'true')
        entries = lang.find('Strata/Stratum/LexicalEntries')
        entries.append(ET.fromstring('<LexicalEntry id="eMixed"><Allomorphs><Allomorph id="aMixed"><PhoneticShape>actt</PhoneticShape></Allomorph></Allomorphs><MorphemeId>MIXED</MorphemeId><Gloss>mixed</Gloss></LexicalEntry>'))
    if name.endswith('no-owning-table'):
        lang.find('Strata/Stratum').attrib.pop('phonologicalRules')
    folder = out / name
    folder.mkdir(exist_ok=True)
    tree.write(folder / 'grammar.xml', encoding='utf-8', xml_declaration=True)
    xml_path = folder / 'grammar.xml'
    xml_path.write_text('\n'.join(line.rstrip() for line in xml_path.read_text().splitlines()) + '\n')
    words = ['acet', 'ecct', 'accct', 'acat', 'acct', 'et', 'at', 'ecat', 'etct', 'ctat', 'ctet', 'accccct', 'eccccct', 'eccct', 'ecet', 'actt', 'ectt']
    (folder / 'words.txt').write_text('\n'.join(words) + '\n')
    (folder / 'oracle.tsv').unlink(missing_ok=True)
    proc = subprocess.run(['bash', str(root / 'machine/conformance/adapters/hc-dotnet-wrapper.sh'), 'batch', str(folder / 'grammar.xml'), str(folder / 'words.txt'), str(folder / 'oracle.tsv'), oracle], env=env, capture_output=True, timeout=30)
    (folder / 'oracle.log').write_bytes(proc.stderr)
    rows = []
    if (folder / 'oracle.tsv').exists():
        rows = [line for line in (folder / 'oracle.tsv').read_text().splitlines() if len(line.split('\t')) == 5]
    results[name] = dict(exit=proc.returncode, rows=rows, stderr=proc.stderr.decode('utf-16-le', errors='replace'))
    print(name, proc.returncode, [(r.split('\t')[1], r.split('\t')[4]) for r in rows], flush=True)
(out / 'oracle-results.json').write_text(json.dumps(results, indent=2))
