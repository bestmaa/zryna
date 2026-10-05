"""Independent bounded admission of cold BUILD and frozen retained Vec execution receipts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
from pathlib import PureWindowsPath, PurePosixPath
import re
import stat
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'native-cli-smoke'))
from manifest_admission import contracts, graph_digest, verify_bundle, _inventory, _file
from verify_ci_receipts import exact, strict, digest, git, failure_envelope

REGISTRY_SHA = '34cd29a5f146d77e7163b32d21e71e4f5a1fc5fd50f688d197de8bef9b38a508'
SOURCE_SHA = '69f54f0870b70e0818538141e4b89ef44e24424fbade6adef02b389ff86f7917'
IDS = ('score13', 'vec-fault-2-1', 'vec-fault-2-2')
STAGES = {'javascript':'JavaScript', 'webassembly':'WebAssembly', 'native':'Native'}
LAYOUTS = {'type_universe_sha256':'c2afc2b6b9796aee45700e688c51dac215f58ae64008126859cfb48ae54cb557',
           'linear32_sha256':'39d79f35e8ffa1bfeb36ee720c43ce65367bf5490f1680f589f1efc0b6848c34',
           'linux_x86_64_sha256':'447100f7eca11a5738b690990b0315f56f9ff7c32bbad8e0f379bc7a8478bcc1'}
RUNTIME = {'identifier':'zryna-ownership-runtime-v1', 'version':1,
           'sha256':'64936ba5f00d7a24342add5fed76fd7c7f4c72db115548e0e64e1f2a8999efa2'}


def files(directory):
    found = {}
    assert directory.is_dir() and not directory.is_symlink()
    for path in directory.rglob('*'):
        assert not path.is_symlink() and not getattr(path.lstat(), 'st_file_attributes', 0) & 0x400
        if path.is_file():
            found[path.relative_to(directory).as_posix()] = {'bytes':path.stat().st_size, 'sha256':digest(path)}
    assert found
    return found


def read(path):
    assert not path.is_symlink() and not getattr(path.lstat(), 'st_file_attributes', 0) & 0x400
    assert path.stat().st_size <= 32*1024*1024
    return strict(path.read_bytes())


def audit_tree(root):
    count = 0

    def walk(path):
        nonlocal count
        count += 1
        assert count <= 1024, 'proof inventory exceeds bounded scope'
        mode = path.lstat()
        assert not stat.S_ISLNK(mode.st_mode) and not getattr(mode,'st_file_attributes',0) & 0x400
        assert stat.S_ISDIR(mode.st_mode) or stat.S_ISREG(mode.st_mode)
        if stat.S_ISDIR(mode.st_mode):
            for child in path.iterdir():
                walk(child)
        else:
            assert mode.st_size <= 32*1024*1024
    walk(root)
    for parent in root.absolute().parents:
        assert not parent.is_symlink() and not getattr(parent.lstat(),'st_file_attributes',0) & 0x400


def diagnostic(rows, code, message=None):
    assert type(rows) is list and len(rows) == 1
    value = rows[0]
    assert set(value) == {'code','guidance','message','primary','severity'}
    exact(value['code'],code)
    exact(value['severity'],'error')
    exact(value['primary'],{'kind':'global'})
    assert all(type(value[key]) is str and value[key] for key in ('message','guidance'))
    if message is not None:
        exact(value['message'],message)


def cli_binding(receipt, prior, head, tree, platform, actual):
    for key,wanted in {'status':'passed','format':'zryna.private-cli-ci.v1','head':head,'tree':tree,
                       'platform':platform,'root':receipt['root'],'target':receipt['target'],'inputs':actual}.items():
        exact(prior[key],wanted)
    exact(receipt['tools'],prior['tools'])
    exact(receipt['feature_cli'],prior['binaries']['feature'])
    exact(receipt['default_cli'],prior['binaries']['default'])


def command_exits(commands):
    assert [r['label'] for r in commands] == ['feature-test-build','retained-fault-tests','cold-checkout',
                                             'cold-native-build','cold-build-collision','bootstrap-pnpm-version',
                                             'bootstrap-frozen-install','cold-bootstrap-comparison']
    assert all(set(r) == {'label','argv','cwd','exit'} for r in commands)
    assert all(type(r['exit']) is int for r in commands)
    assert [r['exit'] for r in commands[:4]] == [0,0,0,0] and commands[4]['exit'] != 0
    assert [r['exit'] for r in commands[5:]] == [0,0,0]


def cargo_package_identity(row, receipt, platform):
    path_type = PureWindowsPath if platform == 'win32' else PurePosixPath
    test_path = path_type(receipt['test_binary']['path'])
    assert test_path.is_relative_to(path_type(receipt['target'])/'debug/deps')
    source = path_type(receipt['root'])/'crates/zryna-driver'
    assert path_type(row['target']['src_path']) == source/'src/lib.rs'
    exact(row['target']['kind'],['lib'])
    exact(row['profile']['test'],True)
    assert 'native-provider-internal' in row['features']
    exact(row['package_id'],'path+'+source.as_uri()+'#0.2.3')


def results(case, target, registry):
    if case == 'score13':
        outcome, trace = {'kind':'returned', 'value':{'type':'i32', 'value':13}}, []
    else:
        frozen = [row for row in registry['faults'] if row['id'] == case]
        assert len(frozen) == 1 and case in IDS[1:]
        outcome, trace = frozen[0]['expected'], frozen[0]['trace']
    return [{'target':target, 'outcome':outcome, **({'trace':trace} if trace else {})}]


def run_bundle(directory, target, expected, source='main.zry'):
    manifest = read(directory/'zryna-manifest-v3.json')
    inventory = files(directory)
    assert set(manifest) == {'version','profile','protocol_version','command','entrypoint','graph_sha256',
                             'sources','edges','layouts','runtime_abi','stem','targets','artifacts',
                             'invocation','results','diagnostics'}
    rows = [{'id':0, 'path':source, 'sha256':SOURCE_SHA}]
    exact({key:manifest[key] for key in manifest if key != 'artifacts'}, {
        'version':3,'profile':'zryna-data-ownership-v1','protocol_version':4,'command':'run',
        'entrypoint':source,'graph_sha256':graph_digest(3,rows,[]),'sources':rows,'edges':[],
        'layouts':LAYOUTS,'runtime_abi':RUNTIME,'stem':'fixed-oracle','targets':[target],
        'invocation':{'export':'score','arguments':[]},'results':expected,'diagnostics':[]})
    assert type(manifest['artifacts']) is list and len(manifest['artifacts']) == 1
    artifact = manifest['artifacts'][0]
    assert set(artifact) == {'target','kind','filename','bytes','sha256','metadata'}
    extension, kind = {'javascript':('mjs','ecmascript-module'),
                       'webassembly':('wasm','core-webassembly-module'),
                       'native':('elf','linux-x86-64-invocation-executable')}[target]
    name = f'{target}/fixed-oracle.{extension}'
    assert set(inventory) == {'zryna-manifest-v3.json',name}
    exact({k:artifact[k] for k in artifact if k != 'metadata'},
          {'target':target,'kind':kind,'filename':'fixed-oracle.'+extension,**inventory[name]})
    assert 0 < artifact['bytes'] <= 32*1024*1024
    data = _file(directory,name).read_bytes()
    if target == 'javascript':
        assert 'export ' in data.decode('utf-8')
        exact(artifact['metadata'],{'profile':'ecmascript-module','scalar_abi':'v1'})
    elif target == 'webassembly':
        assert data[:8] == b'\0asm\x01\0\0\0'
        exact(artifact['metadata'],{'profile':'core-webassembly','memory':'linear32-v1',
                                    'imports':False,'scalar_abi':'v1'})
    else:
        assert data[:7] == b'\x7fELF\x02\x01\x01' and int.from_bytes(data[16:18],'little') == 2
        metadata = artifact['metadata']
        assert set(metadata) == {'profile','triple','format','scalar_abi','program_object_sha256',
                                  'runtime_object_sha256','harness_sha256'}
        for key in ('program_object_sha256','runtime_object_sha256','harness_sha256'):
            assert type(metadata[key]) is str and re.fullmatch('[0-9a-f]{64}',metadata[key])
        exact({k:metadata[k] for k in ('profile','triple','format','scalar_abi')},
              {'profile':'linux-x86-64-elf','triple':'x86_64-unknown-linux-gnu',
               'format':'elf-executable','scalar_abi':'v1'})
    return inventory


def mutation_control(value, platform):
    assert set(value) == {'control','checkpoint','platform','attempt','mutated','execution_checkpoints',
                          'publication_checkpoints','failure_kind','diagnostics','results',
                          'output_entries','public_activation'}
    checkpoint = value['checkpoint']
    assert checkpoint in ('Execution','Manifest','Commit','SourceDirectory')
    exact(value['control'],'source-mutation')
    exact(value['platform'],'windows' if platform == 'win32' else 'linux')
    exact(value['output_entries'],0)
    exact(value['public_activation'],False)
    attempt = value['attempt']
    if platform == 'win32':
        assert set(attempt) == {'action','io_error','before_sha256','after_sha256','parent_replaced'}
        exact(attempt['before_sha256'],SOURCE_SHA)
        exact(attempt['parent_replaced'],bool(value['mutated'] and checkpoint == 'SourceDirectory'))
    else:
        exact(attempt,{'action':'changed','io_error':None})
    if value['mutated'] is True:
        exact(attempt['action'],'changed')
        exact(attempt['io_error'],None)
        exact(value['failure_kind'],'Source')
        diagnostic(value['diagnostics'],'ZRYNA-D3004')
        exact(value['results'],[])
        if platform == 'win32':
            if checkpoint == 'SourceDirectory':
                exact(attempt['after_sha256'],SOURCE_SHA)
            else:
                exact(attempt['after_sha256'],hashlib.sha256(b'export function score(): i32 { return 99; }\n').hexdigest())
        execution, publication = {'Execution':(2,['JavaScript']),'SourceDirectory':(2,['JavaScript']),
                                  'Manifest':(3,['JavaScript','Manifest']),
                                  'Commit':(3,['JavaScript','Manifest','Commit'])}[checkpoint]
    else:
        assert platform == 'win32'
        exact(value['mutated'],False)
        exact(attempt['action'],'prevented')
        exact(attempt['io_error'],{'kind':'Uncategorized','raw_os_error':32})
        exact(attempt['after_sha256'],SOURCE_SHA)
        exact(value['failure_kind'],None)
        exact(value['diagnostics'],[])
        exact(value['results'],[{'target':'javascript','outcome':{'kind':'returned','value':{'type':'i32','value':13}}}])
        execution, publication = 3,['JavaScript','Manifest','Commit']
    exact(value['execution_checkpoints'],execution)
    exact(value['publication_checkpoints'],publication)


def verify(root, output, head, platform, archived=False, cli_proof=None, run_id=None, run_attempt=None):
    assert __debug__ and platform in ('linux','win32') and re.fullmatch('[0-9a-f]{40}',head)
    audit_tree(output)
    receipt = read(output/'receipt.json')
    assert set(receipt) == {'format','head','tree','platform','root','cold_root','target','inputs','tools',
                            'feature_cli','default_cli','python','commands','status','run_id','run_attempt',
                            'public_activation','installed_distribution_acceptance','test_binary',
                            'cold_before_native','runtime_path','cold_cleanup_entries','bootstrap_pnpm',
                            'cli_proof','cli_receipt_sha256'}
    exact(receipt['format'],'zryna.native-cold-qualification.v1')
    exact(receipt['status'],'passed')
    exact(receipt['head'],head)
    exact(receipt['platform'],platform)
    assert head == git(root,'rev-parse','HEAD') and receipt['tree'] == git(root,'rev-parse','HEAD^{tree}')
    actual = {name:digest(root/name) for name in git(root,'ls-files').splitlines()}
    exact(receipt['inputs'],actual)
    prior = read((cli_proof if archived else Path(receipt['cli_proof']))/'ci-receipt.json')
    exact(digest((cli_proof if archived else Path(receipt['cli_proof']))/'ci-receipt.json'),receipt['cli_receipt_sha256'])
    cli_binding(receipt,prior,head,receipt['tree'],platform,actual)
    exact(receipt['public_activation'],False)
    exact(receipt['installed_distribution_acceptance'],False)
    exact(receipt['cold_before_native'],True)
    exact(receipt['cold_cleanup_entries'],0)
    assert actual['tests/m3-conformance-v1.json'] == REGISTRY_SHA
    assert actual['tests/m3-fixtures/conformance/vec.zry'] == SOURCE_SHA
    assert receipt['tools']['node']['version'] == 'v22.22.1'
    assert receipt['tools']['cargo']['version'].startswith('cargo 1.97.1 ')
    assert receipt['tools']['rustc']['version'].startswith('rustc 1.97.1 ')
    assert set(receipt['tools']) == {'node','cargo','rustc'}
    exact(receipt['bootstrap_pnpm']['version'],'11.18.0')
    assert set(receipt['bootstrap_pnpm']) == {'path','sha256','version','manifest_sha256'}
    for row in [*receipt['tools'].values(),receipt['feature_cli'],receipt['default_cli'],receipt['test_binary']]:
        assert set(row) == ({'path','sha256','version'} if 'version' in row else {'path','sha256'})
        assert type(row['path']) is str and row['path']
        assert type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}',row['sha256'])
    for key in ('run_id','run_attempt'):
        assert receipt[key] is None or re.fullmatch('[1-9][0-9]*',receipt[key])
    exact(receipt['run_id'],run_id if archived else os.getenv('GITHUB_RUN_ID'))
    exact(receipt['run_attempt'],run_attempt if archived else os.getenv('GITHUB_RUN_ATTEMPT'))
    if not archived:
        assert sys.platform == platform and Path(receipt['root']).resolve() == root.resolve()
        assert not git(root,'status','--porcelain')
        for row in [*receipt['tools'].values(),receipt['feature_cli'],receipt['default_cli'],receipt['test_binary']]:
            assert digest(Path(row['path'])) == row['sha256']
        assert digest(Path(receipt['bootstrap_pnpm']['path'])) == receipt['bootstrap_pnpm']['sha256']
        pnpm_manifest = Path(receipt['bootstrap_pnpm']['path']).parent.parent/'package.json'
        assert digest(pnpm_manifest) == receipt['bootstrap_pnpm']['manifest_sha256']
        package = read(pnpm_manifest)
        exact(package['name'],'pnpm')
        exact(package['version'],'11.18.0')
        cold = Path(receipt['cold_root'])
        assert git(cold,'rev-parse','HEAD') == head and not git(cold,'status','--porcelain')
        assert {name:digest(cold/name) for name in actual} == actual
        assert not list((cold/'.zryna/out').iterdir())
        assert Path(receipt['runtime_path']).is_dir() and not list(Path(receipt['runtime_path']).iterdir())
    commands = receipt['commands']
    command_exits(commands)
    exact(commands[0]['argv'],[receipt['tools']['cargo']['path'],'test','--locked','--offline','-p',
                             'zryna-driver','--features','native-provider-internal','--lib','--no-run','--message-format=json'])
    exact(commands[1]['argv'],[receipt['test_binary']['path'],'native_provider_faults','--skip','native_provider_faults::corpus::','--nocapture','--test-threads=1'])
    build_records = [strict(line) for line in (output/'feature-test-build.stdout').read_bytes().splitlines()]
    built = [row['executable'] for row in build_records if row.get('reason') == 'compiler-artifact'
             and row.get('executable') and row['target']['name'] == 'zryna_driver' and row['profile']['test']]
    exact(built,[receipt['test_binary']['path']])
    built_row = next(row for row in build_records if row.get('executable') == built[0])
    cargo_package_identity(built_row,receipt,platform)
    exact(commands[2]['argv'],['git','-C',receipt['root'],'worktree','add','--detach',receipt['cold_root'],head])
    for row in commands[:3]:
        exact(row['cwd'],receipt['root'])
    for row in [*commands[3:5],*commands[6:]]:
        exact(row['cwd'],receipt['cold_root'])
    exact(commands[5]['cwd'],receipt['root'])
    cold_command = [receipt['feature_cli']['path'],'build','tests/m3-fixtures/conformance/vec.zry',
                    '--root',receipt['cold_root'],'--name','private-smoke-m3-vec','--target','all',
                    '--json','--profile','data-ownership-v1','--native-frontend']
    exact(commands[3]['argv'],cold_command)
    exact(commands[4]['argv'],cold_command)
    pnpm_command = [receipt['tools']['node']['path'],receipt['bootstrap_pnpm']['path']]
    exact(commands[5]['argv'],[*pnpm_command,'--version'])
    exact(commands[6]['argv'],[*pnpm_command,'install','--frozen-lockfile'])
    exact((output/'bootstrap-pnpm-version.stdout').read_text().strip(),'11.18.0')
    exact(commands[7]['argv'],[receipt['default_cli']['path'],*cold_command[1:-1],'--node',receipt['tools']['node']['path']])
    assert len(re.findall(r'test result: ok\. 3 passed; 0 failed; 0 ignored;',
                         (output/'retained-fault-tests.stdout').read_text())) == 1
    registry = read(root/'tests/m3-conformance-v1.json')
    targets = ('javascript','webassembly','native') if platform == 'linux' else ('javascript','webassembly')
    expected_names = {f'{t}-{case}-{provider}' for t in targets for case in IDS for provider in ('native-retained','bootstrap')}
    expected_names |= {t+'-cold-collision' for t in targets}
    expected_names |= {'mutation-'+name for name in ('Execution','Manifest','Commit','SourceDirectory')}
    observations = output/'observations'
    assert {p.name for p in observations.iterdir()} == expected_names
    for target in targets:
        for case in IDS:
            inventories = []
            for provider in ('native-retained','bootstrap'):
                path = observations/f'{target}-{case}-{provider}'
                expected = results(case,target,registry)
                value = read(path/'observation.json')
                exact(value,{'provider':provider,'case':case,'platform':'windows' if platform == 'win32' else 'linux',
                             'registry_sha256':REGISTRY_SHA,'source_sha256':SOURCE_SHA,'results':expected,
                             'execution_checkpoints':3 if provider == 'native-retained' else 0,
                             'publication_checkpoints':[STAGES[target],'Manifest','Commit'] if provider == 'native-retained' else [],
                             'runtime':'canonical driver runner/decoder; explicit Node target runtime',
                             'full_m3_fault_corpus':False,'installed_no_node_acceptance':False,'public_activation':False})
                assert digest(path/'main.zry') == SOURCE_SHA
                inventories.append(run_bundle(path/'bundle',target,expected))
            exact(inventories[0],inventories[1])
        path = observations/(target+'-cold-collision')
        value = read(path/'control.json')
        assert set(value) == {'control','target','platform','native_first','execution_checkpoints','publication_checkpoints',
                              'failure_kind','diagnostics','output_entries','results','public_activation'}
        for key,wanted in {'control':'cold-collision','target':target,'platform':'windows' if platform == 'win32' else 'linux',
                           'native_first':True,'execution_checkpoints':3,'publication_checkpoints':[STAGES[target],'Manifest','Commit'],
                           'failure_kind':'Preparation','output_entries':1,'results':results('score13',target,registry),
                           'public_activation':False}.items():
            exact(value[key],wanted)
        diagnostic(value['diagnostics'],'ZRYNA-C1009','create-only output bundle already exists')
        assert digest(path/'main.zry') == SOURCE_SHA
        run_bundle(path/'bundle',target,value['results'])
    mutations = []
    for checkpoint in ('Execution','Manifest','Commit','SourceDirectory'):
        path = observations/('mutation-'+checkpoint)
        value = read(path/'control.json')
        exact(value['checkpoint'],checkpoint)
        mutation_control(value,platform)
        if not value['mutated']:
            run_bundle(path/'bundle','javascript',value['results'],
                       'sources/main.zry' if checkpoint == 'SourceDirectory' else 'main.zry')
        else:
            assert not (path/'bundle').exists()
        mutations.append({'checkpoint':checkpoint,'action':value['attempt']['action']})
    fixed,_ = contracts(root,output,actual)
    native = read(output/'cold-native-build.stdout')
    assert (output/'cold-native-build.stdout').read_bytes() == (output/'cold-bootstrap-comparison.stdout').read_bytes()
    native_files = _inventory(output/'cold-native-bundle')
    exact(native_files,_inventory(output/'cold-bootstrap-bundle'))
    verify_bundle(output/'cold-native-bundle','m3-vec',fixed['m3-vec'],native_files,native)
    verify_bundle(output/'cold-bootstrap-bundle','m3-vec',fixed['m3-vec'],native_files,native)
    collision = read(output/'cold-build-collision.stdout')
    failure_envelope(collision,'ZRYNA-C1009')
    diagnostic(collision['diagnostics'],'ZRYNA-C1009','create-only output bundle already exists')
    return {'head':head,'tree':receipt['tree'],'platform':platform,'observations':len(targets)*6,
            'cold_collisions':len(targets),'mutation_controls':mutations,'cold_build_objects':3,
            'passed':len(targets)*7+6,'failed':0,'ignored':0,'archived':archived,
            'run_id':receipt['run_id'],'run_attempt':receipt['run_attempt'],
            'public_activation':False,'full_m3_fault_corpus':False,'installed_distribution_acceptance':False}


def main():
    if not __debug__:
        raise RuntimeError('optimized receipt admission forbidden')
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('root','output'):
        parser.add_argument('--'+flag,type=Path,required=True)
    parser.add_argument('--head',required=True)
    parser.add_argument('--platform',choices=('linux','win32'),required=True)
    args = parser.parse_args()
    result = verify(args.root,args.output,args.head,args.platform)
    with (args.output/'admission.json').open('x') as stream:
        json.dump(result,stream,indent=2)
        stream.write('\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
