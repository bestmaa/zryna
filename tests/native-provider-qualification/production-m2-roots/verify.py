"""Independent complete Linux/Windows three-root BUILD proof admission."""
import argparse
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys

sys.dont_write_bytecode = True
if not __debug__:
    raise RuntimeError('optimized proof admission is forbidden')


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


c = module('three_m2_contracts', Path(__file__).with_name('contracts.py'))
b = module('three_m2_cli_authority', Path(__file__).parent.parent/'production-manifests/verify.py')
m = c.m
BUILD_LABELS = b.BUILD_LABELS
ROLES = ('native', 'create-only', 'bootstrap')
TOP = b.TOP | {'selected_environment', 'removed_node_environment_present', 'metadata_target'}
ROW = {'id', 'status', 'source', 'profile', 'files', 'initial_output_absent', 'final_output_absent',
       'success_json_exact', 'manifest_bytes_exact', 'create_only', 'node_on_path', 'pnpm_on_path',
       'before_collision_inventory', 'after_collision_inventory'}
COMMAND = {'label', 'argv', 'cwd', 'exit', 'timed_out', 'stdout', 'stderr', 'selected_environment'}


def environment(receipt, platform):
    output = b.absolute(receipt['output'], platform)
    target = str(output.parent/(output.name+'-metadata-target'))
    m.exact(receipt['metadata_target'], target)
    return {'PATH': str(output/'empty-runtime-path'), 'CARGO': receipt['tools']['cargo']['path'],
            'RUSTC': receipt['tools']['rustc']['path'], 'CARGO_NET_OFFLINE': 'true',
            'CARGO_TARGET_DIR': target}


def verify(root, output, head, platform, live=False, run_id=None, run_attempt=None):
    assert __debug__ and type(head) is str and b.HEX40.fullmatch(head)
    assert platform in ('linux', 'win32') and type(live) is bool
    root, output = Path(root).absolute(), Path(output).absolute()
    found_files, found_dirs = b.audit_tree(output)
    receipt = m.read(m.file(output, 'receipt.json'))
    assert type(receipt) is dict and set(receipt) == TOP
    for key, value in {'format': 'zryna.production-m2-roots.v1', 'status': 'passed', 'head': head,
                       'tree': b.git(root, 'rev-parse', 'HEAD^{tree}'), 'platform': platform,
                       'public_activation': False, 'installed_distribution_acceptance': False,
                       'generated_inputs': {}, 'scope': c.SCOPE,
                       'counts': {'passed': 3, 'failed': 0, 'ignored': 0},
                       'removed_node_environment_present': False}.items():
        m.exact(receipt[key], value)
    inputs = b.committed_inputs(root, head)
    m.exact(receipt['inputs'], inputs)
    native_output = b.absolute(receipt['output'], platform)
    native_root = b.absolute(receipt['root'], platform)
    assert not native_output.is_relative_to(native_root) and not native_root.is_relative_to(native_output)
    m.exact(receipt['runtime_path'], str(native_output/'empty-runtime-path'))
    assert not list((output/'empty-runtime-path').iterdir())
    selected = environment(receipt, platform)
    m.exact(receipt['selected_environment'], selected)
    for key, wanted in (('run_id', run_id), ('run_attempt', run_attempt)):
        value = receipt[key]
        assert value is None or (type(value) is str and re.fullmatch('[1-9][0-9]*', value))
        if wanted is not None:
            m.exact(value, str(wanted))
        if live and os.getenv('GITHUB_'+key.upper()) is not None:
            m.exact(value, os.environ['GITHUB_'+key.upper()])
    build_names = b.cli_build(receipt, output, head, receipt['tree'], platform, inputs)
    contracts = c.contracts(root, inputs)
    records, commands = receipt['records'], receipt['commands']
    assert type(records) is list and len(records) == 3
    assert type(commands) is list and len(commands) == 9
    assert [row['id'] for row in records] == list(c.CASES)
    expected_files = {'receipt.json', *(f'cli-build/{name}' for name in build_names)}
    for index, label in enumerate(c.CASES):
        row, contract = records[index], contracts[label]
        assert type(row) is dict and set(row) == ROW
        for key, value in {'id': label, 'status': 'passed', 'source': contract['source'],
                           'profile': 'control-flow-v1', 'initial_output_absent': True,
                           'final_output_absent': True, 'success_json_exact': True,
                           'manifest_bytes_exact': True, 'create_only': True,
                           'node_on_path': False, 'pnpm_on_path': False}.items():
            m.exact(row[key], value)
        for key in ('before_collision_inventory', 'after_collision_inventory'):
            m.exact(row[key], row['files'])
        native_stdout = None
        for role_index, role in enumerate(ROLES):
            command = commands[3*index+role_index]
            assert type(command) is dict and set(command) == COMMAND
            name = label+'-'+role
            for key, value in {'label': name, 'cwd': receipt['root'],
                               'exit': 4 if role == 'create-only' else 0,
                               'timed_out': False, 'selected_environment': selected}.items():
                m.exact(command[key], value)
            binary = receipt['binaries']['default' if role == 'bootstrap' else 'feature']['path']
            argv = [binary, 'build', contract['source'], '--root', receipt['root'], '--name',
                    c.stem(label), '--target', 'all', '--json', '--profile', 'control-flow-v1']
            argv += ['--node', receipt['tools']['node']['path']] if role == 'bootstrap' else ['--native-frontend']
            m.exact(command['argv'], argv)
            logs = {}
            for suffix in ('stdout', 'stderr'):
                log_name = name+'.'+suffix
                expected_files.add(log_name)
                path = m.file(output, log_name)
                m.exact(command[suffix], {'bytes': path.stat().st_size, 'sha256': m.digest(path)})
                logs[suffix] = path.read_bytes()
            m.exact(logs['stderr'], b'')
            parsed = m.strict(logs['stdout'])
            if role == 'create-only':
                b.collision(parsed)
            else:
                bundle = label+'-'+role+'-bundle'
                found = c.verify_bundle(output/bundle, label, contract, row['files'], parsed)
                expected_files.update(bundle+'/'+name for name in found)
                if role == 'native':
                    native_stdout = logs['stdout']
                else:
                    m.exact(logs['stdout'], native_stdout)
        for name in row['files']:
            m.exact(m.file(output/(label+'-native-bundle'), name).read_bytes(),
                    m.file(output/(label+'-bootstrap-bundle'), name).read_bytes())
    assert len(expected_files) == 57 and found_files == expected_files
    expected_dirs = {'empty-runtime-path'}
    for name in expected_files:
        expected_dirs.update(str(parent) for parent in PurePosixPath(name).parents if str(parent) != '.')
    assert found_dirs == expected_dirs
    if live:
        assert platform == sys.platform and Path(receipt['root']).resolve() == root.resolve()
        assert Path(receipt['output']).resolve() == output.resolve()
        assert Path(receipt['python']).absolute() == Path(sys.executable).absolute()
        for row in [*receipt['tools'].values(), *receipt['binaries'].values()]:
            assert m.digest(Path(row['path'])) == row['sha256']
        for label in c.CASES:
            try:
                (root/'.zryna/out'/(c.stem(label)+'.build')).lstat()
            except FileNotFoundError:
                continue
            raise AssertionError('final production output remains')
    return {'status': 'passed', 'head': head, 'tree': receipt['tree'], 'platform': platform,
            'counts': receipt['counts'], 'scope': c.SCOPE, 'run_id': receipt['run_id'],
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
    print(json.dumps(verify(args.root, args.output, args.head, args.platform, args.live,
                            args.run_id, args.run_attempt)))


if __name__ == '__main__':
    main()
