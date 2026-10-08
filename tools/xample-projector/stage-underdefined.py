"""Transcribe measured stored keys, or replay the staged synthetic project witnesses."""
import argparse
from collections import Counter
import datetime
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('measurement', Path(__file__).with_name('measure-underdefined.py'))
measurement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measurement)
PROVENANCE = ('# oracle-provenance: XAMPLE xample.dll/xample64.dll file version 3.12.23.21 '
              '(XAmpleManagedWrapper 9.3.10.1452), FieldWorks ParserCore/HCLoader 9.3.10.1452 '
              '+ LCModel 11.0.0.55173, Machine/HC 3.8.2.0; XAMPLE off-project stored keys '
              'supply PanGloss minimum expectations; C# HC control/off/on are observations; '
              'PanGloss not executed. Binary hashes: ../engine-provenance.json.\n')
PROJECT_DIRS = {'control': 'fieldworks-control', 'off': 'fieldworks', 'on': 'fieldworks-on'}
FINDINGS = {
    '01-root-letter': 'Remove x, retaining root xuma. XAMPLE preserves its keys; HC off drops the root and cannot segment it, while on restores it.',
    '02-affix-letter': 'Remove x, retaining suffix x. XAMPLE preserves mumax; HC off drops the affix and on restores it.',
    '03-multigraph': 'Remove ch but retain LDML {ch} and root chuma. HC on segments ch,u,m,a. XAMPLE preserves the root; case 9 discriminates its morphological boundary behavior.',
    '04-combining-mark': 'Remove decomposed a+U+0303. XAMPLE accepts NFD mãma but rejects NFC mãma, even in the control. HC accepts both spellings once segmentable; off fails at the combining mark.',
    '05-missing-class': 'Missing V in / [V] _ is omitted before XAMPLE export. Both engines accept unrestricted suffixes; HC warns. A defined V={a,u} control rejects mums/muxs.',
    '06-feature-class': 'Existing PhNCFeatures V exports as / []_ without a string class. XAMPLE warns and accepts no suffix analyses; HC applies the feature restriction. Identical matrices also merge HC root identities; case 11 isolates that effect.',
    '07-featureless-phoneme': 'Featureless x unifies with V but is not subsumed, with or without defaults. HC rejects muxs. Blanket full-mask membership is not observed; case 12 tests a rule.',
    '08-rule-context': 'Undefined free-text rule letters cannot be saved in this structured FieldWorks context. Deleting referenced x deletes the left-context object and leaves an unconditional rule. XAMPLE preserves literal roots; HC loses muma. The inventory-complete, featureless control also rejects xpuma; literal-rule inversion remains unresolved.',
    '09-multigraph-boundary': 'XAMPLE chuma has two keys: root chuma and c+huma. HC declared/provisional ch greedily loses c+huma. Removing ch with acceptance off permits declared c+h and restores both keys.',
    '10-undefined-environment-letter': 'Delete x from / x_. FieldWorks omits the unreadable environment before XAMPLE; HC warns and drops it. On restores mux and accepts unrestricted mumas, which a restored binding restriction would reject.',
    '11-distinct-feature-class': 'Add unique identity features to case 6. HC root conflation disappears; XAMPLE still warns about its malformed class environment and accepts no suffix analyses.',
    '12-featureless-rule-class': 'm→p / [V]_ works for muma/mupa and xmuma/xmupa. Clearing x features does not cause xpupa, but mupa gains root xuma through inverse unification. The defined control also rejects xuma despite no literal m; that inversion/validation concern remains unresolved.'
}


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def keys(row, engine):
    result = []
    for analysis in row['analyses']:
        morphs = []
        for morph in analysis['morphemes']:
            if engine == 'xample' and not morph['storedKeyComplete']:
                raise ValueError('incomplete XAMPLE stored key')
            key = {'allomorph': morph['allomorphGuid'],
                   'msa': morph['storedMsaGuid'] if engine == 'xample' else morph['msaGuid'],
                   'inflection_type': morph['inflectionTypeGuid']}
            if not key['allomorph'] or not key['msa']:
                raise ValueError('unresolved stored identity')
            morphs.append(key)
        if not morphs:
            raise ValueError('empty analysis is not a stored key')
        result.append(morphs)
    return result


def multiset(values):
    return Counter(json.dumps(value, sort_keys=True) for value in values)


def copy_project(source, destination):
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination / 'project.fwdata')
    ws_dir = destination / 'WritingSystemStore'
    ws_dir.mkdir(exist_ok=True)
    for ws in (source.parent / 'WritingSystemStore').glob('*.ldml'):
        compact_ldml(ws, ws_dir / ws.name)


def compact_ldml(source, destination):
    # Retain all grapheme data; omit CLDR display names, calendars, and other UI payload.
    tree = ET.parse(source)
    root = tree.getroot()
    for child in list(root):
        if child.tag not in ('identity', 'characters', 'layout', 'special'):
            root.remove(child)
    ET.register_namespace('sil', 'urn://www.sil.org/ldml/0.1')
    ET.register_namespace('palaso', 'urn://palaso.org/ldmlExtensions/v1')
    ET.indent(tree, space='  ')
    tree.write(destination, encoding='utf-8', xml_declaration=True)


def summarize(case):
    modes = {}
    for mode in PROJECT_DIRS:
        captures = {e: read(case / mode / (f'{e}.json')) for e in ('xample', 'hc')}
        assert [r['word'] for r in captures['xample']['words']] == [r['word'] for r in captures['hc']['words']]
        rows = []
        for xr, hr in zip(captures['xample']['words'], captures['hc']['words']):
            assert not xr['engineError'] and not xr['reachedMaxAnalyses'], xr
            assert hr['projectionAgrees'], hr
            rows.append({'word': xr['word'],
                         'xample': {'stored_analysis_keys': keys(xr, 'xample'), 'engine_error': xr['engineError']},
                         'csharp_hc': {'stored_analysis_keys': keys(hr, 'hc'), 'engine_error': hr['engineError']}})
        modes[mode] = {'words': rows, 'hc_load_diagnostics': captures['hc']['diagnostics'],
                       'xample_native_log': f'measurements/{mode}/xample.json.xample.log'}
    return modes


def stage(source, destination):
    if destination.exists():
        raise ValueError('refusing to overwrite staged evidence; select a fresh --out-dir')
    destination.mkdir(parents=True)
    first = source / measurement.CASES[0][0] / 'off/response.json'
    pins = read(first)['assemblyVersions']
    fwdir = Path(measurement.ENV.get('PANGLOSS_FIELDWORKS_DIR', 'C:/Program Files/SIL/FieldWorks 9'))
    # The native runtime pin verifies the loaded xample.dll equals xample64.dll.
    binaries = {name: {'fileVersion': version, 'sha256': digest(fwdir / name)} for name, version in pins.items()}
    binaries['xample.dll'] = {'fileVersion': pins['xample64.dll'], 'sha256': digest(fwdir / 'xample.dll')}
    measurement.write_json(destination / 'engine-provenance.json', {
        'schemaVersion': 1, 'recordedUtc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'binaries': binaries, 'projectorExeSha256': digest(measurement.EXE),
        'fieldworksSourceInspected': '089eb9027b6d81be7883c40960f0de3ffa04b699',
        'machineSourceInspected': 'a4b29742b6274a01c7398ca3e800a19fa6d2c9aa',
        'sourceHeadsAreNotInstalledBinaryBuildProvenance': True})
    for definition in measurement.CASES:
        name = definition[0]
        case, target = source / name, destination / name
        manifest = read(case / 'case.json')
        modes = summarize(case)
        target.mkdir()
        shutil.copy2(case / 'source.grammar.xml', target / 'grammar.xml')
        project_paths = {mode: Path(manifest['project' + mode.capitalize()]) for mode in PROJECT_DIRS}
        hashes = {}
        ws_hashes = {}
        for mode, project in project_paths.items():
            copy_project(project, target / PROJECT_DIRS[mode])
            hashes[mode] = digest(project)
            ws_hashes[mode] = {ws.name: {'sourceSha256': digest(ws),
                                      'stagedSha256': digest(target / PROJECT_DIRS[mode] / 'WritingSystemStore' / ws.name)}
                               for ws in (project.parent / 'WritingSystemStore').glob('*.ldml')}
            assert read(case / mode / 'response.json')['sourceSha256'] == hashes[mode]
            assert read(case / mode / 'hc.json')['sourceSha256'] == hashes[mode]
            assert read(case / mode / 'xample.json')['sourceSha256'] == hashes[mode]
            assert read(case / mode / 'xample.json')['loadedEnginePinsVerified'] is True
            shutil.copytree(case / mode, target / 'measurements' / mode)
            # Normalize only metadata source paths; engine results/logs are preserved verbatim.
            response_path = target / 'measurements' / mode / 'response.json'
            response = read(response_path)
            response['sourcePath'] = f'../../{PROJECT_DIRS[mode]}/project.fwdata'
            measurement.write_json(response_path, response)
        copy_project(case / 'author/Synthetic/Synthetic.fwdata', target / 'fieldworks-authored')
        author = read(case / 'author/author-response.json')
        author['projectPath'] = 'fieldworks-authored/project.fwdata'
        author['grammarPath'] = 'grammar.xml'
        measurement.write_json(target / 'author-response.json', author)
        for request in case.glob('*.request.json'):
            shutil.copy2(request, target / request.name)
        for response in case.glob('*/probe-response.json'):
            shutil.copy2(response, target / (response.parent.name + '.response.json'))
        mutation = case / 'mutated/mutation-response.json'
        if mutation.exists():
            shutil.copy2(mutation, target / 'mutation-response.json')
        rows = []
        for row in modes['off']['words']:
            rows.append({**row, 'pangloss': {'minimum_stored_analysis_keys': row['xample']['stored_analysis_keys'],
                                            'allow_additional_analyses': True, 'executed': False}})
        words = {'schemaVersion': 1, 'language': name, 'inspired_by': ['invented lexemes; no real language'],
                 'requires': [], 'fieldworks_producible': True,
                 'measurement_protocol': 'underdefined-stored-keys-v1', 'words': rows}
        (target / 'words.yaml').write_text(PROVENANCE + json.dumps(words, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        measurement.write_json(target / 'measurement.json', {'schemaVersion': 1, 'projectSha256': hashes,
                               'writingSystemSha256': ws_hashes, 'ldmlRetainedElements': ['identity', 'characters', 'layout', 'special'], 'modes': modes})
        (target / 'STAGING.md').write_text(
            PROVENANCE + f'\nSynthetic probe `{name}`. `grammar.xml` is the supported author input, '
            'before LibLCM configuration/mutation. The saved project in `fieldworks/` is the '
            'off-state source of the PanGloss minimum; its on and control counterparts are adjacent.\n\n'
            '`words.yaml` uses ordered stored-analysis keys, preserving duplicate analyses. It is '
            'an explicitly staged measurement schema, awaiting the other lane\'s gate; it is not '
            'a legacy signature fixture and has not run in PanGloss. Empty minimum lists do not '
            'require PanGloss rejection. Full engine diagnostics, raw XAMPLE XML, native load logs, '
            'HC XML, and XAMPLE exports are under `measurements/`.\n\n'
            'Responses normalize only metadata source paths. Invocation/log paths describe the '
            'original local execution. Author GUIDs map invented R0/R1/R2 and A to source objects. '
            'No inflectional variants or paired circumfixes occur; null inflection types are real.\n\n'
            f'Measured purpose and findings: {FINDINGS[name]}\n\n'
            'Project .fwdata bytes are unchanged. LDML retains identity, characters, layout, and '
            'special elements; unrelated CLDR display/calendar payload and idchangelog.xml are '
            'omitted. Source and staged LDML hashes are in measurement.json. Replay checks '
            'complete stored keys, errors, diagnostics, and segmentation with these compact '
            'witnesses; UI equivalence of omitted metadata is not claimed.\n', encoding='utf-8')
    print(f'Staged {len(measurement.CASES)} measured cases at {destination}')


def replay(fixtures, scratch):
    if scratch.exists():
        raise ValueError('replay scratch must be fresh')
    word_count = 0
    for definition in measurement.CASES:
        name = definition[0]
        fixture = fixtures / name
        summary = read(fixture / 'measurement.json')
        for mode, project_dir in PROJECT_DIRS.items():
            project = fixture / project_dir / 'project.fwdata'
            assert digest(project) == summary['projectSha256'][mode]
            for name_ws, hashes in summary['writingSystemSha256'][mode].items():
                assert digest(project.parent / 'WritingSystemStore' / name_ws) == hashes['stagedSha256']
            copy_project(project, scratch / name / mode / 'project')
            out = measurement.measure(scratch / name, scratch / name / mode / 'project/project.fwdata',
                                      [r['word'] for r in summary['modes'][mode]['words']], mode)
            captures = {e: read(out / (e + '.json')) for e in ('xample', 'hc')}
            for index, expected in enumerate(summary['modes'][mode]['words']):
                for engine, label in (('xample', 'xample'), ('hc', 'csharp_hc')):
                    actual = captures[engine]['words'][index]
                    assert actual['word'] == expected['word']
                    assert multiset(keys(actual, engine)) == multiset(expected[label]['stored_analysis_keys']), (name, mode, expected['word'], engine)
                    assert actual['engineError'] == expected[label]['engine_error'], (name, mode, expected['word'], engine, actual)
                    if engine == 'xample':
                        assert not actual['reachedMaxAnalyses'] and captures[engine]['loadedEnginePinsVerified']
                    else:
                        assert actual['projectionAgrees']
                        original = read(fixture / 'measurements' / mode / 'hc.json')['words'][index]
                        assert actual.get('segmentCount') == original.get('segmentCount')
                word_count += 1
            assert captures['hc']['diagnostics'] == summary['modes'][mode]['hc_load_diagnostics']
            assert digest(project) == summary['projectSha256'][mode]
    print(f'Replay verified {word_count} word/state rows, both engines, complete stored-key multisets and statuses.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--measure-dir', type=Path, default=measurement.SCRATCH)
    parser.add_argument('--out-dir', type=Path, default=ROOT / 'conformance-staging/underdefined')
    parser.add_argument('--replay', action='store_true')
    parser.add_argument('--scratch', type=Path, default=ROOT / '_lane/scratch/underdefined-replay')
    args = parser.parse_args()
    for path in (args.out_dir, args.scratch):
        if not path.resolve().is_relative_to(ROOT):
            parser.error('all writes must stay in this worktree')
    if args.replay:
        replay(args.out_dir, args.scratch)
    else:
        stage(args.measure_dir, args.out_dir)
