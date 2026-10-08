import {
  protocolVersion,
  providerVersion,
} from './configuration.mjs';
import {
  failRequest,
} from './errors.mjs';
import {
  isRecord,
  requireExactKeys,
  requireRequestId,
  validateAnalyzeParams,
} from './request.mjs';
import {
  DiagnosticCollector,
} from '../syntax/diagnostics.mjs';
import {
  normalizeSource,
} from '../syntax/source.mjs';

function handle(request) {
  if (!isRecord(request)) failRequest('request must be an object');
  requireRequestId(request.id);
  if (request.method === 'handshake') {
    requireExactKeys(request, ['id', 'method'], 'handshake request');
    return {
      id: request.id,
      result: {
        provider: 'typescript-6', provider_version: providerVersion, protocol_version: protocolVersion,
        capabilities: {
          module_resolution: false, semantic_diagnostics: false,
          control_flow_v1: true, data_ownership_syntax_v1: true,
        },
      },
    };
  }
  if (request.method === 'analyze') {
    requireExactKeys(request, ['id', 'method', 'params'], 'analyze request');
    const files = validateAnalyzeParams(request.params);
    const collector = new DiagnosticCollector();
    const budgets = {
      sourceBytes: 0, imports: 0, bindings: 0, functions: 0, parameters: 0,
      blocks: 0, statements: 0, expressions: 0, locals: 0,
      dataDeclarations: 0, members: 0, types: 0, aggregateOperands: 0, matchArms: 0,
    };
    const normalized = files.map((file, index) => normalizeSource(file, index, collector, budgets));
    return { id: request.id, result: { schema_version: protocolVersion, files: normalized, diagnostics: collector.finish() } };
  }
  failRequest('request method is unsupported');
}

export {
  handle
};
