from pathlib import Path
import xml.etree.ElementTree as ET
import subprocess, os, json

root=Path.cwd()
evidence=root/'docs/divergences/evidence/067-ambiguous-disagreement'
evidence.mkdir(exist_ok=True)
env=dict(os.environ,PATH='/home/johnm/.dotnet:'+os.environ['PATH'])
oracle='/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
results={}
for bound,side in [('bounded','right'),('unbounded','left')]:
    source=root/f'conformance-staging/edge-cases/nullable-disagree-{bound}-rtl-{side}'
    name=f'partial-class-disagree-{bound}-rtl-{side}'
    tree=ET.parse(source/'grammar.xml')
    tree.find('Language/Name').text=name
    cls=tree.find('Language/NaturalClasses/SegmentNaturalClass')
    cls.remove(cls.find("Segment[@segment='cU']"))
    folder=root/'conformance-staging/edge-cases'/name
    folder.mkdir(exist_ok=True)
    ET.indent(tree,space='  ')
    tree.write(folder/'grammar.xml',encoding='utf-8',xml_declaration=True)
    ET.parse(folder/'grammar.xml')
    words=[a+b for a in 'iyau' for b in 'iyau']
    txt=evidence/f'{name}.words.txt'
    txt.write_text('\n'.join(words)+'\n')
    tsv=evidence/f'{name}.oracle.tsv'
    proc=subprocess.run(['bash',str(root/'machine/conformance/adapters/hc-dotnet-wrapper.sh'),'batch',str(folder/'grammar.xml'),str(txt),str(tsv),oracle],env=env,capture_output=True,text=True,timeout=30)
    (evidence/f'{name}.oracle.log').write_text(proc.stdout+proc.stderr)
    if proc.returncode: raise SystemExit(f'oracle failed: {name}: {proc.returncode}: {proc.stderr}')
    rows=[r.split('\t') for r in tsv.read_text().splitlines() if len(r.split('\t'))==5]
    assert len(rows)==16 and all(r[3]=='ok' for r in rows)
    assert [(r[1],r[4]) for r in rows if r[4]!='-']==[('ia','AU|ia')]
    lines=['# oracle-provenance: founding-oracle','# hc.dll, Machine 18cf242f4b114b0eb9bac304b4b171ca2f499a39.',f'language: {name}','inspired_by: ["synthetic feature disagreement with a partial class"]','sources: ["HCLoader.cs:2338-2344,2745-2770"]','requires: [phonology]','fieldworks_producible: true','words:']
    for row in rows:
        lines.append(f'  - word: {row[1]}')
        lines.extend(['    expect_fail: true'] if row[4]=='-' else ['    parses:',f'      - signature: "{row[4]}"'])
        lines.extend(['    exercises:',f'      - "quantifier.{bound}-unlowerable"','      - "right-to-left-rewrite.unlowerable"'])
    (folder/'words.yaml').write_text('\n'.join(lines)+'\n')
    (folder/'STAGING.md').write_text(f'# {name}\n\nSynthetic partial vowel class with ambiguous minus alpha variables. C# still requires\nrewritten ia and rejects au despite the omitted u in the explicit class.\nOracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`; sixteen complete rows.\nThis remains a documented FST refusal pending lowering proof, not a permanent carve-out.\nEntry: docs/divergences/067-ambiguous-disagreement-proposals.md.\nFieldWorks: HCLoader.cs:2338-2344,2745-2770.\nUpstream PR: none (network closed).\n')
    results[name]=rows
    print(name,'oracle exit 0;16 complete rows;AU|ia',flush=True)
(evidence/'partial-class-oracle-results.json').write_text(json.dumps(results,indent=2)+'\n')
