"""Execute one explicit frozen physical allocation group at one clean exact source head."""
import argparse
import hashlib
import json
import os
import platform
import re
from pathlib import Path
import subprocess
import sys

TEST = 'ownership_commands::conformance::native_provider_faults::physical::owned_shared_physical_group_executes_through_retained_source'
TESTS = {'owned-shared':TEST,
         'string':'ownership_commands::conformance::native_provider_faults::physical::string_physical_group_executes_through_retained_source',
         'owned-vec':'ownership_commands::conformance::native_provider_faults::physical::owned_vec_physical_group_executes_through_retained_source',
         'vec':'ownership_commands::conformance::native_provider_faults::physical::vec_physical_group_executes_through_retained_source',
         'handles':'ownership_commands::conformance::native_provider_faults::physical::handles_physical_group_executes_through_retained_source'}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def git(root, *args):
    return subprocess.check_output(['git','-C',str(root),*args], text=True).strip()


def inputs(root):
    return {n:digest(root/n) for n in git(root,'ls-files').splitlines()}


def main():
    if not __debug__ or sys.version_info < (3,11):
        raise RuntimeError('Python3.11+ without optimization required')
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('root','target','output','cargo','rustc','node'):
        p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--head',required=True)
    p.add_argument('--group',choices=TESTS,default='owned-shared')
    a=p.parse_args();root=a.root.resolve(strict=True)
    assert sys.platform == 'linux' and platform.machine() == 'x86_64' and git(root,'rev-parse','HEAD')==a.head
    assert not git(root,'status','--porcelain','--untracked-files=all')
    out=a.output.absolute();target=a.target.absolute()
    assert not out.exists() and not out.is_symlink() and not out.resolve().is_relative_to(root)
    assert not target.resolve().is_relative_to(root) and not out.is_relative_to(target)
    out.mkdir();(out/'observations').mkdir()
    tools={}
    for name,prefix in [('cargo','cargo 1.97.1 '),('rustc','rustc 1.97.1 '),('node','v22.22.1')]:
        path=getattr(a,name).absolute();assert path.is_file()
        version=subprocess.check_output([str(path),'--version'],text=True).strip()
        assert version.startswith(prefix)
        tools[name]={'path':str(path),'sha256':digest(path),'version':version}
    before=inputs(root)
    env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_NET_OFFLINE='true',
             CARGO=tools['cargo']['path'],RUSTC=tools['rustc']['path'],CARGO_BUILD_JOBS='2',
             CARGO_PROFILE_TEST_DEBUG='0',ZRYNA_TEST_NODE=tools['node']['path'],
             ZRYNA_M3_NATIVE_PHYSICAL_EVIDENCE=str(out/'observations'))
    receipt={'format':'zryna.retained-physical-corpus.v2','group':a.group,'head':a.head,
             'tree':git(root,'rev-parse','HEAD^{tree}'),'platform':sys.platform,'root':str(root),
             'target':str(target),'inputs':before,'tools':tools,'commands':[],
             'run_id':os.getenv('GITHUB_RUN_ID'),'run_attempt':os.getenv('GITHUB_RUN_ATTEMPT'),
             'status':'failed','public_activation':False,'installed_no_cargo_acceptance':False}
    def run(label,argv,timeout):
        r=subprocess.run(argv,cwd=root,env=env,capture_output=True,timeout=timeout)
        (out/(label+'.stdout')).write_bytes(r.stdout);(out/(label+'.stderr')).write_bytes(r.stderr)
        receipt['commands'].append({'label':label,'argv':argv,'cwd':str(root),'exit':r.returncode})
        return r
    try:
        build=run('feature-test-build',[tools['cargo']['path'],'test','--locked','--offline','-p',
                  'zryna-driver','--features','native-provider-internal','--lib','--no-run','--message-format=json'],2400)
        assert build.returncode==0
        records=[json.loads(line) for line in build.stdout.splitlines()]
        matches=[row['executable'] for row in records if row.get('reason')=='compiler-artifact'
                 and row.get('executable') and row['target']['name']=='zryna_driver' and row['profile']['test']]
        assert len(matches)==1
        binary=Path(matches[0]);receipt['test_binary']={'path':str(binary),'sha256':digest(binary)}
        result=run('physical-corpus',[str(binary),TESTS[a.group],'--exact','--nocapture','--test-threads=1'],2400)
        assert inputs(root)==before and not git(root,'status','--porcelain','--untracked-files=all')
        assert digest(binary)==receipt['test_binary']['sha256']
        assert all(digest(Path(row['path']))==row['sha256'] for row in tools.values())
        assert result.returncode==0
        summaries=re.findall(r'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out;', result.stdout.decode(), re.M)
        assert len(summaries)==1
        receipt['status']='passed'
    except (AssertionError,OSError,ValueError,subprocess.SubprocessError) as error:
        receipt['failure']=str(error) or type(error).__name__
        if isinstance(error,subprocess.TimeoutExpired):
            receipt['failure']+='; process-tree cleanup unconfirmed; cannot qualify'
    with (out/'receipt.json').open('x') as f:json.dump(receipt,f,indent=2);f.write('\n')
    print(json.dumps({k:receipt[k] for k in ['head','tree','platform','status','run_id','run_attempt']}))
    return int(receipt['status']!='passed')


if __name__=='__main__':sys.exit(main())
