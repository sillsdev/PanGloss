from pathlib import Path
import xml.etree.ElementTree as ET
import subprocess, os, copy

root=Path.cwd()
name='ambiguous-disagree-repeated-minus-partial-focus'
folder=root/'conformance-staging/edge-cases'/name
folder.mkdir(exist_ok=True)
tree=ET.parse(root/'conformance-staging/edge-cases/nullable-disagree-bounded-ltr-left/grammar.xml')
tree.find('Language/Name').text=name
classes=tree.find('Language/NaturalClasses')
cls=copy.deepcopy(classes.find('SegmentNaturalClass'))
cls.set('id','ncPartial')
cls.find('Name').text='partial vowels'
cls.remove(cls.find("Segment[@segment='cU']"))
classes.append(cls)
rule=tree.find('Language/PhonologicalRuleDefinitions/PhonologicalRule')
seq=rule.find('PhoneticInput/PhoneticSequence')
seq.remove(seq.findall('SimpleContext')[1])
seq.find('SimpleContext').set('naturalClass','ncPartial')
out=rule.find('PhonologicalSubrules/PhonologicalSubrule/PhoneticOutput')
out.clear()
ET.SubElement(ET.SubElement(out,'PhoneticSequence'),'Segment',segment='cI')
q=rule.find('.//OptionalSegmentSequence')
q.set('min','1');q.set('max','1')
sc=q.find('SimpleContext')
variables=ET.SubElement(sc,'AlphaVariables')
ET.SubElement(variables,'AlphaVariable',variableFeature='varBack',polarity='minus')
entry=tree.find('Language/Strata/Stratum/LexicalEntries/LexicalEntry')
entry.find('Allomorphs/Allomorph/PhoneticShape').text='iu'
entry.find('MorphemeId').text='IU'
entry.find('Gloss').text='iu'
ET.indent(tree,space='  ')
tree.write(folder/'grammar.xml',encoding='utf-8',xml_declaration=True)
evidence=root/'docs/divergences/evidence/067-ambiguous-disagreement'
txt=evidence/f'{name}.words.txt'
txt.write_text('\n'.join(a+b for a in 'iyau' for b in 'iyau')+'\n')
tsv=evidence/f'{name}.oracle.tsv'
oracle='/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
env=dict(os.environ,PATH='/home/johnm/.dotnet:'+os.environ['PATH'])
proc=subprocess.run(['bash',str(root/'machine/conformance/adapters/hc-dotnet-wrapper.sh'),'batch',str(folder/'grammar.xml'),str(txt),str(tsv),oracle],env=env,capture_output=True,text=True,timeout=30)
(evidence/f'{name}.oracle.log').write_text(proc.stdout+proc.stderr)
if proc.returncode: raise SystemExit(f'oracle failed: {proc.returncode}: {proc.stderr}')
rows=[r.split('\t') for r in tsv.read_text().splitlines() if len(r.split('\t'))==5]
assert len(rows)==16 and all(r[3]=='ok' for r in rows)
lines=['# oracle-provenance: founding-oracle','# hc.dll, Machine 18cf242f4b114b0eb9bac304b4b171ca2f499a39.',f'language: {name}','inspired_by: ["synthetic repeated minus alpha with partial focus"]','sources: ["HCLoader.cs:2338-2344,2745-2770,2799-2808"]','requires: [phonology]','fieldworks_producible: true','words:']
for row in rows:
 lines.append(f'  - word: {row[1]}')
 lines.extend(['    expect_fail: true'] if row[4]=='-' else ['    parses:',f'      - signature: "{row[4]}"'])
 lines.extend(['    exercises:', '      - "quantifier.bounded-compilable"'])
(folder/'words.yaml').write_text('\n'.join(lines)+'\n')
(folder/'STAGING.md').write_text(f'# {name}\n\nSynthetic repeated minus alpha on a complete class and an agreeing focus on\na partial class. hc.dll requires ii from iu.\nOracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`; 16 complete rows.\nEntry: docs/divergences/067-ambiguous-disagreement-proposals.md.\nFieldWorks: HCLoader.cs:2338-2344,2745-2770,2799-2808.\nUpstream PR: none (network closed).\n')
print('oracle exit 0;16 complete rows;',[(r[1],r[4]) for r in rows if r[4]!='-'])
