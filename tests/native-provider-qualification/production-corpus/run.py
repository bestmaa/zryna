"""Actual production CLI BUILD parity for all 33 roots and 36 rejection obligations."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


c = load('production_corpus_contracts', Path(__file__).with_name('contracts.py'))
p = load('production_build_utilities', Path(__file__).resolve().parents[1]/'production-manifests/run.py')


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
    assert p.git(root, 'rev-parse', 'HEAD') == args.head and not p.git(root, 'status', '--porcelain')
    output = args.output.absolute()
    p.absent(output)
    for left, right in ((output.resolve(), root), (output.resolve(), prior)):
        assert not left.is_relative_to(right) and not right.is_relative_to(left)
    cli = c.read(prior/'ci-receipt.json')
    before = p.inputs(root)
    assert cli['status'] == 'passed' and cli['head'] == args.head
    assert cli['tree'] == p.git(root, 'rev-parse', 'HEAD^{tree}')
    assert cli['platform'] == sys.platform and cli['inputs'] == before
    assert Path(cli['root']).resolve() == root
    tools, binaries = cli['tools'], cli['binaries']
    identities = [*tools.values(), *binaries.values()]
    assert all(p.digest(Path(row['path'])) == row['sha256'] for row in identities)
    assert not output.resolve().is_relative_to(Path(cli['target']).resolve())
    contracts = c.contracts(root, before)
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
    receipt = {'format': 'zryna.production-corpus-parity.v1', 'head': args.head,
               'tree': cli['tree'], 'platform': sys.platform, 'root': str(root),
               'output': str(output), 'cli_proof': str(prior),
               'cli_receipt_sha256': p.digest(prior/'ci-receipt.json'), 'inputs': before,
               'generated_inputs': {}, 'tools': tools, 'binaries': binaries,
               'cli_build_files': p.files(build), 'python': sys.executable,
               'runtime_path': str(empty), 'commands': [], 'records': [], 'status': 'failed',
               'run_id': os.getenv('GITHUB_RUN_ID'), 'run_attempt': os.getenv('GITHUB_RUN_ATTEMPT'),
               'public_activation': False, 'installed_distribution_acceptance': False,
               'scope': c.SCOPE}

    def execute(label, argv):
        try:
            process = subprocess.run(argv, cwd=root, env=env, capture_output=True, timeout=180)
            stdout, stderr, code, timed_out = process.stdout, process.stderr, process.returncode, False
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code, timed_out = error.stdout or b'', error.stderr or b'', None, True
        for suffix, raw in (('stdout', stdout), ('stderr', stderr)):
            (output/(label+'.'+suffix)).write_bytes(raw)
        receipt['commands'].append({'label': label, 'argv': argv, 'cwd': str(root),
                                    'exit': code, 'timed_out': timed_out})
        command = receipt['commands'][-1]
        for suffix in ('stdout', 'stderr'):
            path = output/(label+'.'+suffix)
            command[suffix] = {'bytes': path.stat().st_size, 'sha256': p.digest(path)}
        assert not timed_out, 'command timeout; process-tree cleanup unconfirmed; cannot qualify'
        return code, stdout, stderr

    for label, contract in contracts.items():
        try:
            stem = 'corpus-build-'+label
            destination = root/'.zryna/out'/(stem+'.build')
            p.absent(destination)
            arguments = ['build', contract['source'], '--root', str(root), '--name', stem,
                         '--target', 'all', '--json', '--profile', 'data-ownership-v1']
            native_argv = [binaries['feature']['path'], *arguments, '--native-frontend']
            code, native, stderr = execute(label+'-native', native_argv)
            assert stderr == b''
            accepted = contract['disposition'] == 'accepted'
            assert code == (0 if accepted else 3)
            admitted = p.files(destination) if accepted else {}
            if accepted:
                assert c.strict(native)['ok'] is True
                code, rejected, stderr = execute(label+'-create-only', native_argv)
                assert code == 4 and stderr == b'' and c.strict(rejected)['ok'] is False
                assert p.files(destination) == admitted, 'collision changed complete output'
                shutil.move(destination, output/(label+'-native-bundle'))
                assert p.files(output/(label+'-native-bundle')) == admitted
            else:
                c.exact(c.strict(native), c.failure(contract))
            p.absent(destination)
            code, bootstrap, stderr = execute(label+'-bootstrap',
                [binaries['default']['path'], *arguments, '--node', tools['node']['path']])
            assert code == (0 if accepted else 3) and stderr == b'' and bootstrap == native
            if accepted:
                assert p.files(destination) == admitted
                shutil.move(destination, output/(label+'-bootstrap-bundle'))
                assert p.files(output/(label+'-bootstrap-bundle')) == admitted
            else:
                c.exact(c.strict(bootstrap), c.failure(contract))
            p.absent(destination)
            receipt['records'].append({'id': label, 'disposition': contract['disposition'],
                'status': 'passed', 'source': contract['source'], 'profile': 'data-ownership-v1',
                'files': admitted, 'initial_output_absent': True, 'final_output_absent': True,
                'provider_json_exact': True, 'create_only': accepted,
                'node_on_path': False, 'pnpm_on_path': False})
        except (AssertionError, ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
            receipt['records'].append({'id': label, 'status': 'failed',
                                       'reason': str(error) or type(error).__name__})
            if 'cleanup unconfirmed' in str(error):
                break
    receipt['counts'] = {status: sum(row['status'] == status for row in receipt['records'])
                         for status in ('passed', 'failed', 'ignored')}
    try:
        assert len(receipt['records']) == 69 and receipt['counts'] == {'passed': 69, 'failed': 0, 'ignored': 0}
        assert len(receipt['commands']) == 171
        assert p.inputs(root) == before and p.git(root, 'rev-parse', 'HEAD') == args.head
        assert not p.git(root, 'status', '--porcelain')
        assert all(p.digest(Path(row['path'])) == row['sha256'] for row in identities)
        receipt['status'] = 'passed'
    except (AssertionError, OSError) as error:
        receipt['failure'] = str(error) or type(error).__name__
    with (output/'receipt.json').open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    print(json.dumps({key: receipt[key] for key in ('head', 'platform', 'counts', 'status', 'scope')}))
    return int(receipt['status'] != 'passed')


if __name__ == '__main__':
    sys.exit(main())
