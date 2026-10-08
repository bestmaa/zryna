import process from 'node:process';
import { once } from 'node:events';
import { TextDecoder } from 'node:util';
import {
  maxRequestBytes,
  maxResponseBytes,
} from './configuration.mjs';
import {
  handle,
} from './dispatch.mjs';
import {
  AdapterError,
  failRequest,
} from './errors.mjs';
import {
  isRequestId,
  rejectDuplicateObjectKeys,
} from './request.mjs';
import {
  compactText,
} from '../syntax/diagnostics.mjs';

function errorResponse(id, error) {
  return { id: isRequestId(id) ? id : null, error: { code: error instanceof AdapterError ? error.code : 'ZRYNA-F1003', message: compactText(error instanceof Error ? error.message : String(error)) } };
}

async function writeResponse(response) {
  let serialized = JSON.stringify(response);
  if (Buffer.byteLength(serialized, 'utf8') > maxResponseBytes) serialized = JSON.stringify(errorResponse(response?.id, new AdapterError('ZRYNA-F1002', 'response exceeds the byte limit')));
  if (!process.stdout.write(`${serialized}\n`)) await once(process.stdout, 'drain');
}

async function processLine(bytes) {
  let request;
  try {
    let text;
    try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
    catch { failRequest('request is not valid UTF-8'); }
    if (!text.trim()) return;
    rejectDuplicateObjectKeys(text);
    try { request = JSON.parse(text); }
    catch { failRequest('request is not valid JSON'); }
    await writeResponse(handle(request));
  } catch (error) {
    await writeResponse(errorResponse(request?.id, error));
  }
}

async function runWorker() {
  let lineParts = [];
  let lineBytes = 0;
  let discardingOversizedLine = false;
  for await (const chunk of process.stdin) {
    let cursor = 0;
    while (cursor < chunk.length) {
      const newline = chunk.indexOf(0x0a, cursor);
      const end = newline === -1 ? chunk.length : newline;
      const part = chunk.subarray(cursor, end);
      if (!discardingOversizedLine) {
        if (lineBytes + part.length > maxRequestBytes) {
          lineParts = [];
          lineBytes = 0;
          discardingOversizedLine = true;
        } else if (part.length > 0) {
          lineParts.push(part);
          lineBytes += part.length;
        }
      }
      if (newline === -1) break;
      if (discardingOversizedLine) await writeResponse(errorResponse(null, new AdapterError('ZRYNA-F1002', 'request exceeds the byte limit')));
      else await processLine(Buffer.concat(lineParts, lineBytes));
      lineParts = [];
      lineBytes = 0;
      discardingOversizedLine = false;
      cursor = newline + 1;
    }
  }
  if (discardingOversizedLine) await writeResponse(errorResponse(null, new AdapterError('ZRYNA-F1002', 'request exceeds the byte limit')));
  else if (lineBytes > 0) await processLine(Buffer.concat(lineParts, lineBytes));
}

export {
  runWorker
};
