from pathlib import Path
import re, subprocess, json, sys
ROOT=Path.cwd()
E=ROOT/'docs/divergences/evidence/069-overridden-alpha'
DLL='/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
records=[]
for folder in sorted((E/'matrix').iterdir()):
    if not (folder/'oracle.tsv').exists() or not (folder/'rust.tsv').exists():
        continue
    words=(folder/'words.txt').read_text().splitlines()
    script=folder/'trace-script.txt';script.write_text('tracing on\n'+'\n'.join('parse '+w for w in words)+'\n')
    p=subprocess.run(['/home/johnm/.dotnet/dotnet',DLL,'-i',str(folder/'grammar.xml'),'-s',str(script),'-o',str(folder/'oracle-trace.txt')],capture_output=True,timeout=30)
    assert p.returncode==0,folder
    trace=(folder/'oracle-trace.txt').read_text()
    pairs=[]
    for text in re.findall(r'Phonological Rule Synthesis \[([^\]]+)\]', trace):
        before=re.search(r'Input: ([iyau]+)',text)
        after=re.search(r'Output: ([iyau]+)',text)
        assert before and (after or 'Reason: Pattern' in text),text
        pairs.append((before[1],after[1] if after else before[1]))
    pairs=sorted(set(pairs))
    roots={a for a,b in pairs}
    assert len(roots)==16,(folder.name, len(roots),pairs)
    rows=[r.split('\t') for r in (folder/'oracle.tsv').read_text().splitlines() if len(r.split('\t'))==5]
    expected={'language':folder.name,'requires':['phonology'],'fieldworks_producible':True,'words':[{'word':r[1],'expect_fail':r[4]=='-','parses':[{'signature':s} for s in ([] if r[4]=='-' else r[4].split(';'))]} for r in rows]}
    trace_lines=sorted(set(re.findall(r'Phonological Rule Synthesis \[[^\]]+\]',trace)))
    records.append({'name':folder.name,'xml':(folder/'grammar.xml').read_text(),'oracle_tsv':(folder/'oracle.tsv').read_text(),'rust_tsv':(folder/'rust.tsv').read_text(),'synthesis':pairs,'expected':expected,'oracle_trace_lines':trace_lines})
    print(folder.name,'C# forward roots',len(roots),flush=True)
    (E/'sweep-records.json').write_text(json.dumps(records,indent=2)+'\n')
