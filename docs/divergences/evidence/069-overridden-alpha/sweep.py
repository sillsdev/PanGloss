from pathlib import Path
import copy, itertools, json, os, subprocess, sys, xml.etree.ElementTree as ET

ROOT = Path.cwd()
EVIDENCE = ROOT / 'docs/divergences/evidence/069-overridden-alpha'
ORACLE = '/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
WORDS = [''.join(x) for x in itertools.product('iyau', repeat=2)]
SEED = EVIDENCE / 'plain-full/grammar.xml'

def grammar(shape, overwrite, focus, envsign, side, mode, allroots):
    tree = ET.parse(SEED)
    rule = tree.find('.//PhonologicalRule')
    rule.set('multipleApplicationOrder', {'ltr':'leftToRightIterative', 'rtl':'rightToLeftIterative', 'sim':'simultaneous'}[mode])
    rule.find('PhoneticInput/PhoneticSequence/SimpleContext/AlphaVariables/AlphaVariable').set('polarity', focus)
    env = rule.find('.//Environment')
    original = env.find('LeftEnvironment')
    original.tag = side.title() + 'Environment'
    template = original.find('PhoneticTemplate')
    template.attrib.clear()
    template.set('initialBoundaryCondition' if side == 'left' else 'finalBoundaryCondition', 'true')
    seq = template.find('PhoneticSequence')
    seq.find('SimpleContext/AlphaVariables/AlphaVariable').set('polarity', envsign)
    if shape != 'plain':
        sc = seq.find('SimpleContext'); seq.remove(sc)
        q = ET.SubElement(seq, 'OptionalSegmentSequence', min='1', max='2' if shape == 'bounded' else '-1')
        q.append(sc)
    if not overwrite:
        classes = tree.find('.//NaturalClasses')
        nc = ET.SubElement(classes, 'FeatureNaturalClass', id='ncUnrounded')
        ET.SubElement(nc,'Name').text = 'unrounded'
        ET.SubElement(nc,'FeatureValue',feature='featRound',symbolValues='rdMinus')
        output = rule.find('.//PhoneticOutput/PhoneticSequence')
        output.clear(); ET.SubElement(output,'SimpleContext',naturalClass='ncUnrounded')
    entries = tree.find('.//LexicalEntries')
    entry = copy.deepcopy(entries.find('LexicalEntry'))
    if allroots:
        entries.clear()
        for word in WORDS:
            e=copy.deepcopy(entry); e.set('id','e'+word)
            e.find('.//Allomorph').set('id','a'+word)
            e.find('.//PhoneticShape').text=word
            e.find('MorphemeId').text=word.upper(); e.find('Gloss').text=word
            entries.append(e)
    else:
        word='iu' if side=='left' else 'ui'
        entries.find('.//PhoneticShape').text=word
        entries.find('.//MorphemeId').text=word.upper()
    ET.indent(tree,space='  ')
    return tree

def oracle(folder):
    words=folder/'words.txt'; words.write_text('\n'.join(WORDS)+'\n')
    proc=subprocess.run(['bash',str(ROOT/'machine/conformance/adapters/hc-dotnet-wrapper.sh'),'batch',str(folder/'grammar.xml'),str(words),str(folder/'oracle.tsv'),ORACLE],env=dict(os.environ,PATH='/home/johnm/.dotnet:'+os.environ['PATH']),capture_output=True,text=True,timeout=30)
    (folder/'oracle.log').write_text(proc.stdout+proc.stderr)
    assert proc.returncode==0, (folder,proc.stderr)
    rows=[x.split('\t') for x in (folder/'oracle.tsv').read_text().splitlines() if len(x.split('\t'))==5]
    assert len(rows)==16 and all(r[3]=='ok' for r in rows)
    return rows

def signatures(value):
    return sorted([] if value=='-' else value.split(';'))

def rust(folder):
    env=dict(os.environ,PANGLOSS_EXTRA_ARGS=f'batch {folder}/grammar.xml {folder}/words.txt {folder}/rust.tsv --threads 1 --word-timeout-ms 5000')
    with (folder/'managed-run.log').open('w') as log:
        p=subprocess.run(['pwsh','-NoProfile','-File','rust/tools/pg.ps1','-Mode','run','-Exe','rust/target/release/pangloss','-RunCaptureStdout',str(folder/'rust.stdout')],env=env,stdout=log,stderr=subprocess.STDOUT)
    assert p.returncode==0,folder
    rows=[x.split('\t') for x in (folder/'rust.tsv').read_text().splitlines() if len(x.split('\t'))==5]
    assert len(rows)==16 and all(r[3]=='ok' for r in rows),folder
    return rows

def stage():
    for shape,mode in [('plain','ltr'),('bounded','ltr'),('unbounded','ltr'),('bounded','rtl')]:
        name=f'overridden-alpha-{shape}-{mode}-left'
        folder=EVIDENCE/name; folder.mkdir(parents=True,exist_ok=True)
        tree=grammar(shape,True,'plus','minus','left',mode,False)
        tree.find('Language/Name').text=name
        tree.write(folder/'grammar.xml',encoding='utf-8',xml_declaration=True)
        rows=oracle(folder)
        staged=ROOT/'conformance-staging/edge-cases'/name; staged.mkdir(exist_ok=True)
        (staged/'grammar.xml').write_bytes((folder/'grammar.xml').read_bytes())
        lines=['# oracle-provenance: founding-oracle','# hc.dll, Machine 18cf242f4b114b0eb9bac304b4b171ca2f499a39.',f'language: {name}','inspired_by: ["synthetic vowel neutralization with alpha environment"]','sources: ["HCLoader.cs:2033-2067,2338-2344,2745-2770,2799-2808"]','requires: [phonology]','fieldworks_producible: true','words:']
        for row in rows:
            lines.append(f'  - word: {row[1]}')
            lines.extend(['    expect_fail: true'] if row[4]=='-' else ['    parses:',f'      - signature: "{row[4]}"'])
            if shape!='plain':
                lines.extend(['    exercises:',f'      - "quantifier.{shape}-compilable"'])
                if mode=='rtl': lines.append('      - "right-to-left-rewrite.reversal"')
        (staged/'words.yaml').write_text('\n'.join(lines)+'\n')
        (staged/'STAGING.md').write_text(f'# {name}\n\nSynthetic neutralization of an alpha-governed feature. hc.dll requires ii from iu.\nFounding oracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.\nRecorded XML and all 16 oracle rows: docs/divergences/evidence/069-overridden-alpha/{name}/.\nOwner regression: docs/divergences/069-analysis-binds-overridden-alpha.md.\nFieldWorks: HCLoader.cs:2033-2067,2338-2344,2745-2770,2799-2808.\nUpstream PR: none (network closed).\n')
        print('staged',name,[(r[1],r[4]) for r in rows if r[4]!='-'],flush=True)

def sweep():
    results=[]
    for shape,overwrite,focus,envsign,side,mode in itertools.product(['plain','bounded','unbounded'],[True,False],['plus','minus'],['plus','minus'],['left','right'],['ltr','rtl','sim']):
        name=f'{shape}-{"overwrite" if overwrite else "retain"}-{focus}-{envsign}-{side}-{mode}'
        folder=EVIDENCE/'matrix'/name;folder.mkdir(parents=True,exist_ok=True)
        tree=grammar(shape,overwrite,focus,envsign,side,mode,True)
        tree.find('Language/Name').text=name
        tree.write(folder/'grammar.xml',encoding='utf-8',xml_declaration=True)
        o=oracle(folder);r=rust(folder)
        diffs=[{'word':a[1],'oracle':a[4],'rust':b[4]} for a,b in zip(o,r) if a[1]!=b[1] or signatures(a[4])!=signatures(b[4])]
        item={'name':name,'rows':16,'differences':diffs};results.append(item)
        (EVIDENCE/'matrix-summary.json').write_text(json.dumps(results,indent=2)+'\n')
        print(name,'rows16', 'DIFF '+str(diffs) if diffs else 'agree',flush=True)
        if diffs:
            raise SystemExit('STOP: inspect parity disagreement before continuing')

if __name__=='__main__':
    stage() if sys.argv[1]=='stage' else sweep()
