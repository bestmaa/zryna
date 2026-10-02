// Detached receipts bind actual retained bytes without a toolkit/archive self-hash cycle.
import { bytes, orderedPaths, portablePath, sha256 } from '../distribution/canonical.mjs';
import { decodeJson } from '../../examples/playground/restricted/json.mjs';
import { exact, fail, isHash } from '../../examples/playground/restricted/limits.mjs';

const MAXIMUM = 262144;
const canonical = input => {
  if (!(input instanceof Uint8Array) || input.length < 1 || input.length > MAXIMUM) fail('M6-RECEIPT-BYTES');
  const value = decodeJson(input, MAXIMUM);
  if (!bytes(value).equals(Buffer.from(input))) fail('M6-RECEIPT-CANONICAL');
  return value;
};

export function retainedDescriptor(value, prefix) {
  exact(value, ['path', 'bytes', 'sha256']);
  portablePath(value.path);
  if (!value.path.startsWith(prefix) || !Number.isSafeInteger(value.bytes) || value.bytes < 0 ||
      value.bytes > MAXIMUM || !isHash(value.sha256)) fail('M6-RETAINED');
}

export function gateObservation(gate) {
  const { receipt, ...observation } = gate;
  return observation;
}

export function validateGateReceipt(input, gate, record) {
  if (input.length !== gate.receipt.bytes || sha256(input) !== gate.receipt.sha256) fail('M6-RECEIPT-DIGEST');
  const value = canonical(input);
  exact(value, ['format', 'version', 'integration', 'observation', 'materials', 'execution']);
  if (value.format !== 'zryna.m6-gate-receipt.v1' || value.version !== 1 ||
      !bytes(value.integration).equals(bytes(record.integration)) ||
      !bytes(value.observation).equals(bytes(gateObservation(gate)))) fail('M6-RECEIPT-OBSERVATION');
  const expected = gate.artifactRoles.map(role => record.artifacts.find(artifact => artifact.role === role));
  if (!bytes(value.materials).equals(bytes(expected))) fail('M6-RECEIPT-MATERIALS');
  exact(value.execution, ['cwd', 'profile', 'corpusSha256', 'inputs', 'stdout', 'stderr']);
  const { cwd, profile, corpusSha256, inputs, stdout, stderr } = value.execution;
  if (cwd !== '.') portablePath(cwd);
  if (typeof profile !== 'string' || !/^[a-z0-9][a-z0-9-]{0,63}$/.test(profile) ||
      !isHash(corpusSha256) || !Array.isArray(inputs) || inputs.length < 1 || inputs.length > 64) fail('M6-RECEIPT-EXECUTION');
  const prefix = `retained/${gate.id}/`;
  for (const descriptor of [...inputs, stdout, stderr]) retainedDescriptor(descriptor, prefix);
  if (!inputs.some(descriptor => descriptor.sha256 === corpusSha256)) fail('M6-RECEIPT-CORPUS');
  orderedPaths([...inputs, stdout, stderr].map(item => item.path).sort());
  return value;
}

export function validateRetainedReceipts(record, carriers) {
  if (!Array.isArray(carriers) || carriers.length < record.gates.length || carriers.length > 256) fail('M6-RETAINED-INVENTORY');
  const captured = new Map();
  let total = 0;
  for (const carrier of carriers) {
    exact(carrier, ['path', 'data']);
    portablePath(carrier.path);
    if (!(carrier.data instanceof Uint8Array) || carrier.data.length > MAXIMUM ||
        (total += carrier.data.length) > 16777216 || captured.has(carrier.path)) fail('M6-RETAINED-INVENTORY');
    captured.set(carrier.path, Buffer.from(carrier.data));
  }
  orderedPaths([...captured.keys()].sort());
  const expected = new Set();
  const receipts = [];
  for (const gate of record.gates) {
    const input = captured.get(gate.receipt.path);
    if (!input) fail('M6-RETAINED-MISSING');
    expected.add(gate.receipt.path);
    const receipt = validateGateReceipt(input, gate, record);
    for (const descriptor of [...receipt.execution.inputs, receipt.execution.stdout, receipt.execution.stderr]) {
      const data = captured.get(descriptor.path);
      if (!data || data.length !== descriptor.bytes || sha256(data) !== descriptor.sha256) fail('M6-RETAINED-DIGEST');
      if (expected.has(descriptor.path)) fail('M6-RETAINED-INVENTORY');
      expected.add(descriptor.path);
    }
    receipts.push(receipt);
  }
  if (expected.size !== captured.size) fail('M6-RETAINED-INVENTORY');
  return receipts;
}

// Assembly preserves the caller's observed counts, commands and environment; it invents none.
export function createGateReceipt({ integration, observation, materials, execution }) {
  const input = bytes({ format: 'zryna.m6-gate-receipt.v1', version: 1,
    integration, observation, materials, execution });
  const gate = { ...observation, receipt: { path: `evidence/${observation.id}.json`,
    bytes: input.length, sha256: sha256(input) } };
  validateGateReceipt(input, gate, { integration, artifacts: materials });
  return input;
}
