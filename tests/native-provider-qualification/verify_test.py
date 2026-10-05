"""Hostile evidence controls independent of the qualification producer."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from pnpm_entry import resolve as resolve_pnpm
from verify import SOURCE_SHA, cli_binding, command_exits, diagnostic, exact, mutation_control, read, results, strict

ROOT = Path(__file__).resolve().parents[2]


def prevented(checkpoint='Execution'):
    return {'control':'source-mutation','checkpoint':checkpoint,'platform':'windows',
            'attempt':{'action':'prevented','io_error':{'kind':'PermissionDenied','raw_os_error':32},
                       'before_sha256':SOURCE_SHA,'after_sha256':SOURCE_SHA,'parent_replaced':False},
            'mutated':False,'execution_checkpoints':3,'publication_checkpoints':['JavaScript','Manifest','Commit'],
            'failure_kind':None,'diagnostics':[],
            'results':[{'target':'javascript','outcome':{'kind':'returned','value':{'type':'i32','value':13}}}],
            'output_entries':0,'public_activation':False}


def source_diagnostic(code='ZRYNA-D3004'):
    return {'code':code,'guidance':'stop concurrent mutation and retry','message':'source changed',
            'primary':{'kind':'global'},'severity':'error'}


class AdmissionControls(unittest.TestCase):
    def reject(self, value):
        with self.assertRaises((AssertionError, ValueError, KeyError, TypeError)):
            mutation_control(value,'win32')

    def test_four_actual_prevented_checkpoint_shapes(self):
        for checkpoint in ('Execution','Manifest','Commit','SourceDirectory'):
            mutation_control(prevented(checkpoint),'win32')

    def test_denial_requires_exact_sharing_violation_and_unchanged_bytes(self):
        for field, values in {'io_error':[None,{'kind':'PermissionDenied','raw_os_error':5},
                                          {'kind':'Other','raw_os_error':32}],
                              'after_sha256':['a'*64], 'parent_replaced':[True]}.items():
            for wrong in values:
                value = prevented()
                value['attempt'][field] = wrong
                self.reject(value)

    def test_denial_is_not_a_source_failure_or_different_execution(self):
        for key, wrong in [('failure_kind','Source'),('diagnostics',[{'code':'ZRYNA-D3004'}]),
                           ('execution_checkpoints',2),('publication_checkpoints',['Manifest','Commit']),
                           ('output_entries',1),('public_activation',True),('mutated',0)]:
            value = prevented()
            value[key] = wrong
            self.reject(value)

    def test_detected_write_requires_source_code_changed_bytes_and_cleanup(self):
        value = prevented()
        value.update(mutated=True,failure_kind='Source',diagnostics=[source_diagnostic()],
                     results=[],execution_checkpoints=2,publication_checkpoints=['JavaScript'])
        value['attempt'].update(action='changed',io_error=None,
                               after_sha256=hashlib.sha256(b'export function score(): i32 { return 99; }\n').hexdigest())
        mutation_control(value,'win32')
        for key, wrong in [('failure_kind','Preparation'),('diagnostics',[{'code':'ZRYNA-C1009'}]),
                           ('results',[{}]),('output_entries',1)]:
            changed = copy.deepcopy(value)
            changed[key] = wrong
            self.reject(changed)
        value['attempt']['after_sha256'] = SOURCE_SHA
        self.reject(value)

    def test_nested_directory_replacement_requires_changed_identity_identical_bytes(self):
        value = prevented('SourceDirectory')
        value.update(mutated=True,failure_kind='Source',diagnostics=[source_diagnostic()],
                     results=[],execution_checkpoints=2,publication_checkpoints=['JavaScript'])
        value['attempt'].update(action='changed',io_error=None,parent_replaced=True)
        mutation_control(value,'win32')
        value['attempt']['parent_replaced'] = False
        self.reject(value)

    def test_frozen_faults_require_entire_ordered_trace_and_exact_typed_score(self):
        registry = read(ROOT/'tests/m3-conformance-v1.json')
        first = results('vec-fault-2-1','javascript',registry)
        second = results('vec-fault-2-2','webassembly',registry)
        self.assertNotIn('trace',first[0])
        self.assertEqual(len(second[0]['trace']),2)
        changed = copy.deepcopy(second)
        changed[0]['trace'].reverse()
        with self.assertRaises(AssertionError):
            exact(changed,second)
        score = results('score13','javascript',registry)
        changed = copy.deepcopy(score)
        changed[0]['outcome']['value']['value'] = True
        with self.assertRaises(AssertionError):
            exact(changed,score)

    def test_duplicate_nonfinite_and_optimized_shape_claims_reject(self):
        for raw in ('{"mutated":false,"mutated":true}','{"output_entries":NaN}',
                    '{"value":13.0}'):
            with self.assertRaises((AssertionError,ValueError)):
                strict(raw)
        value = prevented()
        value['unknown'] = True
        self.reject(value)

    def test_diagnostics_require_complete_schema_types_and_owning_code(self):
        diagnostic([source_diagnostic()],'ZRYNA-D3004')
        for field, wrong in [('severity','warning'),('primary',{'kind':'source'}),('guidance',''),
                             ('message',True),('code','ZRYNA-C1009')]:
            value = source_diagnostic()
            value[field] = wrong
            with self.assertRaises(AssertionError):
                diagnostic([value],'ZRYNA-D3004')
        with self.assertRaises(AssertionError):
            diagnostic([{'code':'ZRYNA-D3004'}],'ZRYNA-D3004')

    def test_coherent_command_receipt_rejects_boolean_exits_reordering_and_extra_steps(self):
        labels = ['feature-test-build','retained-fault-tests','cold-checkout','cold-native-build',
                  'cold-build-collision','bootstrap-pnpm-version','bootstrap-frozen-install','cold-bootstrap-comparison']
        rows = [{'label':name,'argv':['fixed'],'cwd':'fixture','exit':1 if index == 4 else 0}
                for index,name in enumerate(labels)]
        command_exits(rows)
        for index in range(len(rows)):
            changed = copy.deepcopy(rows)
            changed[index]['exit'] = bool(rows[index]['exit'])
            with self.assertRaises(AssertionError):
                command_exits(changed)
        for changed in (rows[::-1],rows+[rows[0]],rows[:-1]):
            with self.assertRaises(AssertionError):
                command_exits(changed)

    def test_coherent_cli_identity_rejects_source_platform_binary_and_tool_substitution(self):
        tools = {'node':{'path':'explicit-node','sha256':'a'*64,'version':'v22.22.1'}}
        feature, default = {'path':'feature','sha256':'b'*64},{'path':'default','sha256':'c'*64}
        receipt = {'root':'exact-source','target':'external-target','tools':tools,
                   'feature_cli':feature,'default_cli':default}
        prior = {'status':'passed','format':'zryna.private-cli-ci.v1','head':'1'*40,'tree':'2'*40,
                 'platform':'win32','root':receipt['root'],'target':receipt['target'],
                 'inputs':{'fixture.zry':SOURCE_SHA},'tools':tools,
                 'binaries':{'feature':feature,'default':default}}
        cli_binding(receipt,prior,'1'*40,'2'*40,'win32',prior['inputs'])
        for key, wrong in [('head','3'*40),('tree','4'*40),('platform','linux'),('inputs',{}),
                           ('root','other-source'),('tools',{}),('status','failed'),
                           ('binaries',{'feature':default,'default':feature})]:
            changed = copy.deepcopy(prior)
            changed[key] = wrong
            with self.assertRaises(AssertionError):
                cli_binding(receipt,changed,'1'*40,'2'*40,'win32',prior['inputs'])


class PnpmLayoutControls(unittest.TestCase):
    def package(self, root, version):
        script = root/'bin/pnpm.mjs'
        script.parent.mkdir(parents=True)
        script.write_text('// independent package manager layout fixture\n')
        (root/'package.json').write_text(json.dumps({'name':'pnpm','version':version}))
        return script

    def test_local_node_entry_and_command_shim_layout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = self.package(root/'node_modules/pnpm','11.18.0')
            shim = root/'node_modules/.bin/pnpm.cmd'
            shim.parent.mkdir()
            shim.write_text('independent command shim fixture')
            self.assertEqual(resolve_pnpm(shim),script.resolve())
            self.assertEqual(resolve_pnpm(script),script.resolve())

    def test_action_self_update_layout_selects_requested_version_over_bootstrap(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            home = root/'node_modules/.bin'
            shim = home/'bin/pnpm.cmd'
            shim.parent.mkdir(parents=True)
            shim.write_text('self-update command shim fixture')
            self.package(root/'node_modules/pnpm','11.19.0')
            requested = self.package(home/'global/v11/opaque-install/node_modules/pnpm','11.18.0')
            self.assertEqual(resolve_pnpm(shim,home),requested.resolve())
            with self.assertRaises(AssertionError):
                resolve_pnpm(shim)

    def test_missing_wrong_version_or_ambiguous_requested_entries_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            shim = home/'bin/pnpm.cmd'
            shim.parent.mkdir()
            shim.write_text('command shim fixture')
            self.package(home/'global/v11/old/node_modules/pnpm','11.19.0')
            with self.assertRaises(AssertionError):
                resolve_pnpm(shim,home)
            self.package(home/'global/v11/first/node_modules/pnpm','11.18.0')
            self.package(home/'global/v11/second/node_modules/pnpm','11.18.0')
            with self.assertRaises(AssertionError):
                resolve_pnpm(shim,home)


if __name__ == '__main__':
    unittest.main()
