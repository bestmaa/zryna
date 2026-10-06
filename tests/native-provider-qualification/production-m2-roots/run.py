"""Actual production M2 BUILD pairs on the current source-bound Linux/Windows CLI."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('m2_build_contract', Path(__file__).with_name('contracts.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def inputs(root):
    names = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z']).decode().split('\0')
    return {name: c.m.digest(root/name) for name in names if name}


def files(directory):
    c.m.real(directory, True)
    result = {}
    for path in directory.rglob('*'):
        c.m.real(path, path.is_dir())
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = {'bytes': path.stat().st_size, 'sha256': c.m.digest(path)}
    return result


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise AssertionError('refuse existing output: '+str(path))


def main():
    assert __debug__ and sys.version_info >= (3, 11) and sys.platform in ('linux', 'win32')
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('root', 'cli-proof', 'output'):
        parser.add_argument('--'+key, type=Path, required=True)
    parser.add_argument('--head', required=True)
    args = parser.parse_args()
    root, prior, output = args.root.resolve(strict=True), args.cli_proof.resolve(strict=True), args.output.absolute()
    assert git(root, 'rev-parse', 'HEAD') == args.head and not git(root, 'status', '--porcelain')
    absent(output)
    for left, right in ((output, root), (output, prior), (prior, root)):
        assert not left.resolve().is_relative_to(right) and not right.is_relative_to(left.resolve())
    before = inputs(root)
    cli = c.m.read(prior/'ci-receipt.json')
    for key, value in {'head': args.head, 'tree': git(root, 'rev-parse', 'HEAD^{tree}'),
                       'platform': sys.platform, 'root': str(root), 'status': 'passed', 'inputs': before}.items():
        c.m.exact(cli[key], value)
    contracts = c.contracts(root, before)
    identities = [*cli['tools'].values(), *cli['binaries'].values()]
    assert all(c.m.digest(Path(row['path'])) == row['sha256'] for row in identities)
    output.mkdir()
    build = output/'cli-build'
    (build/'smoke').mkdir(parents=True)
    for name in ('ci-receipt.json', 'smoke/receipt.json',
                 *(row['label']+'.'+suffix for row in cli['commands'] for suffix in ('stdout', 'stderr'))):
        shutil.copy2(prior/name, build/name)
    empty = output/'empty-runtime-path'
    empty.mkdir()
    metadata_target = str(output.parent/(output.name+'-metadata-target'))
    absent(Path(metadata_target))
    selected = {'PATH': str(empty), 'CARGO': cli['tools']['cargo']['path'],
                'RUSTC': cli['tools']['rustc']['path'], 'CARGO_NET_OFFLINE': 'true',
                'CARGO_TARGET_DIR': metadata_target}
    env = dict(os.environ, **selected)
    for key in tuple(env):
        if key.startswith(('NODE', 'NPM', 'PNPM', 'ZRYNA_NODE', 'ZRYNA_TEST_NODE')):
            env.pop(key)
    assert all(shutil.which(name, path=env['PATH']) is None for name in ('node', 'npm', 'pnpm'))
    receipt = {'format': 'zryna.production-m2-roots.v1', 'head': args.head, 'tree': cli['tree'],
        'platform': sys.platform, 'root': str(root), 'output': str(output), 'cli_proof': str(prior),
        'cli_receipt_sha256': c.m.digest(prior/'ci-receipt.json'), 'inputs': before, 'generated_inputs': {},
        'tools': cli['tools'], 'binaries': cli['binaries'], 'cli_build_files': files(build),
        'python': sys.executable, 'runtime_path': str(empty), 'commands': [], 'records': [],
        'status': 'failed', 'run_id': os.getenv('GITHUB_RUN_ID'), 'run_attempt': os.getenv('GITHUB_RUN_ATTEMPT'),
        'public_activation': False, 'installed_distribution_acceptance': False,
        'scope': c.SCOPE, 'selected_environment': selected, 'metadata_target': metadata_target,
        'removed_node_environment_present': any(key.startswith(('NODE', 'NPM', 'PNPM', 'ZRYNA_NODE', 'ZRYNA_TEST_NODE')) for key in env)}

    def execute(label, command):
        try:
            result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=180)
            stdout, stderr, code, timed_out = result.stdout, result.stderr, result.returncode, False
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code, timed_out = error.stdout or b'', error.stderr or b'', None, True
        record = {'label': label, 'argv': command, 'cwd': str(root), 'exit': code,
                  'timed_out': timed_out, 'selected_environment': dict(selected)}
        for suffix, raw in (('stdout', stdout), ('stderr', stderr)):
            path = output/(label+'.'+suffix)
            with path.open('xb') as stream:
                stream.write(raw)
            record[suffix] = {'bytes': len(raw), 'sha256': c.m.digest(path)}
        receipt['commands'].append(record)
        assert not timed_out, 'command timeout; process-tree cleanup unconfirmed; cannot qualify'
        return code, stdout, stderr

    for label, contract in contracts.items():
        try:
            destination = root/'.zryna/out'/(c.stem(label)+'.build')
            absent(destination)
            arguments = ['build', contract['source'], '--root', str(root), '--name', c.stem(label),
                         '--target', 'all', '--json', '--profile', 'control-flow-v1']
            native_command = [cli['binaries']['feature']['path'], *arguments, '--native-frontend']
            code, native, error = execute(label+'-native', native_command)
            assert code == 0 and error == b'', error.decode(errors='replace')
            admitted = c.m.inventory(destination)
            c.verify_bundle(destination, label, contract, admitted, c.m.strict(native))
            before_collision = c.m.inventory(destination)
            code, rejected, error = execute(label+'-create-only', native_command)
            assert code == 4 and error == b'' and c.m.strict(rejected)['ok'] is False
            after_collision = c.m.inventory(destination)
            assert admitted == before_collision == after_collision
            shutil.move(destination, output/(label+'-native-bundle'))
            absent(destination)
            code, bootstrap, error = execute(label+'-bootstrap',
                [cli['binaries']['default']['path'], *arguments, '--node', cli['tools']['node']['path']])
            assert code == 0 and error == b'' and bootstrap == native, error.decode(errors='replace')
            c.verify_bundle(destination, label, contract, admitted, c.m.strict(bootstrap))
            shutil.move(destination, output/(label+'-bootstrap-bundle'))
            absent(destination)
            for name in admitted:
                assert (output/(label+'-native-bundle')/name).read_bytes() == (output/(label+'-bootstrap-bundle')/name).read_bytes()
            receipt['records'].append({'id': label, 'status': 'passed', 'source': contract['source'],
                'profile': 'control-flow-v1', 'files': admitted, 'initial_output_absent': True,
                'final_output_absent': True, 'success_json_exact': True, 'manifest_bytes_exact': True,
                'create_only': True, 'node_on_path': False, 'pnpm_on_path': False,
                'before_collision_inventory': before_collision, 'after_collision_inventory': after_collision})
        except (AssertionError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
            receipt['records'].append({'id': label, 'status': 'failed', 'reason': str(error) or type(error).__name__})
            if 'cleanup unconfirmed' in str(error):
                break
    receipt['counts'] = {status: sum(row['status'] == status for row in receipt['records'])
                         for status in ('passed', 'failed', 'ignored')}
    try:
        assert len(receipt['records']) == 3 and receipt['counts'] == {'passed': 3, 'failed': 0, 'ignored': 0}
        assert inputs(root) == before and git(root, 'rev-parse', 'HEAD') == args.head
        assert not git(root, 'status', '--porcelain') and files(build) == receipt['cli_build_files']
        assert all(c.m.digest(Path(row['path'])) == row['sha256'] for row in identities)
        receipt['status'] = 'passed'
    except (AssertionError, OSError, subprocess.SubprocessError) as error:
        receipt['failure'] = str(error) or type(error).__name__
    with (output/'receipt.json').open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    print(json.dumps({key: receipt[key] for key in ('head', 'platform', 'status', 'counts', 'scope')}))
    return int(receipt['status'] != 'passed')


if __name__ == '__main__':
    sys.exit(main())
