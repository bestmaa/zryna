"""Linux production v5 ingestion through the unchanged owned-CFG observers; no public activation."""
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys

ingestion = pathlib.Path(__file__).resolve().parent
source = ingestion.parent / "m7-generic-owned-cfg"
repository = source.parents[1]
output = pathlib.Path(sys.argv[1]).resolve()
assert platform.system() == 'Linux' and platform.machine() == 'x86_64'
assert int(os.environ['CARGO_BUILD_JOBS']) <= 2
assert int(os.environ['RUST_TEST_THREADS']) <= 2
assert subprocess.check_output(['rustc', '--version'], text=True).startswith('rustc 1.97.1 ')
assert subprocess.check_output(['cargo', '--version'], text=True).startswith('cargo 1.97.1 ')
assert subprocess.check_output(['node', '--version'], text=True).strip() == 'v22.22.1'
output.mkdir(parents=True, exist_ok=True)
target = pathlib.Path(os.environ['CARGO_TARGET_DIR']).resolve() / 'debug'
names = ['zryna_abi', 'zryna_ir', 'zryna_layout', 'zryna_ownership_runtime_abi',
         'zryna_semantics', 'zryna_source', 'zryna_syntax', 'zryna_native_mir',
         'zryna_backend_javascript', 'zryna_backend_webassembly', 'zryna_backend_native', 'zryna_frontend']
command = ['cargo', 'build', '--locked', '--message-format=json-render-diagnostics']
for name in names:
    command += ['-p', name.replace('_', '-')]
build = subprocess.run(command, cwd=repository, check=True, timeout=600, text=True, stdout=subprocess.PIPE)
(output / 'build-messages.jsonl').write_text(build.stdout)
build_messages = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
command = ['rustc', '--edition=2024', '-L', 'dependency=' + str(target / 'deps'),
           str(ingestion / 'emit.rs'), '-o', str(output / 'emitter')]
for name in names:
    library = target / ('lib' + name + '.rlib')
    assert library.is_file(), library
    command += ['--extern', name + '=' + str(library)]
json_libraries = [pathlib.Path(filename) for message in build_messages
                  if message.get('reason') == 'compiler-artifact' and message['target']['name'] == 'serde_json'
                  for filename in message['filenames'] if filename.endswith('.rlib')]
assert len(json_libraries) == 1, json_libraries
command += ['--extern', 'serde_json=' + str(json_libraries[0])]
subprocess.run(command, cwd=repository, check=True, timeout=120)
subprocess.run([str(output / 'emitter'), str(output)], cwd=repository, check=True, timeout=30)

compiler = pathlib.Path('/usr/bin/gcc')
assert subprocess.check_output([str(compiler), '-dumpmachine'], text=True).strip() == 'x86_64-linux-gnu'
assert 12 <= int(subprocess.check_output([str(compiler), '-dumpversion'], text=True).split('.')[0]) <= 15
header=repository/'crates/zryna-ownership-runtime-abi/include/zryna_ownership_runtime_v1.h'
assert (output/'ownership-runtime-v1.h').read_bytes()==header.read_bytes()
oracles={'replacement': ['alloc:1:α', 'alloc:2:残', 'alloc:3:new', 'drop:1:α', 'drop:3:new', 'drop:2:残'], 'reinitialized': ['alloc:1:old', 'alloc:2:new', 'drop:1:old', 'drop:2:new'], 'option': ['alloc:1:old', 'alloc:2:new', 'drop:1:old', 'drop:2:new'], 'scalar': [], 'early': ['alloc:1:outer', 'alloc:2:value', 'alloc:3:early', 'drop:3:early', 'drop:2:value', 'drop:1:outer'], 'selfMove': ['alloc:1:self', 'drop:1:self'], 'updated': []}
expected=[]
for name,events in oracles.items(): expected+=['call:'+name]+events
for name,fault,events in [('replacement',1,['fail:1']),('replacement',2,['alloc:1:α','fail:2','drop:1:α']),('replacement',3,['alloc:1:α','alloc:2:残','fail:3','drop:2:残','drop:1:α']),('option',2,['alloc:1:old','fail:2','drop:1:old']),('early',3,['alloc:1:outer','alloc:2:value','fail:3','drop:2:value','drop:1:outer'])]:
 expected+=['fault:'+name+':'+str(fault)]+events+['call:'+name]+oracles[name]
records=[]
for name in ['false','true']:
 executable=output/(name+'-native')
 command=[str(compiler),'-std=c11','-O0','-Wall','-Wextra','-Werror','-no-pie','-Wl,--build-id=none','-I',str(output),str(source/'execute.c'),str(output/(name+'.o')),'-o',str(executable)]
 subprocess.run(command,check=True,timeout=30)
 result=subprocess.run([str(executable)],text=True,capture_output=True,timeout=10,check=True)
 assert result.stdout.splitlines()==expected,(result.stdout,expected)
 portable=[]
 for target,suffix in [('javascript','mjs'),('webassembly','wasm')]:
  proof=subprocess.run(['node',str(repository/'tests/m7-generic-owned-cfg'/(target+'.mjs')),str(output/(name+'.'+suffix))],text=True,capture_output=True,check=True,timeout=10)
  portable.append(json.loads(proof.stdout));print(proof.stdout.strip())
 records.append({'fixture':name,'exit_code':result.returncode,'fixed_successes':12,'controlled_failure_sigill':5,'malloc_free_utf8_events':result.stdout.splitlines(),'compiler_command':command,'portable':portable})
 print('Native owned '+name+': 12 fixed successes / 5 cleanup-before-SIGILL failures / PASS')
artifacts=[]
for name in ['false','true']:
 for suffix in ['mjs','wasm','zir','o','-native']:
  path=output/(name+suffix if suffix=='-native' else name+'.'+suffix);data=path.read_bytes()
  artifacts.append({'path':str(path),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
(output/'observations.json').write_text(json.dumps({'same_sealed_program_per_fixture':True,'actual_malloc_free_and_portable_allocations':True,'native':records,'artifacts':artifacts,'compiler_version':subprocess.check_output([str(compiler),'--version'],text=True).splitlines()[0],'public_activation':False},indent=2)+'\n')
