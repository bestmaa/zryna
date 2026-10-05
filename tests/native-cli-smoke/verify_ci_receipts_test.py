"""Hostile receipt tests use synthetic files; they are not compiler/platform proof."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('private_cli_receipts',Path(__file__).with_name('verify_ci_receipts.py'))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        self.root,self.output = base/'source',base/'proof'
        self.root.mkdir(); self.output.mkdir()
        (base/'target').mkdir()
        (self.root/'.gitignore').write_text('.zryna/\n')
        repo = Path(__file__).resolve().parents[2]
        registry = json.loads((repo/'tests/m3-conformance-v1.json').read_text())
        paths = ['examples/universal/add.zry','tests/m2-fixtures/valid/main.zry',
                 'tests/m2-fixtures/valid/math.zry','tests/m2-conformance-v1.json','tests/m3-conformance-v1.json',
                 'scripts/run-native-cli-smoke.py'] + [row['path'] for row in registry['fixtures']]
        for name in set(paths):
            path = self.root/name
            path.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(repo/name,path)
        subprocess.run(['git','init','-q',str(self.root)],check=True)
        subprocess.run(['git','-C',str(self.root),'add','.'],check=True)
        subprocess.run(['git','-C',str(self.root),'-c','user.name=Receipt Test',
                        '-c','user.email=receipt-test@example.invalid','commit','-qm','fixture'],check=True)
        self.head = oracle.git(self.root,'rev-parse','HEAD')
        tree = oracle.git(self.root,'rev-parse','HEAD^{tree}')
        inputs = {name:oracle.digest(self.root/name) for name in oracle.git(self.root,'ls-files').splitlines()}
        tools,binaries = {},{}
        for name,prefix in (('node','v22.22.1'),('cargo','cargo 1.97.1 (synthetic)'),('rustc','rustc 1.97.1 (synthetic)')):
            p=base/name; p.write_text(name); tools[name]={'path':str(p),'sha256':oracle.digest(p),'version':prefix}
            (self.output/(name+'-version.stdout')).write_text(prefix+'\n')
        for name in ('default','feature'):
            p=self.output/(name+'-cli'); p.write_text(name); binaries[name]={'path':str(p),'sha256':oracle.digest(p)}
        commands=[{'label':name+'-version','argv':[tools[name]['path'],'--version'],'exit':0} for name in ('cargo','rustc','node')]
        for name,extra in (('default',[]),('feature',['--features','native-provider-internal'])):
            commands.append({'label':name+'-build','argv':[tools['cargo']['path'],'build','--locked','--offline','-p','zryna',*extra],'exit':0})
        commands.append({'label':'cli-smoke','argv':[sys.executable,'-B',str(self.root/'scripts/run-native-cli-smoke.py'),
            '--root',str(self.root),'--default-cli',binaries['default']['path'],'--feature-cli',binaries['feature']['path'],
            '--node',tools['node']['path'],'--cargo',tools['cargo']['path'],'--rustc',tools['rustc']['path'],
            '--output',str(self.output/'smoke')],'exit':0})
        for row in commands:
            row.update(cwd=str(self.root),target=str(base/'target'))
        self.ci={'format':'zryna.private-cli-ci.v1','head':self.head,'tree':tree,'platform':'linux',
                 'root':str(self.root),'target':str(base/'target'),'status':'passed','inputs':inputs,
                 'tools':tools,'binaries':binaries,'commands':commands,'smoke_receipt':'smoke/receipt.json',
                 'public_activation':False,'installed_distribution_acceptance':False,'python':sys.executable}
        smoke=self.output/'smoke'; smoke.mkdir(); (smoke/'empty-path').mkdir()
        cases,generated = oracle.contracts(self.root,smoke,inputs)
        for name,row in generated.items():
            path = self.root/name
            path.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(self.root/row['original'],path)
        rows=[]
        for label in oracle.POSITIVES:
            contract = cases[label]
            manifest='zryna-manifest-v'+('1' if label=='m1' else '2' if label=='m2' else '3')+'.json'
            stem = 'private-smoke-'+label
            elf = bytearray(64)
            elf[:7] = b'\x7fELF\x02\x01\x01'
            elf[16:18] = (1).to_bytes(2,'little')
            elf[18:20] = (62).to_bytes(2,'little')
            elf[20:24] = (1).to_bytes(4,'little')
            elf[52:54] = (64).to_bytes(2,'little')
            payloads = {'javascript/'+stem+'.mjs':b'export const syntheticUnitFixture = 0;\n',
                        'webassembly/'+stem+'.wasm':b'\0asm\x01\0\0\0',
                        'native/'+stem+'.o':bytes(elf)}
            meta = {'version':contract['version'],'profile':('zryna-m1-cli-v1' if label=='m1' else
                    'zryna-control-flow-v1' if label=='m2' else 'zryna-data-ownership-v1'),
                    'command':'build','entrypoint':contract['source'],'stem':stem,
                    'targets':['javascript','webassembly','native'],'artifacts':[],
                    'invocation':None,'results':[],'diagnostics':[]}
            if label=='m1': meta['source_sha256']=inputs[contract['source']]
            else:
                from manifest_admission import graph_digest
                meta.update(sources=contract['sources'],edges=contract['edges'],
                            graph_sha256=graph_digest(contract['version'],contract['sources'],contract['edges']))
            if label.startswith('m3-'):
                meta.update(protocol_version=4,layouts={key:'a'*64 for key in
                    ('type_universe_sha256','linear32_sha256','linux_x86_64_sha256')},
                    runtime_abi={'identifier':'zryna-ownership-runtime-v1','version':1,'sha256':'a'*64})
            for route in ('bootstrap','native'):
                bundle=smoke/(label+'-'+route+'-bundle'); bundle.mkdir()
                for name,data in payloads.items():
                    path=bundle/name; path.parent.mkdir(); path.write_bytes(data)
                if not meta['artifacts']:
                    for target,kind,extension in (('javascript','ecmascript-module','mjs'),
                            ('webassembly','core-webassembly-module','wasm'),('native','linux-x86-64-relocatable-object','o')):
                        name=target+'/'+stem+'.'+extension
                        row={'target':target,'kind':kind,'bytes':(bundle/name).stat().st_size,'sha256':oracle.digest(bundle/name)}
                        if label.startswith('m3-'):
                            row['filename']=stem+'.'+extension
                            if target=='javascript': row['metadata']={'profile':'ecmascript-module','scalar_abi':'v1'}
                            elif target=='webassembly': row['metadata']={'profile':'core-webassembly','memory':'linear32-v1','imports':False,'scalar_abi':'v1'}
                            else: row['metadata']={'profile':'linux-x86-64-elf','triple':'x86_64-unknown-linux-gnu',
                                'format':'elf-relocatable','scalar_abi':'v1','program_object_sha256':row['sha256'],
                                'runtime_object_sha256':None,'harness_sha256':None}
                        else: row['path']=name
                        meta['artifacts'].append(row)
                (bundle/manifest).write_text(json.dumps(meta))
                (smoke/(label+'-'+route+'.stdout')).write_text(json.dumps({'version':1,'ok':True,'command':'build',
                    'manifest':'.zryna/out/'+stem+'.build/'+manifest,'results':[],'diagnostics':[]}))
            (smoke/(label+'-create-only.stdout')).write_text('{"ok":false}')
            rows.append({'id':label,'status':'passed','detail':{'source':contract['source'],'profile':contract['profile'],
                         'files':oracle.files(bundle),'success_json_exact':True,
                         'manifest_bytes_exact':True,'create_only':True,'node_on_path':False,'pnpm_on_path':False}})
        for label,code in oracle.NEGATIVES.items():
            raw=json.dumps({'ok':False,'diagnostics':[{'code':code}]})
            (smoke/(label+'.stdout')).write_text(raw); (smoke/(label+'-bootstrap.stdout')).write_text(raw)
            rows.append({'id':label,'status':'passed','detail':{'expected_code':code,'exit':1,'final_bundle_absent':True}})
        for label in oracle.CONTROLS[:-1]:
            detail={'exit':2}
            if label in ('private-project-denied','private-component-denied'):
                code='ZRYNA-C2001' if label=='private-project-denied' else 'ZRYNA-C1013'
                (smoke/(label+'.stdout')).write_text(json.dumps({'ok':False,'diagnostics':[{'code':code}]}))
            elif label=='source-checkout-still-needs-cargo':
                detail['architecture_gate_retained']=True; (smoke/(label+'.stdout')).write_text('ZRYNA-A1101')
            else:
                (smoke/(label+'.stderr')).write_text('--native-frontend --node rejected')
            rows.append({'id':label,'status':'passed','detail':detail})
        rows.append({'id':'source-and-binary-identity','status':'passed'})
        self.smoke={'version':1,'head':self.head,'tree':tree,'inputs':inputs,'public_activation':False,
                    'path':str(smoke/'empty-path'),'counts':{'passed':21,'failed':0,'ignored':0},'records':rows,
                    'binaries':{'default_cli':binaries['default'],'feature_cli':binaries['feature'],
                                **{name:{k:row[k] for k in ('path','sha256')} for name,row in tools.items()}},
                    'generated_inputs':generated,'blocked_acceptance':['ordinary installed CLI without Node/pnpm/Cargo',
                        'public activation','native run selection','cross-platform installed distribution proof']}

    def verify(self):
        (self.output/'ci-receipt.json').write_text(json.dumps(self.ci))
        (self.output/'smoke/receipt.json').write_text(json.dumps(self.smoke))
        return oracle.verify(self.root,self.output,self.head,'linux')

    def test_complete_synthetic_receipt_schema(self):
        self.assertEqual(self.verify()['passed'],21)

    def test_malformed_scope_and_outcomes_reject(self):
        mutations=[lambda: self.ci.update(head='a'*40),lambda: self.ci.update(platform='win32'),
            lambda:self.ci.update(public_activation=True),lambda:self.ci.update(installed_distribution_acceptance=True),
            lambda:self.ci.update(status='failed'),lambda:self.ci.update(unreviewed=True),
            lambda:self.ci['commands'][4].update(exit=True),lambda:self.ci['commands'][4]['argv'].pop(),
            lambda:self.smoke['counts'].update(passed=True),lambda:self.smoke['counts'].update(ignored=1),
            lambda:self.smoke['records'].pop(),lambda:self.smoke['records'][-1].update(id='m1'),
            lambda:self.smoke['records'][0]['detail'].update(create_only=False),
            lambda:self.smoke['records'][0]['detail']['files']['native/private-smoke-m1.o'].update(bytes=True),
            lambda:self.ci['commands'][5].update(argv=['unrelated-proof']),
            lambda:self.ci.update(target='/unrelated'),
            lambda:self.smoke['records'][0]['detail'].update(source='unrelated.zry'),
            lambda:self.smoke.update(blocked_acceptance=[])]
        original=(copy.deepcopy(self.ci),copy.deepcopy(self.smoke))
        for change in mutations:
            with self.subTest(change=change):
                self.ci,self.smoke=copy.deepcopy(original)
                change()
                with self.assertRaises(AssertionError):self.verify()

    def test_substituted_binary_bundle_source_and_diagnostics_reject(self):
        self.verify()
        paths=[Path(self.ci['binaries']['feature']['path']),self.root/'examples/universal/add.zry',
               self.output/'smoke/m1-native-bundle/native/private-smoke-m1.o',self.output/'smoke/m2-bare-import.stdout']
        for path in paths:
            before=path.read_bytes(); path.write_bytes(b'substituted')
            with self.subTest(path=path),self.assertRaises((AssertionError,ValueError)):self.verify()
            path.write_bytes(before)

    def test_duplicate_and_nonfinite_json_reject(self):
        for raw in ('{"head":1,"head":2}','{"value":NaN}','{"value":Infinity}','{"value":1e999}'):
            with self.subTest(raw=raw),self.assertRaises((AssertionError,ValueError)):oracle.strict(raw)


if __name__=='__main__':
    unittest.main()
