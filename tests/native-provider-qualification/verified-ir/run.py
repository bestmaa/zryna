#!/usr/bin/env python3
"""Run the private complete IR proof at an exact clean local revision."""
import argparse
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
RESERVATION = 'tests/native-provider-qualification/verified-ir/'
PROVIDER = 'zryna-verified-ir-proof'

def module(name, path):
    spec = importlib.util.spec_from_file_location(name,path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

sys.path.insert(0,str(HERE))
import verify
import derive_inventory

LEGACY = module('legacy_ir_support', ROOT / 'scripts/run-native-provider-corpus.py')
SUPPORT = LEGACY.SUPPORT
LEGACY.PROVIDER = PROVIDER

def authority():
    changed = SUPPORT.git('diff','--name-only',H4,'HEAD').splitlines()
    verify.require(all(path.startswith(RESERVATION) for path in changed),'reserved proof-only revision; no changed H4 production or shared proof files')
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

def compiler_inputs():
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
    return dict(tracked=result,typescript_runtime=dict(path=str(resolved),files=dependency))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence-dir',required=True,type=Path)
    parser.add_argument('--baseline-dir',required=True,type=Path)
    parser.add_argument('--cargo',default='cargo')
    parser.add_argument('--node',required=True,type=Path)
    parser.add_argument('--target-dir',required=True,type=Path)
    args = parser.parse_args()
    evidence = args.evidence_dir.resolve()
    target = args.target_dir.resolve()
    verify.require(not evidence.is_relative_to(ROOT) and not target.is_relative_to(ROOT),'external owned evidence and target')
    evidence.mkdir(parents=True,exist_ok=False)
    state = dict(schema_version=1,repository_sha=SUPPORT.git('rev-parse','HEAD'),input_revision=H4,
                 status='failed',public_activation=False,passed=[],failed=[],ignored=[],
                 unrun=['Windows validation','shared workflow registration','public activation','installed no-Node acceptance'])
    try:
        verify.require(not SUPPORT.git('status','--porcelain'),'clean exact revision')
        subprocess.run(['git','merge-base','--is-ancestor',H4,'HEAD'],cwd=ROOT,check=True)
        before = authority()
        state['inputs'] = before
        state['compiler_inputs'] = compiler_inputs()
        inventory_bytes = (HERE/'inventory.json').read_bytes()
        inventory = verify.strict_json(inventory_bytes)
        baseline_bytes = (args.baseline_dir/'corpus.json').read_bytes()
        baseline_state = verify.strict_json((args.baseline_dir/'receipt.json').read_bytes())
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
        node = args.node.resolve(strict=True)
        node_version = subprocess.check_output([str(node),'--version'],text=True).strip()
        verify.require(node_version == 'v22.22.1','pinned Node22.22.1')
        state['node'] = dict(path=str(node),version=node_version,sha256=SUPPORT.digest(node))
        cargo = shutil.which(args.cargo)
        verify.require(cargo is not None,'pinned Cargo available')
        env = dict(os.environ)
        for name in ('RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER'):
            env.pop(name,None)
        env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_TARGET_DIR=str(target),CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2')
        rustc = Path(cargo).parent / ('rustc.exe' if os.name == 'nt' else 'rustc')
        env['RUSTC'] = str(rustc)
        for tool,prefix in ((cargo,'cargo 1.97.1 '),(str(rustc),'rustc 1.97.1 ')):
            version = subprocess.check_output([tool,'--version'],env=env,text=True).strip()
            verify.require(version.startswith(prefix),'pinned compiler tool version')
            state[Path(tool).stem+'_version'] = version
        rustup = Path(cargo).parent / ('rustup.exe' if os.name == 'nt' else 'rustup')
        state['compiler_tools'] = {}
        for name in ('cargo','rustc'):
            resolved = Path(subprocess.check_output([str(rustup),'which','--toolchain','1.97.1',name],env=env,text=True).strip())
            state['compiler_tools'][name] = dict(path=str(resolved),sha256=SUPPORT.digest(resolved))
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
        SUPPORT.run([cargo,'generate-lockfile','--offline'],package,evidence/'lock.log',env=env)
        SUPPORT.verify_registry_lock(original,tomllib.loads((package/'Cargo.lock').read_text()))
        state['harness_lock_sha256'] = SUPPORT.digest(package/'Cargo.lock')
        SUPPORT.run([cargo,'build','--locked','--offline','-j','2'],package,evidence/'build.log',env=env)
        state['passed'].append('locked-offline-build')
        SUPPORT.run([cargo,'fmt','--','--check'],package,evidence/'format.log',env=env)
        SUPPORT.run([cargo,'clippy','--locked','--offline','-j','2','--','-D','warnings'],package,evidence/'clippy.log',env=env)
        state['passed'].extend(('format','strict-clippy'))
        binary = target/'debug'/(PROVIDER+('.exe' if os.name == 'nt' else ''))
        state['executable_sha256'] = SUPPORT.digest(binary)
        observations = evidence/'observations'
        exit_code = LEGACY.run_corpus([str(binary),str(ROOT),str(node),str(HERE/'inventory.json'),str(observations)],evidence,evidence,env)
        state['collector_exit'] = exit_code
        verify.require(exit_code == 0,'complete collector; see retained corpus stderr')
        receipt = verify.strict_json(verify.read_bounded(evidence/'corpus.json',verify.MAX_RECEIPT))
        admission = verify.verify(inventory_bytes,receipt,observations)
        (evidence/'admission.json').write_text(json.dumps(admission,indent=2)+'\n')
        state['admission'] = admission
        state['passed'].append('107-case-independent-complete-IR-admission')
        SUPPORT.run([sys.executable,'-B',str(HERE/'verify_test.py'),'--inventory',str(HERE/'inventory.json'),'--receipt',str(evidence/'corpus.json'),'--output',str(observations)],ROOT,evidence/'hostile-controls.log',env=env)
        state['passed'].append('independent-hostile-receipt-controls')
        state['after_sha'] = SUPPORT.git('rev-parse','HEAD')
        state['after_clean'] = not bool(SUPPORT.git('status','--porcelain'))
        state['after_inputs'] = authority()
        state['after_executable_sha256'] = SUPPORT.digest(binary)
        state['after_node_sha256'] = SUPPORT.digest(node)
        verify.require(state['after_sha'] == state['repository_sha'] and state['after_clean'] and state['after_inputs'] == before and state['after_executable_sha256'] == state['executable_sha256'] and state['after_node_sha256'] == state['node']['sha256'],'exact revision/source/tool binding after proof')
        state['after_compiler_inputs'] = compiler_inputs()
        verify.require(state['after_compiler_inputs'] == state['compiler_inputs'],'complete actual compiler and worker inputs after proof')
        state['after_compiler_tools'] = {name:dict(path=tool['path'],sha256=SUPPORT.digest(Path(tool['path']))) for name,tool in state['compiler_tools'].items()}
        verify.require(state['after_compiler_tools'] == state['compiler_tools'],'resolved pinned compiler bytes after proof')
        state['exact_revision_after_proof'] = True
        state['status'] = 'passed'
    except Exception as error:
        state['failed'].append(str(error))
        raise
    finally:
        (evidence/'receipt.json').write_text(json.dumps(state,indent=2)+'\n')

if __name__ == '__main__':
    main()
