"""Unqualified tool-path diagnosis; never builds or admits compiler/provider proof."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

sys.dont_write_bytecode = True
if not __debug__:
    raise RuntimeError('tool diagnosis forbids optimized Python')
import verify as ir

MAX_TOOL = 128 * 1024 * 1024
MAX_QUERY = 1024 * 1024
TOOLCHAIN = '1.97.1'


def require(value, message):
    if not value:
        raise ValueError(message)


def component(path):
    info = path.lstat()
    linked = stat.S_ISLNK(info.st_mode)
    reparse = bool(getattr(info, 'st_file_attributes', 0) & 0x400)
    target, error = None, None
    if linked or reparse:
        try:
            target = os.readlink(path)
        except OSError as found:
            error = repr(found)
    return dict(path=str(path), mode=info.st_mode, device=info.st_dev,
                inode=info.st_ino, bytes=info.st_size, mtime_ns=info.st_mtime_ns,
                ctime_ns=info.st_ctime_ns, links=info.st_nlink,
                file_attributes=getattr(info, 'st_file_attributes', 0),
                reparse_tag=getattr(info, 'st_reparse_tag', None),
                symbolic_link=linked, reparse=reparse, readlink_target=target,
                readlink_error=error)


def components(path):
    return [component(part) for part in (path, *path.parents)]


def component_identity(row):
    keys = ['path', 'mode', 'device', 'inode', 'file_attributes', 'reparse_tag',
            'symbolic_link', 'reparse', 'readlink_target', 'readlink_error']
    if not stat.S_ISDIR(row['mode']):
        keys += ['bytes', 'mtime_ns', 'ctime_ns', 'links']
    return {key: row[key] for key in keys}


def same_components(left, right):
    return list(map(component_identity, left)) == list(map(component_identity, right))


def tool_identity(row):
    return dict(original_guard_path=row['original_guard_path'],
                original_components=list(map(component_identity, row['original_components_before'])),
                canonical_components=list(map(component_identity, row['canonical_components_before'])),
                canonical_path=row['canonical_resolved_path'], bytes=row['canonical_bytes'],
                sha256=row['canonical_sha256'], guard_accepted=row['original_strict_guard_accepted'],
                guard_error=row['original_strict_guard_error'])


def same_tools(left, right):
    return set(left) == set(right) and all(tool_identity(left[key]) == tool_identity(right[key]) for key in left)


def canonical_bytes(path):
    ir.ordinary_path(path)
    require(stat.S_ISREG(path.lstat().st_mode), 'canonical tool is an ordinary regular file')
    require(all(stat.S_ISDIR(parent.lstat().st_mode) for parent in path.parents),
            'canonical tool parents are ordinary directories')
    return ir.read_bounded(path, MAX_TOOL)


def inspect_tool(original):
    """Keep original guard outcome distinct from canonical target bytes and stability."""
    path = Path(original).absolute()
    require('\0' not in str(path) and '..' not in path.parts, 'absolute original lexical tool path')
    before = components(path)
    resolved = path.resolve(strict=True)
    canonical_before = components(resolved)
    raw = canonical_bytes(resolved)
    guard_error = None
    try:
        original_bytes = ir.read_bounded(path, MAX_TOOL)
    except ValueError as error:
        guard_error = str(error)
    else:
        require(original_bytes == raw, 'original and canonical capability bytes agree')
    after = components(path)
    canonical_after = components(resolved)
    require(same_components(before, after) and same_components(canonical_before, canonical_after)
            and path.resolve(strict=True) == resolved, 'tool component identity changed during diagnosis')
    require(canonical_bytes(resolved) == raw and same_components(components(resolved), canonical_after),
            'canonical tool bytes or identity changed during diagnosis')
    offenders = [row for row in before if row['symbolic_link'] or row['reparse']]
    offender = offenders[0] if guard_error == 'no linked or reparse artifact parents' and offenders else None
    return dict(original_lexical_path=str(original), original_guard_path=str(path),
                original_row=dict(path=str(original), sha256=hashlib.sha256(raw).hexdigest()),
                original_components_before=before, original_components_after=after,
                original_strict_guard_accepted=guard_error is None,
                original_strict_guard_error=guard_error,
                observed_offending_component=offender,
                canonical_resolved_path=str(resolved), canonical_components_before=canonical_before,
                canonical_components_after=canonical_after, canonical_regular_file=True,
                canonical_bytes=len(raw), canonical_sha256=hashlib.sha256(raw).hexdigest(),
                stable_components_and_tool_bytes=True)


def query(argv, env):
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    completed = subprocess.run(list(map(str, argv)), env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, timeout=30, check=False)
    require(len(completed.stdout) <= MAX_QUERY and len(completed.stderr) <= MAX_QUERY,
            'bounded tool metadata query output')
    record = dict(argv=list(map(str, argv)), started_at=started,
                  selected_environment={name: env.get(name) for name in
                                        ('RUSTUP_TOOLCHAIN', 'RUSTC', 'CARGO_HOME', 'RUSTUP_HOME', 'PATH',
                                         'NODE_OPTIONS', 'NODE_PATH', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                                         'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')},
                  completed_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  exit=completed.returncode, stdout=completed.stdout.decode('utf8'),
                  stderr=completed.stderr.decode('utf8'),
                  stdout_bytes=len(completed.stdout), stderr_bytes=len(completed.stderr),
                  stdout_sha256=hashlib.sha256(completed.stdout).hexdigest(),
                  stderr_sha256=hashlib.sha256(completed.stderr).hexdigest())
    require(type(record['exit']) is int and record['exit'] == 0, 'successful tool-only metadata query')
    return record


def source_observation():
    root = Path(__file__).resolve().parents[3]
    def git(*args):
        return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()
    return dict(repository_root=str(root), repository_sha=git('rev-parse', 'HEAD'),
                repository_tree_sha=git('rev-parse', 'HEAD^{tree}'),
                source_clean_observed=not bool(git('status', '--porcelain')))


def diagnose(cargo_arg, rustup_arg, node_arg):
    """Match run.py's lexical selections; query only versions and rustup which."""
    cargo = shutil.which(str(cargo_arg))
    require(cargo is not None, 'original supplied Cargo selection available')
    node = Path(node_arg).resolve(strict=True)
    rustup = Path(rustup_arg).resolve(strict=True)
    initial_rustc = Path(cargo).parent / ('rustc.exe' if os.name == 'nt' else 'rustc')
    initial_paths = dict(supplied_cargo=cargo, rustup=str(rustup), node=str(node),
                         initial_rustc=str(initial_rustc))
    initial = {name: inspect_tool(path) for name, path in initial_paths.items()}
    env = dict(os.environ)
    for name in ('RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS',
                 'CARGO_ENCODED_RUSTFLAGS', 'NODE_OPTIONS', 'NODE_PATH'):
        env.pop(name, None)
    env['RUSTUP_TOOLCHAIN'] = TOOLCHAIN
    env['RUSTC'] = str(initial_rustc)
    queries = []
    for path, prefix in ((cargo, 'cargo 1.97.1 '), (initial_rustc, 'rustc 1.97.1 '),
                         (node, 'v22.22.1')):
        row = query([path, '--version'], env)
        version = row['stdout'].strip()
        if path == node:
            require(version == 'v22.22.1', 'exact pinned Node metadata version')
        else:
            require(version.startswith(prefix), 'pinned compiler metadata version')
        queries.append(row)
    selected = dict(supplied_cargo=cargo, rustup=str(rustup), node=str(node))
    for name in ('cargo', 'rustc'):
        row = query([rustup, 'which', '--toolchain', TOOLCHAIN, name], env)
        queries.append(row)
        selected[name] = row['stdout'].strip()
        require(Path(selected[name]).is_absolute() and '\0' not in selected[name],
                'raw rustup which absolute tool result')
    before = {name: inspect_tool(path) for name, path in selected.items()}
    env['RUSTC'] = selected['rustc']
    for name in ('cargo', 'rustc'):
        row = query([selected[name], '--version'], env)
        require(row['stdout'].strip().startswith(name + ' 1.97.1 '), 'actual selected tool version')
        queries.append(row)
    after = {name: inspect_tool(path) for name, path in selected.items()}
    require(same_tools(before, after), 'selected tool capability changed across metadata queries')
    require(same_tools(initial, {name: inspect_tool(path) for name, path in initial_paths.items()}),
            'original supplied tool capability changed across metadata queries')
    return dict(format='zryna.tool-provenance.diagnostic.v1', status='UNQUALIFIED',
                diagnostic_integrity='stable', platform=sys.platform,
                original_arguments=dict(cargo=str(cargo_arg), rustup=str(rustup_arg), node=str(node_arg)),
                original_live_guard_row_order=['cargo', 'rustc', 'node', 'rustup', 'supplied_cargo'],
                selected_rows=before, selected_rows_after=after, initial_selection_rows=initial,
                metadata_queries=queries, compiler_provider_executions=0, builds=0,
                guard_source_sha256=hashlib.sha256(Path(ir.__file__).read_bytes()).hexdigest(),
                diagnostic_source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                source_observation=source_observation(),
                IR_admission=False, public_activation=False,
                observation_scope='this diagnostic process and host only; never the departed H6 runner',
                guard_policy='unchanged original verify.read_bounded link/reparse predicates')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('cargo', 'rustup', 'node'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.absolute()
    root = Path(__file__).resolve().parents[3]
    require('..' not in output.parts and not output.is_relative_to(root),
            'diagnostic output stays outside controlled source')
    ir.ordinary_path(output.parent)
    try:
        output.lstat()
    except FileNotFoundError:
        pass
    else:
        raise ValueError('create-only diagnostic output')
    state = dict(format='zryna.tool-provenance.diagnostic.v1', status='UNQUALIFIED',
                 IR_admission=False, compiler_provider_executions=0, builds=0, public_activation=False,
                 diagnostic_integrity='blocked', platform=sys.platform)
    try:
        state = diagnose(args.cargo, args.rustup, args.node)
    except Exception as error:
        state['error'] = dict(type=type(error).__name__, message=str(error))
        raise
    finally:
        with output.open('x', encoding='utf8') as stream:
            json.dump(state, stream, indent=2, sort_keys=True); stream.write('\n')
        print(json.dumps(state, sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
