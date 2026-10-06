"""Additional tool bindings challenged against the original admitted 393-file proof."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile

import admission
from admission_test import Controls, digest
import archive_contract


class CapabilityControls(Controls):
    def coherent_row(self, state):
        for field in ('tool_capabilities','after_tool_capabilities'):
            state[field]['node']['sha256'] = '0' * 64
        state['node']['sha256'] = '0' * 64

    def swapped_checkpoint(self, state):
        rows = state['commands'][1]['tool_capabilities_before']
        rows['cargo'],rows['rustc'] = rows['rustc'],rows['cargo']

    def run(self):
        self.receipt('omitted-current-tool-role',lambda s:s['tool_capabilities'].pop('rustup'))
        self.receipt('tool-length-boolean',lambda s:s['tool_capabilities']['node'].update(bytes=False))
        self.receipt('unrecorded-resolution-component',lambda s:s['tool_capabilities']['supplied_cargo']['resolution'].pop(0))
        self.receipt('unrecorded-canonical-ancestry',lambda s:s['tool_capabilities']['node']['canonical_components'].pop(0))
        self.receipt('coherent-legacy-capability-forgery-with-unmodified-original-selection',self.coherent_row)
        self.receipt('swapped-actual-command-capability-roles',self.swapped_checkpoint)
        self.receipt('omitted-command-capability-checkpoint',lambda s:s['commands'][4].pop('tool_capabilities_after'))
        self.receipt('unbound-original-explicit-selector',lambda s:s['tool_selection'].update(node_argument=s['tool_selection']['node_argument']+'-other'))
        self.receipt('unbound-original-Cargo-selector',lambda s:s['tool_selection'].update(cargo_argument=s['tool_selection']['cargo_argument']+'-other'))
        self.receipt('omitted-original-metadata-query',lambda s:s['metadata_queries'].pop())
        self.receipt('cargo-proxy-invocation-replaced-by-rustup-command',lambda s:s['metadata_queries'][1]['argv'].__setitem__(0,s['rustup']['path']))
        self.receipt('metadata-Node-prefix-collision',lambda s:s['metadata_queries'][0].update(stdout='v22.22.10\n'))
        self.receipt('metadata-after-proof-execution',lambda s:s['metadata_queries'][-1].update(completed_at=s['commands'][-1]['completed_at']))
        self.admit()
        if len(self.passed)!=13 or len(set(self.passed))!=13:
            raise AssertionError('complete additional thirteen tool capability admission controls')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('root','proof'):
        parser.add_argument('--'+name,required=True,type=Path)
    for name in ('head','platform'):
        parser.add_argument('--'+name,required=True)
    parser.add_argument('--run-id')
    parser.add_argument('--run-attempt')
    args=parser.parse_args()
    original=args.proof.absolute()
    admission.verify(args.root,original,args.head,args.platform,live=False,
                     run_id=args.run_id,run_attempt=args.run_attempt)
    files={}
    for path in original.rglob('*'):
        admission.ir.ordinary_path(path)
        if path.is_file():files[path.relative_to(original).as_posix()]=digest(path.read_bytes())
    expected=archive_contract.expected_files((original/'inventory.json').read_bytes(),args.platform)
    if set(files)!=expected or len(files)!=393:
        raise AssertionError('complete original admitted 393-file proof required')
    with tempfile.TemporaryDirectory(prefix='tool-capability-admission-controls-') as temporary:
        copied=Path(temporary)/'proof'
        shutil.copytree(original,copied)
        controls=CapabilityControls(args,copied)
        controls.run()
    for relative,expected_hash in files.items():
        if digest((original/relative).read_bytes())!=expected_hash:
            raise AssertionError('original proof changed: '+relative)
    print(json.dumps(dict(tool_capability_admission_controls_passed=13,controls=controls.passed,
                         baseline_cases=107,original_proof_files=393,original_proof_unchanged=True,
                         compiler_provider_executions=0,synthetic_tamper_controls=True,
                         execution_attestation='original hosted API/log/ZIP chain remains required'),sort_keys=True))


if __name__=='__main__':
    main()
