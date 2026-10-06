#!/usr/bin/env python3
"""Run the private complete IR proof at an exact clean local revision."""
import argparse
import datetime
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
H4 = '0a5f86b77a84c37e19e0dd59388c8a782d385131'
INTEGRATION_BASE = 'ec0cab5b4669dedd3e41fddda45449db34f73ce0'
RESERVATION = 'tests/native-provider-qualification/verified-ir/'
REGISTRATION = {
    '.github/workflows/native-provider-activation.yml',
    'tests/native-cli-smoke/workflow-job.json',
    'tests/native-provider-activation-workflow.test.mjs',
}
# Independently reviewed installation preparation; exact Git modes and blobs only.
# This grants no runtime authority and leaves the original H4/H5 checks intact.
INSTALLATION_PREPARATION = {
    'crates/zryna-driver/src/distribution/mod.rs': {'mode': '100644', 'blob': '083baf3b0c7ce8e96121ead001f88dc41441f45c'},
    'crates/zryna-driver/src/distribution/native_installation/descriptor.rs': {'mode': '100644', 'blob': 'fd1b65fda0133dd51c4724199ba2bbd4f0327515'},
    'crates/zryna-driver/src/distribution/native_installation/descriptor_tests.rs': {'mode': '100644', 'blob': 'f9a0fb1d144b42aa91ffecf2854bc0186eb77bd5'},
    'crates/zryna-driver/src/distribution/native_installation/identity.rs': {'mode': '100644', 'blob': '768f2c8d3242eadca0032f88115950905150a5d8'},
    'crates/zryna-driver/src/distribution/native_installation/mod.rs': {'mode': '100644', 'blob': '3227e6ea1d6ebf86a957028228524b5309c2ead0'},
    'crates/zryna-driver/src/distribution/native_installation/sources.rs': {'mode': '100644', 'blob': '3804280125d5dc76564f3dab14a2fbc369b04ffe'},
    'tests/native-provider-qualification/installed-native-capability/README.md': {'mode': '100644', 'blob': '71e04cabbee99b137c04d5eec6de27c6ea719e67'},
    'tests/native-provider-qualification/installed-native-capability/build.py': {'mode': '100644', 'blob': '4ff41ea8d0d0e47afd53f86f6500aa9023b7b073'},
    'tests/native-provider-qualification/installed-native-capability/cases.py': {'mode': '100644', 'blob': '8958f78e29a1b740d3d8a4d5aece2ea38c96157c'},
    'tests/native-provider-qualification/installed-native-capability/independent_cases.py': {'mode': '100644', 'blob': 'fc9c9f5fd9d1b37cabdf73f2225a1ceb9a0308a1'},
    'tests/native-provider-qualification/installed-native-capability/mutations.rs': {'mode': '100644', 'blob': '5708883c984d345d41bcc2dabef38423e01b870d'},
    'tests/native-provider-qualification/installed-native-capability/probe.rs': {'mode': '100644', 'blob': '3dd76530320a52bd73af9ffec77ea8e297909308'},
    'tests/native-provider-qualification/installed-native-capability/run.py': {'mode': '100644', 'blob': '3cd094ee2ef784f74c5f41c7ecf89388f02ef72c'},
    'tests/native-provider-qualification/installed-native-capability/source_registration_test.py': {'mode': '100644', 'blob': 'dbe805b69573ab79169d4f5fd4f2b3eaaaf01a33'},
}

def qualified_change(path):
    if path.startswith(RESERVATION) or path in REGISTRATION:
        return True
    expected = INSTALLATION_PREPARATION.get(path)
    if expected is None:
        return False
    actual = SUPPORT.git('ls-tree', 'HEAD', '--', path)
    fields = actual.split('\t')
    return len(fields) == 2 and fields[1] == path and fields[0].split() == [expected['mode'], 'blob', expected['blob']]

PROVIDER = 'zryna-verified-ir-proof'
ENVIRONMENT = (
    'RUSTUP_TOOLCHAIN','CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_BUILD_JOBS',
    'RUSTC','CARGO_HOME','RUSTUP_HOME','PATH','PATHEXT','NODE_OPTIONS','NODE_PATH',
    'RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS',
)

def module(name, path):
    spec = importlib.util.spec_from_file_location(name,path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

sys.path.insert(0,str(HERE))
import verify
import derive_inventory
import tool_capability

LEGACY = module('legacy_ir_support', ROOT / 'scripts/run-native-provider-corpus.py')
SUPPORT = LEGACY.SUPPORT
LEGACY.PROVIDER = PROVIDER

def authority():
    subprocess.run(['git','merge-base','--is-ancestor',INTEGRATION_BASE,'HEAD'],cwd=ROOT,check=True)
    changed = SUPPORT.git('diff','--name-only',INTEGRATION_BASE,'HEAD').splitlines()
    verify.require(all(qualified_change(path) for path in changed),
                   'exact qualified H5 base, reserved IR proof, explicit CI registration and reviewed installation blobs only')
    paths = set(LEGACY.inventory(ROOT))
    paths.update(('Cargo.toml','Cargo.lock','rust-toolchain.toml','rustfmt.toml',
                  'scripts/run-native-provider-corpus.py','scripts/run-native-provider-activation.py'))
    paths.update(p.relative_to(ROOT).as_posix() for p in (ROOT/'tests/native-provider-corpus').iterdir() if p.is_file())
    paths.update(('crates/zryna-ir/src/lib.rs','crates/zryna-ir/src/control_flow_v1.rs','crates/zryna-ir/src/data_ownership_v1.rs'))
    result = {}
    for path in sorted(paths):
        tracked = subprocess.check_output(['git','show',f'{H4}:{path}'],cwd=ROOT)
        current = (ROOT/path).read_bytes()
        verify.require(tracked == current,'unchanged pinned H4 authority: '+path)
        result[path] = verify.digest(current)
    result.update({p.relative_to(ROOT).as_posix():SUPPORT.digest(p) for p in HERE.iterdir() if p.is_file()})
    return result

def compiler_inputs(node):
    result = {}
    tree = subprocess.check_output(['git','ls-tree','-rz','HEAD'],cwd=ROOT)
    for row in tree.split(b'\0'):
        if not row:
            continue
        metadata,path_bytes = row.split(b'\t',1)
        mode,kind,blob = metadata.decode().split()
        path = path_bytes.decode()
        verify.require(mode in ('100644','100755') and kind == 'blob','regular tracked compiler input')
        data = verify.read_bounded(ROOT/path,32*1024*1024)
        actual_blob = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        verify.require(actual_blob == blob,'exact HEAD compiler input bytes: '+path)
        result[path] = dict(bytes=len(data),sha256=verify.digest(data),git_blob=blob)
    runtime = ROOT/'adapters/typescript-6/node_modules/@typescript/typescript6'
    resolved = runtime.resolve(strict=True)
    package = verify.strict_json((resolved/'package.json').read_bytes())
    verify.require(package['name'] == '@typescript/typescript6' and package['version'] == '6.0.2','pinned TypeScript runtime package')
    dependency = {}
    for path in sorted(resolved.rglob('*')):
        if path.is_file():
            data = verify.read_bounded(path,32*1024*1024)
            dependency[path.relative_to(resolved).as_posix()] = dict(bytes=len(data),sha256=verify.digest(data))
        else:
            verify.require(path.is_dir() and not path.is_symlink(),'regular dependency tree')
    actual = (resolved.parent/'old').resolve(strict=True)
    probe_env = dict(os.environ)
    for name in ('NODE_OPTIONS','NODE_PATH'):
        probe_env.pop(name,None)
    probe = 'const {createRequire}=require("node:module");process.stdout.write(createRequire(process.argv[1]).resolve("@typescript/old/package.json"));'
    node_resolved = Path(subprocess.check_output([str(node),'--eval',probe,str(resolved/'lib/typescript.js')],env=probe_env,text=True)).resolve(strict=True).parent
    verify.require(node_resolved == actual,'actual nearest Node compiler alias resolution; no shadow dependency')
    actual_package = verify.strict_json((actual/'package.json').read_bytes())
    verify.require(actual_package['name'] == 'typescript' and actual_package['version'] == '6.0.3',
                   'locked TypeScript implementation behind the @typescript/old alias')
    verify.require((resolved/'lib/typescript.js').read_bytes() == b'module.exports = require("@typescript/old");\n',
                   'exact pinned shim compiler delegation')
    implementation = {}
    for path in sorted(actual.rglob('*')):
        if path.is_file():
            data = verify.read_bounded(path,32*1024*1024)
            implementation[path.relative_to(actual).as_posix()] = dict(bytes=len(data),sha256=verify.digest(data))
        else:
            verify.require(path.is_dir() and not path.is_symlink(),'regular actual compiler dependency tree')
    return dict(tracked=result,
                typescript_runtime=dict(path=str(resolved),name='@typescript/typescript6',version='6.0.2',files=dependency),
                typescript_implementation=dict(path=str(actual),name='typescript',version='6.0.3',files=implementation))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence-dir',required=True,type=Path)
    parser.add_argument('--baseline-dir',required=True,type=Path)
    parser.add_argument('--cargo',default='cargo')
    parser.add_argument('--rustup',required=True,type=Path)
    parser.add_argument('--node',required=True,type=Path)
    parser.add_argument('--target-dir',required=True,type=Path)
    args = parser.parse_args()
    evidence = args.evidence_dir.resolve()
    target = args.target_dir.resolve()
    verify.require(not evidence.is_relative_to(ROOT) and not target.is_relative_to(ROOT),'external owned evidence and target')
    verify.require(not target.exists(),'fresh owned IR target without cache reuse')
    verify.require(not evidence.is_relative_to(target) and not target.is_relative_to(evidence),'separate evidence and compiler target')
    evidence.mkdir(parents=True,exist_ok=False)
    state = dict(schema_version=1,repository_sha=SUPPORT.git('rev-parse','HEAD'),input_revision=H4,
                 head_tree=SUPPORT.git('rev-parse','HEAD^{tree}'),platform=sys.platform,
                 repository_root=str(ROOT),evidence_root=str(evidence),target_root=str(target),python=sys.executable,
                 integration_base=INTEGRATION_BASE,run_id=os.getenv('GITHUB_RUN_ID'),
                 run_attempt=os.getenv('GITHUB_RUN_ATTEMPT'),commands=[],
                 status='failed',public_activation=False,passed=[],failed=[],ignored=[],
                 unrun=['public activation','installed no-Node acceptance','full legacy #414 obligation integration'])
    try:
        verify.require(not SUPPORT.git('status','--porcelain'),'clean exact revision')
        subprocess.run(['git','merge-base','--is-ancestor',H4,'HEAD'],cwd=ROOT,check=True)
        before = authority()
        state['inputs'] = before
        supplied_cargo = shutil.which(args.cargo)
        verify.require(supplied_cargo is not None,'pinned Cargo available')
        verify.require(Path.cwd() == ROOT,'runner invocation at its exact source checkout')
        argument_paths = dict(node=os.path.abspath(str(args.node)),rustup=os.path.abspath(str(args.rustup)))
        argument_capabilities = {name:tool_capability.snapshot(path) for name,path in argument_paths.items()}
        node = args.node.resolve(strict=True)
        rustup = args.rustup.resolve(strict=True)
        initial_rustc = Path(supplied_cargo).parent / ('rustc.exe' if os.name == 'nt' else 'rustc')
        selection_paths = dict(node=str(node),rustup=str(rustup),supplied_cargo=supplied_cargo,
                               initial_rustc=str(initial_rustc))
        state['tool_selection'] = dict(cargo_argument=args.cargo,node_argument=str(args.node),
                                       rustup_argument=str(args.rustup),paths=selection_paths,
                                       argument_capabilities=argument_capabilities,
                                       before={name:tool_capability.snapshot(path) for name,path in selection_paths.items()})
        state['metadata_queries'] = []
        env = dict(os.environ)
        for name in ('RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','NODE_OPTIONS','NODE_PATH'):
            env.pop(name,None)
        env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_TARGET_DIR=str(target),CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',
                   RUSTC=str(initial_rustc))

        def query(command):
            command = list(map(str,command))
            record = dict(argv=command,cwd=str(ROOT),
                          selected_environment={name:env.get(name) for name in ENVIRONMENT},
                          started_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                          before=tool_capability.snapshot(command[0]),exit=None)
            try:
                result = subprocess.run(command,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=False)
                record.update(exit=result.returncode,stdout=result.stdout.decode('utf8'),stderr=result.stderr.decode('utf8'))
            finally:
                record['completed_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
                record['after'] = tool_capability.snapshot(command[0])
                state['metadata_queries'].append(record)
            verify.require(record['exit'] == 0,'successful actual pinned tool metadata query')
            verify.require(tool_capability.same(record['before'],record['after']),'metadata tool capability changed')
            return record['stdout'].strip()

        node_version = query([str(node),'--version'])
        verify.require(node_version == 'v22.22.1','pinned Node22.22.1')
        state['node'] = dict(path=str(node),version=node_version,sha256=SUPPORT.digest(node))
        state['compiler_inputs'] = compiler_inputs(node)
        inventory_bytes = (HERE/'inventory.json').read_bytes()
        inventory = verify.strict_json(inventory_bytes)
        baseline_bytes = (args.baseline_dir/'corpus.json').read_bytes()
        baseline_state = verify.strict_json((args.baseline_dir/'receipt.json').read_bytes())
        for name in ('corpus.json','receipt.json'):
            verify.require((args.baseline_dir/name).read_bytes() == (HERE/'baseline-H4'/name).read_bytes(),
                           'exact committed historical baseline bytes: '+name)
        state['baseline_runner_sha256'] = SUPPORT.digest(args.baseline_dir/'receipt.json')
        verify.require(baseline_state['status'] == 'passed' and baseline_state['repository_sha'] == H4 and baseline_state['exact_revision_after_corpus'] is True,'retained exact clean unchanged H4 baseline')
        verify.require(verify.digest(baseline_bytes) == inventory['baseline_raw_sha256'],'raw H4 baseline receipt binding')
        baseline = verify.strict_json(baseline_bytes)
        rederived = derive_inventory.derive(ROOT,baseline,baseline_bytes)
        verify.require(verify.exact(rederived,inventory),'rederived complete source/context inventory against H4 Git')
        shutil.copy2(args.baseline_dir/'corpus.json',evidence/'baseline-corpus.json')
        shutil.copy2(args.baseline_dir/'receipt.json',evidence/'baseline-runner.json')
        shutil.copy2(HERE/'inventory.json',evidence/'inventory.json')
        state['inventory_sha256'] = verify.digest(inventory_bytes)
        state['baseline_raw_sha256'] = verify.digest(baseline_bytes)
        state['h4_graph_evidence'] = dict(observed=85,derived_only=18,inapplicable=4,open_obligations=inventory['open_obligations'])
        for tool,prefix in ((supplied_cargo,'cargo 1.97.1 '),(str(initial_rustc),'rustc 1.97.1 ')):
            version = query([tool,'--version'])
            verify.require(version.startswith(prefix),'pinned compiler tool version')
            state[Path(tool).stem+'_version'] = version
        state['rustup'] = dict(path=str(rustup),sha256=SUPPORT.digest(rustup))
        state['supplied_cargo'] = dict(path=supplied_cargo,sha256=SUPPORT.digest(Path(supplied_cargo)))
        state['compiler_tools'] = {}
        for name in ('cargo','rustc'):
            resolved = Path(query([str(rustup),'which','--toolchain','1.97.1',name]))
            state['compiler_tools'][name] = dict(path=str(resolved),sha256=SUPPORT.digest(resolved))
        cargo = state['compiler_tools']['cargo']['path']
        rustc = Path(state['compiler_tools']['rustc']['path'])
        env['RUSTC'] = str(rustc)
        for tool,prefix in ((cargo,'cargo 1.97.1 '),(str(rustc),'rustc 1.97.1 ')):
            verify.require(query([tool,'--version']).startswith(prefix),
                           'actually invoked resolved compiler version')
        state['tool_selection']['after'] = {name:tool_capability.snapshot(path) for name,path in selection_paths.items()}
        state['tool_selection']['after_argument_capabilities'] = {name:tool_capability.snapshot(path) for name,path in argument_paths.items()}
        verify.require(all(tool_capability.same(argument_capabilities[name],row)
                           for name,row in state['tool_selection']['after_argument_capabilities'].items()),'original tool argument capability changed')
        verify.require(all(tool_capability.same(state['tool_selection']['before'][name],row)
                           for name,row in state['tool_selection']['after'].items()),'selected tool capability changed')
        capability_paths = dict(node=str(node),rustup=str(rustup),supplied_cargo=supplied_cargo,
                                cargo=cargo,rustc=str(rustc))
        state['tool_capabilities'] = {name:tool_capability.snapshot(path) for name,path in capability_paths.items()}

        def capabilities():
            rows = {name:tool_capability.snapshot(path) for name,path in capability_paths.items()}
            verify.require(all(tool_capability.same(state['tool_capabilities'][name],row) for name,row in rows.items()),
                           'actual tool capability changed during proof')
            return rows
        selected_env = {name:env.get(name) for name in ENVIRONMENT}
        state['selected_environment'] = selected_env

        def execute(label, command, cwd):
            record = dict(label=label,argv=list(map(str,command)),cwd=str(cwd),
                          selected_environment=selected_env,
                          tool_capabilities_before=capabilities(),
                          started_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),exit=None)
            paths = {name:evidence/(label+'.'+name) for name in ('stdout','stderr')}
            try:
                with paths['stdout'].open('xb') as output, paths['stderr'].open('xb') as errors:
                    options = {'start_new_session':True} if os.name == 'posix' else {'creationflags':subprocess.CREATE_NEW_PROCESS_GROUP}
                    process = subprocess.Popen(record['argv'],cwd=cwd,env=env,stdout=output,stderr=errors,**options)
                    try:
                        record['exit'] = process.wait(timeout=1800)
                    except BaseException:
                        SUPPORT.stop_tree(process,errors)
                        raise
            finally:
                record['completed_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
                record['tool_capabilities_after'] = capabilities()
                record['captures'] = {name:dict(path=path.name,bytes=path.stat().st_size,sha256=SUPPORT.digest(path))
                                      for name,path in paths.items() if path.exists()}
                state['commands'].append(record)
            verify.require(record['exit'] == 0,'command failed: '+label+'; see retained captures')
            return record['exit']
        package = evidence/'package'
        src = package/'src'
        src.mkdir(parents=True)
        for source in HERE.glob('*.rs'):
            shutil.copy2(source,src/source.name)
        shutil.copy2(ROOT/'tests/native-provider-corpus/providers.rs',src/'providers.rs')
        for name in ('rustfmt.toml','rust-toolchain.toml','Cargo.lock'):
            shutil.copy2(ROOT/name,package/name)
        original = tomllib.loads((ROOT/'Cargo.lock').read_text())
        version = tomllib.loads((ROOT/'Cargo.toml').read_text())['workspace']['package']['version']
        LEGACY.manifest(package,version,original)
        execute('lock',[cargo,'generate-lockfile','--offline'],package)
        SUPPORT.verify_registry_lock(original,tomllib.loads((package/'Cargo.lock').read_text()))
        state['harness_lock_sha256'] = SUPPORT.digest(package/'Cargo.lock')
        execute('build',[cargo,'build','--locked','--offline','-j','2'],package)
        state['passed'].append('locked-offline-build')
        execute('format',[cargo,'fmt','--','--check'],package)
        execute('clippy',[cargo,'clippy','--locked','--offline','-j','2','--','-D','warnings'],package)
        state['passed'].extend(('format','strict-clippy'))
        binary = target/'debug'/(PROVIDER+('.exe' if os.name == 'nt' else ''))
        state['executable_sha256'] = SUPPORT.digest(binary)
        retained = evidence/('verified-ir-proof'+('.exe' if os.name == 'nt' else ''))
        shutil.copy2(binary,retained)
        state['executable'] = dict(path=str(binary),retained_path=retained.name,
                                  bytes=retained.stat().st_size,sha256=SUPPORT.digest(retained))
        observations = evidence/'observations'
        exit_code = execute('collector',[str(binary),str(ROOT),str(node),str(HERE/'inventory.json'),str(observations)],evidence)
        state['collector_exit'] = exit_code
        shutil.copy2(evidence/'collector.stdout',evidence/'corpus.json')
        verify.require(exit_code == 0,'complete collector; see retained corpus stderr')
        receipt = verify.strict_json(verify.read_bounded(evidence/'corpus.json',verify.MAX_RECEIPT))
        admission = verify.verify(inventory_bytes,receipt,observations)
        (evidence/'admission.json').write_text(json.dumps(admission,indent=2)+'\n')
        state['admission'] = admission
        state['passed'].append('107-case-independent-complete-IR-admission')
        execute('hostile-controls',[sys.executable,'-B',str(HERE/'verify_test.py'),'--inventory',str(HERE/'inventory.json'),'--receipt',str(evidence/'corpus.json'),'--output',str(observations)],ROOT)
        state['passed'].append('independent-hostile-receipt-controls')
        state['after_sha'] = SUPPORT.git('rev-parse','HEAD')
        state['after_clean'] = not bool(SUPPORT.git('status','--porcelain'))
        state['after_inputs'] = authority()
        state['after_executable_sha256'] = SUPPORT.digest(binary)
        state['after_node_sha256'] = SUPPORT.digest(node)
        verify.require(state['after_sha'] == state['repository_sha'] and state['after_clean'] and state['after_inputs'] == before and state['after_executable_sha256'] == state['executable_sha256'] and state['after_node_sha256'] == state['node']['sha256'],'exact revision/source/tool binding after proof')
        state['after_compiler_inputs'] = compiler_inputs(node)
        verify.require(state['after_compiler_inputs'] == state['compiler_inputs'],'complete actual compiler and worker inputs after proof')
        state['after_compiler_tools'] = {name:dict(path=tool['path'],sha256=SUPPORT.digest(Path(tool['path']))) for name,tool in state['compiler_tools'].items()}
        verify.require(state['after_compiler_tools'] == state['compiler_tools'],'resolved pinned compiler bytes after proof')
        verify.require(SUPPORT.digest(rustup) == state['rustup']['sha256'],'explicit rustup bytes after proof')
        verify.require(SUPPORT.digest(Path(state['supplied_cargo']['path'])) == state['supplied_cargo']['sha256'],
                       'supplied compiler capability bytes after proof')
        state['after_tool_capabilities'] = capabilities()
        state['exact_revision_after_proof'] = True
        state['status'] = 'passed'
    except Exception as error:
        state['failed'].append(str(error))
        raise
    finally:
        encoded = (json.dumps(state,separators=(',',':'))+'\n').encode()
        if len(encoded) > verify.MAX_RECEIPT:
            state['status'] = 'failed'
            state['failed'].append('original runner receipt exceeds unchanged 2MiB admission bound')
            encoded = (json.dumps(state,separators=(',',':'))+'\n').encode()
        with (evidence/'receipt.json').open('xb') as output:
            output.write(encoded)
        verify.require(len(encoded) <= verify.MAX_RECEIPT,'original runner receipt exceeds unchanged 2MiB admission bound')
    print(json.dumps(dict(format='zryna.verified-ir.stdout.v1',status=state['status'],
                         head=state['repository_sha'],tree=state['head_tree'],platform=sys.platform,
                         inventory_sha256=state['inventory_sha256'],corpus_sha256=SUPPORT.digest(evidence/'corpus.json'),
                         runner_receipt_sha256=SUPPORT.digest(evidence/'receipt.json'),
                         executable_sha256=state['executable_sha256'],admission=state['admission']),sort_keys=True))

if __name__ == '__main__':
    main()
