#!/usr/bin/env python3
"""Execute bounded private installation proof; ordinary/default acceptance stays open."""
import argparse
import datetime
import json
import os
import pathlib
import shutil
import stat
import subprocess
import sys

from build import build, image, sha, snapshot, write
from cases import expected_summary, materialize, pre_mutation, tree
from independent_cases import COMPILE_BINDINGS, MARKER_PREFIX, OBSERVATION_CASES, REAL_CASES, TARGETS


def authenticate(path, raw_expected, binding, platform, unprepared=False):
    for item in [path, *path.parents]:
        info = item.lstat()
        assert not stat.S_ISLNK(info.st_mode)
        if hasattr(info, 'st_file_attributes'):
            assert not info.st_file_attributes & 0x400
    info = path.lstat()
    assert stat.S_ISREG(info.st_mode) and info.st_nlink == 1
    raw = path.read_bytes()
    assert sha(raw) == sha(raw_expected)
    if not unprepared:
        image(raw, binding, platform)
    return {'bytes': len(raw), 'sha256': sha(raw), 'single_link': True,
            'initial_image_matches_actual_source_build': True}


def invoke(case, label, executable, project, backups, empty, scenario, protocol, env_override=None):
    env = {'PATH': str(empty), 'HOME': str(case), 'LANG': 'C', 'LC_ALL': 'C', 'TMPDIR': str(case)}
    for key in ['SYSTEMROOT', 'SystemRoot', 'WINDIR', 'COMSPEC', 'PATHEXT']:
        if key in os.environ:
            env[key] = os.environ[key]
    if sys.platform == 'win32':
        env.update(TEMP=str(case), TMP=str(case))
    if env_override:
        env.update(env_override)
    assert all(shutil.which(name, path=env['PATH']) is None for name in ['node', 'npm', 'pnpm', 'cargo', 'rustc'])
    assert 'NODE_PATH' not in env and 'NODE_OPTIONS' not in env
    argv = [str(executable), str(project), str(backups), scenario, str(protocol)]
    before = tree(case / 'live')
    start = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (case / (label + '.stdout')).open('xb') as stdout, (case / (label + '.stderr')).open('xb') as stderr:
        process = subprocess.Popen(argv, cwd=case, env=env, stdin=subprocess.DEVNULL,
                                   stdout=stdout, stderr=stderr)
        try:
            result = process.wait(timeout=120)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise
    raw = (case / (label + '.stdout')).read_bytes()
    error = (case / (label + '.stderr')).read_bytes()
    assert len(raw) + len(error) <= 1024 * 1024 and not error
    value = json.loads(raw)
    record = {'argv': argv, 'cwd': str(case), 'selected_environment': env, 'exit': result,
              'direct_child_reaped': True, 'started_at': start,
              'completed_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'stdout': {'bytes': len(raw), 'sha256': sha(raw)},
              'stderr': {'bytes': len(error), 'sha256': sha(error)},
              'before': before, 'after': tree(case / 'live'), 'result': value}
    write(case / (label + '-execution.json'), json.dumps(record, indent=2).encode() + b'\n')
    return record


def proof(source, output, built, platform):
    receipt = json.loads((built / 'build-receipt.json').read_text())
    original = (built / 'prepared-image').read_bytes()
    assert sha(original) == receipt['prepared_image']['sha256']
    before = snapshot(source)
    output.mkdir(mode=0o700)
    results = []

    def perform(case_id, protocol=3, phase=None, full_m2=False, unprepared=False, expected_reject=False, observe=False, unique_id=None, boundary=None):
        case = output / (unique_id or case_id)
        case.mkdir(mode=0o700)
        install, project, backups, empty, executable = materialize(source, built, case, protocol, platform, unprepared, full_m2)
        selected = (built / ('unprepared-image' if unprepared else 'prepared-image')).read_bytes()
        initial = authenticate(executable, selected, receipt['binding'], platform, unprepared)
        baseline = None
        if not full_m2 and not unprepared:
            baseline = invoke(case, 'baseline', executable, project, backups, empty, 'positive', protocol)
            assert baseline['exit'] == 0 and baseline['result']['summary'] == expected_summary(source, protocol)
        if case_id == 'relocated-complete-installation':
            relocated = case / 'live/relocated-installation'
            install.rename(relocated)
            executable = relocated / TARGETS[platform][1]
            authenticate(executable, selected, receipt['binding'], platform)
        if phase == 'pre-capture':
            executable = pre_mutation(case_id, install, project, backups, executable)
            if case_id in ('installation-parent-link', 'executable-hardlink'):
                try:
                    authenticate(executable, selected, receipt['binding'], platform)
                except AssertionError:
                    write(case / 'external-initial-image-rejection.json', json.dumps({'case': case_id,
                        'status': 'rejected-before-execution', 'scope': 'external initial image path/single-link guard',
                        'before_mutation_positive': True, 'after': tree(case / 'live')}, indent=2).encode() + b'\n')
                    results.append({'id': case_id, 'status': 'passed-external-initial-image-rejection', 'probe_executed_after_mutation': False})
                    shutil.rmtree(case / 'live')
                    return
                raise AssertionError('unsafe image unexpectedly admitted')
        env_override = None
        if case_id == 'runtime-env-cannot-override-compiled-authority':
            env_override = dict(zip(COMPILE_BINDINGS, ['f' * 64, 'e' * 40, 'd' * 40]))
        record = invoke(case, 'candidate', executable, project, backups, empty,
                        case_id if phase in ('after-capture', 'after-source-capture') or case_id.startswith('callback-') or case_id in ('non-frozen-request', 'wrong-package-identity') else 'positive',
                        protocol, env_override)
        mutation = record['result'].get('mutation')
        if observe:
            status = 'observed-owning-package-contract'
            assert record['exit'] in (0, 2)
            assert mutation and mutation['id'] == case_id and mutation['effective'] is True
            if record['exit'] == 0:
                assert record['result']['status'] == 'verified' and record['result']['summary'] == expected_summary(source, protocol)
            else:
                assert record['result']['status'] == 'rejected' and record['result']['phase'] == 'syntax-verification'
                assert record['result']['diagnostics'] and all(row['code'] == 'ZRYNA-P4004' for row in record['result']['diagnostics'])
        elif mutation and mutation.get('effective') is False:
            assert record['exit'] in (0, 2)
            status = 'blocked-effective-mutation; observed-OS-prevention'
        elif expected_reject or phase or full_m2 or unprepared or case_id.startswith('callback-'):
            assert record['exit'] == 2 and record['result']['status'] == 'rejected', case_id
            expected_phase = {'pre-capture': 'installation-capture',
                              'after-capture': 'source-capture',
                              'after-source-capture': 'syntax-verification'}.get(phase)
            if unprepared:
                expected_phase = 'installation-capture'
            elif full_m2:
                expected_phase = 'package-setup'
            elif case_id.startswith('callback-'):
                expected_phase = 'consumer-postcheck'
            elif case_id in ('non-frozen-request', 'wrong-package-identity'):
                expected_phase = 'source-capture'
            assert expected_phase is not None and record['result']['phase'] == expected_phase, case_id
            if full_m2:
                assert record['result']['diagnostics'] == {
                    'code': 'ZRYNA-P4004',
                    'message': 'ZRYNA-P4004: package file exceeds its byte limit'}
            if case_id.startswith('callback-'):
                assert 'ZRYNA-C4220' in json.dumps(record['result']['diagnostics'])
            status = 'passed-real-rejection'
        else:
            assert record['exit'] == 0 and record['result']['summary'] == expected_summary(source, protocol), case_id
            status = 'passed-real-private-capability'
        results.append({'id': unique_id or case_id, 'scenario': case_id, 'status': status, 'protocol': protocol,
                        'declared_boundary': boundary,
                        'initial_image': initial, 'candidate_exit': record['exit'],
                        'phase': record['result'].get('phase'), 'baseline_passed': baseline is not None,
                        'independent_original_positive_prior': baseline is None and (full_m2 or unprepared)})
        # Only stopped disposable test copies are removed; original images, byte/state journals,
        # all stdout/stderr, source declarations and receipts remain outside this live directory.
        assert record['direct_child_reaped'] and selected == (built / ('unprepared-image' if unprepared else 'prepared-image')).read_bytes()
        shutil.rmtree(case / 'live')

    for protocol in [2, 3, 4]:
        perform({2: 'real-native-v2-package', 3: 'real-native-v3-bounded-import-package', 4: 'real-native-v4-ownership-package'}[protocol], protocol)
    for protocol in [2, 3, 4]:
        for case_id in ['relocated-complete-installation', 'runtime-env-cannot-override-compiled-authority']:
            perform(case_id, protocol, unique_id=case_id + '-v' + str(protocol))
    for case_id, phase, boundary in REAL_CASES:
        perform(case_id, phase=phase, boundary=boundary)
    for observation in OBSERVATION_CASES:
        perform(observation['id'], phase=observation['phase'], observe=True)
    perform('unprepared-real-build', unprepared=True)
    perform('original-full-M2-package-byte-limit-rejection', full_m2=True)
    for case_id in ['callback-license-change', 'callback-error-license-change', 'non-frozen-request', 'wrong-package-identity']:
        perform(case_id, expected_reject=True)
    for case_id, forged in [('missing-private-marker', original.replace(MARKER_PREFIX, b'x' * len(MARKER_PREFIX))),
                           ('foreign-purpose-marker', original.replace(MARKER_PREFIX, b'Y' * len(MARKER_PREFIX))),
                           ('modified-executable-bytes', original[:-1] + bytes([original[-1] ^ 1]))]:
        try:
            image(forged, receipt['binding'], platform)
            assert sha(forged) == receipt['prepared_image']['sha256']
        except AssertionError:
            results.append({'id': case_id, 'status': 'passed-external-image-reader-rejection', 'executed': False,
                            'original_sha256': sha(original), 'forged_sha256': sha(forged)})
        else:
            raise AssertionError(case_id)
    assert snapshot(source) == before and sha((built / 'prepared-image').read_bytes()) == sha(original)
    result = {'status': 'passed-bounded-private-installation-proof', 'head': before['head'], 'tree': before['tree'],
              'source_before': before, 'source_after': snapshot(source), 'cases': results,
              'prepared_image': receipt['prepared_image'], 'unprepared_image': receipt['unprepared_image'],
              'shared_actual_original_image_bytes_retained': str(built / 'prepared-image'),
              'installed_provider_Node_processes': 0, 'runtime_backend_processes': 0,
              'private_only': True, 'public_defaults_changed': False, 'ordinary_no_Node_acceptance': False,
              'original_full_M2_positive': 'blocked by unchanged 1024-byte package-source limit',
              'bounded_v3_successor': {'original_positive_id': 'real-native-v3-import-package',
                                      'new_positive_id': 'real-native-v3-bounded-import-package',
                                      'fixture': 'crates/zryna-frontend/tests/native_parser_v3_calls',
                                      'separately_frozen': True, 'full_M2_credit': False},
              'running_image_versus_installed_path': 'mapped to executable-path-replaced-identical-bytes; no extra independently executed case',
              'late_mutated_invalid_syntax': 'not exercised by this bounded run',
              'Windows_execution_credit': platform == 'win32', 'OS_denied_mutations_separate': True,
              'ephemeral_fixture_cleanup': 'only stopped copied fixtures after full retained byte/state journals and verified original image recovery; no source/evidence/cache deletion',
              'completed_at': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    write(output / 'receipt.json', json.dumps(result, indent=2).encode() + b'\n')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--target', type=pathlib.Path, required=True)
    parser.add_argument('--cargo', required=True)
    args = parser.parse_args()
    assert sys.platform in TARGETS
    args.output.mkdir(mode=0o700)
    build(args.source, args.output / 'build', args.target, args.cargo, sys.platform)
    result = proof(args.source, args.output / 'cases', args.output / 'build', sys.platform)
    print(json.dumps({'status': result['status'], 'cases': len(result['cases']), 'head': result['head'], 'ordinary_no_Node_acceptance': False}))
