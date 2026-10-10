"""Frozen 33 BUILD roots and 36 diagnostic obligations; never runtime authority."""
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('corpus_manifest_helpers',
    Path(__file__).resolve().parents[1]/'production-manifests/manifest.py')
m = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(m)
exact, strict, digest, read = m.exact, m.strict, m.digest, m.read
REGISTRY = 'tests/native-provider-qualification/production-corpus/fixtures.json'
REGISTRY_SHA = '1326226f6c1bfa8122564d6507d70df4c6acf75060fb1a650b02c36ed562155e'
HISTORICAL_SHA = 'bdd5cc4b0d624a130c4b6c9330855638b79c0afdab00ebb4bb64116829f6d229'
COUNTS = {'accepted': 33, 'rejected': 36, 'diagnostics': 67}
SCOPE = {'accepted_build_pairs': 33, 'rejected_diagnostic_pairs': 36,
         'diagnostic_rows_per_provider': 67, 'cli_invocations': 171,
         'bundle_files': 264, 'target_artifacts': 198,
         'runtime_execution': False, 'verified_ir_serialized': False,
         'layout_semantics_reconstructed': False}


def contracts(root, inputs):
    root = Path(root)
    assert digest(m.file(root, REGISTRY)) == inputs[REGISTRY] == REGISTRY_SHA
    value = read(m.file(root, REGISTRY))
    exact(set(value), {'version', 'profile', 'historical_classification_sha256', 'counts', 'fixtures'})
    exact(value['version'], 1)
    exact(value['profile'], 'data-ownership-v1')
    exact(value['historical_classification_sha256'], HISTORICAL_SHA)
    exact(value['counts'], COUNTS)
    rows = value['fixtures']
    assert type(rows) is list and len(rows) == 69
    found = {}
    for row in rows:
        exact(set(row), {'id', 'disposition', 'source', 'sources', 'edges', 'diagnostics'})
        assert row['id'] not in found
        assert row['id'] == 'm3-'+row['source'].removeprefix('tests/m3-fixtures/').removesuffix('.zry').replace('/', '-')
        assert row['disposition'] in ('accepted', 'rejected')
        assert row['sources'][0]['path'] == row['source']
        for index, source in enumerate(row['sources']):
            exact(set(source), {'id', 'path', 'sha256'})
            exact(source['id'], index)
            assert digest(m.file(root, source['path'])) == source['sha256'] == inputs[source['path']]
        assert bool(row['diagnostics']) == (row['disposition'] == 'rejected')
        found[row['id']] = row
    exact({kind: sum(row['disposition'] == kind for row in rows)
           for kind in ('accepted', 'rejected')}, {'accepted': 33, 'rejected': 36})
    exact(sum(len(row['diagnostics']) for row in rows), 67)
    return found


def failure(contract):
    assert contract['disposition'] == 'rejected'
    return {'version': 1, 'command': 'build', 'manifest': None, 'ok': False, 'results': [],
            'diagnostics': contract['diagnostics']}


def expected_manifest(label, contract, actual, layouts, runtime):
    assert contract['id'] == label and contract['disposition'] == 'accepted'
    assert type(layouts) is dict and set(layouts) == {
        'type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256'}
    for value in layouts.values():
        m.hexadecimal(value)
    exact(runtime, {'identifier': 'zryna-ownership-runtime-v1', 'version': 1,
                    'sha256': runtime['sha256']})
    m.hexadecimal(runtime['sha256'])
    stem = 'corpus-build-'+label
    artifacts = []
    for target, kind, extension in zip(m.TARGETS, m.KINDS, m.EXTENSIONS):
        name = f'{stem}.{extension}'
        row = actual[f'{target}/{name}']
        metadata = {'profile': 'ecmascript-module', 'scalar_abi': 'v1'}
        if target == 'webassembly':
            metadata = {'profile': 'core-webassembly', 'memory': 'linear32-v1',
                        'imports': False, 'scalar_abi': 'v1'}
        elif target == 'native':
            metadata = {'profile': 'linux-x86-64-elf', 'triple': 'x86_64-unknown-linux-gnu',
                        'format': 'elf-relocatable', 'scalar_abi': 'v1',
                        'program_object_sha256': row['sha256'],
                        'runtime_object_sha256': None, 'harness_sha256': None}
        artifacts.append({'target': target, 'kind': kind, 'filename': name,
                          'bytes': row['bytes'], 'sha256': row['sha256'], 'metadata': metadata})
    return {'version': 3, 'profile': 'zryna-data-ownership-v1', 'protocol_version': 4,
            'command': 'build', 'entrypoint': contract['source'],
            'graph_sha256': m.graph_digest(3, contract['sources'], contract['edges']),
            'sources': contract['sources'], 'edges': contract['edges'],
            'layouts': {key: layouts[key] for key in
                        ('type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256')},
            'runtime_abi': {key: runtime[key] for key in ('identifier', 'version', 'sha256')},
            'stem': stem, 'targets': list(m.TARGETS), 'artifacts': artifacts,
            'invocation': None, 'results': [], 'diagnostics': []}


def verify_bundle(directory, label, contract, declared, success):
    directory = Path(directory)
    for parent in directory.absolute().parents:
        m.real(parent, True)
    actual = m.inventory(directory)
    stem = 'corpus-build-'+label
    paths = [f'{target}/{stem}.{extension}' for target, extension in zip(m.TARGETS, m.EXTENSIONS)]
    name = 'zryna-manifest-v3.json'
    assert set(actual) == {name, *paths}, 'expected exactly the complete four-file BUILD bundle'
    assert all(0 < row['bytes'] <= 32*1024*1024 for row in actual.values())
    exact(declared, actual)
    m.formats(directory, paths)
    exact(success, {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [],
                    'ok': True, 'manifest': f'.zryna/out/{stem}.build/{name}'})
    path = m.file(directory, name)
    value = read(path)
    expected = expected_manifest(label, contract, actual, value['layouts'], value['runtime_abi'])
    exact(value, expected)
    assert path.read_bytes() == m.canonical(expected), 'noncanonical manifest wire bytes'
    return actual
