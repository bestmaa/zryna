"""Build and exercise the private CLI at one exact hosted source revision."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def inventory(root):
    names = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z']).decode().split('\0')
    return {name: digest(root/name) for name in names if name}


def main():
    if not __debug__ or sys.version_info < (3, 11):
        raise RuntimeError('Python 3.11+ without optimization is required')
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'target', 'output', 'node', 'cargo', 'rustc'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--head', required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    if sys.platform not in ('linux', 'win32') or not re.fullmatch('[0-9a-f]{40}', args.head):
        parser.error('proof requires a supported host and an exact head SHA')
    if git(root, 'rev-parse', 'HEAD') != args.head or git(root, 'status', '--porcelain'):
        parser.error('source must be the exact clean committed checkout')
    output, target = args.output.absolute(), args.target.absolute()
    for path in (output, target):
        if path.exists() or path.is_symlink() or path.resolve().is_relative_to(root):
            parser.error('target and evidence must be new external paths')
    if output.resolve().is_relative_to(target.resolve()) or target.resolve().is_relative_to(output.resolve()):
        parser.error('target and evidence paths must be independent')
    tools = {name: getattr(args, name).absolute() for name in ('node', 'cargo', 'rustc')}
    if any(not path.is_file() for path in tools.values()):
        parser.error('explicit executable tools are required')
    output.mkdir()
    target.mkdir()
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_NET_OFFLINE='true',
               CARGO=str(tools['cargo']), RUSTC=str(tools['rustc']), CARGO_BUILD_JOBS='2',
               CARGO_PROFILE_DEV_DEBUG='0')
    before = inventory(root)
    receipt = {'format':'zryna.private-cli-ci.v1', 'head':args.head,
               'tree':git(root, 'rev-parse', 'HEAD^{tree}'), 'platform':sys.platform,
               'root':str(root), 'target':str(target), 'status':'failed',
               'inputs':before, 'tools':{}, 'binaries':{}, 'commands':[],
               'smoke_receipt':'smoke/receipt.json', 'public_activation':False,
               'installed_distribution_acceptance':False, 'python':sys.executable}

    def run(label, command, timeout=2400):
        process = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=timeout)
        (output/(label+'.stdout')).write_bytes(process.stdout)
        (output/(label+'.stderr')).write_bytes(process.stderr)
        receipt['commands'].append({'label':label,'argv':command,'exit':process.returncode,
                                    'cwd':str(root),'target':str(target)})
        if process.returncode:
            raise RuntimeError(f'{label} exited {process.returncode}; complete logs retained')
        return process.stdout.decode().strip()

    try:
        for name, prefix in (('cargo','cargo 1.97.1 '),('rustc','rustc 1.97.1 '),('node','v22.22.1')):
            version = run(name+'-version', [str(tools[name]), '--version'], 30)
            correct = version == prefix if name == 'node' else version.startswith(prefix)
            assert correct, version
            receipt['tools'][name] = {'path':str(tools[name]),'sha256':digest(tools[name]),'version':version}
        suffix = '.exe' if sys.platform == 'win32' else ''
        for label, features in (('default', []), ('feature', ['--features','native-provider-internal'])):
            run(label+'-build', [str(tools['cargo']), 'build', '--locked', '--offline', '-p', 'zryna', *features])
            binary = output/(label+'-cli'+suffix)
            shutil.copy2(target/'debug'/('zryna'+suffix), binary)
            receipt['binaries'][label] = {'path':str(binary),'sha256':digest(binary)}
        run('cli-smoke', [sys.executable, '-B', str(root/'scripts/run-native-cli-smoke.py'),
            '--root',str(root),'--default-cli',receipt['binaries']['default']['path'],
            '--feature-cli',receipt['binaries']['feature']['path'], '--node',str(tools['node']),
            '--cargo',str(tools['cargo']),'--rustc',str(tools['rustc']), '--output',str(output/'smoke')], 1800)
        assert before == inventory(root) and not git(root, 'status', '--porcelain')
        assert git(root, 'rev-parse', 'HEAD') == args.head
        assert all(digest(Path(row['path'])) == row['sha256'] for row in receipt['binaries'].values())
        assert all(digest(tools[name]) == row['sha256'] for name,row in receipt['tools'].items())
        receipt['status'] = 'passed'
    except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        receipt['failure'] = str(error) or type(error).__name__
        if isinstance(error,subprocess.TimeoutExpired):
            receipt['failure'] += '; process-tree cleanup is unconfirmed; proof cannot qualify'
    with (output/'ci-receipt.json').open('x') as stream:
        json.dump(receipt,stream,indent=2)
        stream.write('\n')
    print(json.dumps({key:receipt[key] for key in ('head','tree','platform','status','public_activation')}))
    return int(receipt['status'] != 'passed')


if __name__ == '__main__':
    sys.exit(main())
