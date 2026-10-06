"""Three frozen accepted M2 entry roots; no new profile or runtime authority."""
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('m2_manifest_helpers',
    Path(__file__).resolve().parent.parent/'production-manifests/manifest.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
REGISTRY = 'tests/m2-conformance-v1.json'
REGISTRY_SHA = 'cf07d765c26364cd127b8fdba7d6cefec535876b71916402624c6d77c1140c18'
CASES = {
    'm2-bare-import-dependency': 'tests/m2-fixtures/invalid/bare-import/dep.zry',
    'm2-case-colliding-dependency': 'tests/m2-fixtures/invalid/case-colliding-module/Dep.zry',
    'm2-valid-math-entry': 'tests/m2-fixtures/valid/math.zry',
}
FROZEN = {
    'm2-bare-import-dependency': '8e8f9d38f3ab6b459434289bca4889bc866f9f70805dbebe5005a1bb3b4d34dc',
    'm2-case-colliding-dependency': '8e8f9d38f3ab6b459434289bca4889bc866f9f70805dbebe5005a1bb3b4d34dc',
    'm2-valid-math-entry': 'a833600f480d718c86be001d55644b5e06a2d0cf9c52520f39fdc26e8185328c',
}
GRAPHS = {
    'm2-bare-import-dependency': 'e2048bc016e197a293483c2b99634111f20c1e8056fb4f2c6132b3cbdfd02f60',
    'm2-case-colliding-dependency': 'd3ac9c5e714172b864667ef80d6e7e4230625da06eda1caf16b4aa3d4e9a501f',
    'm2-valid-math-entry': '582f97c031f19c0036d8b114f5406b0e5462851aa35b34ee445cb062bdc21aa0',
}
SCOPE = {'provider_pairs': 3, 'cli_invocations': 9, 'bundle_files': 24, 'target_artifacts': 18,
         'runtime_execution': False, 'verified_ir_serialized': False,
         'layout_semantics_reconstructed': False}


def stem(label):
    assert label in CASES
    return 'production-m2-roots-'+label


def contracts(root, inputs):
    root = Path(root)
    assert m.digest(m.file(root, REGISTRY)) == inputs[REGISTRY] == REGISTRY_SHA
    registry = m.read(root/REGISTRY)
    m.exact(registry['schemaVersion'], 1)
    m.exact(registry['profile'], 'zryna-control-flow-fixed-oracle-v1')
    m.exact(registry['targetOrder'], list(m.TARGETS))
    fixtures = registry['fixtureFiles']
    assert type(fixtures) is list and len(fixtures) == 14
    by_path = {row['path']: row for row in fixtures}
    assert len(by_path) == 14
    result = {}
    for label, path in CASES.items():
        assert m.digest(m.file(root, path)) == inputs[path] == FROZEN[label]
        m.exact(by_path[path]['sha256'], FROZEN[label])
        sources = [{'id': 0, 'path': path, 'sha256': FROZEN[label]}]
        assert m.graph_digest(2, sources, []) == GRAPHS[label]
        result[label] = {'source': path, 'profile': 'control-flow-v1', 'version': 2,
                         'sources': sources, 'edges': []}
    return result


def expected_manifest(label, contract, inventory):
    m.exact(set(contract), {'source', 'profile', 'version', 'sources', 'edges'})
    m.exact(contract['source'], CASES[label])
    m.exact(contract['profile'], 'control-flow-v1')
    m.exact(contract['version'], 2)
    artifacts = []
    for target, kind, extension in zip(m.TARGETS, m.KINDS, m.EXTENSIONS):
        path = f'{target}/{stem(label)}.{extension}'
        row = inventory[path]
        artifacts.append({'target': target, 'kind': kind, 'path': path,
                          'bytes': row['bytes'], 'sha256': row['sha256']})
    return {'version': 2, 'profile': 'zryna-control-flow-v1', 'command': 'build',
            'entrypoint': contract['source'], 'graph_sha256': GRAPHS[label],
            'sources': contract['sources'], 'edges': contract['edges'], 'stem': stem(label),
            'targets': list(m.TARGETS), 'artifacts': artifacts, 'invocation': None,
            'results': [], 'diagnostics': []}


def verify_bundle(directory, label, contract, declared, success):
    directory = Path(directory)
    actual = m.inventory(directory)
    paths = [f'{target}/{stem(label)}.{extension}' for target, extension in zip(m.TARGETS, m.EXTENSIONS)]
    name = 'zryna-manifest-v2.json'
    assert set(actual) == {name, *paths}
    assert all(0 < row['bytes'] <= 32*1024*1024 for row in actual.values())
    m.exact(declared, actual)
    m.formats(directory, paths)
    m.exact(success, {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [],
                     'ok': True, 'manifest': f'.zryna/out/{stem(label)}.build/{name}'})
    expected = expected_manifest(label, contract, actual)
    m.exact(m.read(directory/name), expected)
    assert (directory/name).read_bytes() == m.canonical(expected), 'noncanonical M2 manifest bytes'
    return actual
