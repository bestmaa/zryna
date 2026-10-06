"""Read complete IR observations independently of the Rust collector's assertions."""
import argparse
import hashlib
import json
import struct
import stat
from pathlib import Path, PurePosixPath
import topology

MAX_OBSERVATION = 8 * 1024 * 1024
MAX_RECEIPT = 2 * 1024 * 1024

def require(value, message):
    if not value:
        raise ValueError(message)

def digest(data):
    return hashlib.sha256(data).hexdigest()

def strict_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'duplicate JSON key: ' + key)
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError('nonfinite JSON')))

def exact(left, right):
    return json.dumps(left, sort_keys=True, separators=(',',':')) == json.dumps(right, sort_keys=True, separators=(',',':'))

def ordinary_path(path):
    for component in (path,*path.parents):
        metadata = component.lstat()
        require(not stat.S_ISLNK(metadata.st_mode) and not getattr(metadata,'st_file_attributes',0) & getattr(stat,'FILE_ATTRIBUTE_REPARSE_POINT',0x400),'no linked or reparse artifact parents')

def read_bounded(path, limit):
    ordinary_path(path)
    require(path.is_file() and not path.is_symlink(), 'regular retained artifact: ' + str(path))
    require(path.stat().st_size <= limit, 'artifact size bound')
    with path.open('rb') as source:
        data = source.read(limit + 1)
    require(len(data) <= limit, 'artifact size bound after read')
    return data

def graph_digest(case):
    # Independent reader serialization; entrypoint is explicit and need not be FileId0.
    parts = [('ZRYNA-' + case['profile'].upper() + '-GRAPH\0').encode(), struct.pack('<I', 1)]
    def text(value):
        encoded = value.encode('utf-8')
        parts.extend((struct.pack('<I', len(encoded)), encoded))
    text(case['entrypoint'])
    parts.append(struct.pack('<I', len(case['sources'])))
    for source in case['sources']:
        text(source['path'])
        parts.append(bytes.fromhex(source['sha256']))
    parts.append(struct.pack('<I', len(case['edges'])))
    for edge in case['edges']:
        for name in ('importer', 'specifier', 'imported', 'local'):
            text(edge[name])
    return digest(b''.join(parts))

def keys(value, expected, label):
    require(type(value) is dict and set(value) == set(expected.split()), 'complete ' + label + ' fields')

def check_spans(value, sources):
    if isinstance(value, dict):
        for name, child in value.items():
            if name == 'span':
                require(type(child) is list and len(child) == 3 and all(type(n) is int for n in child), 'typed span')
                file_id, start, end = child
                require(0 <= file_id < len(sources), 'source-bound FileId')
                require(0 <= start <= end <= sources[file_id]['bytes'], 'source-bound byte span')
            else:
                check_spans(child, sources)
    elif isinstance(value, list):
        for child in value:
            check_spans(child, sources)

def check_projection(profile, value, sources):
    if profile == 'm1':
        keys(value, 'profile abi functions', 'M1 program')
        for function in value['functions']:
            keys(function, 'export abi_export parameters result expressions body', 'M1 function')
    else:
        if profile == 'm2':
            keys(value, 'entry_module abi modules', 'M2 program')
        else:
            keys(value, 'identity source_map_identity type_universe_identity runtime_contract abi linear32_layouts linux_x86_64_layouts modules', 'M3 program')
        modules = value['modules']
        require(type(modules) is list and len(modules) == len(sources), 'complete canonical module set')
        require([m['source_file'] for m in modules] == list(range(len(sources))), 'module source authorities and order')
        for module in modules:
            keys(module, 'id source_file functions' + (' data_declarations' if profile == 'm3' else ''), 'module')
            if profile == 'm3':
                require(type(module['data_declarations']) is int and module['data_declarations'] >= 0, 'data declarations preserved')
            for function in module['functions']:
                if profile == 'm2':
                    keys(function, 'id public_export parameters result span blocks', 'M2 function')
                else:
                    keys(function, 'id public_export result_type parameters borrow_parameters places cleanup_plans blocks', 'M3 function')
                    for parameter in function['parameters']:
                        keys(parameter, 'id type span', 'value parameter')
                    for parameter in function['borrow_parameters']:
                        keys(parameter, 'id referent access span', 'borrow parameter')
                    for place in function['places']:
                        keys(place, 'id type is_copy span kind', 'place')
                    for plan in function['cleanup_plans']:
                        keys(plan, 'id actions span site', 'cleanup plan')
                for block in function['blocks']:
                    keys(block, 'id parameters instructions terminator', 'block')
                    for parameter in block['parameters']:
                        keys(parameter, 'id type span', 'block parameter')
                    for instruction in block['instructions']:
                        if profile == 'm2':
                            keys(instruction, 'result type span operation', 'M2 instruction')
                        else:
                            keys(instruction, 'operation result result_type span value_operands place_operands cleanup backend_view allocation_failure_actions drop_actions bool_literal i32_literal string_utf8_bytes callee call_arguments variant borrow borrow_access vec_clone_element_cleanup aggregate_clone_element_cleanup aggregate_clone_fallible_leaf_count vec_clone_failure_actions aggregate_clone_failure_actions', 'M3 instruction')
                    keys(block['terminator'], 'span operation' if profile == 'm2' else 'operation span value_operands place_operands cleanup trap backend_view edges enum_arms drop_actions', 'terminator')
    check_spans(value, sources)

def verify(inventory_bytes, receipt, output):
    inventory = strict_json(inventory_bytes)
    keys(receipt, 'schema_version inventory_sha256 public_activation observation_format cases', 'receipt')
    require(type(receipt['schema_version']) is int and receipt['schema_version'] == 1 and receipt['public_activation'] is False, 'test-only receipt version')
    require(receipt['observation_format'] == 'revision-bound-sealed-Debug-plus-getters-v1', 'exact observation format')
    require(receipt['inventory_sha256'] == digest(inventory_bytes), 'exact independent inventory binding')
    expected = inventory['cases']
    actual = receipt['cases']
    require(len(expected) == len(actual) == 107, 'complete 107 case census')
    require(len({row['id'] for row in actual}) == 107, 'unique case IDs')
    require(inventory['repository_sha'] == '0a5f86b77a84c37e19e0dd59388c8a782d385131', 'pinned inventory revision')
    statuses = [c['baseline_graph_observation']['status'] for c in expected]
    require(statuses.count('observed-in-unchanged-H4') == 85 and statuses.count('derived-only-no-H4-graph-observation') == 18 and statuses.count('not-applicable-M1') == 4, 'honest observed/derived graph census')
    missing = [dict(case_id=c['id'], obligation='authentic-graph-observation-missing-from-unchanged-H4-receipt') for c in expected if c['baseline_graph_observation']['status'] == 'derived-only-no-H4-graph-observation']
    require(inventory['open_obligations'] == missing, 'missing H4 authentic observations remain open')
    files = set()
    hashes = 0
    for ordinal, (case, row) in enumerate(zip(expected, actual)):
        keys(row, 'id profile entrypoint sources edges graph_sha256 observations', 'case')
        for field in ('id', 'profile', 'entrypoint', 'edges', 'graph_sha256'):
            require(exact(row[field], case[field]), 'exact case/context ' + field + ': ' + case['id'])
        snapshot = {k:case[k] for k in ('profile','entrypoint','sources','edges')}
        snapshot_bytes = json.dumps(snapshot, sort_keys=True, separators=(',',':')).encode()
        require(digest(snapshot_bytes) == case['context_snapshot_sha256'], 'immutable case/context snapshot binding')
        baseline = case['baseline_graph_observation']
        require(baseline['baseline_raw_sha256'] == inventory['baseline_raw_sha256'], 'exact raw baseline binding')
        if baseline['status'] == 'observed-in-unchanged-H4':
            require(baseline['sha256'] == case['graph_sha256'], 'authentic observed baseline graph')
        else:
            require(baseline['sha256'] is None, 'derived graph cannot relabel absent authentic H4 observation')
        sources = [{k: s[k] for k in ('file_id','path','sha256','bytes')} for s in case['sources']]
        require(exact(row['sources'], sources), 'complete original canonical source map: ' + case['id'])
        require([s['file_id'] for s in sources] == list(range(len(sources))), 'dense canonical FileIds')
        require([s['path'] for s in sources] == sorted(s['path'] for s in sources), 'canonical source order')
        require(case['entrypoint'] in [s['path'] for s in sources], 'explicit entrypoint membership')
        if case['profile'] == 'm1':
            require(case['graph_sha256'] is None and not case['edges'], 'M1 graph inapplicable')
        else:
            require(graph_digest(case) == row['graph_sha256'], 'independent graph hash')
            hashes += 1
        for source in sources:
            relative = f"case-{ordinal:03}/sources/{source['path']}"
            require(relative not in files and '..' not in PurePosixPath(relative).parts, 'unique contained original source')
            files.add(relative)
            source_data = read_bounded(output / relative, MAX_RECEIPT)
            require(len(source_data) == source['bytes'] and digest(source_data) == source['sha256'], 'retained immutable original source/context bytes')
        observations = row['observations']
        require(len(observations) == 2, 'both complete provider observations')
        observed = []
        for provider, artifact in zip(('worker','native'), observations):
            keys(artifact, 'provider path sha256 bytes', 'artifact')
            relative = f'case-{ordinal:03}/{provider}.json'
            require(artifact['provider'] == provider and artifact['path'] == relative, 'exact provider artifact identity')
            require(PurePosixPath(relative).as_posix() == relative and relative not in files, 'unique contained artifact')
            files.add(relative)
            data = read_bounded(output / relative, MAX_OBSERVATION)
            require(type(artifact['bytes']) is int and len(data) == artifact['bytes'] and digest(data) == artifact['sha256'], 'retained artifact bytes/hash')
            value = strict_json(data)
            keys(value, 'schema_version raw_debug getters', 'complete observation')
            require(type(value['schema_version']) is int and value['schema_version'] == 1, 'observation version')
            raw = value['raw_debug']
            require(type(raw) is str and raw.startswith('VerifiedProgram {') and len(raw.encode()) <= 2*1024*1024, 'bounded whole sealed program Debug')
            fields = ('program:', 'abi:')
            if case['profile'] != 'm1':
                fields += ('abi_indices:',)
            if case['profile'] == 'm3':
                fields += ('identity:', 'linear32:', 'linux_x86_64:', 'borrow_indices:')
            require(all(field in raw for field in fields), 'complete sealed root fields')
            check_projection(case['profile'],value['getters'],sources)
            topology.check(case['profile'],raw,value['getters'],sources,case['entrypoint'])
            observed.append(data)
        require(observed[0] == observed[1], 'exact full sealed IR and getter equality: ' + case['id'])
    actual_files = set()
    for path in output.rglob('*'):
        ordinary_path(path)
        if path.is_file():
            actual_files.add(path.relative_to(output).as_posix())
        else:
            require(path.is_dir(), 'regular proof filesystem entries')
    require(actual_files == files, 'complete exact retained source/observation file inventory')
    require(hashes == 103, 'complete graph census')
    return {'passed':107,'graph_hashes':hashes,'original_library_comparison':'pending-unverified-original',
            'h4_observed_graph_hashes':85,'h4_derived_only_graph_hashes':18,
            'open_h4_graph_obligations':missing,'current_collector_observed_graph_hashes':103,
            'public_activation':False,'observation_scope':'same-run revision-bound complete sealed IR equality'}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inventory', type=Path)
    parser.add_argument('receipt', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(read_bounded(args.inventory,MAX_RECEIPT), strict_json(read_bounded(args.receipt,MAX_RECEIPT)), args.output)))
