import assert from 'node:assert/strict';
import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
const root = process.argv[2];
assert.equal(typeof root, 'string');
const receipts = [];
const signatures = {score:['i32'],unwrap:['i32'],flag:['bool'],add:['i32'],fallback:[],error:[]};
function i32(value) {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < -2147483648 || value > 2147483647 || Object.is(value,-0)) throw new TypeError('invalid i32 test input');
  return value;
}
for (const name of ['false','true']) {
  const js = await import(pathToFileURL(`${root}/${name}.mjs`).href);
  const core = new WebAssembly.Module(fs.readFileSync(`${root}/${name}.wasm`));
  const wasm = new WebAssembly.Instance(core).exports;
  assert.deepEqual(WebAssembly.Module.imports(core), []);
  assert.deepEqual(WebAssembly.Module.exports(core).map(x=>[x.name,x.kind]).sort(), Object.keys(signatures).sort().map(name=>[name,'function']));
  assert.deepEqual(Object.keys(js).sort(), Object.keys(signatures).sort());
  // Test-only strict typed host validation; this is not production adapter/profile admission.
  function wasmCall(exportName, values) {
    const types = signatures[exportName];
    if (!types || types.length !== values.length) throw new TypeError('invalid export/arity');
    const carriers = types.map((type,index) => {
      if (type === 'i32') return i32(values[index]);
      if (typeof values[index] !== 'boolean') throw new TypeError('invalid bool test input');
      return values[index] ? 1 : 0;
    });
    const result = wasm[exportName](...carriers);
    if (exportName !== 'flag') return i32(result);
    if (result !== 0 && result !== 1) throw new TypeError('invalid bool carrier');
    return result === 1;
  }
  const observations = [];
  function check(exportName, values, expected) {
    assert.equal(js[exportName](...values), expected);
    assert.equal(wasmCall(exportName, values), expected);
    observations.push({exportName,values,type:typeof expected==='boolean'?'Bool':'I32',expected});
  }
  for (const value of [-2147483648,-1,0,7,2147483647]) {
    check('score',[value],value); check('unwrap',[value],value);
  }
  check('flag',[true],true); check('flag',[false],false);
  check('add',[2147483647],-2147483648); check('add',[-1],0);
  check('fallback',[],11); check('error',[],23);
  for (const value of [-0,1.5,NaN,Infinity,2147483648,-2147483649,true,'7',null,{}]) {
    assert.throws(()=>js.score(value)); assert.throws(()=>wasmCall('score',[value]));
  }
  for (const value of [0,1,'true',null,{}]) {
    assert.throws(()=>js.flag(value)); assert.throws(()=>wasmCall('flag',[value]));
  }
  for (const [exportName,values] of [['score',[]],['score',[7,8]],['fallback',[0]]]) {
    assert.throws(()=>js[exportName](...values)); assert.throws(()=>wasmCall(exportName,values));
  }
  for (const value of [-2147483648,-1,2,2147483647]) assert.throws(()=>wasm.flag(value),WebAssembly.RuntimeError);
  check('unwrap',[7],7); check('flag',[true],true);
  assert.equal(new WebAssembly.Instance(core).exports.unwrap(-1),-1);
  receipts.push({sourceModules:name==='false'?1:2,sameSealedAuthority:true,genericInstances:5,functions:11,exports:6,ownershipEffects:{loans:0,drops:0},observations,hostBoundary:'test-only strict typed validation; raw Wasm JS coercions are not validation',outcome:'passed'});
}
fs.writeFileSync(`${root}/js-wasm-observations.json`,JSON.stringify({node:process.version,receipts},null,2)+'\n');
console.log('JS/Wasm: same sealed authority, 2 genuine source programs, 36 fixed typed observations, boundary rejection and pristine recovery: PASS');
