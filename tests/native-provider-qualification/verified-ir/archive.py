"""Preserve and admit the original closed complete-IR proof ZIP without reconstruction."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import stat
import sys
import zipfile
import archive_contract as c

sys.dont_write_bytecode = True
if not __debug__:
    raise RuntimeError('complete archive admission forbids optimized Python')


def real(path, directory=False):
    info = path.lstat()
    c.require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400,
              'linked or reparse proof entry: ' + str(path))
    c.require((stat.S_ISDIR if directory else stat.S_ISREG)(info.st_mode),
              'ordinary proof entry: ' + str(path))
    return info


def location(path):
    path = Path(path).absolute()
    c.require('..' not in path.parts, 'contained filesystem path')
    for parent in path.parents:
        real(parent, True)
    return path


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise ValueError('create-only destination exists: ' + str(path))


def identity(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_mode


def handle_identity(info):
    # Windows path stat adds filename-derived .exe execute bits that fstat lacks.
    # Type/identity/content metadata still agree; full path modes remain checked below.
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, stat.S_IFMT(info.st_mode)


def read_file(path, limit):
    path = location(path)
    before = real(path)
    c.require(before.st_size <= limit, 'retained file size bound')
    with path.open('rb') as stream:
        c.require(handle_identity(os.fstat(stream.fileno())) == handle_identity(before),
                  'stable opened regular file')
        raw = stream.read(limit + 1)
        c.require(handle_identity(os.fstat(stream.fileno())) == handle_identity(before),
                  'stable regular file read')
    c.require(identity(real(path)) == identity(before), 'stable retained file path')
    c.require(len(raw) == before.st_size and len(raw) <= limit, 'bounded complete file bytes')
    return raw


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def census(root, platform):
    root = location(root)
    real(root, True)
    inventory = read_file(root / c.INVENTORY, c.MAX_FILE)
    files = c.expected_files(inventory, platform)
    return inventory, files, c.expected_directories(files)


def snapshot(proof, files, directories, platform):
    proof = location(proof)
    real(proof, True)
    found, found_dirs, total = {}, set(), 0

    def walk(path, depth):
        nonlocal total
        c.require(depth <= 32, 'bounded proof depth')
        info = path.lstat()
        directory = stat.S_ISDIR(info.st_mode)
        real(path, directory)
        name = path.relative_to(proof).as_posix()
        if directory:
            if path != proof:
                c.require(name in directories, 'unexpected proof directory')
                found_dirs.add(name)
            for child in sorted(path.iterdir()):
                walk(child, depth + 1)
        else:
            c.require(name in files, 'unexpected proof regular file')
            raw = read_file(path, c.member_limit(name, platform))
            found[name] = raw
            total += len(raw)
            c.require(total <= c.MAX_TOTAL, 'aggregate proof size bound')
        c.require(len(found) + len(found_dirs) <= len(files) + len(directories), 'bounded proof census')

    walk(proof, 0)
    c.require(set(found) == files and found_dirs == directories, 'complete proof role census')
    return found


def reader():
    spec = importlib.util.spec_from_file_location('complete_ir_archive_admission',
                                                Path(__file__).with_name('admission.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.verify


def check_admission(value, head, platform, live):
    c.require(value['status'] == 'passed' and value['head'] == head
              and value['platform'] == platform and value['live'] is live, 'current runner admission')
    c.require(type(value['counts']) is dict
              and all(type(n) is int for n in value['counts'].values())
              and value['counts'] == {'passed': 107, 'failed': 0, 'ignored': 0}, '107 admitted cases')
    c.require(type(value['hostile_controls']) is int and value['hostile_controls'] == 49,
              '49 actual receipt controls')
    c.require(type(value['commands']) is int and value['commands'] == 6
              and value['public_activation'] is False, 'six private command admissions')


def zip_info(name, directory=False):
    info = zipfile.ZipInfo(name + ('/' if directory else ''))
    info.create_system = 3
    info.compress_type = zipfile.ZIP_STORED if directory else zipfile.ZIP_DEFLATED
    info.external_attr = ((stat.S_IFDIR | 0o700) if directory else (stat.S_IFREG | 0o600)) << 16
    if directory:
        info.external_attr |= 0x10
    return info


def validate(root, archive, platform):
    """Read every original member and CRC before recovery can create any destination."""
    inventory, expected, directories = census(root, platform)
    raw_archive = read_file(archive, c.MAX_ARCHIVE)
    files, seen, found_dirs = {}, set(), set()
    with zipfile.ZipFile(io.BytesIO(raw_archive)) as source:
        entries = source.infolist()
        c.require(len(entries) == len(expected) + len(directories), 'exact original ZIP entry census')
        c.require(sum(item.file_size for item in entries) <= c.MAX_TOTAL, 'ZIP aggregate size bound')
        for item in entries:
            name = item.filename
            c.require(item.orig_filename == name, 'original raw ZIP name')
            directory = item.is_dir()
            role = name[:-1] if directory else name
            c.canonical(role)
            c.require(name == PurePosixPath(role).as_posix() + ('/' if directory else ''),
                      'canonical original ZIP name')
            c.require(role.casefold() not in seen, 'duplicate or case-colliding ZIP entry')
            seen.add(role.casefold())
            c.require(not item.flag_bits & ~0x808, 'unsupported or encrypted ZIP flags')
            c.require(item.create_system == 3, 'explicit original portable ZIP type')
            c.require(item.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED),
                      'supported ZIP compression')
            expected_attr = (((stat.S_IFDIR | 0o700) << 16) | 0x10 if directory
                             else (stat.S_IFREG | 0o600) << 16)
            c.require(item.external_attr == expected_attr, 'exact portable ZIP mode and type')
            if directory:
                c.require(role in directories and item.file_size == 0, 'original directory role')
                c.require(source.read(item) == b'', 'original empty directory entry bytes')
                found_dirs.add(role)
            else:
                c.require(role in expected, 'original regular-file role')
                c.require(item.file_size <= c.member_limit(role, platform), 'ZIP member size bound')
                raw = source.read(item)
                c.require(len(raw) == item.file_size, 'complete original member bytes')
                files[role] = raw
    c.require(set(files) == expected and found_dirs == directories, 'closed original archive census')
    c.require(files['inventory.json'] == inventory, 'archive inventory equals trusted current source')
    return files, directories, raw_archive


def pack(root, proof, head, platform, archive, *, reader_override=None):
    """Explicit Python injection is guard-test-only; CLI always calls real live admission."""
    root, proof, archive = location(root), location(proof), location(archive)
    c.require(not proof.is_relative_to(root) and not archive.is_relative_to(root),
              'proof and archive remain outside controlled source')
    c.require(not archive.is_relative_to(proof), 'archive outside proof tree')
    absent(archive)
    inventory, expected, directories = census(root, platform)
    admitted = (reader_override or reader())(root, proof, head, platform, live=True)
    check_admission(admitted, head, platform, True)
    files = snapshot(proof, expected, directories, platform)
    c.require(files['inventory.json'] == inventory, 'retained inventory equals source authority')
    with archive.open('xb') as stream, zipfile.ZipFile(stream, 'w') as output:
        for name in sorted(directories):
            output.writestr(zip_info(name, True), b'')
        for name, raw in sorted(files.items()):
            output.writestr(zip_info(name), raw)
    c.require(snapshot(proof, expected, directories, platform) == files, 'proof unchanged while packaging')
    packed, packed_dirs, raw = validate(root, archive, platform)
    c.require(packed == files and packed_dirs == directories, 'all packed original bytes')
    return {'status': 'passed', 'head': head, 'platform': platform, 'file_count': 393,
            'directory_entries': sorted(name + '/' for name in directories),
            'archive_sha256': digest(raw), 'archive_bytes': len(raw), 'live_admission': admitted}


def recover(root, archive, output, platform):
    root, output = location(root), location(output)
    c.require(not output.is_relative_to(root), 'recovery remains outside controlled source')
    absent(output)
    files, directories, raw_archive = validate(root, archive, platform)
    output.mkdir(mode=0o700)
    for name in sorted(directories, key=lambda value: (len(PurePosixPath(value).parts), value)):
        output.joinpath(*PurePosixPath(name).parts).mkdir(mode=0o700)
    for name, raw in sorted(files.items()):
        target = output.joinpath(*PurePosixPath(name).parts)
        with target.open('xb') as stream:
            stream.write(raw)
        target.chmod(0o600)
    c.require(snapshot(output, frozenset(files), directories, platform) == files, 'all recovered original bytes')
    return {'status': 'passed', 'file_count': 393, 'archive_sha256': digest(raw_archive),
            'archive_bytes': len(raw_archive), 'directory_entries': sorted(n + '/' for n in directories),
            'files': {n: {'bytes': len(b), 'sha256': digest(b)} for n, b in files.items()}}


def admit(root, archive, output, head, platform, run_id=None, run_attempt=None):
    recovered = recover(root, archive, output, platform)
    admitted = reader()(root, output, head, platform, live=False, run_id=run_id, run_attempt=run_attempt)
    check_admission(admitted, head, platform, False)
    return {'status': 'passed', 'recovery': recovered, 'admission': admitted}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('pack', 'admit'):
        command = commands.add_parser(name)
        for field in ('root', 'archive', 'proof' if name == 'pack' else 'output'):
            command.add_argument('--' + field, type=Path, required=True)
        command.add_argument('--head', required=True)
        command.add_argument('--platform', choices=('linux', 'win32'), required=True)
        if name == 'admit':
            command.add_argument('--run-id')
            command.add_argument('--run-attempt')
    args = parser.parse_args()
    if args.command == 'pack':
        result = pack(args.root, args.proof, args.head, args.platform, args.archive)
    else:
        result = admit(args.root, args.archive, args.output, args.head, args.platform,
                       args.run_id, args.run_attempt)
    print(json.dumps(result))


if __name__ == '__main__':
    main()
