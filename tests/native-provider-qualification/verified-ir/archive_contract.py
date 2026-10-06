"""Finite original archive roles; this module grants no modeled IR acceptance."""
import json
from pathlib import Path, PurePosixPath

LABELS = ('lock', 'build', 'format', 'clippy', 'collector', 'hostile-controls')
INVENTORY = 'tests/native-provider-qualification/verified-ir/inventory.json'
MAX_FILE = 32 * 1024 * 1024
MAX_BINARY = 128 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
MAX_ARCHIVE = 32 * 1024 * 1024


def require(value, message):
    if not value:
        raise ValueError(message)


def canonical(name):
    require(type(name) is str and name and name.isascii(), 'ASCII archive role')
    require('\\' not in name and ':' not in name and '\0' not in name, 'portable archive role')
    path = PurePosixPath(name)
    require(not path.is_absolute() and path.as_posix() == name, 'canonical archive role')
    require(all(part not in ('', '.', '..') for part in path.parts), 'contained archive role')
    reserved = {'CON', 'PRN', 'AUX', 'NUL', *(f'COM{n}' for n in range(1, 10)),
                *(f'LPT{n}' for n in range(1, 10))}
    for part in path.parts:
        require(all(0x21 <= ord(char) <= 0x7e and char not in '<>"|?*' for char in part),
                'visible portable path segment')
        require(not part.endswith(('.', ' ')) and part.split('.')[0].upper() not in reserved,
                'no Windows-normalized alias')
    return path


def strict_json(raw):
    def unique(rows):
        result = {}
        for key, value in rows:
            require(key not in result, 'duplicate inventory JSON key')
            result[key] = value
        return result

    def reject(value):
        raise ValueError('noninteger inventory number: ' + value)

    return json.loads(raw, object_pairs_hook=unique, parse_float=reject, parse_constant=reject)


def binary_name(platform):
    require(platform in ('linux', 'win32'), 'supported proof platform')
    return 'verified-ir-proof' + ('.exe' if platform == 'win32' else '')


def expected_files(inventory_bytes, platform):
    """The caller supplies trusted current-source inventory, never an archive replacement."""
    inventory = strict_json(inventory_bytes)
    require(type(inventory) is dict and type(inventory.get('cases')) is list, 'source inventory')
    require(len(inventory['cases']) == 107, '107 committed source contexts')
    names = {'inventory.json', 'baseline-corpus.json', 'baseline-runner.json',
             'corpus.json', 'admission.json', 'receipt.json', binary_name(platform)}
    names.update(label + '.' + suffix for label in LABELS for suffix in ('stdout', 'stderr'))
    names.update('package/' + name for name in
                 ('Cargo.toml', 'Cargo.lock', 'rustfmt.toml', 'rust-toolchain.toml',
                  'src/main.rs', 'src/projection.rs', 'src/providers.rs'))
    sources = 0
    ids = set()
    for ordinal, case in enumerate(inventory['cases']):
        require(type(case) is dict and type(case.get('id')) is str, 'typed source context')
        require(case['id'] not in ids, 'unique committed context ID')
        ids.add(case['id'])
        require(type(case.get('sources')) is list and case['sources'], 'original source closure')
        for index, source in enumerate(case['sources']):
            require(type(source) is dict and type(source.get('file_id')) is int
                    and source['file_id'] == index, 'dense original FileIds')
            relative = canonical(source['path']).as_posix()
            name = f'observations/case-{ordinal:03}/sources/{relative}'
            require(name not in names, 'unique original source role')
            names.add(name)
            sources += 1
        names.update(f'observations/case-{ordinal:03}/{provider}.json'
                     for provider in ('worker', 'native'))
    require(sources == 153 and len(names) == 393, 'closed 393 regular-file census')
    require(len({name.casefold() for name in names}) == 393, 'case-unique source archive')
    for name in names:
        canonical(name)
    return frozenset(names)


def expected_directories(files):
    names = set()
    for name in files:
        names.update(parent.as_posix() for parent in canonical(name).parents
                     if parent.as_posix() != '.')
    require(not names.intersection(files), 'file/directory role collision')
    require(len({name.casefold() for name in names | set(files)}) == len(names) + len(files),
            'case-unique file/directory roles')
    return frozenset(names)


def member_limit(name, platform):
    return MAX_BINARY if name == binary_name(platform) else MAX_FILE
