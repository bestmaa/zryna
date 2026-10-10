"""Complete 33 BUILD plus 36 rejection proof ZIP: 621 regular files and one explicit empty directory."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import stat
import sys
import zipfile

sys.dont_write_bytecode = True
if not __debug__:
    raise RuntimeError('archive admission requires Python assertions')

BUILD_LABELS = ('cargo-version', 'rustc-version', 'node-version', 'default-build',
                'feature-build', 'cli-smoke')
EMPTY = 'empty-runtime-path/'
MAX_FILE = 32 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024
MAX_ARCHIVE = 32 * 1024 * 1024


def expected_files():
    """Closed corpus census comes from the committed, hash-pinned independent registry."""
    import importlib.util
    spec = importlib.util.spec_from_file_location('corpus_archive_contracts',
                                                Path(__file__).with_name('contracts.py'))
    c = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(c)
    registry = Path(__file__).with_name('fixtures.json')
    assert c.digest(registry) == c.REGISTRY_SHA
    rows = c.read(registry)['fixtures']
    names = {'receipt.json', 'cli-build/ci-receipt.json', 'cli-build/smoke/receipt.json'}
    for label in BUILD_LABELS:
        names.update('cli-build/' + label + '.' + suffix for suffix in ('stdout', 'stderr'))
    for row in rows:
        label = row['id']
        stem = 'corpus-build-' + label
        accepted = row['disposition'] == 'accepted'
        for role in (('native', 'create-only', 'bootstrap') if accepted else ('native', 'bootstrap')):
            names.update(label + '-' + role + '.' + suffix for suffix in ('stdout', 'stderr'))
        if accepted:
            for role in ('native', 'bootstrap'):
                prefix = label + '-' + role + '-bundle/'
                names.add(prefix + 'zryna-manifest-v3.json')
                names.update(prefix + target + '/' + stem + '.' + extension for target, extension in
                             (('javascript', 'mjs'), ('webassembly', 'wasm'), ('native', 'o')))
    assert len(names) == 621
    return names


def real(path, directory=False):
    info = path.lstat()
    assert not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400
    assert (stat.S_ISDIR if directory else stat.S_ISREG)(info.st_mode), str(path)
    return info


def location(path):
    path = Path(path).absolute()
    assert '..' not in path.parts
    for parent in path.parents:
        real(parent, True)
    return path


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise AssertionError('destination already exists: ' + str(path))


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def reader():
    spec = importlib.util.spec_from_file_location('complete_archive_reader',
                                                Path(__file__).with_name('verify.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.verify


def snapshot(proof):
    real(proof, True)
    files, directories = {}, set()

    def walk(path):
        info = path.lstat()
        is_directory = stat.S_ISDIR(info.st_mode)
        real(path, is_directory)
        name = path.relative_to(proof).as_posix()
        if is_directory:
            if path != proof:
                directories.add(name)
            assert len(files) + len(directories) <= 4096
            for child in sorted(path.iterdir()):
                walk(child)
        else:
            assert info.st_size <= MAX_FILE
            raw = path.read_bytes()
            assert len(raw) == info.st_size
            files[name] = raw
            assert len(files) <= 621 and sum(map(len, files.values())) <= MAX_TOTAL
    walk(proof)
    assert set(files) == expected_files(), 'unexpected or missing proof file'
    expected_dirs = {EMPTY.rstrip('/')}
    for name in files:
        expected_dirs.update(str(p) for p in PurePosixPath(name).parents if str(p) != '.')
    assert directories == expected_dirs, 'unexpected or missing proof directory'
    return files


def zip_info(name, directory=False):
    info = zipfile.ZipInfo(name)
    info.create_system = 3
    info.compress_type = zipfile.ZIP_STORED if directory else zipfile.ZIP_DEFLATED
    info.external_attr = ((stat.S_IFDIR | 0o700) if directory else (stat.S_IFREG | 0o600)) << 16
    if directory:
        info.external_attr |= 0x10
    return info


def pack(root, proof, head, platform, archive, *, reader_override=None):
    """The CLI always invokes the current real live reader; injection is Python-only."""
    proof, archive = location(proof), location(archive)
    assert not archive.is_relative_to(proof), 'archive must be outside the proof tree'
    absent(archive)
    verify = reader_override if reader_override is not None else reader()
    admitted = verify(root, proof, head, platform, live=True)
    assert admitted['status'] == 'passed' and admitted['live'] is True
    assert admitted['head'] == head and admitted['platform'] == platform
    assert admitted['counts'] == {'passed': 69, 'failed': 0, 'ignored': 0}
    files = snapshot(proof)
    with archive.open('xb') as stream, zipfile.ZipFile(stream, 'w') as output:
        output.writestr(zip_info(EMPTY, True), b'')
        for name, raw in sorted(files.items()):
            output.writestr(zip_info(name), raw)
    assert archive.stat().st_size <= MAX_ARCHIVE
    assert snapshot(proof) == files, 'proof changed while packaging'
    packed, _ = validate(archive)
    assert packed == files, 'packed bytes differ from admitted proof'
    return {'status': 'passed', 'head': head, 'platform': platform, 'file_count': 621,
            'directory_entries': [EMPTY], 'archive_sha256': digest(archive.read_bytes()),
            'archive_bytes': archive.stat().st_size, 'live_admission': admitted}


def validate(archive):
    archive = location(archive)
    info = real(archive)
    assert info.st_size <= MAX_ARCHIVE
    expected = expected_files()
    files, names, empty = {}, set(), 0
    with zipfile.ZipFile(archive) as source:
        entries = source.infolist()
        assert len(entries) == 622
        assert sum(item.file_size for item in entries) <= MAX_TOTAL
        for item in entries:
            name = item.filename
            assert item.orig_filename == name, 'normalized raw ZIP name'
            assert name.isascii() and name and '\\' not in name and ':' not in name
            path = PurePosixPath(name)
            assert not path.is_absolute() and all(p not in ('', '.', '..') for p in path.parts)
            assert name == path.as_posix() + ('/' if item.is_dir() else ''), 'noncanonical ZIP name'
            assert name.casefold() not in names, 'duplicate or case-colliding ZIP name'
            names.add(name.casefold())
            assert not item.flag_bits & ~0x808, 'unsupported or encrypted ZIP flags'
            assert item.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
            assert item.create_system == 3, 'ZIP entry lacks explicit portable file type'
            mode = item.external_attr >> 16
            if item.is_dir():
                assert name == EMPTY and stat.S_ISDIR(mode)
                assert item.external_attr == (stat.S_IFDIR | 0o700) << 16 | 0x10
                assert item.file_size == 0 and source.read(item) == b''
                empty += 1
            else:
                assert stat.S_ISREG(mode) and item.external_attr == (stat.S_IFREG | 0o600) << 16
                assert name in expected, 'unexpected proof file'
                assert item.file_size <= MAX_FILE
                raw = source.read(item)
                assert len(raw) == item.file_size
                files[name] = raw
    assert empty == 1 and set(files) == expected, 'incomplete proof archive'
    return files, archive


def recover(archive, output):
    """Validate all bytes before extraction; create the directory only from its ZIP entry."""
    output = location(output)
    absent(output)
    files, archive = validate(archive)
    output.mkdir()
    (output / EMPTY.rstrip('/')).mkdir()
    for name, raw in sorted(files.items()):
        target = output.joinpath(*PurePosixPath(name).parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as stream:
            stream.write(raw)
    assert snapshot(output) == files
    return {'status': 'passed', 'file_count': 621, 'directory_entries': [EMPTY],
            'archive_sha256': digest(archive.read_bytes()), 'archive_bytes': archive.stat().st_size,
            'files': {name: {'bytes': len(raw), 'sha256': digest(raw)} for name, raw in files.items()}}


def admit(root, archive, output, head, platform, run_id=None, run_attempt=None):
    recovered = recover(archive, output)
    admitted = reader()(root, output, head, platform, run_id=run_id, run_attempt=run_attempt)
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
