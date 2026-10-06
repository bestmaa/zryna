#!/usr/bin/env python3
"""Tamper controls on a real retained proof; never executes compiler/provider code."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import tomllib

sys.dont_write_bytecode = True
if not __debug__:
    raise RuntimeError('admission controls require assertions enabled')

import admission
import archive_contract


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def encode(value):
    return (json.dumps(value, indent=2) + '\n').encode()


class Controls:
    def __init__(self, args, proof):
        self.args = args
        self.proof = proof
        self.original = admission.load(proof / 'receipt.json')
        self.backups = {}
        self.passed = []

    def admit(self):
        return admission.verify(self.args.root, self.proof, self.args.head,
                                self.args.platform, live=False,
                                run_id=self.args.run_id,
                                run_attempt=self.args.run_attempt)

    def write(self, relative, raw):
        path = self.proof / relative
        if path not in self.backups:
            self.backups[path] = path.read_bytes()
        path.write_bytes(raw)

    def remove(self, relative):
        path = self.proof / relative
        self.backups[path] = path.read_bytes()
        path.unlink()

    def capture(self, state, label, stream, raw):
        relative = label + '.' + stream
        self.write(relative, raw)
        command = next(c for c in state['commands'] if c['label'] == label)
        command['captures'][stream] = dict(path=relative, bytes=len(raw), sha256=digest(raw))

    def reject(self, label, mutation):
        # Each attack begins with complete real 107-case positive admission.
        result = self.admit()
        if result['counts'] != {'passed': 107, 'failed': 0, 'ignored': 0}:
            raise AssertionError('real baseline incomplete before ' + label)
        state = copy.deepcopy(self.original)
        try:
            mutation(state)
            self.write('receipt.json', encode(state))
            try:
                self.admit()
            except (ValueError, KeyError, TypeError, IndexError, OSError) as error:
                self.passed.append(label)
                print('PASS', label, ':', str(error)[:220], flush=True)
            else:
                raise AssertionError('malformed real-proof evidence admitted: ' + label)
        finally:
            for path, raw in self.backups.items():
                path.write_bytes(raw)
            self.backups.clear()

    def receipt(self, label, mutation):
        self.reject(label, mutation)

    def tracked_compiler(self, state):
        name = 'crates/zryna-ir/src/lib.rs'
        for key in ('compiler_inputs', 'after_compiler_inputs'):
            row = state[key]['tracked'][name]
            row.update(bytes=row['bytes'] + 1, sha256='0' * 64, git_blob='0' * 40)

    def authority(self, state):
        for key in ('inputs', 'after_inputs'):
            state[key]['crates/zryna-ir/src/lib.rs'] = '0' * 64

    def compiler_identity(self, state):
        for key in ('compiler_inputs', 'after_compiler_inputs'):
            state[key]['typescript_implementation']['version'] = '6.0.4'

    def historical_runner(self, state):
        runner = admission.load(self.proof / 'baseline-runner.json')
        runner.update(status='passed', after_clean=False)
        raw = encode(runner)
        self.write('baseline-runner.json', raw)
        state['baseline_runner_sha256'] = digest(raw)

    def historical_census(self, state):
        corpus = admission.load(self.proof / 'baseline-corpus.json')
        corpus['blocked'].pop()
        runner = admission.load(self.proof / 'baseline-runner.json')
        runner['corpus'] = corpus
        corpus_raw, runner_raw = encode(corpus), encode(runner)
        self.write('baseline-corpus.json', corpus_raw)
        self.write('baseline-runner.json', runner_raw)
        state.update(baseline_raw_sha256=digest(corpus_raw),
                     baseline_runner_sha256=digest(runner_raw))

    def dependency(self, state):
        path = self.proof / 'package/Cargo.toml'
        text = path.read_text()
        manifest = tomllib.loads(text)
        original = json.dumps(manifest['dependencies']['zryna-driver']['path'])
        wrong = json.dumps(manifest['dependencies']['zryna-driver']['path'] + '-other')
        if text.count(original) != 1:
            raise AssertionError('one real driver source dependency')
        self.write('package/Cargo.toml', text.replace(original, wrong).encode())

    def feature(self, state):
        text = (self.proof / 'package/Cargo.toml').read_text()
        original = 'features = ["native-provider-internal"]'
        if text.count(original) != 1:
            raise AssertionError('one real private provider feature')
        self.write('package/Cargo.toml', text.replace(original, 'features = []').encode())

    def package_source(self, state):
        raw = (self.proof / 'package/src/main.rs').read_bytes()
        self.write('package/src/main.rs', raw + b'\n// changed after compilation\n')

    def registry_lock(self, state):
        text = (self.proof / 'package/Cargo.lock').read_text()
        checksum = next(p['checksum'] for p in tomllib.loads(text)['package'] if 'checksum' in p)
        raw = text.replace('checksum = "' + checksum + '"',
                           'checksum = "' + '0' * 64 + '"', 1).encode()
        self.write('package/Cargo.lock', raw)
        state['harness_lock_sha256'] = digest(raw)

    def controls(self, state, rename=False):
        lines = (self.proof / 'hostile-controls.stdout').read_text().splitlines()
        summary = admission.ir.strict_json(lines[-1])
        if rename:
            original = summary['controls'][0]
            summary['controls'][0] = 'forged-control-label'
            lines[0] = lines[0].replace('PASS ' + original + ' :',
                                        'PASS forged-control-label :', 1)
        else:
            summary['runtime_evidence'] = 0
        lines[-1] = json.dumps(summary)
        self.capture(state, 'hostile-controls', 'stdout', ('\n'.join(lines) + '\n').encode())

    def collector_stream(self, state):
        original = (self.proof / 'collector.stdout').read_bytes()
        # Valid JSON, coherently rehashed capture, but different from corpus.json.
        self.capture(state, 'collector', 'stdout', b'\n' + original)

    def stale_capture(self, state):
        raw = (self.proof / 'build.stderr').read_bytes()
        self.write('build.stderr', raw + b'changed capture\n')

    def environment(self, state):
        state['selected_environment']['NODE_OPTIONS'] = '--require unbound-hook'
        for command in state['commands']:
            command['selected_environment'] = copy.deepcopy(state['selected_environment'])

    def target(self, state):
        state['selected_environment']['CARGO_TARGET_DIR'] = state['target_root'] + '-other'
        for command in state['commands']:
            command['selected_environment'] = copy.deepcopy(state['selected_environment'])

    def collector_path(self, state):
        state['executable']['path'] += '-other'
        state['commands'][4]['argv'][0] = state['executable']['path']

    def binary_hash(self, state):
        state['executable']['sha256'] = '0' * 64
        state['executable_sha256'] = state['after_executable_sha256'] = '0' * 64

    def run(self):
        self.receipt('collector-exit-bool', lambda s: s.update(collector_exit=False))
        self.receipt('public-activation-claim', lambda s: s.update(public_activation=True))
        self.receipt('wrong-source-head', lambda s: s.update(repository_sha='0' * 40))
        self.receipt('wrong-source-tree', lambda s: s.update(head_tree='0' * 40))
        self.receipt('after-clean-numeric', lambda s: s.update(after_clean=1))
        self.reject('coherent-tracked-compiler-forgery', self.tracked_compiler)
        self.reject('coherent-current-source-authority-forgery', self.authority)
        self.reject('coherent-compiler-identity-forgery', self.compiler_identity)
        self.reject('historical-fake-passed-runner-rehashed', self.historical_runner)
        self.reject('historical-census-and-runner-jointly-rehashed', self.historical_census)
        self.reject('wrong-generated-source-dependency', self.dependency)
        self.reject('omitted-private-source-feature', self.feature)
        self.reject('changed-built-module-source', self.package_source)
        self.reject('changed-registry-lock-rehashed', self.registry_lock)
        self.reject('coherent-control-label-substitution', lambda s: self.controls(s, rename=True))
        self.reject('rehashed-controls-runtime-numeric', self.controls)
        self.reject('collector-stdout-differs-rehashed', self.collector_stream)
        self.reject('stale-original-stream-digest', self.stale_capture)
        self.receipt('missing-original-stream', lambda s: s['commands'][1]['captures'].pop('stderr'))
        self.receipt('commands-out-of-order',
                     lambda s: s['commands'].__setitem__(slice(0, 2), list(reversed(s['commands'][:2]))))
        self.receipt('build-locked-argument-omitted', lambda s: s['commands'][1]['argv'].remove('--locked'))
        self.receipt('command-exit-bool', lambda s: s['commands'][4].update(exit=False))
        self.reject('coherent-execution-hook-environment', self.environment)
        self.reject('coherent-target-environment-substitution', self.target)
        self.reject('coherent-collector-command-path-substitution', self.collector_path)
        self.reject('coherent-collector-hash-forgery', self.binary_hash)
        self.reject('actual-retained-collector-missing',
                    lambda s: self.remove(s['executable']['retained_path']))
        self.receipt('ordered-timestamp-reversal',
                     lambda s: s['commands'][1].update(completed_at=s['commands'][0]['started_at']))
        self.admit()
        if len(self.passed) != 28 or len(set(self.passed)) != 28:
            raise AssertionError('complete exact 28 hostile control census')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'proof'):
        parser.add_argument('--' + name, required=True, type=Path)
    for name in ('head', 'platform'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--run-id')
    parser.add_argument('--run-attempt')
    args = parser.parse_args()
    original = args.proof.absolute()
    admission.verify(args.root, original, args.head, args.platform, live=False,
                     run_id=args.run_id, run_attempt=args.run_attempt)
    inventory = (original / 'inventory.json').read_bytes()
    expected = archive_contract.expected_files(inventory, args.platform)
    files = {}
    for path in original.rglob('*'):
        admission.ir.ordinary_path(path)
        if path.is_file():
            files[path.relative_to(original).as_posix()] = digest(path.read_bytes())
    if set(files) != expected or len(files) != 393:
        raise AssertionError('complete real original 393-file proof required')
    with tempfile.TemporaryDirectory(prefix='verified-ir-admission-controls-') as temporary:
        copied = Path(temporary) / 'proof'
        shutil.copytree(original, copied)
        controls = Controls(args, copied)
        controls.run()
    for relative, expected_hash in files.items():
        if digest((original / relative).read_bytes()) != expected_hash:
            raise AssertionError('original proof changed: ' + relative)
    print(json.dumps(dict(admission_controls_passed=len(controls.passed),
                         controls=controls.passed, baseline_cases=107,
                         original_proof_unchanged=True, compiler_provider_executions=0,
                         synthetic_tamper_controls=True,
                         execution_attestation='original hosted API/log/ZIP chain remains required'),
                     sort_keys=True))


if __name__ == '__main__':
    main()
