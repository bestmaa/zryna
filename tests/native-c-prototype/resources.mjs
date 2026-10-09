import { allowedExportName, reject } from './source.mjs';

const foreign = type => type.startsWith('Ffi');
const scalar = abi => ({ 'c-i32': 'i32', 'c-int': 'i32', bool32: 'bool', unit: 'unit' })[abi];

// Replays only the canonical prototype's straight-line statements and terminal status guards.
// Plans are test observations, never sealed cleanup authority or executed release evidence.
export function inspectFunction(fn, operations) {
  const locals = new Map();
  const obligations = [];
  const plans = [];
  let nextIdentity = 0;
  let pending;
  let terminated = false;
  const check = (condition, detail, node = fn, code) => {
    if (!condition) reject(detail, node, code);
  };
  const value = (type, fields = {}) => ({ type, identity: ++nextIdentity, ...fields });
  const live = token => obligations.includes(token.obligation);
  const cleanup = () => obligations.toReversed().map(owner => ({
    identity: owner.identity, kind: owner.resource.kind, release: owner.resource.release,
  }));
  const bind = (name, item, node) => {
    check(!locals.has(name), 'duplicate-local', node);
    locals.set(name, item);
  };
  for (const parameter of fn.parameters) {
    check(!foreign(parameter.type), 'foreign-parameter');
    bind(parameter.name, value(parameter.type), fn);
  }
  if (fn.exported) {
    check(allowedExportName(fn.name), 'export-name', fn, 'ZRYNA-C4104');
    check(fn.parameters.every(parameter => ['i32', 'bool'].includes(parameter.type))
      && ['i32', 'bool'].includes(fn.result), 'export-carrier', fn, 'ZRYNA-C4104');
  }

  function evaluate(node) {
    if (node.kind === 'local') {
      const item = locals.get(node.name);
      check(item, 'unresolved-local', node);
      check(!['taken', 'consumed'].includes(item.state), 'stale-token', node);
      if (item.obligation) check(live(item), 'consumed-owner', node);
      return item;
    }
    if (['i32', 'bool', 'key'].includes(node.kind)) return value(node.kind, { literal: node.value });
    if (node.kind === 'add') {
      const left = evaluate(node.left);
      const right = evaluate(node.right);
      check(left.type === 'i32' && right.type === 'i32', 'addition-types', node, 'ZRYNA-C4104');
      return value('i32');
    }
    check(node.kind === 'call', 'expression', node);
    check(!fn.exported, 'export-effects', node, 'ZRYNA-C4104');
    const args = node.args.map(evaluate);
    const arity = (...types) => check(args.length === types.length && args.every((arg, index) =>
      arg.type === types[index]), 'primitive-types', node, 'ZRYNA-C4104');
    const fresh = type => value(type, { state: 'fresh' });
    const initialized = token => check(token.state === 'initialized', 'uninitialized-output', node);
    const operation = () => {
      check(args[0]?.type === 'key', 'operation-literal', node);
      const op = operations.get(args[0].literal);
      check(op?.direction === 'import', 'import-operation', node);
      return op;
    };
    switch (node.primitive) {
      case 'borrowBytes':
      case 'borrowUtf8':
        arity(node.primitive === 'borrowBytes' ? 'Vec<i32>' : 'String');
        plans.push({ role: 'loan-precheck', start: node.start,
          traps: node.primitive === 'borrowBytes' ? ['FOREIGN_LENGTH', 'FOREIGN_BYTE_RANGE']
            : ['FOREIGN_LENGTH'] });
        return value('FfiBytes', { source: args[0].identity });
      case 'byteLength':
        arity('FfiBytes');
        return value('i32', { lengthOf: args[0].identity });
      case 'outI32': return fresh('FfiI32Out');
      case 'outBytes': return fresh('FfiBytesOut');
      case 'outCount': return fresh('FfiCountOut');
      case 'outHandle':
        arity('key');
        check([...operations.values()].some(op => op.resources.some(resource =>
          resource.kind === args[0].literal && resource.access === 'create'
          && op.parameters.some(parameter => parameter.abi === 'handle-out'))), 'handle-kind', node);
        return value('FfiHandleOut', { state: 'fresh', kind: args[0].literal });
      case 'readI32':
        arity('FfiI32Out'); initialized(args[0]);
        return value('i32');
      case 'takeHandle': {
        arity('FfiHandleOut'); initialized(args[0]);
        const owner = args[0].obligation;
        check(live(args[0]), 'handle-obligation', node);
        args[0].state = 'taken';
        return value('FfiHandle', { obligation: owner, resource: owner.resource });
      }
      case 'takeBytes': {
        arity('FfiBytesOut', 'FfiCountOut'); args.forEach(initialized);
        check(args[0].obligation && args[0].call === args[1].call
          && args[0].resourceIndex === args[1].resourceIndex, 'unpaired-byte-outputs', node);
        const owner = args[0].obligation;
        args.forEach(item => { item.state = 'taken'; });
        plans.push({ role: 'validate-foreign-bytes', start: node.start, cleanup: cleanup(),
          releasableOnMalformed: owner.resource.releasableOnMalformed });
        return value('FfiOwnedBytes', { obligation: owner, resource: owner.resource });
      }
      case 'copyBytes':
        arity('FfiOwnedBytes');
        check(live(args[0]), 'bytes-obligation', node);
        plans.push({ role: 'copy-prepare-failure', start: node.start, cleanup: cleanup() });
        return value('Vec<i32>');
      case 'release': {
        const op = operation();
        check(op.mode === 'void' && op.parameters.length === 1, 'release-signature', node);
        consume(op, args.slice(1), node);
        return value('unit');
      }
      case 'foreignError':
        // The only admitted dynamic use is inspected by the dominating terminal guard below.
        reject('unguarded-foreign-error', node);
        break;
      case 'rawCall': {
        const op = operation();
        const actual = args.slice(1);
        check(actual.length === op.parameters.length, 'raw-arity', node, 'ZRYNA-C4104');
        const slots = [];
        for (const [index, parameter] of op.parameters.entries()) {
          const item = actual[index];
          const resource = parameter.resource === null ? undefined : op.resources[parameter.resource];
          const types = { 'bytes-in': 'FfiBytes', count: 'i32', 'i32-out': 'FfiI32Out',
            'bytes-owned-out': 'FfiBytesOut', 'count-out': 'FfiCountOut',
            'handle-out': 'FfiHandleOut', 'handle-in': 'FfiHandle', 'bytes-release': 'FfiOwnedBytes' };
          check(item.type === (scalar(parameter.abi) ?? types[parameter.abi]),
            'raw-argument-type', node, 'ZRYNA-C4104');
          if (parameter.abi.endsWith('-out')) {
            check(item.state === 'fresh' && !slots.includes(item), 'output-alias-or-reuse', node);
            if (parameter.abi === 'handle-out') check(item.kind === resource?.kind, 'output-kind', node);
            slots.push(item);
          }
          if (['handle-in', 'bytes-release'].includes(parameter.abi)) matchOwner(item, resource, node);
          if (parameter.abi === 'count') {
            const loan = actual[resource?.slots[0]];
            check(item.lengthOf === loan?.identity, 'unpaired-byte-count', node);
          }
        }
        if (op.mode === 'void') {
          consume(op, actual, node);
          return value('unit');
        }
        if (op.mode === 'direct') return value(scalar(op.result));
        check(op.mode === 'status' && !pending, 'pending-status', node);
        const acquisitions = op.resources.filter(resource => resource.access === 'create').length;
        check(obligations.length + acquisitions <= 64, 'live-obligations', node, 'ZRYNA-C4107');
        const status = value('i32', { operation: op, arguments: actual, slots, start: node.start });
        slots.forEach(slot => { slot.state = 'pending'; });
        pending = status;
        plans.push({ role: 'pre-call-reservation', start: node.start, maximum: acquisitions });
        return status;
      }
      default: reject('primitive', node);
    }
  }

  function matchOwner(item, resource, node) {
    check(resource && item.resource && live(item) && item.resource.kind === resource.kind
      && item.resource.allocator === resource.allocator && item.resource.release === resource.release,
    'owner-library-kind-allocator', node);
  }
  function consume(op, args, node) {
    check(op.resources.length === 1 && op.resources[0].access === 'consume'
      && args.length === 1, 'release-policy', node);
    matchOwner(args[0], op.resources[0], node);
    const expected = op.parameters[0].abi === 'handle-in' ? 'FfiHandle' : 'FfiOwnedBytes';
    check(args[0].type === expected, 'release-kind', node);
    obligations.splice(obligations.indexOf(args[0].obligation), 1);
    args[0].state = 'consumed';
    plans.push({ role: 'release', start: node.start, release: op.key });
  }
  for (const statement of fn.body) {
    check(!terminated, 'after-terminal', statement);
    check(!pending || statement.kind === 'guard', 'missing-status-guard', statement);
    if (statement.kind === 'guard') {
      const status = locals.get(statement.status);
      const terminal = statement.value;
      check(pending && status === pending && terminal.kind === 'call'
        && terminal.primitive === 'foreignError' && terminal.args.length === 2
        && terminal.args[0].kind === 'key' && terminal.args[0].value === status.operation.key
        && terminal.args[1].kind === 'local' && locals.get(terminal.args[1].name) === status,
      'status-dominance', statement);
      plans.push({ role: 'nonzero-status', start: statement.start, operation: status.operation.key,
        recoverable: status.operation.statuses.filter(row => row.code !== 0).map(row => row.code),
        unknown: 'HostAbiFailure', cleanup: cleanup() });
      status.slots.forEach(slot => { slot.state = 'initialized'; slot.call = status.identity; });
      for (const [index, resource] of status.operation.resources.entries()) {
        if (resource.access !== 'create') continue;
        const owner = { identity: ++nextIdentity, resource };
        obligations.push(owner);
        resource.slots.forEach(slot => {
          status.arguments[slot].obligation = owner;
          status.arguments[slot].resourceIndex = index;
        });
      }
      pending = undefined;
    } else {
      const result = evaluate(statement.value);
      if (statement.kind === 'const') {
        check(result.type === statement.type, 'local-type', statement, 'ZRYNA-C4104');
        check(!foreign(result.type) || statement.value.kind === 'call', 'prototype-token-alias', statement);
        bind(statement.name, result, statement);
      } else if (statement.kind === 'return') {
        check(!foreign(result.type) && result.type === fn.result && !pending,
          'return-type-or-escape', statement, 'ZRYNA-C4104');
        plans.push({ role: 'return', start: statement.start, cleanup: cleanup() });
        terminated = true;
      } else check(result.type === 'unit' && !pending, 'discarded-value', statement);
    }
  }
  check(terminated && !pending, 'missing-return');
  return { name: fn.name, plans };
}
