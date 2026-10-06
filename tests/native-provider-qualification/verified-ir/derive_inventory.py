"""Derive a new H4 inventory; never represent this as a recovered Library handoff."""
import argparse
import hashlib
import json
import posixpath
import re
import struct
from pathlib import Path

H4 = '0a5f86b77a84c37e19e0dd59388c8a782d385131'
IMPORT = re.compile(rb'import\s*\{(?P<bindings>[^}]+)\}\s*from\s*(?P<quoted>"(?P<specifier>[^"\n]+)")\s*;')
BINDING = re.compile(rb'\s*(?P<imported>[A-Za-z_$][\w$]*)(?:\s+as\s+(?P<local>[A-Za-z_$][\w$]*))?\s*')

def sha(data):
    return hashlib.sha256(data).hexdigest()

def digest_graph(profile, entry, rows, edges):
    data = bytearray(('ZRYNA-' + profile.upper() + '-GRAPH\0').encode())
    def number(value):
        data.extend(struct.pack('<I', value))
    def text(value):
        value = value.encode('utf-8')
        number(len(value))
        data.extend(value)
    number(1)
    text(entry)
    number(len(rows))
    for row in rows:
        text(row['path'])
        data.extend(bytes.fromhex(row['sha256']))
    number(len(edges))
    for edge in edges:
        for field in ('importer', 'specifier', 'imported', 'local'):
            text(edge[field])
    return sha(data)

def capture(root, profile, entry, available):
    visited = {}
    edges = []
    def visit(path):
        if path in visited:
            return
        origin = available[path]
        data = (root / origin).read_bytes()
        visited[path] = (origin, data)
        imports = list(IMPORT.finditer(data))
        # Finite frozen grammar: reject any import declaration outside the named pattern.
        assert len(imports) == len(re.findall(rb'(?m)^import\b', data)), path
        for item in imports:
            specifier = item['specifier'].decode()
            assert specifier.startswith('./') and specifier.endswith('.zry'), path
            target = posixpath.normpath(posixpath.join(posixpath.dirname(path), specifier))
            assert target in available, (path, target)
            for chunk in re.finditer(rb'[^,]+', item['bindings']):
                binding = BINDING.fullmatch(chunk[0])
                assert binding, (path, chunk[0])
                imported = binding['imported'].decode()
                local = (binding['local'] or binding['imported']).decode()
                base = item.start('bindings') + chunk.start()
                def span(name):
                    return [0, base + binding.start(name), base + binding.end(name)]
                imported_span = span('imported')
                local_span = span('local') if binding['local'] else imported_span.copy()
                edges.append(dict(importer=path, target=target, specifier=specifier,
                                  imported=imported, local=local,
                                  declaration=[0, item.start(), item.end()],
                                  specifier_span=[0, item.start('quoted'), item.end('quoted')],
                                  imported_span=imported_span, local_span=local_span))
            visit(target)
    visit(entry)
    rows = [dict(file_id=index, path=path, origin=origin, sha256=sha(data), bytes=len(data))
            for index, (path, (origin, data)) in enumerate(sorted(visited.items()))]
    ids = {row['path']: row['file_id'] for row in rows}
    for edge in edges:
        for field in ('declaration', 'specifier_span', 'imported_span', 'local_span'):
            edge[field][0] = ids[edge['importer']]
    edges.sort(key=lambda edge: tuple(edge[field].encode() for field in ('importer', 'specifier', 'imported', 'local')))
    return rows, edges, None if profile == 'm1' else digest_graph(profile, entry, rows, edges)

def derive(root, baseline, baseline_bytes):
    registry = {p: json.loads((root / f'tests/{p}-conformance-v1.json').read_text()) for p in ('m1','m2','m3')}
    accepted = [row for row in baseline['cases'] if row['status'] == 'pass' and row['semantic'] == 'accepted']
    assert len(accepted) == 107
    overlays = {}
    physical = {p.relative_to(root).as_posix(): p.relative_to(root).as_posix()
                for p in sorted((root / 'tests/m3-fixtures').rglob('*.zry'))}
    fixtures = {f['id']: f for f in registry['m3']['fixtures']}
    by_path = {f['path']: f for f in fixtures.values()}
    for path in sorted(physical):
        fixture = by_path.get(path)
        if fixture and fixture.get('dependency'):
            physical['tests/m3-fixtures/conformance/math.zry'] = fixtures[fixture['dependency']]['path']
        overlays['m3-source:' + path.removeprefix('tests/m3-fixtures/')] = physical.copy()
    contexts = []
    for row in accepted:
        case_id, profile = row['id'], row['profile']
        entry = row['source']
        if profile == 'm1':
            available = {entry: entry}
        elif profile == 'm2':
            available = {f['path']: f['path'] for f in registry['m2']['fixtureFiles']}
        elif case_id.startswith('m3-source:'):
            available = overlays[case_id]
        else:
            if case_id.startswith('m3-context:'):
                fixture = fixtures[case_id.split(':',1)[1]]
            else:
                key = {'m3-run':'valid','m3-runtime-invalid':'runtimeInvalid','m3-invalid':'invalid'}[case_id.split(':',1)[0]]
                invocation = next(c for c in registry['m3'][key] if c['id'] == case_id.split(':',1)[1])
                fixture = fixtures[invocation['fixture']]
            entry = 'src/main.zry'
            available = {entry: fixture['path']}
            if fixture.get('dependency'):
                available['src/math.zry'] = fixtures[fixture['dependency']]['path']
        sources, edges, graph = capture(root, profile, entry, available)
        observed = row['details'].get('source_graph_sha256')
        if observed:
            assert graph == observed, (case_id, graph, observed)
        snapshot = dict(profile=profile, entrypoint=entry, sources=sources, edges=edges)
        snapshot_sha = sha(json.dumps(snapshot, sort_keys=True, separators=(',',':')).encode())
        contexts.append(dict(id=case_id, profile=profile, source=row['source'], entrypoint=entry,
                             sources=sources, edges=edges, graph_sha256=graph,
                             context_snapshot_sha256=snapshot_sha,
                             baseline_graph_observation=dict(
                                 status='observed-in-unchanged-H4' if observed else ('not-applicable-M1' if profile == 'm1' else 'derived-only-no-H4-graph-observation'),
                                 sha256=observed,
                                 baseline_raw_sha256=sha(baseline_bytes))))
    assert sum(c['graph_sha256'] is not None for c in contexts) == 103
    assert sum(c['baseline_graph_observation']['status'] == 'observed-in-unchanged-H4' for c in contexts) == 85
    occurrences = [c for c in contexts if '-source:' in c['id'] or '-context:' in c['id']]
    assert len(occurrences) == 66
    return dict(schema_version=1, provenance='independently-derived-H4-Git-inventory',
                original_library_comparison='pending-unverified-original', repository_sha=H4,
                baseline_raw_sha256=sha(baseline_bytes),
                baseline_canonical_json_sha256=sha(json.dumps(baseline, separators=(',',':')).encode()),
                open_obligations=[dict(case_id=c['id'], obligation='authentic-graph-observation-missing-from-unchanged-H4-receipt') for c in contexts if c['baseline_graph_observation']['status'] == 'derived-only-no-H4-graph-observation'],
                cases=contexts)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    baseline_bytes = args.baseline.read_bytes()
    value = derive(args.root, json.loads(baseline_bytes), baseline_bytes)
    args.output.write_text(json.dumps(value, indent=2) + '\n')
