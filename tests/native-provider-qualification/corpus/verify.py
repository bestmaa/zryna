"""Admit frozen per-case observations and complete provider bundles; no new execution."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import verify as cold
from verify_ci_receipts import exact, strict, digest, git
from manifest_admission import graph_digest, _file

REGISTRY_SHA='34cd29a5f146d77e7163b32d21e71e4f5a1fc5fd50f688d197de8bef9b38a508'
PREVIOUS=('vec-fault-2-1','vec-fault-2-2')
GROUPS={'string':'text','vec':None,'handles':'handles','weak-live':'check',
        'owned-aggregate':'aggregate','owned-vec':'aggregate','owned-shared':'aggregate'}
TEST='ownership_commands::conformance::native_provider_faults::corpus::remaining_frozen_fault_groups_execute_through_retained_source'


def committed_inputs(root):
    entries=[]
    for record in subprocess.check_output(['git','-C',str(root),'ls-tree','-rz','--full-tree','HEAD']).split(b'\0'):
        if not record:continue
        header,name=record.split(b'\t',1);mode,kind,oid=header.split()
        assert kind==b'blob' and mode in (b'100644',b'100755')
        entries.append((name.decode(),oid))
    assert len(entries)<=5000
    data=subprocess.check_output(['git','-C',str(root),'cat-file','--batch'],
                                 input=b'\n'.join(oid for name,oid in entries)+b'\n')
    assert len(data)<=128*1024*1024
    found={};offset=0
    for name,oid in entries:
        end=data.index(b'\n',offset);header=data[offset:end].split()
        assert header[:2]==[oid,b'blob'];size=int(header[2]);offset=end+1
        found[name]=hashlib.sha256(data[offset:offset+size]).hexdigest();offset+=size
        assert data[offset:offset+1]==b'\n';offset+=1
    assert offset==len(data);return found


def execution_status(status,exit_code,text):
    assert type(exit_code) is int and exit_code in (0,101)
    exact(status,'passed' if exit_code==0 else 'failed')
    summaries=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',text,re.M)
    assert len(summaries)==1
    kind,*numbers=summaries[0];passed,failed,ignored,measured,filtered=map(int,numbers)
    exact([kind,passed,failed,ignored,measured],['ok',1,0,0,0] if exit_code==0 else ['FAILED',0,1,0,0])


def authority(root):
    path=root/'tests/m3-conformance-v1.json';assert digest(path)==REGISTRY_SHA
    registry=cold.read(path);fixtures={r['id']:r for r in registry['fixtures']}
    assert len(fixtures)==len(registry['fixtures']) and len(registry['faults'])==26
    faults=[r for r in registry['faults'] if r['id'] not in PREVIOUS]
    assert len(faults)==24 and len({r['id'] for r in faults})==24
    cases={r['id']:(r,True) for r in faults}
    for fixture in GROUPS:
        rows=[r for r in registry['valid'] if r['id']==r['fixture']==fixture]
        assert len(rows)==1;cases['positive-'+fixture]=(rows[0],False)
    return fixtures,cases


def source_contract(root,fixtures,fixture):
    row=fixtures[fixture];sources=[{'id':0,'path':'main.zry','sha256':row['sha256']}];edges=[]
    assert digest(root/row['path'])==row['sha256']
    imported=GROUPS[fixture]
    if imported:
        other=fixtures[row['dependency']];assert digest(root/other['path'])==other['sha256']
        sources.append({'id':1,'path':'math.zry','sha256':other['sha256']})
        expected=f'import {{ {imported} }} from "./math.zry";\nexport function score(): i32 {{ return {imported}(); }}\n'
        exact((root/row['path']).read_text(),expected)
        edges=[{'importer':'main.zry','target':'math.zry','specifier':'./math.zry','imported':imported,'local':imported}]
    else:assert 'dependency' not in row
    return sources,edges


def expected_result(row,injected,target):
    outcome=row['expected'] if injected else {'kind':'returned','value':{'type':'i32','value':row['expected']}}
    trace=row['trace'] if injected else []
    return [{'target':target,'outcome':outcome,**({'trace':trace} if trace else {})}]


def bundle(directory,target,expected,sources,edges):
    value=cold.read(directory/'zryna-manifest-v3.json');inventory=cold.files(directory)
    assert set(value)=={'version','profile','protocol_version','command','entrypoint','graph_sha256',
                        'sources','edges','layouts','runtime_abi','stem','targets','artifacts','invocation','results','diagnostics'}
    layouts=value['layouts'];assert set(layouts)==set(cold.LAYOUTS)
    assert all(type(v) is str and re.fullmatch('[0-9a-f]{64}',v) for v in layouts.values())
    # Layout digests bind whole provider equality; this does not independently reconstruct layouts.
    exact({k:v for k,v in value.items() if k!='artifacts'},
          {'version':3,'profile':'zryna-data-ownership-v1','protocol_version':4,'command':'run',
           'entrypoint':'main.zry','graph_sha256':graph_digest(3,sources,edges),'sources':sources,'edges':edges,
           'layouts':layouts,'runtime_abi':cold.RUNTIME,'stem':'fixed-oracle','targets':[target],
           'invocation':{'export':'score','arguments':[]},'results':expected,'diagnostics':[]})
    assert type(value['artifacts']) is list and len(value['artifacts'])==1
    a=value['artifacts'][0];assert set(a)=={'target','kind','filename','bytes','sha256','metadata'}
    extension,kind={'javascript':('mjs','ecmascript-module'),'webassembly':('wasm','core-webassembly-module'),
                    'native':('elf','linux-x86-64-invocation-executable')}[target]
    name=f'{target}/fixed-oracle.{extension}'
    assert set(inventory)=={'zryna-manifest-v3.json',name}
    exact({k:v for k,v in a.items() if k!='metadata'},
          {'target':target,'kind':kind,'filename':'fixed-oracle.'+extension,**inventory[name]})
    assert 0<a['bytes']<=32*1024*1024
    data=_file(directory,name).read_bytes()
    if target=='javascript':
        assert 'export ' in data.decode('utf-8');exact(a['metadata'],{'profile':'ecmascript-module','scalar_abi':'v1'})
    elif target=='webassembly':
        assert data[:8]==b'\0asm\x01\0\0\0'
        exact(a['metadata'],{'profile':'core-webassembly','memory':'linear32-v1','imports':False,'scalar_abi':'v1'})
    else:
        assert data[:7]==b'\x7fELF\x02\x01\x01' and int.from_bytes(data[16:18],'little')==2
        metadata=a['metadata'];assert set(metadata)=={'profile','triple','format','scalar_abi',
            'program_object_sha256','runtime_object_sha256','harness_sha256'}
        assert all(type(metadata[k]) is str and re.fullmatch('[0-9a-f]{64}',metadata[k])
                   for k in ['program_object_sha256','runtime_object_sha256','harness_sha256'])
        exact({k:metadata[k] for k in ['profile','triple','format','scalar_abi']},
              {'profile':'linux-x86-64-elf','triple':'x86_64-unknown-linux-gnu','format':'elf-executable','scalar_abi':'v1'})
    return inventory


def pair(root,observations,fixtures,case,row,injected,target,platform):
    fixture=row['fixture'];sources,edges=source_contract(root,fixtures,fixture)
    source_hashes={s['path']:s['sha256'] for s in sources};expected=expected_result(row,injected,target)
    inventories=[]
    for provider in ['native-retained','bootstrap']:
        path=observations/f'{target}-{case}-{provider}'
        value=cold.read(path/'observation.json')
        exact(value,{'case':case,'fixture':fixture,'injected':injected,'provider':provider,'target':target,
                     'platform':'windows' if platform=='win32' else 'linux','registry_sha256':REGISTRY_SHA,
                     'fault':row['fault'] if injected else None,
                     'results':expected,'sources':source_hashes,'native_first':True,
                     'execution_checkpoints':3 if provider=='native-retained' else 0,
                     'publication_checkpoints':[cold.STAGES[target],'Manifest','Commit'] if provider=='native-retained' else [],
                     'cleanup_entries':0,'public_activation':False,'installed_no_cargo_acceptance':False})
        exact({p.name:digest(p) for p in (path/'sources').iterdir()},source_hashes)
        inventories.append(bundle(path/'bundle',target,expected,sources,edges))
    exact(inventories[0],inventories[1])


def verify(root,output,head,platform,archived=False,run_id=None,run_attempt=None):
    assert __debug__ and platform in ('linux','win32')
    count=0
    for p in [output,*output.rglob('*')]:
        count+=1;assert count<=4096
        st=p.lstat();assert not stat.S_ISLNK(st.st_mode) and not getattr(st,'st_file_attributes',0)&0x400
        assert stat.S_ISDIR(st.st_mode) or stat.S_ISREG(st.st_mode)
        if p.is_file():assert st.st_size<=32*1024*1024
    for parent in output.absolute().parents:
        assert not parent.is_symlink() and not getattr(parent.lstat(),'st_file_attributes',0)&0x400
    receipt=cold.read(output/'receipt.json')
    required={'format','head','tree','platform','root','target','inputs','tools','commands','run_id','run_attempt',
              'status','public_activation','installed_no_cargo_acceptance','test_binary'}
    assert set(receipt) in (required,required|{'failure'})
    for k,v in {'format':'zryna.retained-fault-corpus.v1','head':head,'platform':platform,
                'public_activation':False,'installed_no_cargo_acceptance':False}.items():exact(receipt[k],v)
    assert receipt['status'] in ('passed','failed')
    assert git(root,'rev-parse','HEAD')==head and git(root,'rev-parse','HEAD^{tree}')==receipt['tree']
    assert not git(root,'status','--porcelain','--untracked-files=all')
    actual={n:digest(root/n) for n in git(root,'ls-files').splitlines()}
    exact(actual,committed_inputs(root));exact(receipt['inputs'],actual)
    for k in ['run_id','run_attempt']:
        wanted={'run_id':run_id,'run_attempt':run_attempt}[k] if archived else os.getenv('GITHUB_'+k.upper())
        exact(receipt[k],wanted)
    assert set(receipt['tools'])=={'cargo','rustc','node'}
    for name,prefix in [('cargo','cargo 1.97.1 '),('rustc','rustc 1.97.1 '),('node','v22.22.1')]:
        tool=receipt['tools'][name];assert set(tool)=={'path','sha256','version'}
        assert type(tool['version']) is str
        if name=='node':exact(tool['version'],prefix)
        else:assert tool['version'].startswith(prefix)
        assert type(tool['sha256']) is str and re.fullmatch('[0-9a-f]{64}',tool['sha256'])
    assert set(receipt['test_binary'])=={'path','sha256'} and re.fullmatch('[0-9a-f]{64}',receipt['test_binary']['sha256'])
    if not archived:
        assert sys.platform==platform and root.resolve()==Path(receipt['root']).resolve()
        assert not git(root,'status','--porcelain','--untracked-files=all')
        for r in [*receipt['tools'].values(),receipt['test_binary']]:assert digest(Path(r['path']))==r['sha256']
    cmds=receipt['commands'];assert len(cmds)==2 and all(set(c)=={'label','argv','cwd','exit'} for c in cmds)
    exact(cmds[0],{'label':'feature-test-build','argv':[receipt['tools']['cargo']['path'],'test','--locked','--offline',
              '-p','zryna-driver','--features','native-provider-internal','--lib','--no-run','--message-format=json'],
              'cwd':receipt['root'],'exit':0})
    exact({k:cmds[1][k] for k in ['label','argv','cwd']},{'label':'finite-corpus',
          'argv':[receipt['test_binary']['path'],TEST,'--exact','--nocapture','--test-threads=1'],'cwd':receipt['root']})
    assert type(cmds[1]['exit']) is int and cmds[1]['exit'] in (0,101)
    exact(receipt['status'],'passed' if cmds[1]['exit']==0 else 'failed')
    records=[strict(line) for line in (output/'feature-test-build.stdout').read_bytes().splitlines()]
    matches=[r for r in records if r.get('reason')=='compiler-artifact' and r.get('executable')
             and r['target']['name']=='zryna_driver' and r['profile']['test']]
    assert len(matches)==1;exact(matches[0]['executable'],receipt['test_binary']['path'])
    cold.cargo_package_identity(matches[0],receipt,platform)
    text=(output/'finite-corpus.stdout').read_text()
    execution_status(receipt['status'],cmds[1]['exit'],text)
    fixtures,cases=authority(root);targets=('javascript','webassembly','native') if platform=='linux' else ('javascript','webassembly')
    wanted={f'{t}-{case}-{p}' for t in targets for case in cases for p in ['native-retained','bootstrap']}
    observations=output/'observations';assert {p.name for p in observations.iterdir()}==wanted
    rows=[]
    for case,(row,injected) in cases.items():
        for target in targets:
            try:
                pair(root,observations,fixtures,case,row,injected,target,platform)
                rows.append({'case':case,'fixture':row['fixture'],'injected':injected,'target':target,'status':'passed'})
            except (AssertionError,OSError,ValueError,KeyError,TypeError) as error:
                rows.append({'case':case,'fixture':row['fixture'],'injected':injected,'target':target,
                             'status':'failed','failure':str(error) or type(error).__name__})
    failed=[r for r in rows if r['status']!='passed']
    discharged=[c for c,(r,fault) in cases.items() if fault and all(x['status']=='passed' for x in rows if x['case']==c)]
    return {'head':head,'tree':receipt['tree'],'platform':platform,'run_id':receipt['run_id'],'run_attempt':receipt['run_attempt'],
            'status':'passed' if not failed and receipt['status']=='passed' else 'failed','case_rows':rows,
            'passed_pairs':len(rows)-len(failed),'failed_pairs':len(failed),'observations':2*(len(rows)-len(failed)),
            'discharged_logical_fault_ids':discharged,'remaining_logical_fault_ids':[c for c,(r,f) in cases.items() if f and c not in discharged],
            'prior_two_fault_ids':list(PREVIOUS),'positive_groups':list(GROUPS),'archived':archived,
            'layout_scope':'strict identity shape plus complete provider byte equality; no independent layout reconstruction',
            'physical_allocation_probes':'separate, not discharged','public_activation':False,'installed_no_cargo_acceptance':False}


def main():
    import json
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    p.add_argument('--head',required=True);p.add_argument('--platform',choices=['linux','win32'],required=True)
    a=p.parse_args();result=verify(a.root,a.output,a.head,a.platform)
    with (a.output/'admission.json').open('x') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps(result));return int(result['status']!='passed')


if __name__=='__main__':sys.exit(main())
