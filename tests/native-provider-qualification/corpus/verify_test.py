"""Independent coherent malformed-row controls; these do not execute target programs."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('finite_corpus_verify',Path(__file__).with_name('verify.py'))
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
ROOT=Path(__file__).resolve().parents[3]


class CorpusControls(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.output=Path(self.temp.name);self.fixtures,self.cases=v.authority(ROOT)

    def create(self,case='string-fault-2-4'):
        row,injected=self.cases[case];fixture=row['fixture']
        sources,edges=v.source_contract(ROOT,self.fixtures,fixture)
        expected=v.expected_result(row,injected,'javascript')
        for provider in ['native-retained','bootstrap']:
            p=self.output/f'javascript-{case}-{provider}';(p/'sources').mkdir(parents=True)
            for source in sources:
                original=self.fixtures[fixture if source['id']==0 else self.fixtures[fixture]['dependency']]['path']
                (p/'sources'/source['path']).write_bytes((ROOT/original).read_bytes())
            metadata={'case':case,'fixture':fixture,'injected':injected,'provider':provider,'target':'javascript',
                'platform':'linux','registry_sha256':v.REGISTRY_SHA,'fault':row['fault'] if injected else None,
                'results':expected,'sources':{s['path']:s['sha256'] for s in sources},'native_first':True,
                'execution_checkpoints':3 if provider=='native-retained' else 0,
                'publication_checkpoints':['JavaScript','Manifest','Commit'] if provider=='native-retained' else [],
                'cleanup_entries':0,'public_activation':False,'installed_no_cargo_acceptance':False}
            b=p/'bundle';(b/'javascript').mkdir(parents=True)
            artifact=b/'javascript/fixed-oracle.mjs';artifact.write_bytes(b'export function score() { return 17; }\n')
            manifest={'version':3,'profile':'zryna-data-ownership-v1','protocol_version':4,'command':'run',
                'entrypoint':'main.zry','graph_sha256':v.graph_digest(3,sources,edges),'sources':sources,'edges':edges,
                'layouts':{k:'a'*64 for k in v.cold.LAYOUTS},'runtime_abi':v.cold.RUNTIME,'stem':'fixed-oracle',
                'targets':['javascript'],'invocation':{'export':'score','arguments':[]},'results':expected,'diagnostics':[],
                'artifacts':[{'target':'javascript','kind':'ecmascript-module','filename':'fixed-oracle.mjs',
                    'bytes':artifact.stat().st_size,'sha256':v.digest(artifact),
                    'metadata':{'profile':'ecmascript-module','scalar_abi':'v1'}}]}
            self.write(p/'observation.json',metadata);self.write(b/'zryna-manifest-v3.json',manifest)
        return case,row,injected

    def write(self,p,data):p.write_text(json.dumps(data))

    def admit(self,values):
        case,row,injected=values
        v.pair(ROOT,self.output,self.fixtures,case,row,injected,'javascript','linux')

    def change_both(self,case,change):
        for provider in ['native-retained','bootstrap']:
            p=self.output/f'javascript-{case}-{provider}'
            observation=v.cold.read(p/'observation.json');manifest=v.cold.read(p/'bundle/zryna-manifest-v3.json')
            change(observation,manifest,p)
            self.write(p/'observation.json',observation);self.write(p/'bundle/zryna-manifest-v3.json',manifest)

    def reject(self,values):
        with self.assertRaises((AssertionError,ValueError,KeyError,OSError,TypeError)):self.admit(values)

    def test_actual_summary_must_match_exit_status_and_one_executed_test(self):
        good='test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 302 filtered out;\n'
        bad='test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 302 filtered out;\n'
        v.execution_status('passed',0,good);v.execution_status('failed',101,bad)
        for status,code,text in [('passed',0,bad),('failed',101,good),('passed',0,good+bad),
                                 ('passed',0,good.replace('1 passed','0 passed')),('passed',False,good)]:
            with self.assertRaises(AssertionError):v.execution_status(status,code,text)

    def test_committed_source_identity_comes_from_git_blobs(self):
        import subprocess
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            def git(*args):return subprocess.check_output(['git','-C',str(root),*args])
            git('init','-q');git('config','user.email','corpus-test@example.invalid');git('config','user.name','Corpus test')
            path=root/'source.zry';path.write_bytes(b'frozen source\n');git('add','source.zry');git('commit','-qm','authority')
            before=v.committed_inputs(root);git('update-index','--assume-unchanged','source.zry');path.write_bytes(b'substituted source\n')
            self.assertEqual(git('status','--porcelain'),b'')
            self.assertEqual(v.committed_inputs(root),before)
            with self.assertRaises(AssertionError):v.exact({'source.zry':v.digest(path)},before)

    def test_all_frozen_ids_and_seven_source_graphs_are_exact(self):
        self.assertEqual(len(self.cases),31)
        self.assertEqual(sum(x[1] for x in self.cases.values()),24)
        for fixture in v.GROUPS:
            sources,edges=v.source_contract(ROOT,self.fixtures,fixture)
            self.assertEqual(len(sources),1 if fixture=='vec' else 2)
            self.assertEqual(len(edges),0 if fixture=='vec' else 1)

    def test_valid_multisource_row_and_positive_calibration(self):
        self.admit(self.create());self.admit(self.create('positive-owned-vec'))

    def test_entire_trace_reordered_coherently_in_both_providers_rejects(self):
        values=self.create()
        def mutate(o,m,p):
            o['results'][0]['trace'].reverse();m['results']=copy.deepcopy(o['results'])
        self.change_both(values[0],mutate);self.reject(values)

    def test_distinct_selector_with_same_outcome_and_trace_rejects(self):
        values=self.create('vec-fault-2-3')
        self.change_both(values[0],lambda o,m,p:o['fault'].update(ordinal=2))
        self.reject(values)

    def test_wrong_trap_and_boolean_positive_coherent_results_reject(self):
        values=self.create()
        def mutate(o,m,p):
            o['results'][0]['outcome']['code']='zryna.trap.capacity-v1';m['results']=copy.deepcopy(o['results'])
        self.change_both(values[0],mutate);self.reject(values)
        values=self.create('positive-vec')
        def mutate_positive(o,m,p):
            o['results'][0]['outcome']['value']['value']=True;m['results']=copy.deepcopy(o['results'])
        self.change_both(values[0],mutate_positive);self.reject(values)

    def test_dependency_source_substitution_rejects(self):
        values=self.create('owned-vec-fault-2-3')
        def mutate(o,m,p):(p/'sources/math.zry').write_bytes((ROOT/self.fixtures['owned-shared-body']['path']).read_bytes())
        self.change_both(values[0],mutate);self.reject(values)

    def test_forged_graph_with_coherent_new_digest_rejects(self):
        values=self.create()
        def mutate(o,m,p):m['edges']=[];m['graph_sha256']=v.graph_digest(3,m['sources'],[])
        self.change_both(values[0],mutate);self.reject(values)

    def test_complete_bundle_equality_detects_coherently_rehashed_difference(self):
        values=self.create();p=self.output/f'javascript-{values[0]}-bootstrap/bundle'
        artifact=p/'javascript/fixed-oracle.mjs';artifact.write_bytes(b'export function score() { return 99; }\n')
        m=v.cold.read(p/'zryna-manifest-v3.json');m['artifacts'][0].update(bytes=artifact.stat().st_size,sha256=v.digest(artifact))
        self.write(p/'zryna-manifest-v3.json',m);self.reject(values)

    def test_source_dependent_runtime_abi_requires_exact_identity_types_and_digest_shape(self):
        for field,wrong in [('identifier','foreign-runtime'),('version',True),('sha256','not-a-hash')]:
            with tempfile.TemporaryDirectory() as temporary:
                previous=self.output;self.output=Path(temporary);values=self.create()
                self.change_both(values[0],lambda o,m,p:m['runtime_abi'].update({field:wrong}))
                self.reject(values);self.output=previous
        values=self.create()
        p=self.output/f'javascript-{values[0]}-bootstrap/bundle/zryna-manifest-v3.json'
        m=v.cold.read(p);m['runtime_abi']['sha256']='b'*64;self.write(p,m);self.reject(values)

    def test_checkpoint_cleanup_and_activation_claims_reject(self):
        for field,wrong in [('execution_checkpoints',2),('cleanup_entries',1),('native_first',False),('public_activation',True)]:
            with tempfile.TemporaryDirectory() as temporary:
                previous=self.output;self.output=Path(temporary);values=self.create()
                self.change_both(values[0],lambda o,m,p:o.update({field:wrong}));self.reject(values);self.output=previous


if __name__=='__main__':unittest.main()
