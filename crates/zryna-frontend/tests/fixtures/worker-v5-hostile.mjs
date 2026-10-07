// Independent hostile transport, rooted in the frozen authored DTO rather than the producer.
import { createInterface } from 'node:readline';
import { readFileSync, appendFileSync } from 'node:fs';
const [mode, fixture, seen] = process.argv.slice(2);
const info = { provider: 'typescript-6', provider_version: '6.0.3', protocol_version: 5,
  capabilities: { module_resolution: false, semantic_diagnostics: false,
    control_flow_v1: true, data_ownership_syntax_v1: true, bounded_generics_syntax_v1: true } };
const send = value => process.stdout.write(JSON.stringify(value) + '\n');
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (seen) appendFileSync(seen, request.method + '\n');
  if (request.method === 'handshake') {
    if (mode === 'identity') info.provider = 'foreign';
    if (mode === 'version') info.provider_version = '6.0.2';
    if (mode === 'protocol') info.protocol_version = 4;
    if (mode === 'capability') info.capabilities.bounded_generics_syntax_v1 = false;
    if (mode === 'extra-capability') info.capabilities.compiler_authority = true;
    if (mode === 'missing-capability') delete info.capabilities.bounded_generics_syntax_v1;
    send({ id: request.id, result: info });
    continue;
  }
  const result = JSON.parse(readFileSync(fixture, 'utf8'));
  if (mode === 'span') result.files[0].functions[0].span.end -= 1;
  if (mode === 'unknown') result.files[0].functions[0].compiler_authority = true;
  if (mode === 'missing-nullable') delete result.files[1].functions[0].type_parameters;
  if (mode === 'wrong-schema') result.schema_version = 4;
  if (mode === 'provider-reject') { send({ id: request.id, error: { code: 'ZRYNA-F2002', message: 'unsupported source' } }); continue; }
  if (mode === 'invalid-utf8') { process.stdout.write(Buffer.from([0xff, 0x0a])); continue; }
  if (mode === 'timeout') { await new Promise(resolve => setTimeout(resolve, 30_000)); continue; }
  const response = { id: mode === 'wrong-id' ? 999 : request.id, result };
  let wire = JSON.stringify(response);
  if (mode === 'duplicate') wire = wire.replace('"schema_version":5', '"schema_version":5,"schema_version":5');
  if (mode === 'nested-duplicate') wire = wire.replace('"type_parameters":null', '"type_parameters":null,"type_parameters":null');
  if (mode === 'trailing-value') wire += '{}';
  process.stdout.write(wire + '\n');
  if (mode === 'extra-response') send(response);
  if (mode === 'exit') process.exitCode = 3;
  if (mode === 'stderr') process.stderr.write('x'.repeat(8192));
}
