"""Bounded private CLI bundle admission, not a production verifier or execution proof.

This independent case census binds existing manifests to retained source and artifact bytes.
Format headers and layout/ABI identity shapes do not reconstruct backend semantics.
"""
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import stat


if not __debug__:
    raise RuntimeError('optimized manifest admission is forbidden')


TARGETS = ('javascript', 'webassembly', 'native')
KINDS = ('ecmascript-module', 'core-webassembly-module', 'linux-x86-64-relocatable-object')
EXTENSIONS = ('mjs', 'wasm', 'o')
HEX = re.compile('[0-9a-f]{64}')
SINGLES = ('pair', 'array', 'borrow', 'vec')
CONTEXTS = {'string': 'text', 'owned-aggregate': 'aggregate', 'owned-vec': 'aggregate'}


def _exact(actual, expected):
    assert type(actual) is type(expected), 'JSON type differs'
    if isinstance(expected, dict):
        assert set(actual) == set(expected), 'JSON fields differ'
        for key in expected:
            _exact(actual[key], expected[key])
    elif isinstance(expected, list):
        assert len(actual) == len(expected), 'JSON array length differs'
        for value, wanted in zip(actual, expected):
            _exact(value, wanted)
    else:
        assert actual == expected, 'JSON value differs'


def _pairs(rows):
    result = {}
    for key, value in rows:
        assert key not in result, 'duplicate JSON key'
        result[key] = value
    return result


def _finite(value):
    result = float(value)
    assert math.isfinite(result), 'nonfinite JSON number'
    return result


def _json(path):
    assert path.stat().st_size <= 32 * 1024 * 1024, 'manifest/registry exceeds bound'
    return json.loads(path.read_bytes(), object_pairs_hook=_pairs, parse_float=_finite,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def _hex(value):
    assert type(value) is str and HEX.fullmatch(value), 'invalid SHA-256 identity'


def _relative(value):
    assert type(value) is str and re.fullmatch(r'[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*', value)
    assert all(part not in ('.', '..') for part in PurePosixPath(value).parts)
    return PurePosixPath(value)


def _real(path, directory):
    metadata = path.lstat()
    assert not stat.S_ISLNK(metadata.st_mode), 'linked proof path'
    assert not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'reparse proof path'
    assert (stat.S_ISDIR if directory else stat.S_ISREG)(metadata.st_mode), 'unexpected file type'


def _file(root, relative):
    parts = _relative(relative).parts
    _real(root, True)
    path = root
    for index, part in enumerate(parts):
        path = path / part
        _real(path, index != len(parts) - 1)
    assert path.resolve().is_relative_to(root.resolve()), 'escaping proof path'
    return path


def _digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def _bound(root, name, inputs):
    _hex(inputs[name])
    assert _digest(_file(root, name)) == inputs[name], 'source/registry input differs'
    return inputs[name]


def graph_digest(version, sources, edges):
    """Hash the fixed cases' existing graph encoding; their first source is the entrypoint."""
    assert __debug__ and type(version) is int and version in (2, 3)
    assert type(sources) is list and sources and type(edges) is list
    encoded = bytearray(b'ZRYNA-M2-GRAPH\0' if version == 2 else b'ZRYNA-M3-GRAPH\0')

    def integer(value):
        assert type(value) is int and 0 <= value < 2**32
        encoded.extend(value.to_bytes(4, 'little'))

    def text(value):
        assert type(value) is str
        raw = value.encode('utf-8')
        integer(len(raw))
        encoded.extend(raw)

    integer(1)
    text(sources[0]['path'])
    integer(len(sources))
    paths = []
    for index, source in enumerate(sources):
        assert type(source) is dict and set(source) == {'id', 'path', 'sha256'}
        _exact(source['id'], index)
        _relative(source['path'])
        _hex(source['sha256'])
        paths.append(source['path'])
        text(source['path'])
        encoded.extend(bytes.fromhex(source['sha256']))
    assert paths == sorted(set(paths)), 'noncanonical source order'
    integer(len(edges))
    keys = []
    for edge in edges:
        assert type(edge) is dict and set(edge) == {'importer', 'target', 'specifier', 'imported', 'local'}
        assert all(type(value) is str for value in edge.values())
        assert edge['importer'] in paths and edge['target'] in paths
        keys.append(tuple(edge[key] for key in ('importer', 'specifier', 'imported', 'local')))
        for value in keys[-1]:
            text(value)
    assert keys == sorted(set(keys)), 'noncanonical edge order'
    return hashlib.sha256(encoded).hexdigest()


def contracts(root, smoke_output, inputs):
    """Return nine fixed source contracts and the six exact registered fixture copies.

    Generated copies are described here; the caller rehashes them after smoke execution.
    """
    assert __debug__ and type(inputs) is dict
    root = Path(root).absolute()
    _real(root, True)
    m2_path, m3_path = 'tests/m2-conformance-v1.json', 'tests/m3-conformance-v1.json'
    for name in (m2_path, m3_path):
        _bound(root, name, inputs)
    m2, m3 = _json(root / m2_path), _json(root / m3_path)
    _exact(m2['schemaVersion'], 1)
    _exact(m2['profile'], 'zryna-control-flow-fixed-oracle-v1')
    _exact(m3['schemaVersion'], 1)
    _exact(m3['profile'], 'zryna-data-ownership-fixed-oracle-v1')
    _exact(m2['targetOrder'], list(TARGETS))
    _exact(m3['targetOrder'], list(TARGETS))
    fixtures = m3['fixtures']
    assert type(fixtures) is list and all(type(row) is dict for row in fixtures)
    assert all(type(row['id']) is str for row in fixtures)
    assert len({row['id'] for row in fixtures}) == len(fixtures), 'duplicate fixture identity'
    by_id = {row['id']: row for row in fixtures}
    result, generated = {}, {}

    def case(label, source, profile, version, paths, edges):
        rows = [{'id': index, 'path': name, 'sha256': sha} for index, (name, sha) in enumerate(paths)]
        result[label] = {'source': source, 'profile': profile, 'version': version,
                         'sources': rows, 'edges': edges}

    source = 'examples/universal/add.zry'
    case('m1', source, 'i32-v1', 1, [(source, _bound(root, source, inputs))], [])
    source, dependency = 'tests/m2-fixtures/valid/main.zry', 'tests/m2-fixtures/valid/math.zry'
    paths = [(name, _bound(root, name, inputs)) for name in (source, dependency)]
    edges = [{'importer': source, 'target': dependency, 'specifier': './math.zry',
              'imported': 'addPair', 'local': 'addPair'}]
    case('m2', source, 'control-flow-v1', 2, paths, edges)
    _exact(m2['graph']['entrypoint'], source)
    _exact(m2['graph']['sources'], result['m2']['sources'])
    _exact(m2['graph']['edges'], edges)
    _exact(m2['graph']['sha256'], graph_digest(2, result['m2']['sources'], edges))
    for identity in SINGLES:
        source = f'tests/m3-fixtures/conformance/{identity}.zry'
        fixture = by_id[identity]
        _exact(fixture['path'], source)
        _exact(fixture['sha256'], _bound(root, source, inputs))
        assert 'dependency' not in fixture, 'single-source case gained a dependency'
        case('m3-' + identity, source, 'data-ownership-v1', 3, [(source, fixture['sha256'])], [])
    prefix = '.zryna/cache/native-cli-smoke-' + hashlib.sha256(
        str(Path(smoke_output).resolve()).encode()).hexdigest()[:16]
    for identity, imported in CONTEXTS.items():
        label = 'm3-' + identity
        entry, body = by_id[identity], by_id[identity + '-body']
        _exact(entry['dependency'], identity + '-body')
        paths = []
        for filename, fixture, original in (
            ('main.zry', entry, f'tests/m3-fixtures/conformance/{identity}.zry'),
            ('math.zry', body, f'tests/m3-fixtures/conformance/{identity}-body.zry'),
        ):
            _exact(fixture['path'], original)
            _exact(fixture['sha256'], _bound(root, original, inputs))
            name = f'{prefix}/{label}/{filename}'
            _relative(name)
            generated[name] = {'original': original, 'sha256': fixture['sha256']}
            paths.append((name, fixture['sha256']))
        source, dependency = [name for name, _ in paths]
        edges = [{'importer': source, 'target': dependency, 'specifier': './math.zry',
                  'imported': imported, 'local': imported}]
        case(label, source, 'data-ownership-v1', 3, paths, edges)
    assert len(result) == 9 and len(generated) == 6
    return result, generated


def _inventory(directory):
    _real(directory, True)
    found, directories = {}, set()

    def walk(parent):
        for path in sorted(parent.iterdir()):
            name = path.relative_to(directory).as_posix()
            _relative(name)
            if path.is_dir():
                _real(path, True)
                directories.add(name)
                walk(path)
            else:
                _real(path, False)
                assert path.resolve().is_relative_to(directory.resolve())
                found[name] = {'bytes': path.stat().st_size, 'sha256': _digest(path)}
    walk(directory)
    assert directories == set(TARGETS), 'unexpected bundle directory inventory'
    return found


def _formats(directory, paths):
    javascript = _file(directory, paths[0]).read_bytes().decode('utf-8')
    assert 'export ' in javascript, 'missing ECMAScript module export'
    with _file(directory, paths[1]).open('rb') as stream:
        assert stream.read(8) == b'\0asm\x01\0\0\0', 'invalid core Wasm header'
    with _file(directory, paths[2]).open('rb') as stream:
        elf = stream.read(64)
    assert len(elf) == 64 and elf[:16] == b'\x7fELF\x02\x01\x01' + b'\0' * 9
    assert int.from_bytes(elf[16:18], 'little') == 1, 'native artifact is not relocatable'
    assert int.from_bytes(elf[18:20], 'little') == 62, 'native artifact is not x86-64'
    assert int.from_bytes(elf[20:24], 'little') == 1
    assert int.from_bytes(elf[52:54], 'little') == 64, 'invalid ELF64 header size'


def verify_bundle(directory, label, contract, inventory, success):
    """Assert the complete bounded build shape and retained bytes; no runtime/IR authority."""
    assert __debug__
    assert label in ('m1', 'm2', *(f'm3-{name}' for name in (*SINGLES, *CONTEXTS)))
    assert type(contract) is dict and set(contract) == {'source', 'profile', 'version', 'sources', 'edges'}
    version = contract['version']
    _exact(version, 1 if label == 'm1' else 2 if label == 'm2' else 3)
    _exact(contract['profile'], 'i32-v1' if version == 1 else 'control-flow-v1' if version == 2 else 'data-ownership-v1')
    _relative(contract['source'])
    assert contract['sources'][0]['path'] == contract['source']
    stem, manifest_name = 'private-smoke-' + label, f'zryna-manifest-v{version}.json'
    paths = [f'{target}/{stem}.{extension}' for target, extension in zip(TARGETS, EXTENSIONS)]
    directory = Path(directory).absolute()
    actual = _inventory(directory)
    assert set(actual) == {manifest_name, *paths}, 'incomplete or unexpected four-file bundle'
    assert all(0 < row['bytes'] <= 32 * 1024 * 1024 for row in actual.values())
    _exact(inventory, actual)
    _formats(directory, paths)
    _exact(success, {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [],
                     'ok': True, 'manifest': f'.zryna/out/{stem}.build/{manifest_name}'})
    manifest = _json(_file(directory, manifest_name))
    artifacts = []
    for target, kind, path in zip(TARGETS, KINDS, paths):
        row = {'target': target, 'kind': kind, **actual[path]}
        if version < 3:
            row['path'] = path
        else:
            row['filename'] = PurePosixPath(path).name
            metadata = {'profile': 'ecmascript-module', 'scalar_abi': 'v1'}
            if target == 'webassembly':
                metadata = {'profile': 'core-webassembly', 'memory': 'linear32-v1',
                            'imports': False, 'scalar_abi': 'v1'}
            elif target == 'native':
                metadata = {'profile': 'linux-x86-64-elf', 'triple': 'x86_64-unknown-linux-gnu',
                            'format': 'elf-relocatable', 'scalar_abi': 'v1',
                            'program_object_sha256': actual[path]['sha256'],
                            'runtime_object_sha256': None, 'harness_sha256': None}
            row['metadata'] = metadata
        artifacts.append(row)
    expected = {'version': version, 'profile': 'zryna-m1-cli-v1' if version == 1 else
                'zryna-control-flow-v1' if version == 2 else 'zryna-data-ownership-v1',
                'command': 'build', 'entrypoint': contract['source'], 'stem': stem,
                'targets': list(TARGETS), 'artifacts': artifacts, 'invocation': None,
                'results': [], 'diagnostics': []}
    if version == 1:
        _exact(contract['edges'], [])
        assert len(contract['sources']) == 1
        _hex(contract['sources'][0]['sha256'])
        expected['source_sha256'] = contract['sources'][0]['sha256']
    else:
        expected.update(sources=contract['sources'], edges=contract['edges'],
                        graph_sha256=graph_digest(version, contract['sources'], contract['edges']))
    if version == 3:
        layouts, runtime = manifest['layouts'], manifest['runtime_abi']
        assert type(layouts) is dict and set(layouts) == {'type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256'}
        for value in layouts.values():
            _hex(value)
        assert type(runtime) is dict and set(runtime) == {'identifier', 'version', 'sha256'}
        _hex(runtime['sha256'])
        _exact(runtime, {'identifier': 'zryna-ownership-runtime-v1', 'version': 1, 'sha256': runtime['sha256']})
        expected.update(protocol_version=4, layouts=layouts, runtime_abi=runtime)
    _exact(manifest, expected)
