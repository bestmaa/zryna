"""Bind selected tool aliases to stable ordinary bytes; never execute a tool.

Artifact reads retain verify.read_bounded's unchanged no-link policy. This separate
capability follows bounded, recorded tool links. Snapshots detect observed changes;
they do not provide an atomic handle-relative executable sandbox.
"""
from collections import deque
import hashlib
import json
import ntpath
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import posixpath
import re
import stat
import sys

sys.dont_write_bytecode = True
if not __debug__:
    raise RuntimeError('tool capabilities forbid optimized Python')
import verify as ir

FORMAT = 'zryna.tool-capability.v1'
MAX_BYTES = 128 * 1024 * 1024
MAX_COMPONENTS = 4096
MAX_LINKS = 64
REPARSE = 0x400
SYMLINK = 0xA000000C
JUNCTION = 0xA0000003
FIELDS = ('path mode device inode bytes mtime_ns ctime_ns links '
          'file_attributes reparse_tag link_target').split()
ROW_FIELDS = ('format platform path resolved_path bytes sha256 resolution '
              'canonical_components').split()


def require(value, message):
    if not value:
        raise ValueError(message)


def path_class(platform):
    require(platform in ('linux', 'win32'), 'supported tool platform')
    return PureWindowsPath if platform == 'win32' else PurePosixPath


def text(value):
    return type(value) is str and 0 < len(value) <= 4096 and all(ord(c) >= 32 for c in value)


def path_check(value, platform):
    cls = path_class(platform)
    require(text(value), 'bounded tool path')
    parsed = cls(value)
    normal = ntpath.normpath(value) if platform == 'win32' else posixpath.normpath(value)
    require(parsed.is_absolute() and str(parsed) == value == normal
            and '..' not in parsed.parts and not value.startswith('\\\\?\\'),
            'absolute canonical spelling of tool path')
    return parsed


def linked(row):
    return stat.S_ISLNK(row['mode']) or bool(row['file_attributes'] & REPARSE)


def component_check(row, platform):
    require(type(row) is dict and set(row) == set(FIELDS), 'closed tool component fields')
    path_check(row['path'], platform)
    for name in FIELDS[1:-2]:
        require(type(row[name]) is int and row[name] >= 0, 'typed component ' + name)
    require(row['mode'] <= 0xffff and row['file_attributes'] <= 0xffffffff,
            'bounded component metadata')
    tag = row['reparse_tag']
    require(tag is None or (type(tag) is int and 0 <= tag <= 0xffffffff), 'typed reparse tag')
    if row['file_attributes'] & REPARSE:
        require(platform == 'win32' and tag in (SYMLINK, JUNCTION), 'supported tool reparse tag')
        require(stat.S_ISLNK(row['mode']) if tag == SYMLINK else stat.S_ISDIR(row['mode']),
                'reparse type matches tag')
    else:
        require(tag in (None, 0), 'no unexplained reparse tag')
    if linked(row):
        require(text(row['link_target']), 'recorded tool link target')
    else:
        require(row['link_target'] is None and (stat.S_ISREG(row['mode']) or stat.S_ISDIR(row['mode'])),
                'ordinary tool component type')


def identity(row):
    # Changes to unrelated directory entries are not capability replacement.
    ignored = ('bytes', 'mtime_ns', 'ctime_ns', 'links') if stat.S_ISDIR(row['mode']) and not linked(row) else ()
    return {name: value for name, value in row.items() if name not in ignored}


def entry(path):
    info = Path(path).lstat()
    attrs = getattr(info, 'st_file_attributes', 0)
    is_link = stat.S_ISLNK(info.st_mode) or bool(attrs & REPARSE)
    return dict(path=path, mode=info.st_mode, device=info.st_dev, inode=info.st_ino,
                bytes=info.st_size, mtime_ns=info.st_mtime_ns, ctime_ns=info.st_ctime_ns,
                links=info.st_nlink, file_attributes=attrs,
                reparse_tag=getattr(info, 'st_reparse_tag', None),
                link_target=os.readlink(path) if is_link else None)


def link_path(target, platform):
    # Windows readlink returns extended names for normal symlinks/junctions.
    if platform == 'win32' and target.startswith('\\\\?\\UNC\\'):
        target = '\\\\' + target[8:]
    elif platform == 'win32' and target.startswith('\\\\?\\'):
        target = target[4:]
    parsed = path_class(platform)(target)
    require(not (parsed.drive and not parsed.is_absolute()), 'no drive-relative tool link')
    require(not (platform == 'win32' and parsed.root and not parsed.drive), 'no drive-ambiguous tool link')
    return parsed


def walk(path, platform, lookup):
    """Replay the complete resolution traversal using live or archived components."""
    initial = path_check(path, platform)
    current = type(initial)(initial.anchor)
    pending = deque(initial.parts[1:])
    rows, seen = [], set()
    links = 0

    def observe(candidate):
        require(len(rows) < MAX_COMPONENTS, 'tool component bound')
        row = lookup(str(candidate))
        component_check(row, platform)
        require(row['path'] == str(candidate), 'component lookup identity')
        rows.append(row)
        return row

    anchor = observe(current)
    require(stat.S_ISDIR(anchor['mode']) and not linked(anchor), 'ordinary tool root')
    while pending:
        name = pending.popleft()
        if name == '..':
            current = current.parent
            row = observe(current)
            require(stat.S_ISDIR(row['mode']) and not linked(row), 'resolved parent directory')
            continue
        candidate = current / name
        row = observe(candidate)
        if linked(row):
            marker = (str(candidate), tuple(pending))
            require(marker not in seen, 'tool link cycle')
            seen.add(marker)
            links += 1
            require(links <= MAX_LINKS, 'tool link bound')
            target = link_path(row['link_target'], platform)
            if target.is_absolute():
                current = type(initial)(target.anchor)
                anchor = observe(current)
                require(stat.S_ISDIR(anchor['mode']) and not linked(anchor), 'ordinary link target root')
                target_parts = target.parts[1:]
            else:
                target_parts = target.parts
            pending.extendleft(reversed(target_parts))
        else:
            require(stat.S_ISDIR(row['mode']) if pending else stat.S_ISREG(row['mode']),
                    'tool traversal directory or final regular file')
            current = candidate
    require(rows and stat.S_ISREG(rows[-1]['mode']) and not linked(rows[-1]), 'resolved regular tool')
    return str(current), rows


def stable_handle(info):
    # CPython Windows path stat adds .exe execute bits; handle stat does not.
    return (info.st_dev, info.st_ino, stat.S_IFMT(info.st_mode), info.st_size,
            info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)


HANDLE_FIELDS = ('device', 'inode', 'file_type', 'bytes', 'mtime_ns', 'ctime_ns', 'links')


def canonical_identity(info):
    raw = stable_handle(info)
    if sys.platform != 'win32':
        return raw
    # CPython 3.12 path ctime is creation time; handle ctime is change time.
    # Compare the explicit creation field across APIs, retaining all other fields.
    born = getattr(info, 'st_birthtime_ns', None)
    if type(born) is not int or abs(born) >= (1 << 128):
        return None
    return raw[:5] + (born,) + raw[6:]


def same_open_handle(before, after):
    left, right = canonical_identity(before), canonical_identity(after)
    return (stable_handle(before) == stable_handle(after)
            and left is not None and left == right)


def require_open_handle(opened, path_info):
    left, right = stable_handle(opened), stable_handle(path_info)
    comparable_left, comparable_right = canonical_identity(opened), canonical_identity(path_info)
    canonical_equal = comparable_left is not None and comparable_left == comparable_right
    regular = stat.S_ISREG(opened.st_mode)
    bounded = opened.st_size <= MAX_BYTES
    if regular and bounded and canonical_equal:
        return
    # Report raw values and the selected cross-API predicate without normalization.
    # Values outside the stat field bound never expand a rejection log arbitrarily.
    def value(item):
        return item if type(item) is int and abs(item) < (1 << 128) else 'invalid-or-unbounded-integer'
    def birthtime(info):
        item = getattr(info, 'st_birthtime_ns', None)
        return 'unavailable' if item is None else value(item)
    implementation = sys.implementation.name
    platform = sys.platform
    detail = dict(format='zryna.tool-handle-rejection.v1',
                  platform=platform if type(platform) is str and re.fullmatch(r'[a-z0-9_]{1,32}', platform) else 'unknown',
                  python_implementation=implementation if type(implementation) is str and re.fullmatch(r'[a-z_]{1,32}', implementation) else 'unknown',
                  python_version=[value(part) for part in sys.version_info[:3]],
                  regular_handle=regular, within_byte_limit=bounded, metadata_equal=left == right,
                  canonical_identity_equal=canonical_equal,
                  canonical_time_field='birthtime_ns' if sys.platform == 'win32' else 'ctime_ns',
                  byte_limit=MAX_BYTES,
                  observed_birthtime_ns=dict(opened=birthtime(opened), path=birthtime(path_info)),
                  opened={name: value(item) for name, item in zip(HANDLE_FIELDS, left)},
                  path={name: value(item) for name, item in zip(HANDLE_FIELDS, right)},
                  differing_fields=[name for name, a, b in zip(HANDLE_FIELDS, left, right) if a != b])
    raise ValueError('stable canonical open handle; ' + json.dumps(detail, sort_keys=True, separators=(',', ':')))


def read_canonical(path):
    target = Path(path)
    before = entry(path)
    require(stat.S_ISREG(before['mode']) and not linked(before), 'canonical regular tool')
    raw = ir.read_bounded(target, MAX_BYTES)
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0)
    flags |= getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
    with os.fdopen(os.open(target, flags), 'rb') as source:
        opened = os.fstat(source.fileno())
        require_open_handle(opened, target.lstat())
        check = source.read(MAX_BYTES + 1)
        after_handle = os.fstat(source.fileno())
        require(len(check) <= MAX_BYTES and check == raw
                and same_open_handle(opened, after_handle), 'canonical bytes and handle unchanged')
        require_open_handle(after_handle, target.lstat())
    require(entry(path) == before, 'canonical same-path metadata unchanged')
    return len(raw), hashlib.sha256(raw).hexdigest()


def stable_rows(left, right):
    return [identity(row) for row in left] == [identity(row) for row in right]


def snapshot(path):
    raw = str(path)
    path_check(raw, sys.platform)
    resolved, resolution = walk(raw, sys.platform, entry)
    canonical, canonical_rows = walk(resolved, sys.platform, entry)
    require(canonical == resolved and not any(map(linked, canonical_rows)), 'ordinary canonical ancestry')
    size, digest = read_canonical(resolved)
    require(read_canonical(resolved) == (size, digest), 'canonical tool bytes changed')
    resolved_after, after = walk(raw, sys.platform, entry)
    _, canonical_after = walk(resolved, sys.platform, entry)
    require(resolved_after == resolved and stable_rows(resolution, after)
            and stable_rows(canonical_rows, canonical_after), 'tool resolution identity changed')
    result = dict(format=FORMAT, platform=sys.platform, path=raw, resolved_path=resolved,
                  bytes=size, sha256=digest, resolution=resolution, canonical_components=canonical_rows)
    return validate(result, sys.platform)


def validate(row, platform, *, live=False):
    """Validate foreign-host records structurally; live also recaptures the tool."""
    require(type(live) is bool, 'typed live capability mode')
    require(type(row) is dict and set(row) == set(ROW_FIELDS), 'closed tool capability fields')
    require(row['format'] == FORMAT and row['platform'] == platform, 'tool format and platform')
    path_check(row['path'], platform)
    path_check(row['resolved_path'], platform)
    require(type(row['bytes']) is int and 0 <= row['bytes'] <= MAX_BYTES, 'bounded tool bytes')
    require(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'tool digest')
    for field, initial in (('resolution', row['path']), ('canonical_components', row['resolved_path'])):
        rows = row[field]
        require(type(rows) is list and 0 < len(rows) <= MAX_COMPONENTS, 'closed component census')
        by_path = {}
        for component in rows:
            component_check(component, platform)
            old = by_path.setdefault(component['path'], component)
            require(identity(old) == identity(component), 'consistent repeated component identity')
        def archived(name):
            require(name in by_path, 'missing traversal component')
            return by_path[name]
        resolved, replay = walk(initial, platform, archived)
        require(resolved == row['resolved_path'] and stable_rows(replay, rows), 'complete original tool traversal')
        require(rows[-1]['bytes'] == row['bytes'], 'canonical tool length binding')
        if field == 'canonical_components':
            require(not any(map(linked, rows)), 'ordinary canonical component census')
    require(identity(row['resolution'][-1]) == identity(row['canonical_components'][-1]),
            'same canonical file capability in both traversals')
    original = {component['path']: component for component in row['resolution']}
    require(all(component['path'] not in original or identity(component) == identity(original[component['path']])
                for component in row['canonical_components']), 'consistent shared canonical ancestry')
    if live:
        require(platform == sys.platform, 'actual live capability platform')
        require(same(row, snapshot(row['path'])), 'original live capability changed')
    return row


def same(before, after):
    """Exact capability identity; directory content changes are recorded, not replacements."""
    try:
        validate(before, before['platform'])
        validate(after, after['platform'])
        plain = ('format', 'platform', 'path', 'resolved_path', 'bytes', 'sha256')
        return all(before[key] == after[key] for key in plain) and all(
            stable_rows(before[key], after[key]) for key in ('resolution', 'canonical_components'))
    except (ValueError, KeyError, TypeError, IndexError):
        return False
