"""Bind complete IR byte admission to the exact source and retained runner captures."""
import argparse
import ast
import datetime
import hashlib
import importlib.util
import json
import ntpath
import posixpath
from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).resolve().parent))
import verify as ir

HERE = Path(__file__).resolve().parent
LABELS = ('lock','build','format','clippy','collector','hostile-controls')
PASSED = ['locked-offline-build','format','strict-clippy',
          '107-case-independent-complete-IR-admission','independent-hostile-receipt-controls']

def load(path):
    return ir.strict_json(ir.read_bounded(path,ir.MAX_RECEIPT))

def git(root,*args):
    return subprocess.check_output(['git','-C',str(root),*args],text=True).strip()

def stamp(text):
    ir.require(type(text) is str,'typed command timestamp')
    value = datetime.datetime.fromisoformat(text)
    ir.require(value.tzinfo is not None,'timezone-qualified command timestamp')
    return value

def runner_module():
    spec = importlib.util.spec_from_file_location('verified_ir_source_runner',HERE/'run.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def verify(root,proof,head,platform,live=False,run_id=None,run_attempt=None):
    ir.require(__debug__ and platform in ('linux','win32'),'supported explicit host')
    ir.ordinary_path(Path(root).absolute())
    ir.ordinary_path(Path(proof).absolute())
    root = Path(root).resolve(strict=True)
    proof = Path(proof).resolve(strict=True)
    ir.require(root == HERE.parents[2],'admission code belongs to the explicit source checkout')
    ir.require(git(root,'rev-parse','HEAD') == head and not git(root,'status','--porcelain'),
               'clean exact current admission source')
    state = load(proof/'receipt.json')
    pathmod = ntpath if platform == 'win32' else posixpath
    original_root,original_evidence = state['repository_root'],state['evidence_root']
    for path in (original_root,original_evidence,state['target_root']):
        ir.require(type(path) is str and pathmod.isabs(path) and pathmod.normpath(path) == path
                   and '\x00' not in path,'canonical original absolute execution path')
    original_here = pathmod.join(original_root,'tests','native-provider-qualification','verified-ir')
    original_observations = pathmod.join(original_evidence,'observations')
    tree = git(root,'rev-parse','HEAD^{tree}')
    for key,expected in dict(repository_sha=head,head_tree=tree,platform=platform,status='passed',
                             public_activation=False,input_revision='0a5f86b77a84c37e19e0dd59388c8a782d385131',
                             integration_base='ec0cab5b4669dedd3e41fddda45449db34f73ce0',
                             exact_revision_after_proof=True,after_sha=head,after_clean=True,
                             passed=PASSED,failed=[],ignored=[]).items():
        ir.require(ir.exact(state[key],expected),'exact runner claim: '+key)
    if run_id is not None:
        ir.require(str(state['run_id']) == str(run_id),'exact hosted run ID')
    if run_attempt is not None:
        ir.require(str(state['run_attempt']) == str(run_attempt),'exact hosted attempt')
    source_runner = runner_module()
    ir.require(ir.exact(state['inputs'],source_runner.authority()),'pinned H4 and exact current proof authority')
    ir.require(ir.exact(state['after_inputs'],state['inputs']),'unchanged source authority after collection')
    actual_tracked = {}
    for raw in subprocess.check_output(['git','-C',str(root),'ls-tree','-rz','HEAD']).split(b'\0'):
        if not raw:
            continue
        metadata,path = raw.split(b'\t',1)
        mode,kind,blob = metadata.decode().split()
        ir.require(kind == 'blob' and mode in ('100644','100755'),'regular committed source')
        name = path.decode()
        data = ir.read_bounded(root/name,32*1024*1024)
        actual_blob = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        ir.require(actual_blob == blob,'working source equals committed Git blob')
        actual_tracked[name] = dict(bytes=len(data),sha256=ir.digest(data),git_blob=blob)
    ir.require(ir.exact(state['compiler_inputs']['tracked'],actual_tracked),'every actual compiler source input')
    ir.require(ir.exact(state['compiler_inputs'],state['after_compiler_inputs']),'unchanged complete compiler/worker inputs')
    for key,name,version in (('typescript_runtime','@typescript/typescript6','6.0.2'),
                             ('typescript_implementation','typescript','6.0.3')):
        value = state['compiler_inputs'][key]
        ir.require((value['name'],value['version']) == (name,version) and value['files'],
                   'actual frozen worker/compiler dependency identity')
    inventory = ir.read_bounded(proof/'inventory.json',ir.MAX_RECEIPT)
    ir.require(inventory == (HERE/'inventory.json').read_bytes(),'original current committed inventory bytes')
    ir.require(ir.digest(inventory) == state['inventory_sha256'],'runner inventory digest')
    for historical,retained in (('corpus.json','baseline-corpus.json'),('receipt.json','baseline-runner.json')):
        ir.require(ir.read_bounded(proof/retained,ir.MAX_RECEIPT) == (HERE/'baseline-H4'/historical).read_bytes(),
                   'exact historical baseline snapshot: '+historical)
    ir.require(ir.digest((proof/'baseline-corpus.json').read_bytes()) == state['baseline_raw_sha256'],
               'historical corpus digest binding')
    ir.require(ir.digest((proof/'baseline-runner.json').read_bytes()) == state['baseline_runner_sha256'],
               'historical runner digest binding')
    baseline = load(proof/'baseline-runner.json')
    ir.require(baseline['status'] == 'passed' and baseline['repository_sha'] == state['input_revision']
               and baseline['exact_revision_after_corpus'] is True,'authentic retained H4 baseline identity')
    ir.require(ir.exact(baseline['corpus'],load(proof/'baseline-corpus.json')),'whole historical corpus runner binding')
    corpus = ir.read_bounded(proof/'corpus.json',ir.MAX_RECEIPT)
    result = ir.verify(inventory,ir.strict_json(corpus),proof/'observations')
    ir.require(ir.exact(result,state['admission']) and ir.exact(result,load(proof/'admission.json')),
               'independently rederived complete IR admission')
    executable = state['executable']
    ir.require(executable['path'] == pathmod.join(state['target_root'],'debug',
               'zryna-verified-ir-proof'+('.exe' if platform == 'win32' else '')),'exact original target collector path')
    ir.require(executable['retained_path'] == 'verified-ir-proof'+('.exe' if platform == 'win32' else ''),
               'exact retained collector binary name')
    binary = ir.read_bounded(proof/executable['retained_path'],128*1024*1024)
    ir.require(type(executable['bytes']) is int and len(binary) == executable['bytes']
               and ir.digest(binary) == executable['sha256'] == state['executable_sha256']
               == state['after_executable_sha256'],'original actual collector bytes')
    ir.require(binary.startswith(b'MZ' if platform == 'win32' else b'\x7fELF'),'actual host collector format')
    tools = state['compiler_tools']
    ir.require(set(tools) == {'cargo','rustc'} and ir.exact(tools,state['after_compiler_tools']),
               'exact resolved compiler tools')
    ir.require(state['cargo_version'].startswith('cargo 1.97.1 ') and state['rustc_version'].startswith('rustc 1.97.1 ')
               and state['node']['version'] == 'v22.22.1','pinned actual compiler and Node versions')
    commands = state['commands']
    ir.require(type(commands) is list and [c['label'] for c in commands] == list(LABELS),'six actual ordered commands')
    selected = state['selected_environment']
    ir.require(selected['RUSTUP_TOOLCHAIN'] == '1.97.1' and selected['RUSTC'] == tools['rustc']['path']
               and selected['CARGO_INCREMENTAL'] == '0' and selected['CARGO_BUILD_JOBS'] == '2',
               'exact build environment')
    ir.require(selected['CARGO_TARGET_DIR'] == state['target_root'],'original compiler target capability')
    ir.require(all(selected[name] is None for name in ('NODE_OPTIONS','NODE_PATH','RUSTC_WRAPPER',
               'RUSTC_WORKSPACE_WRAPPER','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')),'no unbound execution hooks')
    package = pathmod.join(original_evidence,'package')
    ir.require(commands[4]['cwd'] == original_evidence,'original retained collector cwd')
    ir.require(all(c['cwd'] == package for c in commands[:4]),'one generated locked package')
    expected = [
        [tools['cargo']['path'],'generate-lockfile','--offline'],
        [tools['cargo']['path'],'build','--locked','--offline','-j','2'],
        [tools['cargo']['path'],'fmt','--','--check'],
        [tools['cargo']['path'],'clippy','--locked','--offline','-j','2','--','-D','warnings'],
        [executable['path'],original_root,state['node']['path'],pathmod.join(original_here,'inventory.json'),original_observations],
    ]
    previous = None
    for ordinal,command in enumerate(commands):
        ir.require(type(command['exit']) is int and command['exit'] == 0,'actual successful command exit')
        ir.require(ir.exact(command['selected_environment'],selected),'command environment binding')
        start,stop = stamp(command['started_at']),stamp(command['completed_at'])
        ir.require(start <= stop and (previous is None or previous <= start),'ordered actual command capture times')
        previous = stop
        if ordinal < 5:
            ir.require(ir.exact(command['argv'],expected[ordinal]),'exact actual command argv')
        else:
            args = command['argv']
            ir.require(command['cwd'] == original_root and len(args) == 9 and args == [
                state['python'],'-B',pathmod.join(original_here,'verify_test.py'),'--inventory',pathmod.join(original_here,'inventory.json'),
                '--receipt',pathmod.join(original_evidence,'corpus.json'),'--output',original_observations],
                'actual independent hostile controls invocation')
        ir.require(set(command['captures']) == {'stdout','stderr'},'complete original command streams')
        for stream,row in command['captures'].items():
            ir.require(row['path'] == command['label']+'.'+stream,'canonical command capture path')
            data = ir.read_bounded(proof/row['path'],32*1024*1024)
            ir.require(type(row['bytes']) is int and len(data) == row['bytes'] and ir.digest(data) == row['sha256'],
                       'exact original command stream bytes')
    ir.require((proof/'collector.stdout').read_bytes() == corpus and type(state['collector_exit']) is int and state['collector_exit'] == 0,
               'actual collector machine stdout equals admitted receipt')
    lines = (proof/'hostile-controls.stdout').read_text(encoding='utf8').splitlines()
    controls = ir.strict_json(lines[-1])
    names = [re.fullmatch(r'PASS ([a-zA-Z0-9-]+) : .*',line).group(1) for line in lines[:-1]]
    control_ast = ast.parse((HERE/'verify_test.py').read_bytes())
    source_labels = [node.args[0].value for node in sorted(ast.walk(control_ast),key=lambda node:getattr(node,'lineno',0))
                     if isinstance(node,ast.Call) and isinstance(node.func,ast.Name)
                     and node.func.id in ('reject','receipt_mutation','obs') and node.args
                     and isinstance(node.args[0],ast.Constant) and isinstance(node.args[0].value,str)]
    ir.require(len(source_labels) == 49 and names == source_labels,'all exact independently pinned hostile controls')
    ir.require(len(names) == len(set(names)) == 49 and ir.exact(controls,dict(hostile_controls_passed=49,controls=names,runtime_evidence=False)),
               'all 49 actual independent receipt controls; no skip or duplicate')
    original_lock = __import__('tomllib').loads((root/'Cargo.lock').read_text())
    package_lock = __import__('tomllib').loads((proof/'package/Cargo.lock').read_text())
    source_runner.SUPPORT.verify_registry_lock(original_lock,package_lock)
    ir.require(ir.digest((proof/'package/Cargo.lock').read_bytes()) == state['harness_lock_sha256'],'original generated harness lock')
    for name in ('rustfmt.toml','rust-toolchain.toml'):
        ir.require((proof/'package'/name).read_bytes() == (root/name).read_bytes(),'original pinned package configuration')
    for name,source in (('main.rs',HERE/'main.rs'),('projection.rs',HERE/'projection.rs'),
                         ('providers.rs',root/'tests/native-provider-corpus/providers.rs')):
        ir.require((proof/'package/src'/name).read_bytes() == source.read_bytes(),'exact source of the built collector')
    metadata = __import__('tomllib').loads((root/'Cargo.toml').read_text())
    version = metadata['workspace']['package']['version']
    dependencies = {name:dict(path=pathmod.join(original_root,'crates',name)) for name in source_runner.LEGACY.PATH_DEPENDENCIES}
    dependencies['zryna-driver']['features'] = ['native-provider-internal']
    for name in ('serde','serde_json','sha2'):
        versions = [row['version'] for row in original_lock['package'] if row['name'] == name]
        ir.require(len(versions) == 1,'unique original registry version')
        dependencies[name] = dict(version='='+versions[0])
    dependencies['serde']['features'] = ['derive']
    expected_manifest = dict(package=dict(name='zryna-verified-ir-proof',version=version,edition='2024',**{'rust-version':'1.97'}),
                             workspace={},dependencies=dependencies,profile=dict(dev=dict(debug=0)),lints=dict(rust=dict(unsafe_code='forbid')))
    ir.require(ir.exact(__import__('tomllib').loads((proof/'package/Cargo.toml').read_text()),expected_manifest),
               'exact generated package and all original source dependency paths')
    if live:
        ir.require(platform == sys.platform,'actual current live host')
        ir.require(original_root == str(root) and original_evidence == str(proof),'exact actual live execution paths')
        for row in [*tools.values(),state['node'],state['rustup'],state['supplied_cargo']]:
            ir.require(ir.digest(ir.read_bounded(Path(row['path']),128*1024*1024)) == row['sha256'],
                       'actual original tool capability bytes')
        ir.require(ir.digest(ir.read_bounded(Path(executable['path']),128*1024*1024)) == executable['sha256'],
                   'actual built collector still unchanged')
        ir.require(ir.exact(source_runner.compiler_inputs(Path(state['node']['path'])),state['compiler_inputs']),
                   'actual complete frozen compiler and worker package trees')
    return dict(status='passed',head=head,tree=tree,platform=platform,live=live,
                counts=dict(passed=107,failed=0,ignored=0),hostile_controls=49,
                compiler_source_files=len(actual_tracked),commands=6,public_activation=False,
                graph_hashes=103,historical_H4_graph_observations=85,historical_H4_missing_graph_observations=18,
                installed_no_node_acceptance=False,execution_attestation='requires original hosted API/log/archive binding')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root','proof'):
        parser.add_argument('--'+name,type=Path,required=True)
    for name in ('head','platform'):
        parser.add_argument('--'+name,required=True)
    parser.add_argument('--live',action='store_true')
    parser.add_argument('--run-id')
    parser.add_argument('--run-attempt')
    args = parser.parse_args()
    print(json.dumps(verify(args.root,args.proof,args.head,args.platform,args.live,args.run_id,args.run_attempt),sort_keys=True))
