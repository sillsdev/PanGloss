"""Author and measure synthetic underdefined projects with installed FieldWorks engines."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]
sys.stdout.reconfigure(encoding='utf-8')
EXE = ROOT / 'tools/xample-projector/bin/Debug/XampleProjector.exe'
SCRATCH = ROOT / '_lane/scratch/underdefined-final'
ENV = os.environ.copy()
ENV.update(TEMP=str(ROOT / '_lane/scratch'), TMP=str(ROOT / '_lane/scratch'),
           MSBUILDDISABLENODEREUSE='1', UseSharedCompilation='false')


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def run(folder, name, *args):
    folder.mkdir(parents=True, exist_ok=True)
    try:
        result = subprocess.run([str(EXE), *map(str, args)], cwd=ROOT, env=ENV,
                                capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=120)
    except subprocess.TimeoutExpired as exc:
        write_json(folder / (name + '.invocation.json'), {'args': [str(a) for a in args],
                   'timeoutSeconds': 120, 'stdout': (exc.stdout or b'').decode('utf-8', 'replace'),
                   'stderr': (exc.stderr or b'').decode('utf-8', 'replace')})
        raise
    write_json(folder / (name + '.invocation.json'), {'args': [str(a) for a in args],
               'exitCode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
    if result.returncode:
        raise RuntimeError(f'{name}: exit {result.returncode}\n{result.stderr}\n{result.stdout}')


def grammar(reps, roots, affix='s', prefix=False, segment_class=False):
    segments = ''.join(f'<SegmentDefinition id="seg{i}"><Representations><Representation>{escape(r)}</Representation></Representations></SegmentDefinition>' for i, r in enumerate(reps))
    entries = ''.join(f'<LexicalEntry id="entry{i}" partOfSpeech="pos"><Allomorphs><Allomorph id="allo{i}"><PhoneticShape>{escape(r)}</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R{i}</MorphemeId></LexicalEntry>' for i, r in enumerate(roots))
    output = f'<InsertSegments><PhoneticShape>{escape(affix)}</PhoneticShape></InsertSegments><CopyFromInput index="stem"/>' if prefix else f'<CopyFromInput index="stem"/><InsertSegments><PhoneticShape>{escape(affix)}</PhoneticShape></InsertSegments>'
    nc = f'<SegmentNaturalClass id="ncV" segments="seg{reps.index("a")}"><Name>V</Name></SegmentNaturalClass>' if segment_class else ''
    return f'''<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>SyntheticUnderdefined</Name>
<PartsOfSpeech><PartOfSpeech id="pos"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass>{nc}</NaturalClasses>
<Strata><Stratum characterDefinitionTable="table" morphologicalRuleOrder="unordered"><Name>Main</Name>
<MorphologicalRuleDefinitions><MorphologicalRule id="affix" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos"><Name>A</Name>
<MorphologicalSubrules><MorphologicalSubrule id="affixAllo"><MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput>{output}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>A</MorphemeId></MorphologicalRule></MorphologicalRuleDefinitions>
<AffixTemplates><AffixTemplate requiredPartsOfSpeech="pos"><Name>template</Name><Slot optional="true" morphologicalRules="affix"><Name>slot</Name></Slot></AffixTemplate></AffixTemplates>
<LexicalEntries>{entries}</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>
'''


def author(case, reps, roots, affix, prefix, segment_class=False):
    folder = SCRATCH / case
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'source.grammar.xml').write_text(grammar(reps, roots, affix, prefix, segment_class), encoding='utf-8')
    if not (folder / 'author/author-response.json').exists():
        run(folder, 'author', 'author', '--grammar', folder / 'source.grammar.xml', '--out-dir', folder / 'author', '--name', 'Synthetic')
    path = folder / 'author/author-response.json'
    response = json.loads(path.read_text(encoding='utf-8'))
    project = (path.parent / response['projectPath']).resolve()
    if response['grammarSha256'] != hashlib.sha256((folder / 'source.grammar.xml').read_bytes()).hexdigest() or response['projectSha256'] != hashlib.sha256(project.read_bytes()).hexdigest():
        raise ValueError('cached author witness changed; select fresh --scratch')
    return folder, path, project, response


def configure(folder, project, ops, name):
    request = folder / (name + '.request.json')
    write_json(request, {'schemaVersion': 1, 'baseSha256': hashlib.sha256(project.read_bytes()).hexdigest(), 'operations': ops})
    if not (folder / name / 'probe-response.json').exists():
        run(folder, name, 'configure-probe', '--project', project, '--request', request, '--out-dir', folder / name)
    response = json.loads((folder / name / 'probe-response.json').read_text(encoding='utf-8'))
    clone = (folder / name / response['materializedProjectPath']).resolve()
    if response['baseSha256'] != hashlib.sha256(project.read_bytes()).hexdigest() or response['materializedSha256'] != hashlib.sha256(clone.read_bytes()).hexdigest() or response['operations'] != ops:
        raise ValueError('cached configuration changed; select fresh --scratch')
    return clone


def mutate(folder, project, response, rep):
    index = response['_reps'].index(rep)
    request = folder / 'mutation.request.json'
    write_json(request, {'schemaVersion': 1, 'caseId': folder.name, 'baseSha256': hashlib.sha256(project.read_bytes()).hexdigest(),
                        'operations': [{'op': 'remove_phoneme', 'guid': response['guidMap'][f'seg{index}'], 'assertRepresentations': [rep], 'requireUnreferenced': False}]})
    if not (folder / 'mutated/mutation-response.json').exists():
        run(folder, 'mutate', 'mutate', '--project', project, '--request', request, '--out-dir', folder / 'mutated')
    result = json.loads((folder / 'mutated/mutation-response.json').read_text(encoding='utf-8'))
    clone = (folder / 'mutated' / result['materializedProjectPath']).resolve()
    if result['baseSha256'] != hashlib.sha256(project.read_bytes()).hexdigest() or result['materializedSha256'] != hashlib.sha256(clone.read_bytes()).hexdigest():
        raise ValueError('cached mutation changed; select fresh --scratch')
    return clone


def measure(folder, project, words, name):
    out = folder / name
    out.mkdir(parents=True, exist_ok=True)
    (out / 'words.txt').write_text('\n'.join(words) + '\n', encoding='utf-8')
    run(out, 'project', 'project', '--project', project, '--out-dir', out, '--database', 'Probe')
    run(out, 'parse', 'parse', '--project', project, '--project-dir', out, '--database', 'Probe', '--words', out / 'words.txt', '--out', out / 'xample.json')
    run(out, 'parse-hc', 'parse-hc', '--project', project, '--hc-xml', out / 'Probe.hc.xml', '--words', out / 'words.txt', '--out', out / 'hc.json')
    for engine in ['xample', 'hc']:
        data = json.loads((out / (engine + '.json')).read_text(encoding='utf-8'))
        print(folder.name, name, engine, [(r['word'], len(r['analyses']), r['engineError']) for r in data['words']], flush=True)
    return out


CASES = [
    ('01-root-letter', ['m','u','a','s','x'], ['muma','xuma'], 's', False, 'x', [], ['muma','mumas','xuma','xumas','xumu']),
    ('02-affix-letter', ['m','u','a','x'], ['muma'], 'x', False, 'x', [], ['muma','mumax','mumay']),
    ('03-multigraph', ['m','u','a','s','ch'], ['muma','chuma'], 's', False, 'ch', [{'op':'wordforming','representations':['m','u','a','s','ch']}], ['muma','chuma','chumas','cuma','huma']),
    ('04-combining-mark', ['m','u','a','s','a\u0303'], ['muma','ma\u0303ma'], 's', False, 'a\u0303', [{'op':'wordforming','representations':['m','u','a','s','a\u0303']}], ['muma','ma\u0303ma','ma\u0303mas','mãma','mãmas']),
    ('05-missing-class', ['m','u','a','s','x'], ['muma','mum','mux'], 's', False, None, [{'op':'environment','text':'/ [V] _'}], ['muma','mumas','mum','mums','mux','muxs','s']),
    ('06-feature-class', ['m','u','a','s','x'], ['muma','mum','mux'], 's', False, None, [{'op':'feature-class','assignments':{'m':'-','u':'+','a':'+','s':'-','x':'-'}},{'op':'environment','text':'/ [V] _'}], ['muma','mumas','mum','mums','mux','muxs']),
    ('07-featureless-phoneme', ['m','u','a','s','x'], ['muma','mum','mux'], 's', False, None, [{'op':'feature-class','assignments':{'m':'-','u':'+','a':'+','s':'-'}},{'op':'environment','text':'/ [V] _'}], ['muma','mumas','mum','mums','mux','muxs']),
    ('08-rule-context', ['m','u','a','s','x','p'], ['muma','xuma','xmuma'], 's', False, 'x', [{'op':'rewrite','input':'m','output':'p','left':'x'},{'op':'wordforming','representations':['x','m','u','a','p','s']}], ['muma','xuma','xmuma','xpuma','xpumas']),
    ('09-multigraph-boundary', ['m','u','a','c','h','ch'], ['chuma','huma'], 'c', True, 'ch', [{'op':'wordforming','representations':['m','u','a','c','h','ch']}], ['huma','chuma','cchuma']),
    ('10-undefined-environment-letter', ['m','u','a','s','x'], ['muma','mux'], 's', False, 'x', [{'op':'environment','text':'/ x _'},{'op':'wordforming','representations':['x','m','u','a','s']}], ['muma','mumas','mux','muxs']),
    ('11-distinct-feature-class', ['m','u','a','s','x'], ['muma','mum','mux'], 's', False, None, [{'op':'feature-class','distinct':True,'assignments':{'m':'-','u':'+','a':'+','s':'-','x':'-'}},{'op':'environment','text':'/ [V] _'}], ['muma','mumas','mum','mums','mux','muxs']),
    ('12-featureless-rule-class', ['m','u','a','s','x','p'], ['muma','xmuma','xuma'], 's', False, 'features:x', [{'op':'feature-class','distinct':True,'assignments':{'m':'-','u':'+','a':'+','s':'-','x':'-','p':'-'}},{'op':'rewrite','input':'m','output':'p','leftClass':'V'}], ['muma','mupa','xmuma','xmupa','xpupa','xuma']),
]


def main():
    for case, reps, roots, affix, prefix, removed, ops, words in CASES:
        if options.case and case not in options.case:
            continue
        folder, author_path, project, response = author(case, reps, roots, affix, prefix)
        response['_reps'] = reps
        if ops:
            project = configure(folder, project, ops, 'configured')
        control = project
        if case == '05-missing-class':
            control = configure(folder, project, [{'op':'segment-class','members':['a','u']}], 'defined-class-control')
        measure(folder, control, words, 'control')
        if removed and removed.startswith('features:'):
            project = configure(folder, project, [{'op':'clear-features','representation':removed.split(':')[1]}], 'featureless')
        elif removed:
            project = mutate(folder, project, response, removed)
        measure(folder, project, words, 'off')
        project_on = configure(folder, project, [{'op':'accept-unspecified','value':True}], 'accept-on')
        measure(folder, project_on, words, 'on')
        write_json(folder / 'case.json', {'case':case,'words':words,'projectControl':str(control),'projectOff':str(project),'projectOn':str(project_on), 'removed':removed})


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', action='append', help='measure only this named case (repeatable)')
    parser.add_argument('--scratch', type=Path, default=SCRATCH)
    options = parser.parse_args()
    SCRATCH = options.scratch.resolve()
    if not SCRATCH.is_relative_to(ROOT):
        parser.error('--scratch must be inside this worktree')
    main()
