"""Seven frozen BUILD contracts; artifact parity is not an independent backend oracle."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location(
    'seven_manifest_helpers', Path(__file__).resolve().parents[2]/'native-cli-smoke/manifest_admission.py')
_helpers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_helpers)
exact, digest, file, real = _helpers._exact, _helpers._digest, _helpers._file, _helpers._real
inventory, formats, graph_digest = _helpers._inventory, _helpers._formats, _helpers.graph_digest
hexadecimal = _helpers._hex
TARGETS, KINDS, EXTENSIONS = _helpers.TARGETS, _helpers.KINDS, _helpers.EXTENSIONS
CASES = ('abi', 'bounds', 'enum', 'handles', 'owned-shared', 'weak-expired', 'weak-live')
REGISTRY = 'tests/m3-conformance-v1.json'
REGISTRY_SHA = '34cd29a5f146d77e7163b32d21e71e4f5a1fc5fd50f688d197de8bef9b38a508'
CONTEXTS = {'enum': ('score', 'enumScore'), 'handles': ('handles', 'handles'),
            'owned-shared': ('aggregate', 'aggregate'), 'weak-expired': ('check', 'check'),
            'weak-live': ('check', 'check')}
FROZEN = {
    'abi': 'cafc15d865a5e5e015a598f29e5aeb966c6143ea64050b37d306571ad68f804f',
    'bounds': '34a2b3210f2fa03eb990fbae3cd881b7cf182abdc6ec651dc8bf85c16bd414d9',
    'enum': '5052c0754d74a8b2ad1d3d20496ef4b09fe428f7af1591be1712052b1d6426f2',
    'enum-body': '7d65eeacbcad853fe9470679d714cf08c1796d1d0375bbf2df05a713cf54bdc0',
    'handles': '3367e2543514e4f73d862556b27c4691082acee78aba9e576b7e4168f8397495',
    'handles-body': 'aace98260d527614117b7ad7b40b0da402ee2e9aed72f1889c202113682fa968',
    'owned-shared': 'ca02b53a2edb217c4e7c0b333fe33d2c4a900bcd78713eb7f1d8fde26012d675',
    'owned-shared-body': 'd2c9057d40a558eefb680c23387662ad76a639d33cfc8cb75c737958ba96606c',
    'weak-expired': 'a4ed2aa2b621b69a16c0365a8eb5fdd7db9ca9dddc4cae495f045157c8e92cb1',
    'weak-expired-body': '7f149fe308e1e75761721371ead063c072a7643dae7f7454643ba8c199247225',
    'weak-live': 'a4ed2aa2b621b69a16c0365a8eb5fdd7db9ca9dddc4cae495f045157c8e92cb1',
    'weak-live-body': '77e3f2ee548371a30ab057eb43dc8e486a8fd032ddda8e53ae58aecac5bdd53c',
}


def pairs(rows):
    result = {}
    for key, value in rows:
        assert key not in result, 'duplicate JSON key'
        result[key] = value
    return result


def strict(raw):
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_float=lambda _: (_ for _ in ()).throw(ValueError('floating JSON value')),
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite JSON value')))


def read(path):
    real(path, False)
    assert path.stat().st_size <= 32*1024*1024
    return strict(path.read_bytes())


def contracts(root, output, inputs):
    """Bind the independent seven-case census, frozen bytes and ten contextual copies.

    Output is the producer's resolved native path spelling, including archived Windows paths.
    """
    assert __debug__ and type(inputs) is dict
    root = Path(root)
    real(root, True)
    assert digest(file(root, REGISTRY)) == inputs[REGISTRY] == REGISTRY_SHA
    registry = read(file(root, REGISTRY))
    exact(registry['schemaVersion'], 1)
    exact(registry['profile'], 'zryna-data-ownership-fixed-oracle-v1')
    exact(registry['candidateProfile'], 'zryna-data-ownership-v1-candidate')
    exact(registry['targetOrder'], list(TARGETS))
    rows = registry['fixtures']
    assert type(rows) is list and all(type(row) is dict for row in rows)
    assert all(type(row['id']) is str for row in rows)
    by_id = {row['id']: row for row in rows}
    assert len(by_id) == len(rows)
    prefix = '.zryna/cache/production-manifest-' + hashlib.sha256(str(output).encode()).hexdigest()[:16]
    result, generated = {}, {}
    for identity in CASES:
        label = 'm3-' + identity
        fixture = by_id[identity]
        ids = [identity]
        if identity in CONTEXTS:
            exact(fixture['dependency'], identity+'-body')
            ids.append(identity+'-body')
        else:
            assert 'dependency' not in fixture
        sources = []
        for index, name in enumerate(ids):
            original = f'tests/m3-fixtures/conformance/{name}.zry'
            frozen = by_id[name]
            exact(frozen['path'], original)
            exact(frozen['sha256'], FROZEN[name])
            assert digest(file(root, original)) == inputs[original] == FROZEN[name]
            path = original
            if identity in CONTEXTS:
                path = f'{prefix}/{label}/' + ('main.zry' if index == 0 else 'math.zry')
                generated[path] = {'original': original, 'sha256': FROZEN[name]}
            sources.append({'id': index, 'path': path, 'sha256': FROZEN[name]})
        edges = []
        if identity in CONTEXTS:
            imported, local = CONTEXTS[identity]
            edges = [{'importer': sources[0]['path'], 'target': sources[1]['path'],
                      'specifier': './math.zry', 'imported': imported, 'local': local}]
        result[label] = {'source': sources[0]['path'], 'profile': 'data-ownership-v1',
                         'version': 3, 'sources': sources, 'edges': edges}
    assert len(result) == 7 and len(generated) == 10
    return result, generated


def canonical(value):
    """The fixed ASCII schema uses the existing Rust pretty wire shape plus one LF."""
    return (json.dumps(value, ensure_ascii=False, indent=2)+'\n').encode()


def expected_manifest(label, contract, actual, layouts, runtime):
    assert label in tuple('m3-'+case for case in CASES)
    exact(set(contract), {'source', 'profile', 'version', 'sources', 'edges'})
    exact(contract['profile'], 'data-ownership-v1')
    exact(contract['version'], 3)
    assert type(layouts) is dict and set(layouts) == {
        'type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256'}
    for value in layouts.values():
        hexadecimal(value)
    exact(runtime, {'identifier': 'zryna-ownership-runtime-v1', 'version': 1,
                    'sha256': runtime['sha256']})
    hexadecimal(runtime['sha256'])
    stem = 'production-manifest-'+label
    artifacts = []
    for target, kind, extension in zip(TARGETS, KINDS, EXTENSIONS):
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
            'graph_sha256': graph_digest(3, contract['sources'], contract['edges']),
            'sources': contract['sources'], 'edges': contract['edges'],
            'layouts': {key: layouts[key] for key in
                        ('type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256')},
            'runtime_abi': {key: runtime[key] for key in ('identifier', 'version', 'sha256')},
            'stem': stem, 'targets': list(TARGETS), 'artifacts': artifacts,
            'invocation': None, 'results': [], 'diagnostics': []}


def verify_bundle(directory, label, contract, declared, success):
    assert __debug__
    directory = Path(directory)
    for parent in directory.absolute().parents:
        real(parent, True)
    actual = inventory(directory)
    stem = 'production-manifest-'+label
    paths = [f'{target}/{stem}.{extension}' for target, extension in zip(TARGETS, EXTENSIONS)]
    name = 'zryna-manifest-v3.json'
    assert set(actual) == {name, *paths}, 'expected exactly the complete four-file BUILD bundle'
    assert all(0 < row['bytes'] <= 32*1024*1024 for row in actual.values())
    exact(declared, actual)
    formats(directory, paths)
    exact(success, {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [],
                    'ok': True, 'manifest': f'.zryna/out/{stem}.build/{name}'})
    path = file(directory, name)
    manifest = read(path)
    expected = expected_manifest(label, contract, actual, manifest['layouts'], manifest['runtime_abi'])
    exact(manifest, expected)
    assert path.read_bytes() == canonical(expected), 'noncanonical manifest wire bytes'
    return actual
