"""Independent seven-case production BUILD archive/live admission; never activation."""
import argparse
import hashlib
import importlib.util
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import stat
import subprocess
import sys

sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('production_manifest_contract', Path(__file__).with_name('manifest.py'))
m = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(m)
exact, strict, digest, read = m.exact, m.strict, m.digest, m.read
IDS = tuple('m3-'+case for case in m.CASES)
ROLES = ('native', 'create-only', 'bootstrap')
HEX40 = re.compile('[0-9a-f]{40}')
BUILD_LABELS = ('cargo-version', 'rustc-version', 'node-version', 'default-build', 'feature-build', 'cli-smoke')
TOP = {'format', 'head', 'tree', 'platform', 'root', 'output', 'cli_proof', 'cli_receipt_sha256',
       'inputs', 'generated_inputs', 'tools', 'binaries', 'cli_build_files', 'python', 'runtime_path',
       'commands', 'records', 'status', 'run_id', 'run_attempt', 'public_activation',
       'installed_distribution_acceptance', 'scope', 'counts'}
SCOPE = {'provider_pairs': 7, 'cli_invocations': 21, 'bundle_files': 56, 'target_artifacts': 42,
         'runtime_execution': False, 'layout_semantics_reconstructed': False}


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args]).decode().strip()


def committed_inputs(root, head):
    """Read actual committed blobs, rejecting dirty and assume-unchanged working bytes."""
    assert git(root, 'rev-parse', 'HEAD') == head and not git(root, 'status', '--porcelain')
    rows = subprocess.check_output(['git', '-C', str(root), 'ls-tree', '-r', '-z', head]).split(b'\0')
    entries = []
    for raw in rows:
        if not raw:
            continue
        prefix, name = raw.split(b'\t', 1)
        mode, kind, oid = prefix.split()
        assert mode in (b'100644', b'100755') and kind == b'blob', 'nonregular committed source'
        entries.append((name.decode(), oid))
    assert entries
    output = subprocess.check_output(['git', '-C', str(root), 'cat-file', '--batch'],
                                     input=b''.join(oid+b'\n' for _, oid in entries))
    offset, result = 0, {}
    for name, oid in entries:
        end = output.index(b'\n', offset)
        identity, kind, length = output[offset:end].split()
        assert identity == oid and kind == b'blob'
        offset = end+1
        raw = output[offset:offset+int(length)]
        offset += int(length)
        assert output[offset:offset+1] == b'\n'
        offset += 1
        path = m.file(root, name)
        assert path.read_bytes() == raw, 'working source differs from committed blob'
        result[name] = hashlib.sha256(raw).hexdigest()
    assert offset == len(output)
    return result


def audit_tree(root):
    count, files, directories = 0, set(), set()
    for parent in root.absolute().parents:
        m.real(parent, True)

    def walk(path):
        nonlocal count
        count += 1
        assert count <= 512, 'proof tree exceeds bounded inventory'
        info = path.lstat()
        assert not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400
        assert stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)
        if stat.S_ISDIR(info.st_mode):
            if path != root:
                directories.add(path.relative_to(root).as_posix())
            for child in path.iterdir():
                walk(child)
        else:
            assert info.st_size <= 32*1024*1024
            files.add(path.relative_to(root).as_posix())
    walk(root)
    return files, directories


def absolute(value, platform):
    assert type(value) is str and value
    result = (PureWindowsPath if platform == 'win32' else PurePosixPath)(value)
    assert result.is_absolute() and '..' not in result.parts
    return result


def collision(value):
    exact(value, {'version': 1, 'command': 'build', 'manifest': None, 'ok': False, 'results': [],
                  'diagnostics': [{'code': 'ZRYNA-C1009', 'guidance':
                      'use a writable real output filesystem and a fresh artifact stem',
                      'message': 'create-only output bundle already exists',
                      'primary': {'kind': 'global'}, 'severity': 'error'}]})


def cli_build(receipt, output, head, tree, platform, inputs):
    directory = output/'cli-build'
    names = {'ci-receipt.json', 'smoke/receipt.json',
             *(label+'.'+suffix for label in BUILD_LABELS for suffix in ('stdout', 'stderr'))}
    actual = {name: {'bytes': m.file(directory, name).stat().st_size,
                     'sha256': digest(m.file(directory, name))} for name in names}
    exact(receipt['cli_build_files'], actual)
    prior = read(m.file(directory, 'ci-receipt.json'))
    assert set(prior) == {'format', 'head', 'tree', 'platform', 'root', 'target', 'status', 'inputs',
                          'tools', 'binaries', 'commands', 'smoke_receipt', 'public_activation',
                          'installed_distribution_acceptance', 'python'}
    m.hexadecimal(receipt['cli_receipt_sha256'])
    assert digest(directory/'ci-receipt.json') == receipt['cli_receipt_sha256']
    for key, wanted in {'format': 'zryna.private-cli-ci.v1', 'head': head, 'tree': tree,
                        'platform': platform, 'status': 'passed', 'inputs': inputs,
                        'root': receipt['root'], 'tools': receipt['tools'], 'binaries': receipt['binaries'],
                        'python': receipt['python'], 'smoke_receipt': 'smoke/receipt.json',
                        'public_activation': False, 'installed_distribution_acceptance': False}.items():
        exact(prior[key], wanted)
    assert type(prior['tools']) is dict and set(prior['tools']) == {'node', 'cargo', 'rustc'}
    assert type(prior['binaries']) is dict and set(prior['binaries']) == {'default', 'feature'}
    root_path, prior_path = absolute(receipt['root'], platform), absolute(receipt['cli_proof'], platform)
    target = absolute(prior['target'], platform)
    proof_path = absolute(receipt['output'], platform)
    for left, right in ((target, root_path), (target, proof_path), (target, prior_path),
                        (proof_path, root_path), (proof_path, prior_path), (prior_path, root_path)):
        assert not left.is_relative_to(right) and not right.is_relative_to(left)
    python = absolute(prior['python'], platform)
    for name, row in prior['tools'].items():
        assert set(row) == {'path', 'sha256', 'version'}
        absolute(row['path'], platform)
        m.hexadecimal(row['sha256'])
        assert type(row['version']) is str
        prefix = {'cargo': 'cargo 1.97.1 ', 'rustc': 'rustc 1.97.1 ', 'node': 'v22.22.1'}[name]
        assert row['version'] == prefix if name == 'node' else row['version'].startswith(prefix)
        assert m.file(directory, name+'-version.stdout').read_text().strip() == row['version']
    suffix = '.exe' if platform == 'win32' else ''
    for name, row in prior['binaries'].items():
        assert set(row) == {'path', 'sha256'}
        m.hexadecimal(row['sha256'])
        assert absolute(row['path'], platform) == prior_path/(name+'-cli'+suffix)
    commands = prior['commands']
    assert type(commands) is list and len(commands) == 6
    for index, label in enumerate(BUILD_LABELS):
        command = commands[index]
        assert set(command) == {'label', 'argv', 'exit', 'cwd', 'target'}
        exact(command['label'], label)
        exact(command['exit'], 0)
        exact(command['cwd'], receipt['root'])
        exact(command['target'], prior['target'])
        if index < 3:
            argv = [prior['tools'][label.removesuffix('-version')]['path'], '--version']
        elif index < 5:
            argv = [prior['tools']['cargo']['path'], 'build', '--locked', '--offline', '-p', 'zryna']
            if index == 4:
                argv += ['--features', 'native-provider-internal']
        else:
            argv = [str(python), '-B', str(root_path/'scripts/run-native-cli-smoke.py'),
                    '--root', receipt['root'], '--default-cli', prior['binaries']['default']['path'],
                    '--feature-cli', prior['binaries']['feature']['path'], '--node', prior['tools']['node']['path'],
                    '--cargo', prior['tools']['cargo']['path'], '--rustc', prior['tools']['rustc']['path'],
                    '--output', str(prior_path/'smoke')]
        exact(command['argv'], argv)
    smoke = read(directory/'smoke/receipt.json')
    assert set(smoke) == {'version', 'head', 'tree', 'inputs', 'generated_inputs', 'binaries', 'path',
                          'records', 'counts', 'blocked_acceptance', 'public_activation'}
    for key, value in {'version': 1, 'head': head, 'tree': tree, 'inputs': inputs,
                       'counts': {'passed': 21, 'failed': 0, 'ignored': 0}, 'public_activation': False}.items():
        exact(smoke[key], value)
    previous_ids = {'m1', 'm2', 'm3-pair', 'm3-array', 'm3-borrow', 'm3-vec', 'm3-string',
                    'm3-owned-aggregate', 'm3-owned-vec', 'm1-negative', 'm1-bool', 'm3-moved',
                    'm2-bare-import', 'm2-cycle-main', 'm2-cycle-dep', 'default-feature-disabled',
                    'ordinary-feature-build-needs-node', 'private-project-denied',
                    'private-component-denied', 'source-checkout-still-needs-cargo', 'source-and-binary-identity'}
    assert type(smoke['records']) is list and len(smoke['records']) == 21
    assert {row['id'] for row in smoke['records']} == previous_ids
    for row in smoke['records']:
        assert set(row) == ({'id', 'status'} if row['id'] == 'source-and-binary-identity'
                            else {'id', 'status', 'detail'})
        exact(row['status'], 'passed')
    exact(smoke['binaries'], {
        'default_cli': prior['binaries']['default'], 'feature_cli': prior['binaries']['feature'],
        **{name: {key: prior['tools'][name][key] for key in ('path', 'sha256')}
           for name in ('node', 'cargo', 'rustc')}})
    assert absolute(smoke['path'], platform) == prior_path/'smoke/empty-path'
    exact(smoke['blocked_acceptance'], ['ordinary installed CLI without Node/pnpm/Cargo',
          'public activation', 'native run selection', 'cross-platform installed distribution proof'])
    exact(strict(m.file(directory, 'cli-smoke.stdout').read_bytes()),
          {key: smoke[key] for key in ('head', 'counts', 'blocked_acceptance', 'public_activation')})
    return names


def verify(root, output, head, platform, live=False, run_id=None, run_attempt=None):
    assert __debug__ and type(head) is str and HEX40.fullmatch(head)
    assert platform in ('linux', 'win32') and type(live) is bool
    root, output = Path(root).absolute(), Path(output).absolute()
    found_files, found_directories = audit_tree(output)
    receipt = read(m.file(output, 'receipt.json'))
    assert type(receipt) is dict and set(receipt) == TOP
    exact(receipt['format'], 'zryna.production-manifest-parity.v1')
    exact(receipt['status'], 'passed')
    exact(receipt['head'], head)
    exact(receipt['tree'], git(root, 'rev-parse', 'HEAD^{tree}'))
    exact(receipt['platform'], platform)
    actual = committed_inputs(root, head)
    exact(receipt['inputs'], actual)
    for key in ('public_activation', 'installed_distribution_acceptance'):
        exact(receipt[key], False)
    exact(receipt['scope'], SCOPE)
    exact(receipt['counts'], {'passed': 7, 'failed': 0, 'ignored': 0})
    native_output = absolute(receipt['output'], platform)
    native_root = absolute(receipt['root'], platform)
    assert not native_output.is_relative_to(native_root) and not native_root.is_relative_to(native_output)
    assert absolute(receipt['runtime_path'], platform) == native_output/'empty-runtime-path'
    assert not list((output/'empty-runtime-path').iterdir())
    for field, expected in (('run_id', run_id), ('run_attempt', run_attempt)):
        value = receipt[field]
        assert value is None or (type(value) is str and re.fullmatch('[1-9][0-9]*', value))
        if expected is not None:
            exact(value, str(expected))
        if live and os.getenv('GITHUB_'+field.upper()) is not None:
            exact(value, os.environ['GITHUB_'+field.upper()])
    build_names = cli_build(receipt, output, head, receipt['tree'], platform, actual)
    contracts, generated = m.contracts(root, native_output, actual)
    exact(receipt['generated_inputs'], generated)
    records = receipt['records']
    assert type(records) is list and len(records) == 7
    assert [row['id'] for row in records] == list(IDS)
    commands = receipt['commands']
    assert type(commands) is list and len(commands) == 21
    expected_files = {'receipt.json', *(f'cli-build/{name}' for name in build_names)}
    for index, label in enumerate(IDS):
        row, contract = records[index], contracts[label]
        assert set(row) == {'id', 'status', 'source', 'profile', 'files', 'initial_output_absent',
                            'final_output_absent', 'success_json_exact', 'manifest_bytes_exact',
                            'create_only', 'node_on_path', 'pnpm_on_path'}
        for key, value in {'id': label, 'status': 'passed', 'source': contract['source'],
                           'profile': 'data-ownership-v1', 'initial_output_absent': True,
                           'final_output_absent': True, 'success_json_exact': True,
                           'manifest_bytes_exact': True, 'create_only': True,
                           'node_on_path': False, 'pnpm_on_path': False}.items():
            exact(row[key], value)
        outputs = []
        for role_index, role in enumerate(ROLES):
            command = commands[3*index+role_index]
            assert set(command) == {'label', 'argv', 'cwd', 'exit', 'timed_out', 'stdout', 'stderr'}
            command_label = label+'-'+role
            exact(command['label'], command_label)
            exact(command['cwd'], receipt['root'])
            exact(command['exit'], 4 if role == 'create-only' else 0)
            exact(command['timed_out'], False)
            binary = receipt['binaries']['default' if role == 'bootstrap' else 'feature']['path']
            argv = [binary, 'build', contract['source'], '--root', receipt['root'], '--name',
                    'production-manifest-'+label, '--target', 'all', '--json', '--profile', 'data-ownership-v1']
            argv += ['--node', receipt['tools']['node']['path']] if role == 'bootstrap' else ['--native-frontend']
            exact(command['argv'], argv)
            logs = {}
            for suffix in ('stdout', 'stderr'):
                name = command_label+'.'+suffix
                expected_files.add(name)
                path = m.file(output, name)
                exact(command[suffix], {'bytes': path.stat().st_size, 'sha256': digest(path)})
                logs[suffix] = path.read_bytes()
            exact(logs['stderr'], b'')
            parsed = strict(logs['stdout'])
            if role == 'create-only':
                collision(parsed)
            else:
                bundle_name = label+'-'+role+'-bundle'
                found = m.verify_bundle(output/bundle_name, label, contract, row['files'], parsed)
                expected_files.update(bundle_name+'/'+name for name in found)
                outputs.append(logs['stdout'])
        exact(outputs[0], outputs[1])
        for name in row['files']:
            exact(m.file(output/(label+'-native-bundle'), name).read_bytes(),
                  m.file(output/(label+'-bootstrap-bundle'), name).read_bytes())
    assert found_files == expected_files, 'unexpected or missing proof file'
    expected_dirs = {'empty-runtime-path'}
    for name in expected_files:
        expected_dirs.update(str(parent) for parent in PurePosixPath(name).parents if str(parent) != '.')
    assert found_directories == expected_dirs, 'unexpected proof directory'
    if live:
        assert platform == sys.platform and Path(receipt['root']).resolve() == root.resolve()
        assert Path(receipt['output']).resolve() == output.resolve()
        for row in [*receipt['tools'].values(), *receipt['binaries'].values()]:
            assert digest(Path(row['path'])) == row['sha256']
        assert Path(receipt['python']).absolute() == Path(sys.executable).absolute()
        for name, value in generated.items():
            assert digest(m.file(root, name)) == value['sha256']
        for label in IDS:
            try:
                (root/'.zryna/out'/('production-manifest-'+label+'.build')).lstat()
            except FileNotFoundError:
                continue
            raise AssertionError('final production bundle remains after recovery')
    return {'status': 'passed', 'head': head, 'tree': receipt['tree'], 'platform': platform,
            'counts': receipt['counts'], 'scope': SCOPE, 'run_id': receipt['run_id'],
            'run_attempt': receipt['run_attempt'], 'live': live, 'public_activation': False,
            'installed_distribution_acceptance': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('root', 'output'):
        parser.add_argument('--'+key, type=Path, required=True)
    parser.add_argument('--head', required=True)
    parser.add_argument('--platform', choices=('linux', 'win32'), required=True)
    parser.add_argument('--live', action='store_true')
    parser.add_argument('--run-id')
    parser.add_argument('--run-attempt')
    args = parser.parse_args()
    print(__import__('json').dumps(verify(args.root, args.output, args.head, args.platform,
                                        args.live, args.run_id, args.run_attempt)))


if __name__ == '__main__':
    main()
