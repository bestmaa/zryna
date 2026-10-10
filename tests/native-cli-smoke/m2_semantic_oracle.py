"""Independent frozen one-case M2 BUILD manifest oracle; no compiler or runtime execution.

The normative registry supplies artifact identities, never the candidate bundle.
The v2 wire contract has no v3 layout/runtime ABI fields. This does not establish
exhaustive manifest parity, verified IR, runtime outcomes or installed activation.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess

if not __debug__:
    raise RuntimeError('optimized semantic oracle is forbidden')

SUBJECT_HEAD = '3f23fb7455fa345e14929898b9fa05aceb3b8685'
SUBJECT_TREE = '562822fbc1f30165492b6444fc401fbd8c7a3e78'
SEAL_SHA256 = '087294fdeac915255a48dcc5175b48c6fecc4a821d464c281f5b7ffd936f283c'
PINS = {
    'tests/m2-conformance-v1.json': 'cf07d765c26364cd127b8fdba7d6cefec535876b71916402624c6d77c1140c18',
    'docs/M2_MANIFEST_V2.md': '3c3388830fbf37e21ef556497c7f3e65dcc300c4efa980e0f9cb2db3165a5069',
    'spec/language/CONTROL_FLOW_MODULES_V1.md': 'd4756db36365d95a9f34650cfa0f0ac1d7053ee2ce45e1926c4c80432b8ac905',
}
SOURCE_NAMES = ('tests/m2-fixtures/valid/main.zry', 'tests/m2-fixtures/valid/math.zry')
STEM = 'private-smoke-m2'
TARGETS = ('javascript', 'webassembly', 'native')
KINDS = ('ecmascript-module', 'core-webassembly-module', 'linux-x86-64-relocatable-object')
EXTENSIONS = ('mjs', 'wasm', 'o')
MAX_BYTES = 32 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(path, directory=False):
    metadata = path.lstat()
    require(not stat.S_ISLNK(metadata.st_mode) and not getattr(metadata, 'st_file_attributes', 0) & 0x400,
            'symlink or reparse proof path')
    require((stat.S_ISDIR if directory else stat.S_ISREG)(metadata.st_mode), 'unexpected proof file type')


def file(root, relative):
    require(type(relative) is str and re.fullmatch(r'[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*', relative),
            'invalid relative proof path')
    parts = relative.split('/')
    require(all(part not in ('.', '..') for part in parts), 'escaping proof path')
    for parent in (root, *root.parents):
        regular(parent, True)
    path = root
    for index, part in enumerate(parts):
        path = path / part
        regular(path, index != len(parts) - 1)
    require(path.stat().st_size <= MAX_BYTES, 'proof file exceeds bound')
    return path


def pairs(rows):
    result = {}
    for key, value in rows:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def decode(raw):
    require(len(raw) <= MAX_BYTES, 'JSON exceeds bound')
    def no_float(value):
        raise ValueError('noninteger JSON number')
    return json.loads(raw, object_pairs_hook=pairs, parse_float=no_float, parse_constant=no_float)


def same(actual, expected):
    require(type(actual) is type(expected), 'JSON type differs')
    if type(expected) is dict:
        require(set(actual) == set(expected), 'JSON fields differ')
        for key in expected:
            same(actual[key], expected[key])
    elif type(expected) is list:
        require(len(actual) == len(expected), 'JSON array length differs')
        for item, wanted in zip(actual, expected):
            same(item, wanted)
    else:
        require(actual == expected, 'JSON semantic value differs')


def canonical(manifest):
    return (json.dumps(manifest, ensure_ascii=False, indent=2) + '\n').encode('utf-8')


def expected_manifest(repo):
    """Derive all thirteen fields from pinned contracts and exact frozen source bytes."""
    repo = Path(repo).absolute()
    for name, digest in PINS.items():
        require(sha(file(repo, name)) == digest, 'normative authority changed: ' + name)
    registry = decode(file(repo, 'tests/m2-conformance-v1.json').read_bytes())
    same(registry['schemaVersion'], 1)
    same(registry['targetOrder'], list(TARGETS))
    graph = registry['graph']
    sources = [{'id': index, 'path': name, 'sha256': sha(file(repo, name))}
               for index, name in enumerate(SOURCE_NAMES)]
    same(sources, graph['sources'])
    # The pinned entry fixture has one named import; no provider or candidate is queried.
    text = file(repo, SOURCE_NAMES[0]).read_text(encoding='utf-8')
    imports = re.findall(r'^import \{ ([A-Za-z][A-Za-z0-9]*) \} from "(\./[A-Za-z]+\.zry)";', text, re.M)
    same(imports, [('addPair', './math.zry')])
    edges = [{'importer': SOURCE_NAMES[0], 'target': SOURCE_NAMES[1], 'specifier': imports[0][1],
              'imported': imports[0][0], 'local': imports[0][0]}]
    same(edges, graph['edges'])  # target is normative although not included in the graph hash.
    encoded = bytearray(b'ZRYNA-M2-GRAPH\0')
    def integer(value):
        encoded.extend(value.to_bytes(4, 'little'))
    def string(value):
        raw = value.encode('utf-8'); integer(len(raw)); encoded.extend(raw)
    integer(1); string(SOURCE_NAMES[0]); integer(len(sources))
    for source in sources:
        string(source['path']); encoded.extend(bytes.fromhex(source['sha256']))
    integer(len(edges))
    for edge in edges:
        for key in ('importer', 'specifier', 'imported', 'local'):
            string(edge[key])
    graph_sha = hashlib.sha256(encoded).hexdigest()
    same(graph_sha, graph['sha256'])
    artifacts = []
    require(len(graph['buildArtifacts']) == 3, 'frozen artifact census differs')
    for target, kind, extension, frozen in zip(TARGETS, KINDS, EXTENSIONS, graph['buildArtifacts']):
        same(frozen['target'], target); same(frozen['kind'], kind)
        require(type(frozen['bytes']) is int and frozen['bytes'] > 0, 'invalid frozen byte count')
        require(type(frozen['sha256']) is str and re.fullmatch('[0-9a-f]{64}', frozen['sha256']), 'invalid frozen digest')
        artifacts.append({'target': target, 'kind': kind, 'path': f'{target}/{STEM}.{extension}',
                          'bytes': frozen['bytes'], 'sha256': frozen['sha256']})
    return {'version': 2, 'profile': 'zryna-control-flow-v1', 'command': 'build',
            'entrypoint': SOURCE_NAMES[0], 'graph_sha256': graph_sha, 'sources': sources, 'edges': edges,
            'stem': STEM, 'targets': list(TARGETS), 'artifacts': artifacts,
            'invocation': None, 'results': [], 'diagnostics': []}


def verify_manifest(repo, raw):
    expected = expected_manifest(repo)
    same(decode(raw), expected)
    require(raw == canonical(expected), 'noncanonical manifest wire bytes/order')
    return expected


def verify_bundle(repo, bundle):
    bundle = Path(bundle).absolute()
    expected = expected_manifest(repo)
    regular(bundle, True)
    same({path.name for path in bundle.iterdir()}, {'zryna-manifest-v2.json', *TARGETS})
    for row in expected['artifacts']:
        directory = bundle / row['target']
        regular(directory, True)
        same({path.name for path in directory.iterdir()}, {Path(row['path']).name})
    for row in expected['artifacts']:
        artifact = file(bundle, row['path'])
        require(artifact.stat().st_size == row['bytes'] and sha(artifact) == row['sha256'],
                'artifact differs from independently frozen M2 bytes: ' + row['target'])
    manifest = file(bundle, 'zryna-manifest-v2.json')
    verify_manifest(repo, manifest.read_bytes())
    return {'manifest_sha256': sha(manifest), 'manifest_bytes': manifest.stat().st_size,
            'artifacts': expected['artifacts']}


def qualify(repo, checkpoint):
    """Admit the retained local/Linux/Windows pairs with exact sealed old-source binding."""
    repo, checkpoint = Path(repo).absolute(), Path(checkpoint).absolute()
    require(subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip() == SUBJECT_HEAD,
            'subject source head differs')
    require(subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD^{tree}'], text=True).strip() == SUBJECT_TREE,
            'subject source tree differs')
    require(not subprocess.check_output(['git', '-C', str(repo), 'status', '--porcelain=v1'], text=True).strip(),
            'subject source checkout dirty')
    seal_file = file(checkpoint, 'file-hashes.json')
    require(sha(seal_file) == SEAL_SHA256, 'checkpoint seal differs')
    seal = decode(seal_file.read_bytes())
    same(seal['head'], SUBJECT_HEAD); same(seal['tree'], SUBJECT_TREE)
    sealed = {row['path']: row for row in seal['files']}
    require(len(sealed) == len(seal['files']), 'duplicate sealed path')
    def retained(relative):
        path = file(checkpoint, relative)
        record = sealed[relative]
        require(path.stat().st_size == record['bytes'] and sha(path) == record['sha256'], 'sealed proof differs')
        return path
    hosted = decode(retained('hosted/final-activation-status.json').read_bytes())
    same(hosted['head'], SUBJECT_HEAD); same(hosted['tree'], SUBJECT_TREE)
    same(hosted['run_id'], 37255154089); same(hosted['run_attempt'], 1); same(hosted['status'], 'passed')
    qualifications = []
    for prefix, platform, kind in (('final-cli', 'linux', 'local'),
                                   ('hosted/linux-proof', 'linux', 'hosted'),
                                   ('hosted/windows-proof', 'win32', 'hosted')):
        ci = decode(retained(prefix + '/ci-receipt.json').read_bytes())
        smoke = decode(retained(prefix + '/smoke/receipt.json').read_bytes())
        for receipt in (ci, smoke):
            same(receipt['head'], SUBJECT_HEAD); same(receipt['tree'], SUBJECT_TREE)
        same(ci['platform'], platform); same(ci['status'], 'passed')
        same(ci['public_activation'], False); same(ci['installed_distribution_acceptance'], False)
        same(smoke['counts'], {'passed': 21, 'failed': 0, 'ignored': 0})
        for name, digest in {**PINS, **{row['path']: row['sha256'] for row in expected_manifest(repo)['sources']}}.items():
            same(ci['inputs'][name], digest); same(smoke['inputs'][name], digest)
        records = [row for row in smoke['records'] if row['id'] == 'm2']
        require(len(records) == 1 and records[0]['status'] == 'passed', 'M2 smoke record absent/ambiguous')
        expected_success = {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [], 'ok': True,
                            'manifest': f'.zryna/out/{STEM}.build/zryna-manifest-v2.json'}
        providers = []
        for provider, bundle_name, stdout_name in (('bootstrap', 'm2-bootstrap-bundle', 'm2-bootstrap.stdout'),
                                                   ('native', 'm2-native-bundle', 'm2-native.stdout')):
            bundle_relative = prefix + '/smoke/' + bundle_name
            result = verify_bundle(repo, checkpoint / bundle_relative)
            for child in (checkpoint / bundle_relative).rglob('*'):
                if child.is_file(): retained(child.relative_to(checkpoint).as_posix())
            success = decode(retained(prefix + '/smoke/' + stdout_name).read_bytes())
            same(success, expected_success)
            expected_files = {row['path']: {'bytes': row['bytes'], 'sha256': row['sha256']}
                              for row in result['artifacts']}
            expected_files['zryna-manifest-v2.json'] = {'bytes': result['manifest_bytes'], 'sha256': result['manifest_sha256']}
            same(records[0]['detail']['files'], expected_files)
            providers.append({'provider': provider, **result})
        binding = {}
        if kind == 'hosted':
            live = [row for row in hosted['current_private_cli'] if row['platform'] == platform]
            require(len(live) == 1, 'hosted platform proof absent/ambiguous')
            same(live[0]['on_host_admission']['ci_receipt_sha256'], sha(retained(prefix + '/ci-receipt.json')))
            same(live[0]['on_host_admission']['smoke_receipt_sha256'], sha(retained(prefix + '/smoke/receipt.json')))
            binding = {'run_id': hosted['run_id'], 'run_attempt': hosted['run_attempt'], 'job_id': live[0]['job_id']}
        qualifications.append({'kind': kind, 'platform': platform, 'proof_prefix': prefix, **binding,
                               'ci_receipt_sha256': sha(retained(prefix + '/ci-receipt.json')),
                               'smoke_receipt_sha256': sha(retained(prefix + '/smoke/receipt.json')),
                               'providers': providers})
    return {'format': 'zryna.one-m2-manifest-semantic-oracle.v1', 'subject_head': SUBJECT_HEAD,
            'subject_tree': SUBJECT_TREE, 'checkpoint_seal_sha256': SEAL_SHA256,
            'normative_input_sha256': PINS, 'source_case': list(SOURCE_NAMES), 'manifest_fields': 13,
            'qualifications': qualifications, 'new_compiler_executions': 0,
            'full_production_manifest_parity': False, 'corpus_blocked': 30,
            'installed_acceptance': False, 'public_activation': False,
            'scope': 'One successful M2 BUILD content/wire case, both providers, all three frozen artifacts; excludes run outcomes, atomic publication history, IR reconstruction, M3 schemas and installed activation'}


def oracle_revision(repo, module):
    def git(*arguments):
        return subprocess.check_output(['git', '-C', str(repo), *arguments])
    committed = git('show', 'HEAD:' + module)
    require(not git('status', '--porcelain=v1').strip(), 'oracle source checkout dirty')
    require(hashlib.sha256(committed).hexdigest() == sha(file(repo, module)),
            'oracle module differs from committed source')
    return {'head': git('rev-parse', 'HEAD').decode().strip(),
            'tree': git('rev-parse', 'HEAD^{tree}').decode().strip(), 'clean': True,
            'module_path': module, 'module_matches_commit': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-repo', type=Path, required=True)
    parser.add_argument('--checkpoint', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    oracle_repo = Path(__file__).absolute().parents[2]
    module = Path(__file__).absolute().relative_to(oracle_repo).as_posix()
    output = args.output.absolute()
    require('..' not in output.parts, 'parent traversal in output path')
    for root in (args.source_repo.absolute(), args.checkpoint.absolute(), oracle_repo):
        require('..' not in root.parts, 'parent traversal in input root')
        require(output != root and root not in output.parents, 'output must be outside retained inputs')
    for parent in output.parents:
        regular(parent, True)
    result = qualify(args.source_repo, args.checkpoint)
    result['oracle_sha256'] = sha(Path(__file__))
    result['oracle_revision'] = oracle_revision(oracle_repo, module)
    with output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2); stream.write('\n')
    print(json.dumps({'subject_head': SUBJECT_HEAD, 'qualified_pairs': len(result['qualifications']),
                      'manifest_fields': 13, 'corpus_blocked': 30, 'new_compiler_executions': 0}))


if __name__ == '__main__':
    main()
