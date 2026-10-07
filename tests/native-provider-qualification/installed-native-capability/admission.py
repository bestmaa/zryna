#!/usr/bin/env python3
"""Independent retained private-installation evidence reader; no subject execution."""
import argparse
import base64
import datetime as dt
import json
import ntpath
import os
from pathlib import Path, PureWindowsPath as Win
import posixpath
import re
import stat
import sys

from independent_cases import COMPILE_BINDINGS, MARKER_PREFIX, REAL_CASES, TARGETS
from admission_build import (BUILD_LABELS, MAX, canonical, decode, digest, document, file_binding,
                             exact, image, package_update_only, profile_contract, read, require, source_snapshot, verify_build, verify_unit)

if not __debug__:
    raise RuntimeError('admission requires unoptimized Python')
ORACLE_SHA = 'ed8de9234f721b74b806131082a63d01cc619bcb84b29a4c495ae87ac7489b0e'
FIXTURES = {2: {'main.zry': 'examples/universal/add.zry'},
            3: {'main.zry': 'crates/zryna-frontend/tests/native_parser_v3_calls/main.zry',
                'math.zry': 'crates/zryna-frontend/tests/native_parser_v3_calls/math.zry'},
            4: {'main.zry': 'tests/m3-fixtures/conformance/pair.zry'}}
MAX = 128 * 1024 * 1024


def closed_files(root, expected):
    actual = set()
    for path in Path(root).rglob('*'):
        info = path.lstat()
        require(not path.is_symlink() and not getattr(info, 'st_file_attributes', 0) & 0x400, 'proof link/reparse')
        require(stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode), 'proof special type')
        if stat.S_ISREG(info.st_mode):
            require(info.st_nlink == 1, 'proof hardlink'); actual.add(path.relative_to(root).as_posix())
    require(actual == expected, 'complete proof file census')


def selection_census(rows):
    require(type(rows) is list and [r['id'] for r in rows] == [r[0] for r in selections()], 'ordered fifty selections')


def proof_inventory(root):
    files = {}
    for path in Path(root).rglob('*'):
        info = path.lstat()
        require(not path.is_symlink() and not getattr(info, 'st_file_attributes', 0) & 0x400
                and (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode)), 'proof inventory unsafe entry')
        if stat.S_ISREG(info.st_mode):
            files[path.relative_to(root).as_posix()] = file_binding(read(path, MAX))
        require(len(files) <= 4096 and sum(r['bytes'] for r in files.values()) <= 8 * MAX, 'proof inventory budget')
    require(len({p.casefold() for p in files}) == len(files), 'proof case aliases')
    return files


def pre_mutation(scenario, previous, current):
    before = journal(previous); after = journal(current); prefix = 'installation/'
    path = prefix + ('LICENSE' if scenario.startswith('license-') else 'metadata/native-provider.json')
    if scenario.startswith('descriptor-') and scenario in ('descriptor-byte-change', 'descriptor-duplicate-key', 'descriptor-unknown-field', 'descriptor-foreign-source', 'descriptor-oversize'):
        raw = base64.b64decode(before[path]['base64']); changed = {
            'descriptor-byte-change': raw.replace(b'[2,3,4]', b'[2,3,5]'),
            'descriptor-duplicate-key': b'{"protocols":[2,3,4],' + raw[1:],
            'descriptor-unknown-field': b'{"unapproved":true,' + raw[1:],
            'descriptor-foreign-source': raw.replace(b'https://github.com/zryna/zryna', b'https://github.com/other/zryna'),
            'descriptor-oversize': b'x' * 4097}[scenario]
        require(after[path]['sha256'] == digest(changed) != before[path]['sha256'], 'descriptor mutation bytes')
    elif scenario in ('license-byte-change', 'license-oversize'):
        raw = b'changed license\n' if scenario == 'license-byte-change' else b'x' * 65537
        require(after[path]['sha256'] == digest(raw), 'license mutation bytes')
    elif scenario.endswith('-missing'):
        require(path not in after, 'missing file mutation')
    elif scenario.endswith('-hardlink'):
        require(after[path]['links'] > 1, 'hardlink mutation')
    elif scenario.endswith('-symlink'):
        if scenario == 'metadata-directory-symlink':
            path = prefix + 'metadata'
        require(after[path]['kind'] == 'link', 'link mutation')
    else:
        target = {'extra-root-file': 'unapproved', 'extra-bin-file': 'bin/unapproved',
                  'extra-metadata-file': 'metadata/unapproved', 'extra-root-empty-directory': 'unapproved'}[scenario]
        require(prefix + target in after and prefix + target not in before, 'extra inventory mutation')


def pre_setup(j, previous, scenario):
    require(j['case'] == scenario and j['status'] == 'blocked-pre-mutation-setup' and j['baseline_passed'] is True
            and j['probe_executed_after_mutation'] is False, 'pre-mutation setup journal')
    error = j['error']
    require(set(error) == {'type', 'errno', 'winerror', 'message'} and type(error['message']) is str
            and len(error['message']) <= 8192 and all(type(error[k]) in (int, type(None)) for k in ('errno', 'winerror')), 'bounded setup error')
    same = j['before'] == j['after']; require(j['original_tree_unchanged'] is same, 'pre-mutation unchanged assertion')
    journal(j['before']); journal(j['after']); require(j['before'] == previous, 'setup follows actual baseline')
    denied = error['errno'] in (5, 13) or error['winerror'] in (5, 32, 33, 1314)
    return 'blocked' if same and denied else 'setup_failed'


def mutation_binding(mutation, execution_row, scenario, platform):
    if mutation.get('effective') is not True:
        require(package_update_only(execution_row['before'], execution_row['after']), 'OS prevention fixture changed beyond package Update')
        return
    require(mutation['id'] == scenario, 'selected mutation id')
    require(not package_update_only(execution_row['before'], execution_row['after']), 'actual mutation effect beyond package Update')
    install = scenario.startswith(('descriptor-', 'license-', 'executable-', 'installation-', 'extra-')) or scenario.startswith('callback-')
    prefix = 'installation/' if install else 'sources/'
    for stage in ('before', 'after'):
        whole = journal(execution_row[stage]); own = mutation[stage]
        if stage == 'after' and scenario == 'source-symlink-substitution':
            sparse_link_after(mutation, execution_row, platform)
            continue
        require(type(own) is dict and 0 < len(own) <= 1024, 'mutation closed journal')
        for name, row in own.items():
            key = prefix.rstrip('/') if name == '' else prefix + name.replace('\\', '/')
            require(key in whole and type(row['file']) is bool and type(row['directory']) is bool
                    and whole[key]['kind'] == ('file' if row['file'] else 'directory') and row['file'] != row['directory'], 'mutation tree/candidate crossbinding')
            if row.get('file'):
                require(type(row['bytes']) is int and row['bytes'] == whole[key]['bytes']
                        and row['sha256'] == whole[key]['sha256'], 'mutation bytes/candidate crossbinding')
                file_overlap(row, whole[key], platform, stage == 'before' and key == 'sources/packages/app/zryna.lock.json')
            else:
                require(all(exact(row[k], whole[key][k]) for k in ('device', 'inode', 'links') if k in row), 'mutation directory identity crossbinding')
                if 'mode' in row:
                    require(stat.S_ISDIR(row['mode']) and stat.S_IMODE(row['mode']) == whole[key]['mode'], 'mutation directory mode crossbinding')
    if scenario in ('installation-parent-replaced-identical-tree', 'source-parent-replaced'):
        old = journal(execution_row['before'])[prefix.rstrip('/')]; new = journal(execution_row['after'])[prefix.rstrip('/')]
        backup = journal(execution_row['after'])['mutation-backups/original-tree']
        require(old['kind'] == new['kind'] == backup['kind'] == 'directory'
                and [old['device'], old['inode']] != [new['device'], new['inode']]
                and exact([old['device'], old['inode']], [backup['device'], backup['inode']]), 'actual replaced parent/backup identity')
    target = {'descriptor-replaced-identical-bytes': 'metadata/native-provider.json', 'license-replaced-identical-bytes': 'LICENSE',
              'executable-path-replaced-identical-bytes': TARGETS[platform][1], 'source-replaced-identical-bytes': 'packages/app/main.zry',
              'package-manifest-replaced-identical-bytes': 'packages/app/zryna.package.json'}.get(scenario)
    if target:
        before = {k.replace('\\', '/'): v for k, v in mutation['before'].items()}; after = {k.replace('\\', '/'): v for k, v in mutation['after'].items()}
        require(before[target]['sha256'] == after[target]['sha256'] and before[target] != after[target], 'same-byte replacement identity')
        first = journal(execution_row['before'])[prefix + target]; last = journal(execution_row['after'])[prefix + target]
        backup = journal(execution_row['after'])['mutation-backups/original-file']
        require(first['sha256'] == last['sha256'] == backup['sha256'] and first['bytes'] == last['bytes'] == backup['bytes']
                and exact([first['device'], first['inode']], [backup['device'], backup['inode']])
                and [first['device'], first['inode']] != [last['device'], last['inode']], 'actual original/new replacement identity')


def file_overlap(row, whole, platform, prior_update_lock=False):
    # The probe resolves Update before Rust's capture; only this prior lock identity
    # may differ from Python's earlier checkpoint. Its bytes remain exact above.
    for key in ('device', 'links'):
        if key in row:
            require(exact(row[key], whole[key]), 'mutation file identity crossbinding')
    if 'mode' in row:
        require(stat.S_ISREG(row['mode']) and stat.S_IMODE(row['mode']) == whole['mode'], 'mutation file mode crossbinding')
    if not prior_update_lock:
        if 'inode' in row:
            require(exact(row['inode'], whole['inode']), 'mutation file inode crossbinding')
        if 'modified_ns' in row:
            require(row['modified_ns'] == str(whole['mtime_ns']), 'mutation file modification crossbinding')
        if platform == 'linux' and 'ctime' in row:
            require(exact(row['ctime'], [whole['ctime_ns'] // 1000000000, whole['ctime_ns'] % 1000000000]), 'mutation file change-time crossbinding')
        if platform == 'win32' and 'last_write_time' in row:
            require(type(row['last_write_time']) is int and (row['last_write_time'] - 116444736000000000) * 100 == whole['mtime_ns'], 'Windows mutation file write time')


def sparse_link_after(mutation, execution_row, platform):
    own = mutation['after']; pm = ntpath if platform == 'win32' else posixpath
    require(set(own) == {'substituted_path', 'path_entry', 'symlink_target', 'full_tree_snapshot'}
            and own['full_tree_snapshot'] == 'unavailable: deliberate link is rejected by snapshot guard', 'exact link observation')
    source = 'sources/packages/app/main.zry'; backup = 'mutation-backups/original-file'
    before = journal(execution_row['before']); after = journal(execution_row['after'])
    target = pm.join(execution_row['argv'][2], 'original-file')
    require(own['substituted_path'] == pm.join(execution_row['argv'][1], 'packages', 'app', 'main.zry')
            and own['symlink_target'] == mutation['backup'] == target
            and after[source]['kind'] == 'link' and after[source]['target'] == target, 'source link path/target')
    require(before[source]['kind'] == after[backup]['kind'] == 'file' and after[backup]['links'] == 1
            and before[source]['sha256'] == after[backup]['sha256'] == mutation['original_bytes_sha256']
            and before[source]['bytes'] == after[backup]['bytes'], 'original source backup bytes')
    entry = own['path_entry']; link = after[source]
    require(entry['file'] is False and entry['directory'] is False and type(entry['bytes']) is int
            and entry['bytes'] == link['bytes'] and entry['modified_ns'] == str(link['mtime_ns']), 'link entry observation')
    if platform == 'linux':
        require(stat.S_ISLNK(entry['mode']) and stat.S_IMODE(entry['mode']) == link['mode']
                and all(exact(entry[k], link[k]) for k in ('device', 'inode', 'links'))
                and entry['ctime'] == [link['ctime_ns'] // 1000000000, link['ctime_ns'] % 1000000000], 'Unix link identity')
    else:
        require(type(entry['attributes']) is int and entry['attributes'] & 0x400
                and (entry['last_write_time'] - 116444736000000000) * 100 == link['mtime_ns'], 'Windows link observation')
    old_files = {p: r['sha256'] for p, r in before.items() if r['kind'] == 'file' and p != source}
    new_files = {p: r['sha256'] for p, r in after.items() if r['kind'] == 'file' and p != backup}
    require(exact(old_files, new_files), 'unrelated mutation file bytes changed')


def selections():
    rows = [(name, name, protocol, None, None) for protocol, name in
            ((2, 'real-native-v2-package'), (3, 'real-native-v3-bounded-import-package'), (4, 'real-native-v4-ownership-package'))]
    rows += [(name + '-v' + str(p), name, p, None, None) for p in (2, 3, 4)
             for name in ('relocated-complete-installation', 'runtime-env-cannot-override-compiled-authority')]
    rows += [(name, name, 3, phase, boundary) for name, phase, boundary in REAL_CASES]
    rows += [(name, name, 3, 'after-source-capture' if name == 'source-extra-file' else None, None) for name in
             ('source-extra-file', 'unprepared-real-build', 'original-full-M2-package-byte-limit-rejection',
              'callback-license-change', 'callback-error-license-change', 'non-frozen-request', 'wrong-package-identity',
              'missing-private-marker', 'foreign-purpose-marker', 'modified-executable-bytes')]
    require(len(rows) == 50 and len({r[0] for r in rows}) == 50, 'oracle census')
    return rows


def summary(root, protocol):
    files = {name: digest(read(root / path)) for name, path in FIXTURES[protocol].items()}
    graph = None
    if protocol != 2:
        raw = bytearray(b'ZRYNA-M2-GRAPH\0' if protocol == 3 else b'ZRYNA-M3-GRAPH\0')
        def text(value):
            encoded = value.encode()
            raw.extend(len(encoded).to_bytes(4, 'little') + encoded)
        raw.extend((1).to_bytes(4, 'little')); text('main.zry')
        raw.extend(len(files).to_bytes(4, 'little'))
        for name, sha in sorted(files.items()):
            text(name); raw.extend(bytes.fromhex(sha))
        raw.extend((1 if protocol == 3 else 0).to_bytes(4, 'little'))
        if protocol == 3:
            for value in ('main.zry', './math.zry', 'truth', 'truth'):
                text(value)
        graph = digest(raw)
    return {'protocol': protocol, 'source_sha256': files, 'graph_sha256': graph}


def journal(rows):
    require(type(rows) is dict and 0 < len(rows) <= 1024, 'tree census')
    normalized = {}
    for path, row in rows.items():
        name = path.replace('\\', '/')
        require(name not in normalized and not name.startswith('/') and '..' not in name.split('/'), 'journal alias')
        require(row['kind'] in ('file', 'directory', 'link') and type(row['bytes']) is int and 0 <= row['bytes'] <= MAX, 'journal type/bound')
        require(all(type(row[k]) is int and 0 <= row[k] < 2**128 for k in ('device', 'inode', 'links', 'mode'))
                and row['links'] > 0 and row['mode'] <= 0o7777
                and all(type(row[k]) is int and abs(row[k]) < 2**128 for k in ('mtime_ns', 'ctime_ns')), 'typed journal identity')
        if row['kind'] == 'file':
            require(type(row['links']) is int and row['links'] > 0, 'journal link count type')
            require(re.fullmatch('[a-f0-9]{64}', row['sha256']), 'journal digest')
            if 'base64' in row:
                raw = base64.b64decode(row['base64'], validate=True)
                require(exact(file_binding(raw), {'bytes': row['bytes'], 'sha256': row['sha256']}), 'journal bytes')
            else:
                require(row.get('original_image_reference_sha256') == row['sha256'], 'unretained journal bytes')
        normalized[name] = row
    require(len({p.casefold() for p in normalized}) == len(normalized), 'journal case aliases')
    return normalized


def positive(record, expected):
    require(record['exit'] == 0 and record['result']['status'] == 'verified'
            and exact(record['result']['summary'], expected) and record['result'].get('mutation') is None, 'positive baseline')


def baseline_bytes(record, root, build, platform, protocol):
    rows = journal(record['before']); prefix = 'sources/packages/app/'
    require({k for k in rows if k.startswith('installation/')} ==
            {'installation/bin', 'installation/metadata', 'installation/LICENSE', 'installation/metadata/native-provider.json',
             'installation/' + TARGETS[platform][1]}, 'baseline installation census')
    for name, original in [('LICENSE', 'LICENSE'), ('metadata/native-provider.json', 'native-provider.json')]:
        require(rows['installation/' + name]['sha256'] == digest(read(build / original)), 'baseline installation bytes')
    for name, original in FIXTURES[protocol].items():
        require(rows[prefix + name]['sha256'] == digest(read(root / original)), 'baseline source bytes')
    require({k[len(prefix):] for k, r in rows.items() if k.startswith(prefix) and r['kind'] == 'file'}
            == set(FIXTURES[protocol]) | {'zryna.package.json'}, 'baseline package census')
    manifest = decode(base64.b64decode(rows[prefix + 'zryna.package.json']['base64'], validate=True))
    expected = {'compatibility': {'compiler': '0.1.0', 'profile': {2: 'i32-v1', 3: 'control-flow-v1', 4: 'data-ownership-v1'}[protocol],
                'targets': ['javascript', 'webassembly']}, 'dependencies': [],
                'files': [{'path': name, 'sha256': digest(read(root / path)), 'size': len(read(root / path))} for name, path in sorted(FIXTURES[protocol].items())],
                'format': 'zryna.package.v1', 'name': 'app', 'source': {'kind': 'local', 'locator': 'packages/app', 'revision': ''}, 'version': '1.0.0'}
    require(manifest == expected and base64.b64decode(rows[prefix + 'zryna.package.json']['base64']) == canonical(expected), 'baseline exact package')


def execution(folder, label, platform, protocol, scenario, image_sha):
    row = document(folder / (label + '-execution.json')); pm = ntpath if platform == 'win32' else posixpath
    require(type(row['exit']) is int and row['exit'] in (0, 2, 3) and row['direct_child_reaped'] is True, 'typed reaped exit')
    if row['exit'] == 3:
        require(row['result']['status'] == 'setup-failed' and row['result']['phase'] in ('mutation-setup', 'probe-setup'), 'setup failure exit')
    start = dt.datetime.fromisoformat(row['started_at']); end = dt.datetime.fromisoformat(row['completed_at'])
    require(start.tzinfo and end.tzinfo and 0 <= (end - start).total_seconds() <= 120, 'probe time')
    cwd = row['cwd']; argv = row['argv']; env = row['selected_environment']
    require(pm.isabs(cwd) and pm.basename(cwd) == folder.name and type(argv) is list and len(argv) == 5
            and argv[1:] == [pm.join(cwd, 'live', 'sources'), pm.join(cwd, 'live', 'mutation-backups'), scenario, str(protocol)], 'probe argv')
    allowed = {'PATH', 'HOME', 'LANG', 'LC_ALL', 'TMPDIR', 'SYSTEMROOT', 'SystemRoot', 'WINDIR', 'COMSPEC', 'PATHEXT'}
    if platform == 'win32':
        allowed |= {'TEMP', 'TMP'}
        require(env.get('TEMP') == env.get('TMP') == cwd, 'temporary environment')
    override = folder.name.startswith('runtime-env-cannot-override-compiled-authority-') and label == 'candidate'
    if override:
        allowed |= set(COMPILE_BINDINGS)
        require([env.get(k) for k in COMPILE_BINDINGS] == ['f'*64, 'e'*40, 'd'*40], 'runtime binding attack')
    require(set(env) <= allowed and env.get('PATH') == pm.join(cwd, 'live', 'empty-path')
            and env.get('HOME') == env.get('TMPDIR') == cwd and env.get('LANG') == env.get('LC_ALL') == 'C', 'probe environment')
    before = journal(row['before']); after = journal(row['after'])
    require(before['empty-path']['kind'] == after['empty-path']['kind'] == 'directory'
            and not any(k.startswith('empty-path/') for k in before.keys() | after.keys()), 'PATH not empty')
    exe_relative = pm.relpath(argv[0], pm.join(cwd, 'live')).replace('\\', '/')
    install = 'relocated-installation' if folder.name.startswith('relocated-complete-installation-v') and label == 'candidate' else 'installation'
    require(exe_relative == install + '/' + TARGETS[platform][1]
            and before[exe_relative]['kind'] == 'file' and before[exe_relative]['sha256'] == image_sha
            and before[exe_relative]['links'] == 1, 'executed image path/bytes')
    for stream in ('stdout', 'stderr'):
        raw = read(folder / (label + '.' + stream), 1024 * 1024)
        require(exact(row[stream], file_binding(raw)), 'raw execution stream')
        if stream == 'stdout':
            require(exact(decode(raw), row['result']), 'result/raw stdout')
        else:
            require(not raw, 'probe stderr')
    if row['result']['status'] == 'verified':
        require(row['result'].get('public_default_acceptance') is False and row['result'].get('runtime_or_backend_execution') is False, 'private scope')
    return row


def disposition(result, phase, observation=False):
    if result['exit'] == 3:
        require(result['result']['status'] == 'setup-failed' and result['result']['phase'] in ('mutation-setup', 'probe-setup'), 'runtime setup failure')
        return 'setup_failed'
    mutation = result['result'].get('mutation')
    if mutation:
        require(type(mutation['effective']) is bool, 'mutation effective type')
        if mutation['effective']:
            require(mutation['before'] != mutation['after'], 'mutation identity journal')
        if not mutation['effective']:
            p = mutation['prevention']
            require(p['original_tree_unchanged'] is True and p['exercised_rejection_credit'] is False
                    and (p.get('errno') in (5, 13, 32, 33, 1314) or p.get('kind') == 'PermissionDenied'), 'OS denial evidence')
            require(package_update_only(result['before'], result['after']), 'OS denial fixture changed beyond package Update')
            return 'blocked'
    if observation:
        require(mutation and mutation['effective'] and mutation['id'] == 'source-extra-file', 'owning observation mutation')
        require(result['exit'] in (0, 2), 'observation exit')
        require(result['result']['status'] == ('verified' if result['exit'] == 0 else 'rejected'), 'observation status')
        if result['exit'] == 2:
            require(result['result']['phase'] == 'syntax-verification' and all(d['code'] == 'ZRYNA-P4004' for d in result['result']['diagnostics']), 'owning observation rejection')
        return 'observation'
    require(result['exit'] == 2 and result['result']['status'] == 'rejected' and result['result']['phase'] == phase, 'rejection phase')
    require(result['result'].get('diagnostics'), 'rejection diagnostic')
    if phase == 'consumer-postcheck':
        require(type(result['result']['diagnostics']) is list and any(type(d) is dict and d.get('code') == 'ZRYNA-C4220'
                for d in result['result']['diagnostics']), 'callback postcheck')
    return 'rejected'


def verify(root, proof, head, platform, live=False, run_id=None, run_attempt=None):
    root = Path(root); proof = Path(proof)
    require(platform in TARGETS and re.fullmatch('[a-f0-9]{40}', head), 'head/platform')
    require(digest(read(root / 'tests/native-provider-qualification/installed-native-capability/independent_cases.py')) == ORACLE_SHA, 'frozen oracle')
    src = source_snapshot(root); require(src['head'] == head, 'exact source head')
    before_inputs = proof_inventory(proof)
    build, original = verify_build(root, proof, src, platform, live)
    receipt = document(proof / 'cases/receipt.json')
    require(exact(receipt['source_before'], src) and exact(receipt['source_after'], src) and receipt['head'] == head and receipt['tree'] == src['tree'], 'case source')
    pm = ntpath if platform == 'win32' else posixpath
    require(receipt['prepared_image'] == {k: build['prepared_image'][k] for k in build['prepared_image']}
            and receipt['unprepared_image'] == build['unprepared_image']
            and receipt['shared_actual_original_image_bytes_retained'] == pm.join(build['original_build_root'], 'prepared-image'), 'case/build raw image parity')
    expected = selections(); rows = receipt['cases']
    selection_census(rows)
    counts = dict(positive=0, rejected=0, external_initial=0, synthetic=0, observation=0, blocked=0, setup_failed=0, invocations=0)
    require({p.name for p in (proof / 'cases').iterdir()} == {'receipt.json'} | {r[0] for r in expected[:-3]}, 'case directory census')
    for selected, (ident, scenario, protocol, phase, boundary) in zip(rows, expected):
        folder = proof / 'cases' / ident
        if ident in ('missing-private-marker', 'foreign-purpose-marker', 'modified-executable-bytes'):
            forged = original.replace(MARKER_PREFIX, (b'x' if ident == 'missing-private-marker' else b'Y') * len(MARKER_PREFIX)) if ident != 'modified-executable-bytes' else original[:-1] + bytes([original[-1] ^ 1])
            require(selected['status'] == 'passed-external-image-reader-rejection' and selected['executed'] is False and selected['original_sha256'] == digest(original)
                    and selected['forged_sha256'] == digest(forged) != digest(original), 'nonexecuted synthetic bytes'); counts['synthetic'] += 1; continue
        no_baseline = ident in ('unprepared-real-build', 'original-full-M2-package-byte-limit-rejection')
        if 'protocol' in selected:
            require(type(selected['protocol']) is int and selected['protocol'] == protocol and selected['scenario'] == scenario, 'selected protocol/scenario')
        allowed = {'candidate-execution.json', 'candidate.stdout', 'candidate.stderr'}
        if not no_baseline:
            baseline = execution(folder, 'baseline', platform, protocol, 'positive', digest(original))
            positive(baseline, summary(root, protocol)); baseline_bytes(baseline, root, proof / 'build', platform, protocol)
            counts['invocations'] += 1
            require(pm.dirname(pm.dirname(baseline['cwd'])) == pm.dirname(build['original_build_root']), 'single original proof root')
            allowed |= {'baseline-execution.json', 'baseline.stdout', 'baseline.stderr'}
        if selected['status'] == 'blocked-pre-mutation-setup':
            j = document(folder / 'blocked-pre-mutation-setup.json')
            require(phase == 'pre-capture', 'pre setup owning phase'); counts[pre_setup(j, baseline['after'], scenario)] += 1
            allowed -= {'candidate-execution.json', 'candidate.stdout', 'candidate.stderr'}; allowed.add('blocked-pre-mutation-setup.json')
        elif ident in ('executable-hardlink', 'installation-parent-link'):
            j = document(folder / 'external-initial-image-rejection.json'); after = journal(j['after'])
            require(j['before_mutation_positive'] is True and j['case'] == ident and selected['probe_executed_after_mutation'] is False, 'external initial rejection')
            require(selected['status'] == 'passed-external-initial-image-rejection', 'external initial disposition')
            require(any(r['kind'] == 'link' for r in after.values()) if ident == 'installation-parent-link'
                    else after['installation/' + TARGETS[platform][1]]['links'] > 1, 'unsafe initial image evidence')
            counts['external_initial'] += 1; allowed -= {'candidate-execution.json', 'candidate.stdout', 'candidate.stderr'}; allowed.add('external-initial-image-rejection.json')
        else:
            mode = scenario if phase in ('after-capture', 'after-source-capture') or scenario.startswith('callback-') or scenario in ('non-frozen-request', 'wrong-package-identity') else 'positive'
            raw_sha = build['unprepared_image']['sha256'] if ident == 'unprepared-real-build' else digest(original)
            record = execution(folder, 'candidate', platform, protocol, mode, raw_sha); counts['invocations'] += 1
            if not no_baseline:
                require(dt.datetime.fromisoformat(baseline['completed_at']) <= dt.datetime.fromisoformat(record['started_at']), 'baseline before mutation')
            mutation = record['result'].get('mutation')
            if mutation:
                require(mutation['id'] == scenario, 'selected mutation id'); mutation_binding(mutation, record, scenario, platform)
            if phase == 'pre-capture':
                pre_mutation(scenario, baseline['after'], record['before'])
            if ident.startswith(('real-native-', 'relocated-complete-', 'runtime-env-')):
                positive(record, summary(root, protocol)); counts['positive'] += 1
                require(selected['status'] == 'passed-real-private-capability', 'positive label')
            else:
                expected_phase = {'pre-capture': 'installation-capture', 'after-capture': 'source-capture', 'after-source-capture': 'syntax-verification'}.get(phase)
                expected_phase = expected_phase or ('installation-capture' if no_baseline and ident == 'unprepared-real-build' else 'package-setup' if no_baseline else 'consumer-postcheck' if ident.startswith('callback-') else 'source-capture')
                kind = disposition(record, expected_phase, ident == 'source-extra-file'); counts[kind] += 1
                if kind == 'observation' and record['exit'] == 0:
                    require(record['result']['summary'] == summary(root, protocol), 'owning observation actual summary')
                require(selected['status'] == {'blocked': 'blocked-effective-mutation; observed-OS-prevention',
                        'setup_failed': 'blocked-runtime-mutation-setup', 'observation': 'observed-owning-package-contract',
                        'rejected': 'passed-real-rejection'}[kind], 'derived disposition label')
                if ident == 'original-full-M2-package-byte-limit-rejection':
                    require(record['result']['diagnostics'] == {'code': 'ZRYNA-P4004', 'message': 'ZRYNA-P4004: package file exceeds its byte limit'}, 'full M2 bound')
            require(selected['candidate_exit'] == record['exit'] and selected['phase'] == record['result'].get('phase'), 'selection/raw result')
            if boundary:
                require(selected['declared_boundary'] == boundary, 'frozen boundary')
        require({p.name for p in folder.iterdir()} == allowed, 'per-case closed files')
    require(receipt['private_only'] is True and receipt['public_defaults_changed'] is False
            and receipt['ordinary_no_Node_acceptance'] is False, 'qualification scope')
    if (proof / 'ci-receipt.json').exists():
        ci = document(proof / 'ci-receipt.json')
        require(ci['head'] == head and ci['tree'] == src['tree'] and ci['platform'] == platform
                and exact(ci['source_before'], src) and exact(ci['source_after'], src) and type(ci['case_selections']) is int and ci['case_selections'] == 50, 'CI source')
        require((run_id is None or ci['run_id'] == run_id) and (run_attempt is None or ci['run_attempt'] == run_attempt), 'CI run')
        require(type(ci['schema_version']) is int and ci['schema_version'] == 1 and ci['public_defaults_changed'] is False
                and ci['ordinary_no_Node_acceptance'] is False and re.fullmatch('[1-9][0-9]*', ci['run_id'])
                and re.fullmatch('[1-9][0-9]*', ci['run_attempt']), 'CI typed identity/private scope')
        verify_unit(root, proof, src, build['windows_toolchain'], build['original_source_root'],
                    pm.dirname(build['original_build_root']), build['original_target'])
    require(platform != 'win32' or (proof / 'ci-receipt.json').exists(), 'Windows CI receipt/unit required')
    require(source_snapshot(root) == src, 'source changed during admission')
    files = {'cases/receipt.json', *{'build/' + n for n in ('build-receipt.json', 'native-provider.json', 'LICENSE', 'prepared-image',
             'unprepared-image', 'pinned-tool-identities.json', 'cargo-proof/Cargo.toml', 'cargo-proof/Cargo.lock', 'cargo-proof/probe.rs', 'cargo-proof/mutations.rs')}}
    files |= {'build/' + label + suffix for label in BUILD_LABELS for suffix in ('-execution.json', '.stdout', '.stderr')}
    files |= {p.relative_to(proof).as_posix() for p in (proof / 'cases').glob('*/*')}
    if platform == 'win32':
        files |= {'toolchain/before.json', 'toolchain/after.json'}
        files |= {'toolchain/' + r['raw_relative_path'] for r in [build['windows_toolchain']['python'], *build['windows_toolchain']['tools'].values(), *build['windows_toolchain']['sdk_inputs'].values()]}
        files |= {'toolchain/observed-' + role + '-version' + suffix for role in ('python', 'cargo', 'rustc', 'linker', 'dumpbin') for suffix in ('-execution.json', '.stdout', '.stderr')}
    if (proof / 'ci-receipt.json').exists():
        files |= {'ci-receipt.json', 'private-descriptor-unit-image.exe', 'private-descriptor-unit-image-binding.json',
                  'private-descriptor-units-execution.json', 'private-descriptor-units.stdout', 'private-descriptor-units.stderr'}
    closed_files(proof, files)
    require(proof_inventory(proof) == before_inputs, 'proof changed during independent admission')
    complete = counts['blocked'] == counts['setup_failed'] == 0
    return {'status': 'passed' if complete else 'incomplete', 'evidence_valid': True, 'qualification_complete': complete,
            'head': head, 'tree': src['tree'], 'platform': platform, 'counts': counts, 'source_files': len(src['files']),
            'input_sha256': {p: r['sha256'] for p, r in before_inputs.items()},
            'public_activation': False, 'ordinary_no_Node_acceptance': False,
            'hosted_authority_requires_external_authenticated_source_job_log_artifact_binding': True}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for key in ('root', 'proof', 'head', 'platform'):
        parser.add_argument('--' + key, required=True)
    parser.add_argument('--live', action='store_true')
    parser.add_argument('--run-id'); parser.add_argument('--run-attempt')
    args = parser.parse_args()
    result = verify(**vars(args))
    print(json.dumps(result, sort_keys=True))
    sys.exit(0 if result['qualification_complete'] else 2)
