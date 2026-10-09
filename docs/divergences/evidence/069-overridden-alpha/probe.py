from pathlib import Path
import xml.etree.ElementTree as ET
import subprocess, os, copy, json

root=Path.cwd()
scratch=Path('/tmp/pangloss-lanes/third-parity')
scratch.mkdir(exist_ok=True)
seed=root/'conformance-staging/edge-cases/ambiguous-disagree-repeated-minus-partial-focus/grammar.xml'
oracle='/home/johnm/work/machine.worktrees/w529-conf-base/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll'
results={}
for name,plain,full in [('plain-partial',True,False),('plain-full',True,True),('repeated-full',False,True)]:
 folder=scratch/name;folder.mkdir(exist_ok=True)
 tree=ET.parse(seed);tree.find('Language/Name').text=name
 if full: tree.find('.//PhoneticInput/PhoneticSequence/SimpleContext').set('naturalClass','ncVowel')
 if plain:
  template=tree.find('.//LeftEnvironment/PhoneticTemplate/PhoneticSequence')
  q=template.find('OptionalSegmentSequence');template.remove(q)
  template.append(copy.deepcopy(q.find('SimpleContext')))
 ET.indent(tree,space='  ');tree.write(folder/'grammar.xml',encoding='utf-8',xml_declaration=True)
 words=folder/'words.txt';words.write_text('\n'.join(a+b for a in 'iyau' for b in 'iyau')+'\n')
 env=dict(os.environ,PATH='/home/johnm/.dotnet:'+os.environ['PATH'])
 oracle_out=folder/'oracle.tsv'
 result=subprocess.run(['bash',str(root/'machine/conformance/adapters/hc-dotnet-wrapper.sh'),'batch',str(folder/'grammar.xml'),str(words),str(oracle_out),oracle],env=env,capture_output=True,text=True,timeout=30)
 if result.returncode: raise SystemExit(f'oracle failed: {name}: {result.returncode}: {result.stderr}')
 (folder/'oracle.log').write_text(result.stdout+result.stderr)
 rust_out=folder/'rust.tsv'
 env=dict(os.environ,PANGLOSS_EXTRA_ARGS=f'batch {folder}/grammar.xml {words} {rust_out} --threads 1 --word-timeout-ms 5000')
 with (folder/'managed-run.log').open('w') as log:
  result=subprocess.run(['pwsh','-NoProfile','-File','rust/tools/pg.ps1','-Mode','run','-Exe','rust/target/pg-test-opt/pangloss','-RunCaptureStdout',str(folder/'rust.stdout')],env=env,stdout=log,stderr=subprocess.STDOUT)
 if result.returncode: raise SystemExit(f'managed Rust run failed: {name}: {result.returncode}')
 def rows(p): return [r.split('\t') for r in p.read_text().splitlines() if len(r.split('\t'))==5]
 o=rows(oracle_out);r=rows(rust_out)
 assert len(o)==len(r)==16 and all(row[3]=='ok' for row in o+r)
 diffs=[{'word':a[1],'oracle':a[4],'rust':b[4]} for a,b in zip(o,r) if a[1:2]+a[3:]!=b[1:2]+b[3:]]
 results[name]=diffs
 print(name, 'complete rows 16; diffs',diffs,flush=True)
(scratch/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
