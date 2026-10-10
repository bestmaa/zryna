"""Independent complete production corpus admission; acceptance and rejection stay distinct."""
import argparse
import importlib.util
import os
from pathlib import Path, PurePosixPath
import sys

sys.dont_write_bytecode = True


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


c = load('corpus_admission_contracts', Path(__file__).with_name('contracts.py'))
v = load('sealed_build_admission', Path(__file__).resolve().parents[1]/'production-manifests/verify.py')
exact, strict, digest, read = c.exact, c.strict, c.digest, c.read
TOP = v.TOP
SCOPE = c.SCOPE


def audit_tree(root):
    files, directories = set(), set()
    for parent in root.absolute().parents:
        c.m.real(parent, True)

    def walk(path):
        assert len(files)+len(directories) <= 4096, 'proof tree exceeds bounded inventory'
        directory = path.is_dir()
        c.m.real(path, directory)
        if directory:
            if path != root:
                directories.add(path.relative_to(root).as_posix())
            for child in path.iterdir():
                walk(child)
        else:
            assert path.stat().st_size <= 32*1024*1024
            files.add(path.relative_to(root).as_posix())
    walk(root)
    return files, directories


def verify(root, output, head, platform, live=False, run_id=None, run_attempt=None):
    assert __debug__ and type(head) is str and v.HEX40.fullmatch(head)
    assert platform in ('linux', 'win32') and type(live) is bool
    root, output = Path(root).absolute(), Path(output).absolute()
    found_files, found_directories = audit_tree(output)
    receipt = read(c.m.file(output, 'receipt.json'))
    assert type(receipt) is dict and set(receipt) == TOP
    exact(receipt['format'], 'zryna.production-corpus-parity.v1')
    exact(receipt['status'], 'passed')
    exact(receipt['head'], head)
    exact(receipt['tree'], v.git(root, 'rev-parse', 'HEAD^{tree}'))
    exact(receipt['platform'], platform)
    actual = v.committed_inputs(root, head)
    exact(receipt['inputs'], actual)
    exact(receipt['generated_inputs'], {})
    for key in ('public_activation', 'installed_distribution_acceptance'):
        exact(receipt[key], False)
    exact(receipt['scope'], SCOPE)
    exact(receipt['counts'], {'passed': 69, 'failed': 0, 'ignored': 0})
    native_output = v.absolute(receipt['output'], platform)
    native_root = v.absolute(receipt['root'], platform)
    assert not native_output.is_relative_to(native_root) and not native_root.is_relative_to(native_output)
    assert v.absolute(receipt['runtime_path'], platform) == native_output/'empty-runtime-path'
    assert not list((output/'empty-runtime-path').iterdir())
    for field, expected in (('run_id', run_id), ('run_attempt', run_attempt)):
        value = receipt[field]
        assert value is None or (type(value) is str and __import__('re').fullmatch('[1-9][0-9]*', value))
        if expected is not None:
            exact(value, str(expected))
        if live and os.getenv('GITHUB_'+field.upper()) is not None:
            exact(value, os.environ['GITHUB_'+field.upper()])
    build_names = v.cli_build(receipt, output, head, receipt['tree'], platform, actual)
    contracts = c.contracts(root, actual)
    rows, commands = receipt['records'], receipt['commands']
    assert type(rows) is list and len(rows) == 69
    assert [row['id'] for row in rows] == list(contracts)
    assert type(commands) is list and len(commands) == 171
    expected_files = {'receipt.json', *(f'cli-build/{name}' for name in build_names)}
    cursor = 0
    for row, (label, contract) in zip(rows, contracts.items()):
        assert set(row) == {'id', 'disposition', 'status', 'source', 'profile', 'files',
                            'initial_output_absent', 'final_output_absent', 'provider_json_exact',
                            'create_only', 'node_on_path', 'pnpm_on_path'}
        accepted = contract['disposition'] == 'accepted'
        for key, value in {'id': label, 'disposition': contract['disposition'], 'status': 'passed',
                           'source': contract['source'], 'profile': 'data-ownership-v1',
                           'initial_output_absent': True, 'final_output_absent': True,
                           'provider_json_exact': True, 'create_only': accepted,
                           'node_on_path': False, 'pnpm_on_path': False}.items():
            exact(row[key], value)
        if not accepted:
            exact(row['files'], {})
        outputs = []
        for role in (('native', 'create-only', 'bootstrap') if accepted else ('native', 'bootstrap')):
            command = commands[cursor]
            cursor += 1
            assert set(command) == {'label', 'argv', 'cwd', 'exit', 'timed_out', 'stdout', 'stderr'}
            command_label = label+'-'+role
            exact(command['label'], command_label)
            exact(command['cwd'], receipt['root'])
            exact(command['exit'], 4 if role == 'create-only' else (0 if accepted else 3))
            exact(command['timed_out'], False)
            binary = receipt['binaries']['default' if role == 'bootstrap' else 'feature']['path']
            argv = [binary, 'build', contract['source'], '--root', receipt['root'], '--name',
                    'corpus-build-'+label, '--target', 'all', '--json', '--profile', 'data-ownership-v1']
            argv += ['--node', receipt['tools']['node']['path']] if role == 'bootstrap' else ['--native-frontend']
            exact(command['argv'], argv)
            logs = {}
            for suffix in ('stdout', 'stderr'):
                name = command_label+'.'+suffix
                expected_files.add(name)
                path = c.m.file(output, name)
                exact(command[suffix], {'bytes': path.stat().st_size, 'sha256': digest(path)})
                logs[suffix] = path.read_bytes()
            exact(logs['stderr'], b'')
            value = strict(logs['stdout'])
            if role == 'create-only':
                v.collision(value)
            elif accepted:
                bundle = label+'-'+role+'-bundle'
                found = c.verify_bundle(output/bundle, label, contract, row['files'], value)
                expected_files.update(bundle+'/'+name for name in found)
                outputs.append(logs['stdout'])
            else:
                exact(value, c.failure(contract))
                outputs.append(logs['stdout'])
        exact(outputs[0], outputs[1])
        for name in row['files']:
            exact(c.m.file(output/(label+'-native-bundle'), name).read_bytes(),
                  c.m.file(output/(label+'-bootstrap-bundle'), name).read_bytes())
    exact(cursor, 171)
    assert found_files == expected_files and len(found_files) == 621, 'unexpected or missing proof file'
    expected_dirs = {'empty-runtime-path'}
    for name in expected_files:
        expected_dirs.update(str(parent) for parent in PurePosixPath(name).parents if str(parent) != '.')
    assert found_directories == expected_dirs, 'unexpected proof directory'
    if live:
        assert platform == sys.platform and Path(receipt['root']).resolve() == root.resolve()
        assert Path(receipt['output']).resolve() == output.resolve()
        for item in [*receipt['tools'].values(), *receipt['binaries'].values()]:
            assert digest(Path(item['path'])) == item['sha256']
        assert Path(receipt['python']).absolute() == Path(sys.executable).absolute()
        for label in contracts:
            try:
                (root/'.zryna/out'/('corpus-build-'+label+'.build')).lstat()
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
