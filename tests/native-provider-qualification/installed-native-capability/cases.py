"""Closed real probe setup and complete byte/state capture for test-owned fixtures."""
import base64
import hashlib
import os
import pathlib
import stat

from build import sha, write
from independent_cases import DESCRIPTOR_PATH, TARGETS, canonical_bytes


FIXTURES = {
    2: {'main.zry': 'examples/universal/add.zry'},
    3: {'main.zry': 'crates/zryna-frontend/tests/native_parser_v3_calls/main.zry',
        'math.zry': 'crates/zryna-frontend/tests/native_parser_v3_calls/math.zry'},
    4: {'main.zry': 'tests/m3-fixtures/conformance/pair.zry'},
}


def materialize(source, build, case, protocol, platform, unprepared=False, full_m2=False):
    live = case / 'live'
    live.mkdir(mode=0o700)
    install = live / 'installation'
    project = live / 'sources'
    backups = live / 'mutation-backups'
    backups.mkdir(mode=0o700)
    image = build / ('unprepared-image' if unprepared else 'prepared-image')
    executable = install / TARGETS[platform][1]
    write(executable, image.read_bytes(), 0o700)
    write(install / DESCRIPTOR_PATH, (build / 'native-provider.json').read_bytes())
    write(install / 'LICENSE', (build / 'LICENSE').read_bytes())
    files = dict(FIXTURES[protocol])
    if full_m2:
        files = {'main.zry': 'tests/m2-fixtures/valid/main.zry',
                 'math.zry': 'tests/m2-fixtures/valid/math.zry'}
    inventory = []
    for name, original in sorted(files.items()):
        raw = (source / original).read_bytes()
        write(project / 'packages/app' / name, raw)
        inventory.append({'path': name, 'sha256': sha(raw), 'size': len(raw)})
    profile = {2: 'i32-v1', 3: 'control-flow-v1', 4: 'data-ownership-v1'}[protocol]
    manifest = {'compatibility': {'compiler': '0.1.0', 'profile': profile,
                'targets': ['javascript', 'webassembly']}, 'dependencies': [],
                'files': inventory, 'format': 'zryna.package.v1', 'name': 'app',
                'source': {'kind': 'local', 'locator': 'packages/app', 'revision': ''},
                'version': '1.0.0'}
    write(project / 'packages/app/zryna.package.json', canonical_bytes(manifest))
    empty = live / 'empty-path'
    empty.mkdir(mode=0o700)
    return install, project, backups, empty, executable


def tree(root):
    """Record symlinks as links; never follow an invented fixture entry."""
    rows = {}
    stack = [root]
    while stack:
        path = stack.pop()
        info = path.lstat()
        relative = str(path.relative_to(root))
        row = {'mode': stat.S_IMODE(info.st_mode), 'device': info.st_dev, 'inode': info.st_ino,
               'links': info.st_nlink, 'bytes': info.st_size, 'mtime_ns': info.st_mtime_ns,
               'ctime_ns': info.st_ctime_ns}
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            row.update(kind='link', target=os.readlink(path))
        elif stat.S_ISDIR(info.st_mode):
            row['kind'] = 'directory'
            stack.extend(sorted(path.iterdir(), reverse=True))
        elif stat.S_ISREG(info.st_mode):
            assert info.st_size <= 128 * 1024 * 1024
            raw = path.read_bytes()
            row.update(kind='file', sha256=sha(raw))
            if len(raw) <= 128 * 1024:
                row['base64'] = base64.b64encode(raw).decode()
            else:
                row['original_image_reference_sha256'] = sha(raw)
        else:
            row['kind'] = 'special'
        rows[relative] = row
        assert len(rows) <= 1024
    return rows


def expected_summary(source, protocol):
    files = {name: sha((source / path).read_bytes()) for name, path in FIXTURES[protocol].items()}
    graph = None
    if protocol == 3:
        graph = '077d021020d7a785094bb5ffaa47080d44b60eed26583fba4133167ebe2736c8'
    if protocol == 4:
        document = bytearray(b'ZRYNA-M3-GRAPH\0')
        def text(value):
            raw = value.encode()
            document.extend(len(raw).to_bytes(4, 'little'))
            document.extend(raw)
        document.extend((1).to_bytes(4, 'little'))
        text('main.zry')
        document.extend((1).to_bytes(4, 'little'))
        text('main.zry')
        document.extend(bytes.fromhex(files['main.zry']))
        document.extend((0).to_bytes(4, 'little'))
        graph = hashlib.sha256(document).hexdigest()
    return {'protocol': protocol, 'source_sha256': files, 'graph_sha256': graph}


def pre_mutation(case_id, install, project, outside, executable):
    descriptor = install / DESCRIPTOR_PATH
    license_path = install / 'LICENSE'
    if case_id.startswith('descriptor-') and case_id in {
            'descriptor-byte-change', 'descriptor-duplicate-key', 'descriptor-unknown-field',
            'descriptor-foreign-source', 'descriptor-oversize'}:
        raw = descriptor.read_bytes()
        if case_id == 'descriptor-byte-change':
            raw = raw.replace(b'[2,3,4]', b'[2,3,5]')
        elif case_id == 'descriptor-duplicate-key':
            raw = b'{"protocols":[2,3,4],' + raw[1:]
        elif case_id == 'descriptor-unknown-field':
            raw = b'{"unapproved":true,' + raw[1:]
        elif case_id == 'descriptor-foreign-source':
            raw = raw.replace(b'https://github.com/zryna/zryna', b'https://github.com/other/zryna')
        else:
            raw = b'x' * 4097
        descriptor.write_bytes(raw)
    elif case_id == 'license-byte-change':
        license_path.write_bytes(b'changed license\n')
    elif case_id == 'license-oversize':
        license_path.write_bytes(b'x' * 65537)
    elif case_id in ('descriptor-missing', 'license-missing'):
        (descriptor if case_id == 'descriptor-missing' else license_path).rename(outside / 'original')
    elif case_id in ('extra-root-file', 'extra-bin-file', 'extra-metadata-file'):
        directory = {'extra-root-file': install, 'extra-bin-file': install / 'bin',
                     'extra-metadata-file': install / 'metadata'}[case_id]
        write(directory / 'unapproved', b'extra\n')
    elif case_id == 'extra-root-empty-directory':
        (install / 'unapproved').mkdir(mode=0o700)
    elif case_id in ('descriptor-hardlink', 'license-hardlink', 'executable-hardlink'):
        original = {'descriptor-hardlink': descriptor, 'license-hardlink': license_path,
                    'executable-hardlink': executable}[case_id]
        os.link(original, outside / 'unapproved-hardlink')
    elif case_id in ('descriptor-symlink', 'license-symlink', 'metadata-directory-symlink'):
        original = {'descriptor-symlink': descriptor, 'license-symlink': license_path,
                    'metadata-directory-symlink': install / 'metadata'}[case_id]
        destination = outside / 'original'
        original.rename(destination)
        os.symlink(destination, original, target_is_directory=case_id == 'metadata-directory-symlink')
    elif case_id == 'installation-parent-link':
        alias = outside / 'installation-alias'
        os.symlink(install, alias, target_is_directory=True)
        return alias / executable.relative_to(install)
    else:
        raise ValueError('unsupported pre-capture case: ' + case_id)
    return executable
