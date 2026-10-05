"""Independent admission of current-head private source-build proof, never activation."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

from manifest_admission import contracts, verify_bundle

POSITIVES = ('m1', 'm2', 'm3-pair', 'm3-array', 'm3-borrow', 'm3-vec',
             'm3-string', 'm3-owned-aggregate', 'm3-owned-vec')
NEGATIVES = {'m1-negative':'ZRYNA-M1004', 'm1-bool':'ZRYNA-I1006',
             'm3-moved':'ZRYNA-M3011', 'm2-bare-import':'ZRYNA-F1103',
             'm2-cycle-main':'ZRYNA-D3007', 'm2-cycle-dep':'ZRYNA-D3007'}
CONTROLS = ('default-feature-disabled', 'ordinary-feature-build-needs-node',
            'private-project-denied', 'private-component-denied',
            'source-checkout-still-needs-cargo', 'source-and-binary-identity')
HEX = re.compile('[0-9a-f]{64}')


def pairs(rows):
    result = {}
    for key,value in rows:
        assert key not in result, 'duplicate JSON key'
        result[key] = value
    return result


def strict(data):
    return json.loads(data, object_pairs_hook=pairs,
                      parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)),
                      parse_float=lambda x: (_ for _ in ()).throw(ValueError('unexpected floating value '+x)))


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def exact(value, expected):
    assert type(value) is type(expected)
    if isinstance(expected,dict):
        assert set(value) == set(expected)
        for key in expected:
            exact(value[key],expected[key])
    elif isinstance(expected,list):
        assert len(value) == len(expected)
        for actual,wanted in zip(value,expected):
            exact(actual,wanted)
    else:
        assert value == expected


def files(directory):
    assert directory.is_dir() and not directory.is_symlink()
    result = {}
    for path in directory.rglob('*'):
        assert not path.is_symlink()
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = {'bytes':path.stat().st_size,'sha256':digest(path)}
    assert result, 'missing complete retained bundle'
    return result


def git(root,*args):
    return subprocess.check_output(['git','-C',str(root),*args],text=True).strip()


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise AssertionError('rejected command left a final bundle or link')


def failure_envelope(data, code):
    assert type(data) is dict and set(data) == {'version','command','diagnostics','manifest','ok','results'}
    exact({key:data[key] for key in ('version','command','manifest','ok','results')},
          {'version':1,'command':'build','manifest':None,'ok':False,'results':[]})
    rows = data['diagnostics']
    assert type(rows) is list and rows
    for row in rows:
        assert type(row) is dict and set(row) == {'code','guidance','message','primary','severity'}
        assert type(row['code']) is str and re.fullmatch(r'ZRYNA-[A-Z][0-9]{4}',row['code'])
        assert row['severity'] == 'error'
        assert all(type(row[key]) is str and row[key] for key in ('guidance','message'))
        primary = row['primary']
        assert type(primary) is dict
        if primary.get('kind') == 'global':
            exact(primary,{'kind':'global'})
        elif primary.get('kind') == 'source':
            assert set(primary) == {'kind','span'}
            span = primary['span']
            assert type(span) is dict and set(span) == {'file','start','end'}
            assert all(type(value) is int and value >= 0 for value in span.values())
            assert span['start'] <= span['end']
        else:
            exact(primary,{'kind':'workspace-path','path':'Cargo.toml'})
    assert code in [row['code'] for row in rows]


def verify(root, output, head, platform):
    assert __debug__, 'optimized verification forbidden'
    assert re.fullmatch('[0-9a-f]{40}',head) and platform in ('linux','win32')
    receipt = strict((output/'ci-receipt.json').read_bytes())
    assert set(receipt) == {'format','head','tree','platform','root','target','status','inputs',
                            'tools','binaries','commands','smoke_receipt','public_activation',
                            'installed_distribution_acceptance','python'}
    assert receipt['format'] == 'zryna.private-cli-ci.v1' and receipt['status'] == 'passed'
    assert receipt['head'] == head == git(root,'rev-parse','HEAD')
    assert receipt['tree'] == git(root,'rev-parse','HEAD^{tree}')
    assert receipt['platform'] == platform and Path(receipt['root']).resolve() == root.resolve()
    assert receipt['public_activation'] is False and receipt['installed_distribution_acceptance'] is False
    assert receipt['smoke_receipt'] == 'smoke/receipt.json'
    target = Path(receipt['target'])
    assert target.is_absolute() and target.is_dir() and not target.is_symlink()
    assert not target.resolve().is_relative_to(root.resolve())
    assert not target.resolve().is_relative_to(output.resolve())
    assert not output.resolve().is_relative_to(target.resolve())
    assert Path(receipt['python']).is_absolute() and Path(receipt['python']).is_file()
    assert not git(root,'status','--porcelain')
    names = subprocess.check_output(['git','-C',str(root),'ls-files','-z']).decode().split('\0')
    actual = {name:digest(root/name) for name in names if name}
    assert receipt['inputs'] == actual and actual
    assert set(receipt['tools']) == {'node','cargo','rustc'}
    assert set(receipt['binaries']) == {'default','feature'}
    assert all(set(row) == {'path','sha256','version'} for row in receipt['tools'].values())
    assert all(set(row) == {'path','sha256'} for row in receipt['binaries'].values())
    for row in [*receipt['tools'].values(),*receipt['binaries'].values()]:
        assert type(row['sha256']) is str and HEX.fullmatch(row['sha256'])
        assert Path(row['path']).is_absolute() and digest(Path(row['path'])) == row['sha256']
    suffix = '.exe' if platform == 'win32' else ''
    for label in ('default','feature'):
        assert Path(receipt['binaries'][label]['path']).resolve() == (output/(label+'-cli'+suffix)).resolve()
    assert receipt['tools']['node']['version'] == 'v22.22.1'
    assert receipt['tools']['cargo']['version'].startswith('cargo 1.97.1 ')
    assert receipt['tools']['rustc']['version'].startswith('rustc 1.97.1 ')
    commands = receipt['commands']
    assert [row['label'] for row in commands] == ['cargo-version','rustc-version','node-version',
                                               'default-build','feature-build','cli-smoke']
    assert all(set(row) == {'label','argv','exit','cwd','target'} for row in commands)
    assert all(type(row['exit']) is int and row['exit'] == 0 for row in commands)
    assert all(row['cwd'] == receipt['root'] and row['target'] == receipt['target'] for row in commands)
    for index,name in enumerate(('cargo','rustc','node')):
        exact(commands[index]['argv'],[receipt['tools'][name]['path'],'--version'])
        assert (output/(name+'-version.stdout')).read_text().strip() == receipt['tools'][name]['version']
    for index,extra in ((3,[]),(4,['--features','native-provider-internal'])):
        assert commands[index]['argv'] == [receipt['tools']['cargo']['path'],'build','--locked','--offline','-p','zryna',*extra]
    exact(commands[5]['argv'],[receipt['python'],'-B',str(root/'scripts/run-native-cli-smoke.py'),
        '--root',receipt['root'],'--default-cli',receipt['binaries']['default']['path'],
        '--feature-cli',receipt['binaries']['feature']['path'],'--node',receipt['tools']['node']['path'],
        '--cargo',receipt['tools']['cargo']['path'],'--rustc',receipt['tools']['rustc']['path'],
        '--output',str(output/'smoke')])
    smoke = strict((output/'smoke/receipt.json').read_bytes())
    assert set(smoke) == {'version','head','tree','inputs','generated_inputs','binaries','path',
                          'records','counts','blocked_acceptance','public_activation'}
    assert type(smoke['version']) is int and smoke['version'] == 1
    assert smoke['head'] == head and smoke['tree'] == receipt['tree'] and smoke['inputs'] == actual
    assert smoke['public_activation'] is False
    assert set(smoke['counts']) == {'passed','failed','ignored'}
    assert all(type(v) is int for v in smoke['counts'].values())
    assert smoke['counts'] == {'passed':21,'failed':0,'ignored':0}
    rows = smoke['records']
    assert type(rows) is list and all(type(row) is dict for row in rows)
    assert all(set(row) == ({'id','status'} if row['id'] == 'source-and-binary-identity' else {'id','status','detail'}) for row in rows)
    assert len(rows) == 21 and {row['id'] for row in rows} == set(POSITIVES)|set(NEGATIVES)|set(CONTROLS)
    assert all(row['status'] == 'passed' for row in rows)
    by_id = {row['id']:row for row in rows}
    empty = Path(smoke['path'])
    assert empty.resolve() == (output/'smoke/empty-path').resolve() and empty.is_dir() and not list(empty.iterdir())
    assert set(smoke['binaries']) == {'default_cli','feature_cli','node','cargo','rustc'}
    for key,label in (('default_cli','default'),('feature_cli','feature')):
        assert smoke['binaries'][key] == receipt['binaries'][label]
    for name in ('node','cargo','rustc'):
        assert smoke['binaries'][name] == {k:receipt['tools'][name][k] for k in ('path','sha256')}
    expected_cases,expected_generated = contracts(root,output/'smoke',actual)
    exact(smoke['generated_inputs'],expected_generated)
    for label in POSITIVES:
        detail = by_id[label]['detail']
        assert set(detail) == {'source','profile','files','success_json_exact','manifest_bytes_exact',
                               'create_only','node_on_path','pnpm_on_path'}
        assert detail['source'] == expected_cases[label]['source']
        assert detail['profile'] == expected_cases[label]['profile']
        for key in ('success_json_exact','manifest_bytes_exact','create_only'):
            assert detail[key] is True
        assert detail['node_on_path'] is False and detail['pnpm_on_path'] is False
        baseline, native = files(output/'smoke'/(label+'-bootstrap-bundle')), files(output/'smoke'/(label+'-native-bundle'))
        exact(baseline,native)
        exact(detail['files'],native)
        expected_manifest = 'zryna-manifest-v'+('1' if label=='m1' else '2' if label=='m2' else '3')+'.json'
        assert expected_manifest in native
        assert (output/'smoke'/(label+'-bootstrap.stdout')).read_bytes() == (output/'smoke'/(label+'-native.stdout')).read_bytes()
        success = strict((output/'smoke'/(label+'-native.stdout')).read_bytes())
        verify_bundle(output/'smoke'/(label+'-bootstrap-bundle'),label,expected_cases[label],baseline,success)
        verify_bundle(output/'smoke'/(label+'-native-bundle'),label,expected_cases[label],native,success)
        failure_envelope(strict((output/'smoke'/(label+'-create-only.stdout')).read_bytes()),'ZRYNA-C1009')
    for label,code in NEGATIVES.items():
        detail = by_id[label]['detail']
        assert set(detail) == {'expected_code','exit','final_bundle_absent'}
        assert detail['expected_code'] == code and type(detail['exit']) is int and detail['exit'] != 0
        assert detail['final_bundle_absent'] is True
        absent(root/'.zryna/out'/('private-smoke-'+label+'.build'))
        raw = (output/'smoke'/(label+'.stdout')).read_bytes()
        assert raw == (output/'smoke'/(label+'-bootstrap.stdout')).read_bytes()
        diagnostics = strict(raw)
        failure_envelope(diagnostics,code)
    assert len(smoke['generated_inputs']) == 6
    for name,row in smoke['generated_inputs'].items():
        assert row['original'] in actual and row['sha256'] == actual[row['original']] == digest(root/name)
    assert by_id['source-checkout-still-needs-cargo']['detail']['architecture_gate_retained'] is True
    failure_envelope(strict((output/'smoke/source-checkout-still-needs-cargo.stdout').read_bytes()),'ZRYNA-A1101')
    for label in CONTROLS[:-1]:
        detail = by_id[label]['detail']
        assert type(detail['exit']) is int and detail['exit'] != 0
        if label == 'source-checkout-still-needs-cargo':
            assert set(detail) == {'exit','architecture_gate_retained'}
        else:
            assert set(detail) == {'exit','expected_code'}
            assert detail['expected_code'] == {'private-project-denied':'ZRYNA-C2001',
                'private-component-denied':'ZRYNA-C1013'}.get(label)
        if label in ('private-project-denied','private-component-denied'):
            code = 'ZRYNA-C2001' if label == 'private-project-denied' else 'ZRYNA-C1013'
            data = strict((output/'smoke'/(label+'.stdout')).read_bytes())
            failure_envelope(data,code)
        elif label in ('default-feature-disabled','ordinary-feature-build-needs-node'):
            text = (output/'smoke'/(label+'.stderr')).read_text()
            assert ('--native-frontend' if label == 'default-feature-disabled' else '--node') in text
    absent(root/'.zryna/out/private-smoke-denied.build')
    assert smoke['blocked_acceptance'] == ['ordinary installed CLI without Node/pnpm/Cargo',
        'public activation','native run selection','cross-platform installed distribution proof']
    return {'head':head,'tree':receipt['tree'],'platform':platform,'passed':21,'failed':0,
            'ignored':0,'public_activation':False,'installed_distribution_acceptance':False,
            'ci_receipt_sha256':digest(output/'ci-receipt.json'),
            'smoke_receipt_sha256':digest(output/'smoke/receipt.json')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
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
