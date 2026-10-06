#!/usr/bin/env python3
"""Current Windows installation producer; admitted qualification remains independent."""
import argparse
import json
import os
from pathlib import Path
import re
import sys

from build import build, command, sha, snapshot, write
from run import proof
from toolchain import capture
from windows_build import environment, verify_profile

if not __debug__:
    raise RuntimeError('CI installation proof requires assertions enabled')


def produce(args):
    assert sys.platform == 'win32'
    source = args.source.resolve()
    output = args.output.resolve()
    target = args.target.resolve()
    assert not output.exists() and not target.exists()
    assert not output.is_relative_to(source) and not target.is_relative_to(source)
    assert not output.is_relative_to(target) and not target.is_relative_to(output)
    before = snapshot(source)
    assert before['head'] == args.head and re.fullmatch('[0-9a-f]{40}', args.head)
    assert re.fullmatch('[1-9][0-9]*', args.run_id) and re.fullmatch('[1-9][0-9]*', args.run_attempt)
    output.mkdir(mode=0o700)
    profile = capture(source, output / 'toolchain', args.cargo, args.rustc, args.rustup)
    build(source, output / 'build', target, args.cargo, sys.platform, profile)
    result = proof(source, output / 'cases', output / 'build', sys.platform)
    env = environment(profile, output, target)
    env.update(CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_TEST_OPT_LEVEL='1', CARGO_PROFILE_TEST_CODEGEN_UNITS='16')
    argv = [args.cargo, 'test', '--locked', '--offline', '-p', 'zryna-driver', '--features', 'native-provider-internal',
            '--lib', '--message-format=json', 'distribution::native_installation::descriptor::tests', '--', '--nocapture']
    command(output, 'private-descriptor-units', argv, source, env, source)
    lines = (output / 'private-descriptor-units.stdout').read_text().splitlines()
    unit_images = []
    for line in lines:
        if not line.startswith('{'):
            continue
        row = json.loads(line)
        if row.get('reason') == 'compiler-artifact' and row.get('profile', {}).get('test') and row.get('executable'):
            unit_images.append(Path(row['executable']))
    assert len(unit_images) == 1
    unit_image = unit_images[0]
    assert unit_image.is_relative_to(target) and unit_image.suffix == '.exe'
    raw = unit_image.read_bytes()
    assert raw.startswith(b'MZ') and len(raw) <= 128 * 1024 * 1024
    write(output / 'private-descriptor-unit-image.exe', raw)
    write(output / 'private-descriptor-unit-image-binding.json', json.dumps({'actual_cargo_test_path': str(unit_image),
          'retained_path': 'private-descriptor-unit-image.exe', 'bytes': len(raw), 'sha256': sha(raw)}, indent=2).encode() + b'\n')
    verify_profile(profile)
    write(output / 'toolchain/after.json', json.dumps(profile, indent=2).encode() + b'\n')
    assert snapshot(source) == before
    receipt = {'schema_version': 1, 'head': args.head, 'tree': before['tree'], 'platform': 'win32',
               'run_id': args.run_id, 'run_attempt': args.run_attempt, 'source_before': before,
               'source_after': snapshot(source), 'case_selections': len(result['cases']),
               'public_defaults_changed': False, 'ordinary_no_Node_acceptance': False,
               'producer_only_not_independent_admission': True}
    write(output / 'ci-receipt.json', json.dumps(receipt, indent=2).encode() + b'\n')
    print(json.dumps(receipt | {'source_before': 'retained', 'source_after': 'retained'}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for name in ('source', 'output', 'target'):
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ('head', 'run-id', 'run-attempt', 'cargo', 'rustc', 'rustup'):
        parser.add_argument('--' + name, required=True)
    produce(parser.parse_args())
