"""Exact-head cold private BUILD and retained Vec execution; never public activation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from pnpm_entry import resolve as resolve_pnpm


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def inputs(root):
    names = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z']).decode().split('\0')
    return {name: digest(root/name) for name in names if name}


def files(directory):
    return {p.relative_to(directory).as_posix(): {'bytes':p.stat().st_size, 'sha256':digest(p)}
            for p in sorted(directory.rglob('*')) if p.is_file()}


def main():
    if not __debug__ or sys.version_info < (3, 11):
        raise RuntimeError('Python 3.11+ without optimization required')
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('root', 'cli-proof', 'target', 'output', 'cold-root'):
        parser.add_argument('--'+flag, type=Path, required=True)
    parser.add_argument('--head', required=True)
    args = parser.parse_args()
    root, prior = args.root.resolve(strict=True), args.cli_proof.resolve(strict=True)
    if sys.platform not in ('linux', 'win32'):
        parser.error('only supported qualification hosts may execute')
    assert git(root, 'rev-parse', 'HEAD') == args.head and not git(root, 'status', '--porcelain')
    cli = json.loads((prior/'ci-receipt.json').read_bytes())
    assert cli['status'] == 'passed' and cli['head'] == args.head
    assert cli['platform'] == sys.platform and cli['tree'] == git(root, 'rev-parse', 'HEAD^{tree}')
    target = args.target.resolve(strict=True)
    assert str(target) == cli['target']
    output, cold = args.output.absolute(), args.cold_root.absolute()
    assert output != cold
    for path in (output, cold):
        assert not path.exists() and not path.is_symlink()
        assert not path.resolve().is_relative_to(root)
        assert not path.resolve().is_relative_to(target)
    assert not output.is_relative_to(cold) and not cold.is_relative_to(output)
    output.mkdir()
    tools = cli['tools']
    feature = cli['binaries']['feature']
    default = cli['binaries']['default']
    for row in [*tools.values(), feature, default]:
        assert digest(Path(row['path'])) == row['sha256']
    before = inputs(root)
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_NET_OFFLINE='true',
               CARGO=tools['cargo']['path'], RUSTC=tools['rustc']['path'],
               CARGO_BUILD_JOBS='2', CARGO_PROFILE_TEST_DEBUG='0',
               ZRYNA_TEST_NODE=tools['node']['path'])
    receipt = {'format':'zryna.native-cold-qualification.v1', 'head':args.head,
               'tree':cli['tree'], 'platform':sys.platform, 'root':str(root),
               'cold_root':str(cold), 'target':str(target), 'inputs':before,
               'cli_proof':str(prior), 'cli_receipt_sha256':digest(prior/'ci-receipt.json'),
               'tools':tools, 'feature_cli':feature, 'default_cli':default, 'python':sys.executable,
               'commands':[], 'status':'failed', 'run_id':os.getenv('GITHUB_RUN_ID'),
               'run_attempt':os.getenv('GITHUB_RUN_ATTEMPT'),
               'public_activation':False, 'installed_distribution_acceptance':False}

    def run(label, command, cwd=root, process_env=env, timeout=2400):
        process = subprocess.run(command, cwd=cwd, env=process_env, capture_output=True, timeout=timeout)
        (output/(label+'.stdout')).write_bytes(process.stdout)
        (output/(label+'.stderr')).write_bytes(process.stderr)
        receipt['commands'].append({'label':label, 'argv':command, 'cwd':str(cwd),
                                    'exit':process.returncode})
        return process

    try:
        built = run('feature-test-build', [tools['cargo']['path'], 'test', '--locked', '--offline',
                    '-p', 'zryna-driver', '--features', 'native-provider-internal', '--lib',
                    '--no-run', '--message-format=json'])
        assert built.returncode == 0
        artifacts = [json.loads(line) for line in built.stdout.splitlines()]
        executables = [row['executable'] for row in artifacts
                       if row.get('reason') == 'compiler-artifact' and row.get('executable')
                       and row['target']['name'] == 'zryna_driver' and row['profile']['test']]
        assert len(executables) == 1
        binary = Path(executables[0])
        receipt['test_binary'] = {'path':str(binary), 'sha256':digest(binary)}
        observations = output/'observations'
        observations.mkdir()
        fault_env = dict(env, ZRYNA_M3_NATIVE_FAULT_EVIDENCE=str(observations))
        executed = run('retained-fault-tests', [str(binary), 'native_provider_faults', '--skip', 'native_provider_faults::corpus::', '--nocapture',
                       '--test-threads=1'], process_env=fault_env, timeout=1800)
        assert executed.returncode == 0
        # Full production architecture validation happens in this fresh source checkout.
        created = run('cold-checkout', ['git', '-C', str(root), 'worktree', 'add', '--detach',
                                      str(cold), args.head])
        assert created.returncode == 0
        assert inputs(cold) == before and not git(cold, 'status', '--porcelain')
        assert git(cold, 'rev-parse', 'HEAD') == args.head and not (cold/'.zryna').exists()
        empty = output/'empty-runtime-path'
        empty.mkdir()
        no_node = dict(env, PATH=str(empty))
        for key in tuple(no_node):
            if key.startswith(('NODE', 'NPM', 'PNPM', 'ZRYNA_NODE', 'ZRYNA_TEST_NODE')):
                no_node.pop(key)
        assert all(shutil.which(name, path=no_node['PATH']) is None for name in ('node', 'npm', 'pnpm'))
        receipt['cold_before_native'] = True
        receipt['runtime_path'] = str(empty)
        source, stem = 'tests/m3-fixtures/conformance/vec.zry', 'private-smoke-m3-vec'
        command = [feature['path'], 'build', source, '--root', str(cold), '--name', stem,
                   '--target', 'all', '--json', '--profile', 'data-ownership-v1', '--native-frontend']
        destination = cold/'.zryna/out'/(stem+'.build')
        native = run('cold-native-build', command, cwd=cold, process_env=no_node, timeout=180)
        assert native.returncode == 0 and json.loads(native.stdout)['ok'] is True
        admitted = files(destination)
        collision = run('cold-build-collision', command, cwd=cold, process_env=no_node, timeout=180)
        assert collision.returncode != 0
        diagnostic = json.loads(collision.stdout)
        assert diagnostic['ok'] is False and len(diagnostic['diagnostics']) == 1
        assert diagnostic['diagnostics'][0]['code'] == 'ZRYNA-C1009'
        assert diagnostic['diagnostics'][0]['message'] == 'create-only output bundle already exists'
        assert files(destination) == admitted and len(list((cold/'.zryna/out').iterdir())) == 1
        shutil.copytree(destination, output/'cold-native-bundle')
        shutil.rmtree(destination)
        # Bootstrap owns a frozen adapter dependency install; native cold BUILD did not need it.
        shim = shutil.which('pnpm')
        assert shim is not None
        pnpm = resolve_pnpm(shim,os.getenv('PNPM_HOME'))
        version = run('bootstrap-pnpm-version',[tools['node']['path'],str(pnpm),'--version'],timeout=30)
        assert version.returncode == 0 and version.stdout.decode().strip() == '11.18.0'
        receipt['bootstrap_pnpm'] = {'path':str(pnpm),'sha256':digest(pnpm),'version':'11.18.0',
                                    'manifest_sha256':digest(pnpm.parent.parent/'package.json')}
        install = run('bootstrap-frozen-install',[tools['node']['path'],str(pnpm),
                      'install','--frozen-lockfile'],cwd=cold,timeout=600)
        assert install.returncode == 0
        baseline_command = [cli['binaries']['default']['path'], *command[1:-1],
                            '--node', tools['node']['path']]
        bootstrap = run('cold-bootstrap-comparison', baseline_command, cwd=cold,
                        process_env=no_node, timeout=180)
        assert bootstrap.returncode == 0 and bootstrap.stdout == native.stdout
        assert files(destination) == admitted
        shutil.move(destination, output/'cold-bootstrap-bundle')
        assert not list((cold/'.zryna/out').iterdir())
        assert inputs(cold) == before == inputs(root)
        assert not git(cold, 'status', '--porcelain') and not git(root, 'status', '--porcelain')
        assert digest(binary) == receipt['test_binary']['sha256']
        assert all(digest(Path(row['path'])) == row['sha256'] for row in [*tools.values(), feature, default])
        receipt['cold_cleanup_entries'] = 0
        receipt['status'] = 'passed'
    except (AssertionError, ValueError, OSError, StopIteration, subprocess.SubprocessError) as error:
        receipt['failure'] = str(error) or type(error).__name__
        if isinstance(error, subprocess.TimeoutExpired):
            receipt['failure'] += '; process-tree cleanup unconfirmed; proof cannot qualify'
    with (output/'receipt.json').open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    print(json.dumps({key:receipt[key] for key in ('head', 'tree', 'platform', 'status', 'run_id', 'run_attempt')}))
    return int(receipt['status'] != 'passed')


if __name__ == '__main__':
    sys.exit(main())
