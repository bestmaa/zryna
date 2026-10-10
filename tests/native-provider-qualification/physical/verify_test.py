"""Hostile reader controls use fabricated frames, never claim native execution."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('physical_verify',Path(__file__).with_name('verify.py'))
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
ROOT=Path(__file__).resolve().parents[3]

class PhysicalControls(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.output=Path(self.temp.name);self.fixtures,self.cases=v.authority(ROOT)

    def create(self,case=v.PHYSICAL_CASE):
        row,injected=self.cases[case];sources,edges=v.source_contract(ROOT,self.fixtures,'owned-shared')
        expected=v.expected_result(row,injected,'native')
        for provider in ['native-retained','bootstrap']:
            p=self.output/f'native-{case}-{provider}';(p/'sources').mkdir(parents=True)
            for s in sources:
                f=self.fixtures['owned-shared' if s['id']==0 else 'owned-shared-body']
                (p/'sources'/s['path']).write_bytes((ROOT/f['path']).read_bytes())
            o={'case':case,'fixture':'owned-shared','injected':injected,'provider':provider,'target':'native',
               'platform':'linux','registry_sha256':v.REGISTRY_SHA,'fault':v.FAULT if injected else None,
               'trace_oracle':v.ORACLE if injected else None,'results':expected,'sources':{s['path']:s['sha256'] for s in sources},
               'native_first':True,'execution_checkpoints':3 if provider=='native-retained' else 0,
               'publication_checkpoints':['Native','Manifest','Commit'] if provider=='native-retained' else [],
               'cleanup_entries':0,'zero_live_finalization':True,'physical_allocation_release_counts':None,
               'public_activation':False,'installed_no_cargo_acceptance':False}
            b=p/'bundle';(b/'native').mkdir(parents=True)
            artifact=b/'native/fixed-oracle.elf';artifact.write_bytes(b'\x7fELF\x02\x01\x01'+b'\0'*9+b'\x02\0')
            m={'version':3,'profile':'zryna-data-ownership-v1','protocol_version':4,'command':'run',
               'entrypoint':'main.zry','graph_sha256':v.graph_digest(3,sources,edges),'sources':sources,'edges':edges,
               'layouts':{k:'a'*64 for k in v.cold.LAYOUTS},'runtime_abi':v.cold.RUNTIME,'stem':'fixed-oracle',
               'targets':['native'],'invocation':{'export':'score','arguments':[]},'results':expected,'diagnostics':[],
               'artifacts':[{'target':'native','kind':'linux-x86-64-invocation-executable','filename':'fixed-oracle.elf',
                 'bytes':artifact.stat().st_size,'sha256':v.digest(artifact),'metadata':{'profile':'linux-x86-64-elf',
                 'triple':'x86_64-unknown-linux-gnu','format':'elf-executable','scalar_abi':'v1',
                 'program_object_sha256':'a'*64,'runtime_object_sha256':'b'*64,'harness_sha256':'c'*64}}]}
            self.write(p/'observation.json',o);self.write(b/'zryna-manifest-v3.json',m)
        return case,row,injected

    def write(self,p,v):p.write_text(json.dumps(v))
    def admit(self,values):
        case,row,injected=values;v.pair(ROOT,self.output,self.fixtures,case,row,injected,'native','linux')
    def mutate(self,values,fn):
        for provider in ['native-retained','bootstrap']:
            p=self.output/f'native-{values[0]}-{provider}';o=v.cold.read(p/'observation.json');m=v.cold.read(p/'bundle/zryna-manifest-v3.json')
            fn(o,m,p);self.write(p/'observation.json',o);self.write(p/'bundle/zryna-manifest-v3.json',m)
    def reject(self,values):
        with self.assertRaises((AssertionError,ValueError,KeyError,OSError,TypeError)):self.admit(values)

    def test_exact_group_and_positive_control(self):
        self.assertEqual(set(self.cases),{v.PHYSICAL_CASE,'positive-owned-shared'})
        self.admit(self.create());self.admit(self.create('positive-owned-shared'))

    def test_logical_injection_cannot_substitute_for_physical(self):
        x=self.create();self.mutate(x,lambda o,m,p:o.update(fault={'code':2,'ordinal':2}));self.reject(x)

    def test_same_trace_wrong_physical_ordinal_rejects(self):
        x=self.create();self.mutate(x,lambda o,m,p:o['fault'].update(ordinal=2,command=0x26000002));self.reject(x)

    def test_coherent_reordered_trace_rejects(self):
        x=self.create()
        def change(o,m,p):o['results'][0]['trace'].reverse();m['results']=copy.deepcopy(o['results'])
        self.mutate(x,change);self.reject(x)

    def test_coherent_wrong_trap_rejects(self):
        x=self.create()
        def change(o,m,p):o['results'][0]['outcome']['code']='zryna.trap.refcount-v1';m['results']=copy.deepcopy(o['results'])
        self.mutate(x,change);self.reject(x)

    def test_fake_physical_counts_and_nonzero_cleanup_reject(self):
        for field,value in [('physical_allocation_release_counts',{'allocated':3,'released':3}),('zero_live_finalization',False),('cleanup_entries',1),('public_activation',True),('execution_checkpoints',2)]:
            with tempfile.TemporaryDirectory() as temp:
                self.output=Path(temp);x=self.create();self.mutate(x,lambda o,m,p:o.update({field:value}));self.reject(x)

    def test_source_substitution_and_forged_graph_reject(self):
        x=self.create();self.mutate(x,lambda o,m,p:(p/'sources/math.zry').write_bytes(b'export function aggregate(): i32 { return 43; }\n'));self.reject(x)
        self.output=Path(self.temp.name)/'second';self.output.mkdir();x=self.create()
        def change(o,m,p):m['edges']=[];m['graph_sha256']=v.graph_digest(3,m['sources'],[])
        self.mutate(x,change);self.reject(x)

    def test_complete_provider_bundle_equality(self):
        x=self.create();p=self.output/f'native-{x[0]}-bootstrap/bundle';a=p/'native/fixed-oracle.elf';a.write_bytes(a.read_bytes()+b'changed')
        m=v.cold.read(p/'zryna-manifest-v3.json');m['artifacts'][0].update(bytes=a.stat().st_size,sha256=v.digest(a));self.write(p/'zryna-manifest-v3.json',m);self.reject(x)

if __name__=='__main__':unittest.main()
