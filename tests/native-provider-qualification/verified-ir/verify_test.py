#!/usr/bin/env python3
"""Hostile controls against independent admission, never OS/runtime execution evidence."""
import argparse
import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import subprocess
import verify


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory',required=True,type=Path)
    parser.add_argument('--receipt',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args = parser.parse_args()
    original_inventory = args.inventory.read_bytes()
    original_receipt = verify.strict_json(args.receipt.read_bytes())
    passed = []
    with tempfile.TemporaryDirectory(prefix='verified-ir-controls-') as temporary:
        output = Path(temporary)/'observations'
        shutil.copytree(args.output,output)
        verify.verify(original_inventory,original_receipt,output)
        def reject(name, mutate, *, baseline_verified=False):
            if not baseline_verified:
                verify.verify(original_inventory,original_receipt,output)
            inventory = verify.strict_json(original_inventory)
            receipt = copy.deepcopy(original_receipt)
            backups = {}
            def observation(ordinal,providers,change):
                for side in providers:
                    artifact = receipt['cases'][ordinal]['observations'][side]
                    path = output/artifact['path']
                    if path not in backups:
                        backups[path] = path.read_bytes()
                    value = verify.strict_json(backups[path])
                    change(value)
                    data = json.dumps(value,sort_keys=True,separators=(',',':')).encode()
                    path.write_bytes(data)
                    artifact.update(bytes=len(data),sha256=verify.digest(data))
            try:
                mutate(inventory,receipt,observation)
                inventory_bytes = original_inventory if inventory == verify.strict_json(original_inventory) else json.dumps(inventory,indent=2).encode()
                # Refresh the outer hash so controls exercise semantic admission, not stale hashes.
                receipt['inventory_sha256'] = verify.digest(inventory_bytes)
                try:
                    verify.verify(inventory_bytes,receipt,output)
                except (ValueError,KeyError,TypeError,IndexError) as error:
                    passed.append(name)
                    print('PASS',name,':',str(error)[:180])
                else:
                    raise AssertionError('hostile evidence admitted: '+name)
            finally:
                for path,data in backups.items():
                    path.write_bytes(data)
        def receipt_mutation(name,change, **options):
            reject(name,lambda inv,receipt,observe:change(receipt), **options)
        def obs(name,ordinal,sides,change):
            reject(name,lambda inv,receipt,observe:observe(ordinal,sides,change))
        receipt_mutation('missing-case',lambda r:r['cases'].pop())
        receipt_mutation('duplicate-case',lambda r:r['cases'].__setitem__(1,copy.deepcopy(r['cases'][0])))
        receipt_mutation('case-order',lambda r:r['cases'].__setitem__(slice(0,2),list(reversed(r['cases'][:2]))))
        receipt_mutation('case-profile',lambda r:r['cases'][0].update(profile='m3'))
        receipt_mutation('wrong-explicit-entrypoint',lambda r:r['cases'][4].update(entrypoint='tests/m2-fixtures/valid/math.zry'))
        receipt_mutation('source-fileid-bool',lambda r:r['cases'][0]['sources'][0].update(file_id=False))
        receipt_mutation('source-sha',lambda r:r['cases'][0]['sources'][0].update(sha256='0'*64))
        receipt_mutation('source-length',lambda r:r['cases'][0]['sources'][0].update(bytes=0))
        receipt_mutation('extra-source',lambda r:r['cases'][0]['sources'].append(copy.deepcopy(r['cases'][0]['sources'][0])))
        receipt_mutation('missing-provider',lambda r:r['cases'][0]['observations'].pop())
        receipt_mutation('provider-swap',lambda r:r['cases'][0]['observations'].reverse())
        receipt_mutation('artifact-escape',lambda r:r['cases'][0]['observations'][0].update(path='../escape.json'))
        receipt_mutation('artifact-length-bool',lambda r:r['cases'][0]['observations'][0].update(bytes=True))
        receipt_mutation('activation-claim',lambda r:r.update(public_activation=True))
        receipt_mutation('schema-bool',lambda r:r.update(schema_version=True))
        receipt_mutation('extra-receipt-field',lambda r:r.update(ignored=[]))
        edge_case = next(i for i,c in enumerate(original_receipt['cases']) if c['edges'])
        receipt_mutation('graph-target',lambda r:r['cases'][edge_case]['edges'][0].update(target='other.zry'))
        receipt_mutation('graph-import-span',lambda r:r['cases'][edge_case]['edges'][0].update(imported_span=[0,0,0]))
        receipt_mutation('graph-hash',lambda r:r['cases'][edge_case].update(graph_sha256='f'*64))
        reject('context-snapshot-binding',lambda inv,r,o:inv['cases'][0].update(context_snapshot_sha256='0'*64))
        missing_case = next(i for i,c in enumerate(verify.strict_json(original_inventory)['cases']) if c['baseline_graph_observation']['status']=='derived-only-no-H4-graph-observation')
        reject('derived-relabeled-observed',lambda inv,r,o:inv['cases'][missing_case]['baseline_graph_observation'].update(status='observed-in-unchanged-H4',sha256=inv['cases'][missing_case]['graph_sha256']))
        reject('missing-H4-obligation-erased',lambda inv,r,o:inv['open_obligations'].pop())
        obs('one-sided-whole-sealed-divergence',0,[1],lambda v:v.update(raw_debug=v['raw_debug'].replace('Parameter(0)','Parameter(9)',1)))
        obs('both-sided-root-field-omission',4,[0,1],lambda v:v.update(raw_debug=v['raw_debug'].replace('abi_indices:','omitted_indices:',1)))
        obs('both-sided-M1-expression-omission',0,[0,1],lambda v:v['getters'].update(functions=[]))
        obs('both-sided-M2-opcode-change',4,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'][0]['instructions'][0].update(operation='BogusOperation'))
        obs('both-sided-M2-result-change',4,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'][0]['instructions'][0].update(result='ValueIdentity(999999)'))
        obs('both-sided-M2-span-change',4,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'][0]['instructions'][0].update(span=[0,0,0]))
        obs('both-sided-M2-terminator-change',4,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'][0]['terminator'].update(operation='BogusTerminator'))
        obs('both-sided-entry-module-index-bool',4,[0,1],lambda v:v['getters'].update(entry_module='ModuleIdentity(false)'))
        obs('both-sided-module-source-bool',4,[0,1],lambda v:v['getters']['modules'][0].update(source_file=False))
        m3 = next(i for i,c in enumerate(original_receipt['cases']) if c['profile']=='m3')
        obs('both-sided-runtime-contract-change',m3,[0,1],lambda v:v['getters'].update(runtime_contract='BogusRuntime'))
        obs('both-sided-place-Copy-authority-change',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['places'][0].update(is_copy=123))
        obs('both-sided-malformed-optional-getter',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'][0]['instructions'][0].update(callee='Bogus'))
        obs('both-sided-private-function-omission',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'].pop())
        obs('both-sided-data-declaration-change',m3,[0,1],lambda v:v['getters']['modules'][0].update(data_declarations=99))
        obs('both-sided-data-declaration-bool',m3,[0,1],lambda v:v['getters']['modules'][0].update(data_declarations=False))
        obs('both-sided-place-omission',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['places'].pop())
        obs('both-sided-block-omission',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'].pop())
        obs('both-sided-instruction-omission',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['blocks'][0]['instructions'].pop())
        obs('both-sided-ABI-omission',m3,[0,1],lambda v:v['getters'].update(abi='None'))
        obs('both-sided-layout-omission',m3,[0,1],lambda v:v['getters'].update(linear32_layouts='None'))
        obs('both-sided-program-identity-change',m3,[0,1],lambda v:v['getters'].update(source_map_identity='SourceMapIdentity(99999)'))
        obs('both-sided-source-span-outside-map',m3,[0,1],lambda v:v['getters']['modules'][0]['functions'][0]['parameters'][0].update(span=[0,0,999999]))
        obs('both-sided-required-field-omission',m3,[0,1],lambda v:v['getters']['modules'][0].pop('data_declarations'))
        obs('both-sided-extra-projection-field',m3,[0,1],lambda v:v['getters'].update(hidden='ignored'))
        verify.verify(original_inventory,original_receipt,output)
        extra = output/'unreported.json'
        extra.write_text('{}')
        try:
            receipt_mutation('unreported-retained-file',lambda r:None,baseline_verified=True)
        finally:
            extra.unlink()
        source = output/'case-000/sources'/original_receipt['cases'][0]['sources'][0]['path']
        source_bytes = source.read_bytes()
        verify.verify(original_inventory,original_receipt,output)
        source.write_bytes(source_bytes+b'\n')
        try:
            receipt_mutation('original-source-context-bytes-mutated',lambda r:None,baseline_verified=True)
        finally:
            source.write_bytes(source_bytes)
        child = output/'case-000'
        held = output/'held-case-000'
        verify.verify(original_inventory,original_receipt,output)
        child.rename(held)
        try:
            if os.name == 'nt':
                subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(child),str(held)],
                               check=True,capture_output=True)
            else:
                child.symlink_to(held,target_is_directory=True)
            receipt_mutation('linked-parent-artifact',lambda r:None,baseline_verified=True)
        finally:
            if child.exists() or child.is_symlink():
                if os.name == 'nt':
                    child.rmdir()
                else:
                    child.unlink()
            held.rename(child)
        verify.verify(original_inventory,original_receipt,output)
    print(json.dumps({'hostile_controls_passed':len(passed),'controls':passed,'runtime_evidence':False}))

if __name__ == '__main__':
    main()
