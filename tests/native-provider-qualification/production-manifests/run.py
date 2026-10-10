"""Seven actual source-checkout BUILD pairs; installation and activation stay unclaimed."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

CASES = ('abi', 'bounds', 'enum', 'handles', 'owned-shared', 'weak-expired', 'weak-live')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def inputs(root):
    names = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z']).decode().split('\0')
    return {name: digest(root/name) for name in names if name}


def real(path, directory):
    metadata = path.lstat()
    assert not stat.S_ISLNK(metadata.st_mode)
    assert not getattr(metadata, 'st_file_attributes', 0) & 0x400
    assert (stat.S_ISDIR if directory else stat.S_ISREG)(metadata.st_mode)


def files(directory):
    real(directory, True)
    result = {}
    for path in sorted(directory.rglob('*')):
        real(path, path.is_dir())
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = {
                'bytes': path.stat().st_size, 'sha256': digest(path)}
    assert result
    return result


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise AssertionError('refuse any existing output, including links')


def main():
    if not __debug__ or sys.version_info < (3, 11):
        raise RuntimeError('Python 3.11+ without optimization required')
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('root', 'cli-proof', 'output'):
        parser.add_argument('--'+flag, type=Path, required=True)
    parser.add_argument('--head', required=True)
    args = parser.parse_args()
    root, prior = args.root.resolve(strict=True), args.cli_proof.resolve(strict=True)
    assert sys.platform in ('linux', 'win32')
    assert git(root, 'rev-parse', 'HEAD') == args.head and not git(root, 'status', '--porcelain')
    output = args.output.absolute()
    absent(output)
    assert not output.resolve().is_relative_to(root)
    assert not output.resolve().is_relative_to(prior)
    assert not prior.is_relative_to(output.resolve())
    cli = json.loads((prior/'ci-receipt.json').read_bytes())
    before = inputs(root)
    assert cli['status'] == 'passed' and cli['head'] == args.head
    assert cli['tree'] == git(root, 'rev-parse', 'HEAD^{tree}')
    assert cli['platform'] == sys.platform and cli['inputs'] == before
    assert Path(cli['root']).resolve() == root
    tools, binaries = cli['tools'], cli['binaries']
    identities = [*tools.values(), *binaries.values()]
    assert all(digest(Path(row['path'])) == row['sha256'] for row in identities)
    assert not output.resolve().is_relative_to(Path(cli['target']).resolve())
    output.mkdir()
    build = output/'cli-build'
    build.mkdir()
    shutil.copy2(prior/'ci-receipt.json', build/'ci-receipt.json')
    (build/'smoke').mkdir()
    shutil.copy2(prior/'smoke/receipt.json', build/'smoke/receipt.json')
    for command in cli['commands']:
        for suffix in ('.stdout', '.stderr'):
            name = command['label']+suffix
            shutil.copy2(prior/name, build/name)
    empty = output/'empty-runtime-path'
    empty.mkdir()
    env = dict(os.environ, PATH=str(empty), CARGO=tools['cargo']['path'],
               RUSTC=tools['rustc']['path'], CARGO_NET_OFFLINE='true')
    for key in tuple(env):
        if key.startswith(('NODE', 'NPM', 'PNPM', 'ZRYNA_NODE', 'ZRYNA_TEST_NODE')):
            env.pop(key)
    assert all(shutil.which(name, path=env['PATH']) is None for name in ('node', 'npm', 'pnpm'))
    registry = json.loads((root/'tests/m3-conformance-v1.json').read_bytes())
    fixtures = {row['id']: row for row in registry['fixtures']}
    assert len(fixtures) == len(registry['fixtures'])
    prefix = '.zryna/cache/production-manifest-'+hashlib.sha256(str(output.resolve()).encode()).hexdigest()[:16]
    receipt = {'format': 'zryna.production-manifest-parity.v1', 'head': args.head,
               'tree': cli['tree'], 'platform': sys.platform, 'root': str(root),
               'output': str(output), 'cli_proof': str(prior),
               'cli_receipt_sha256': digest(prior/'ci-receipt.json'), 'inputs': before,
               'generated_inputs': {}, 'tools': tools, 'binaries': binaries,
               'cli_build_files': files(build), 'python': sys.executable,
               'runtime_path': str(empty), 'commands': [], 'records': [], 'status': 'failed',
               'run_id': os.getenv('GITHUB_RUN_ID'), 'run_attempt': os.getenv('GITHUB_RUN_ATTEMPT'),
               'public_activation': False, 'installed_distribution_acceptance': False,
               'scope': {'provider_pairs': 7, 'cli_invocations': 21, 'bundle_files': 56,
                         'target_artifacts': 42, 'runtime_execution': False,
                         'layout_semantics_reconstructed': False}}

    def fixture(identity):
        entry = fixtures[identity]
        original = root/entry['path']
        assert digest(original) == entry['sha256'] == before[entry['path']]
        if 'dependency' not in entry:
            return entry['path']
        assert entry['dependency'] == identity+'-body'
        body = fixtures[entry['dependency']]
        for directory in (root/'.zryna', root/'.zryna/cache', root/prefix):
            if directory.exists() or directory.is_symlink():
                real(directory, True)
            else:
                directory.mkdir()
        directory = root/prefix/('m3-'+identity)
        directory.mkdir(exist_ok=False)
        for filename, row in (('main.zry', entry), ('math.zry', body)):
            assert digest(root/row['path']) == row['sha256'] == before[row['path']]
            destination = directory/filename
            destination.write_bytes((root/row['path']).read_bytes())
            assert digest(destination) == row['sha256']
            receipt['generated_inputs'][destination.relative_to(root).as_posix()] = {
                'original': row['path'], 'sha256': row['sha256']}
        return (directory/'main.zry').relative_to(root).as_posix()

    def execute(label, command):
        timed_out = False
        try:
            process = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=180)
            stdout, stderr, code = process.stdout, process.stderr, process.returncode
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code, timed_out = error.stdout or b'', error.stderr or b'', None, True
        for suffix, raw in (('stdout', stdout), ('stderr', stderr)):
            (output/(label+'.'+suffix)).write_bytes(raw)
        receipt['commands'].append({'label': label, 'argv': command, 'cwd': str(root),
                                    'exit': code, 'timed_out': timed_out,
                                    **{suffix: {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
                                       for suffix, raw in (('stdout', stdout), ('stderr', stderr))}})
        assert not timed_out, 'command timeout; process-tree cleanup unconfirmed; cannot qualify'
        return code, stdout, stderr

    for identity in CASES:
        label = 'm3-'+identity
        try:
            source = fixture(identity)
            stem = 'production-manifest-'+label
            destination = root/'.zryna/out'/(stem+'.build')
            absent(destination)
            arguments = ['build', source, '--root', str(root), '--name', stem,
                         '--target', 'all', '--json', '--profile', 'data-ownership-v1']
            native_command = [binaries['feature']['path'], *arguments, '--native-frontend']
            code, native, error = execute(label+'-native', native_command)
            assert code == 0, 'native BUILD failed: '+error.decode(errors='replace')
            assert json.loads(native)['ok'] is True
            admitted = files(destination)
            code, rejected, _ = execute(label+'-create-only', native_command)
            assert code != 0 and json.loads(rejected)['ok'] is False
            assert files(destination) == admitted, 'collision changed complete output'
            shutil.move(destination, output/(label+'-native-bundle'))
            assert files(output/(label+'-native-bundle')) == admitted
            absent(destination)
            code, bootstrap, error = execute(label+'-bootstrap',
                [binaries['default']['path'], *arguments, '--node', tools['node']['path']])
            assert code == 0, 'bootstrap BUILD failed: '+error.decode(errors='replace')
            assert bootstrap == native and files(destination) == admitted
            shutil.move(destination, output/(label+'-bootstrap-bundle'))
            assert files(output/(label+'-bootstrap-bundle')) == admitted
            absent(destination)
            receipt['records'].append({'id': label, 'status': 'passed', 'source': source,
                'profile': 'data-ownership-v1', 'files': admitted, 'initial_output_absent': True,
                'final_output_absent': True, 'success_json_exact': True, 'manifest_bytes_exact': True,
                'create_only': True, 'node_on_path': False, 'pnpm_on_path': False})
        except (AssertionError, ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
            receipt['records'].append({'id': label, 'status': 'failed',
                                       'reason': str(error) or type(error).__name__})
            if 'cleanup unconfirmed' in str(error):
                break
    receipt['counts'] = {status: sum(row['status'] == status for row in receipt['records'])
                         for status in ('passed', 'failed', 'ignored')}
    try:
        assert len(receipt['records']) == 7 and receipt['counts'] == {'passed': 7, 'failed': 0, 'ignored': 0}
        assert inputs(root) == before and git(root, 'rev-parse', 'HEAD') == args.head
        assert not git(root, 'status', '--porcelain')
        assert all(digest(root/name) == row['sha256'] for name, row in receipt['generated_inputs'].items())
        assert all(digest(Path(row['path'])) == row['sha256'] for row in identities)
        assert files(build) == receipt['cli_build_files']
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
