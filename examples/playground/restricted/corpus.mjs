// Fixed source/input/oracle evidence is exercised by the real compiler and browser gate.
export const corpus = Object.freeze([
  { id: 'addition', source: 'export function add(a: i32, b: i32): i32 { return a + b; }\n',
    export: 'add', cases: [
      { args: [20, 22], value: 42 },
      { args: [2147483647, 1], value: -2147483648 },
      { args: [-2147483648, -1], value: 2147483647 },
      { args: [0, 0], value: 0 }, { args: [-7, 2], value: -5 },
    ] },
  { id: 'edited-literal', source: 'export function value(a: i32): i32 { return a + 3; }\n',
    export: 'value', cases: [{ args: [39], value: 42 }, { args: [-3], value: 0 }] },
  { id: 'nested-addition', source: 'export function total(a: i32, b: i32, c: i32): i32 { return a + b + c; }\n',
    export: 'total', cases: [{ args: [10, 12, 20], value: 42 }] },
  { id: 'multiple-exports', source: 'export function zero(): i32 { return 0; }\nexport function constant(): i32 { return 42; }\n',
    export: 'constant', cases: [{ args: [], value: 42 }], extra: { export: 'zero', args: [], value: 0 } },
  { id: 'any-rejected', source: 'export function invalid(a: any): i32 { return 1; }\n', code: 'ZRYNA-M1004' },
  { id: 'bool-gated', source: 'export function identity(a: bool): bool { return a; }\n', code: 'ZRYNA-I1006' },
  { id: 'parse-rejected', source: 'export function broken(: i32 { return ;\n', provider: true },
  { id: 'unresolved', source: 'export function missing(): i32 { return absent; }\n', rejected: true },
  { id: 'unsupported-native', source: 'export function main(): i32 { const value: i32 = 1; return value; }\n', rejected: true },
]);
