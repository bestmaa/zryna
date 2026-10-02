import { connect } from './client.mjs';
import { createController } from './controller.mjs';
import { corpus } from './corpus.mjs';
import { isI32, limits, utf8 } from './limits.mjs';
import { readArguments, renderArguments, renderDiagnostics } from './presentation.mjs';

export function consumeCapability(location, history) {
  const capability = location.hash.slice(1);
  history.replaceState(null, '', `${location.pathname}${location.search}`);
  if (!/^[a-f0-9]{64}$/.test(capability)) throw new Error('PLAYGROUND-CAPABILITY');
  return capability;
}

export function mountPlayground(document, controller) {
  const elements = Object.fromEntries(['preset', 'source', 'byte-count', 'compile', 'cancel',
    'status', 'diagnostic-empty', 'diagnostics', 'export', 'arguments', 'run', 'observation',
    'identity', 'source-error'].map(id => [id, document.getElementById(id)]));
  const source = elements.source;
  let editSerial = 0;
  let metadata = null;
  let fields = [];
  let pending = null;
  let terminal = false;
  let validSource = true;

  function controls() {
    const busy = pending !== null || controller.busy;
    elements.compile.disabled = terminal || busy || !validSource;
    elements.cancel.disabled = terminal || !pending || pending.cancelled;
    elements.export.disabled = terminal || busy || !metadata;
    elements.run.disabled = terminal || busy || !metadata;
    for (const { input } of fields) input.disabled = terminal || busy;
    source.disabled = terminal;
    elements.preset.disabled = terminal;
  }

  function clearObservation() {
    elements.observation.textContent = 'No observation.';
  }

  function clearCompilation() {
    metadata = null;
    fields = [];
    elements.export.replaceChildren();
    elements.arguments.replaceChildren();
    elements.diagnostics.replaceChildren();
    elements['diagnostic-empty'].textContent = 'Compile this source revision to receive diagnostics.';
    elements.identity.textContent = '';
    clearObservation();
  }

  function edit() {
    editSerial++;
    clearCompilation();
    controller.cancel();
    validSource = false;
    try {
      const count = utf8(source.value).length;
      elements['byte-count'].textContent = `${count} / ${limits.source} UTF-8 bytes`;
      controller.edit(source.value);
      validSource = count <= limits.source;
      elements['source-error'].textContent = validSource ? '' :
        `Source exceeds ${limits.source} UTF-8 bytes. Remove ${count - limits.source} ` +
        `${count - limits.source === 1 ? 'byte' : 'bytes'} before compiling.`;
    } catch (error) {
      elements['byte-count'].textContent = `Invalid Unicode / ${limits.source} UTF-8 bytes`;
      elements['source-error'].textContent = `Source cannot be compiled: ${error.message}`;
    }
    source.setAttribute('aria-invalid', String(!validSource));
    elements.status.textContent = pending ? 'Source edited. Cancelling the previous operation…' :
      'Source edited. Compile this revision.';
    controls();
  }

  function selectExport(initial = []) {
    const entry = metadata?.exports.find(value => value.logical === elements.export.value);
    fields = entry ? renderArguments(document, elements.arguments, entry.arity, initial) : [];
    clearObservation();
    for (const { input } of fields) input.addEventListener('input', () => {
      clearObservation();
      readArguments(fields);
    });
    controls();
  }

  function publishCompilation(result) {
    renderDiagnostics(document, elements.diagnostics, result.report, source);
    elements['diagnostic-empty'].textContent = result.report.diagnostics.length ? '' :
      (result.status === 'unavailable' ? 'No source diagnostics were produced.' : 'No compiler diagnostics.');
    if (result.status === 'unavailable') {
      elements.status.textContent = `Compiler unavailable: ${result.failure.code} — ${result.failure.message}`;
      return;
    }
    if (result.status === 'rejected') {
      elements.status.textContent = 'Compilation rejected. Review the compiler diagnostics.';
      return;
    }
    metadata = result;
    for (const entry of result.exports) {
      const option = document.createElement('option');
      option.value = entry.logical;
      option.textContent = `${entry.logical} (${entry.arity} arguments)`;
      elements.export.append(option);
    }
    const preset = corpus.find(value => value.source === source.value && value.export);
    const entry = result.exports.find(value => value.logical === preset?.export) ?? result.exports[0];
    elements.export.value = entry.logical;
    selectExport(preset?.cases?.[0]?.args ?? []);
    elements.identity.textContent = `Source revision ${result.revision}; source SHA-256 ${result.sourceSha256}; ` +
      `component SHA-256 ${result.identity.componentSha256}`;
    elements.status.textContent = 'Compiled. Choose an export and run it.';
  }

  function current(operation) {
    return pending === operation && !operation.cancelled && operation.editSerial === editSerial &&
      operation.revision === controller.revision;
  }

  async function operate(kind, work, publish) {
    if (pending || controller.busy || terminal) return;
    const operation = { kind, revision: controller.revision, editSerial, cancelled: false };
    pending = operation;
    elements.status.textContent = kind === 'compile' ? 'Compiling source…' : 'Running export…';
    controls();
    try {
      const result = await work();
      if (!current(operation)) return;
      if (result.revision !== operation.revision) throw new Error('PLAYGROUND-STALE');
      publish(result);
    } catch (error) {
      if (error.message === 'PLAYGROUND-HOST-TEARDOWN') {
        terminal = true;
        clearCompilation();
        elements.status.textContent = 'Compiler session ended because cleanup could not be confirmed. Restart the playground.';
      } else if (current(operation)) {
        const message = `${kind === 'compile' ? 'Compilation' : 'Evaluation'} failed: ${error.message}`;
        elements.status.textContent = message;
        if (kind === 'run') elements.observation.textContent = message;
      }
    } finally {
      if (pending === operation) {
        pending = null;
        if (!terminal && operation.editSerial !== editSerial) {
          elements.status.textContent = 'Source edited. Compile this revision.';
        } else if (!terminal && operation.cancelled) {
          elements.status.textContent = 'Cancelled. The previous operation has finished cleanup.';
        }
        controls();
      }
    }
  }

  async function compile() {
    if (!validSource || pending || controller.busy || terminal) return;
    clearCompilation();
    await operate('compile', () => controller.compile(), publishCompilation);
  }

  async function run() {
    if (!metadata || pending || controller.busy || terminal) return;
    const entry = metadata.exports.find(value => value.logical === elements.export.value);
    const args = readArguments(fields, true);
    clearObservation();
    if (!args) {
      elements.observation.textContent = 'Check the highlighted argument values before running.';
      return;
    }
    const componentSha256 = metadata.identity.componentSha256;
    await operate('run', () => controller.evaluate(entry.logical, args), result => {
      if (result.logical !== entry.logical || result.componentSha256 !== componentSha256 || !isI32(result.value)) {
        throw new Error('PLAYGROUND-OBSERVATION');
      }
      elements.observation.textContent = `${result.logical}(${args.join(', ')}) = ${result.value} (signed i32)`;
      elements.identity.textContent = `Source revision ${result.revision}; source SHA-256 ${metadata.sourceSha256}; ` +
        `observed component SHA-256 ${result.componentSha256}`;
      elements.status.textContent = 'Evaluation completed.';
    });
  }

  for (const example of corpus) {
    const option = document.createElement('option');
    option.value = example.id;
    option.textContent = example.id.replaceAll('-', ' ');
    elements.preset.append(option);
  }
  elements.preset.value = corpus[0].id;
  source.value = corpus[0].source;
  source.addEventListener('input', edit);
  source.addEventListener('keydown', event => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      void compile();
    }
  });
  elements.preset.addEventListener('change', () => {
    const example = corpus.find(value => value.id === elements.preset.value);
    if (!example) return;
    source.value = example.source;
    edit();
    source.focus();
  });
  elements.export.addEventListener('change', () => selectExport());
  elements.compile.addEventListener('click', () => void compile());
  elements.run.addEventListener('click', () => void run());
  elements.cancel.addEventListener('click', () => {
    if (!pending || pending.cancelled) return;
    pending.cancelled = true;
    controller.cancel();
    clearObservation();
    elements.status.textContent = 'Cancelling. Waiting for cleanup…';
    controls();
  });
  edit();
  return Object.freeze({ compile, run });
}

async function start() {
  const status = document.getElementById('status');
  try {
    const capability = consumeCapability(window.location, window.history);
    const options = await connect(capability);
    mountPlayground(document, createController(options));
  } catch (error) {
    status.textContent = `Compiler connection unavailable: ${error.message}`;
  }
}

if (typeof document !== 'undefined') void start();
