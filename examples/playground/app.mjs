import { corpus, parseI32 } from './corpus.mjs';
import { createRunner } from './controller.mjs';

const form = document.querySelector('#run-form');
const preset = document.querySelector('#preset');
const first = document.querySelector('#first');
const second = document.querySelector('#second');
const output = document.querySelector('#output');
const runButton = document.querySelector('#run');
const cancelButton = document.querySelector('#cancel');
const runner = createRunner();
let pending = false;

for (const [index, entry] of corpus.entries()) {
  const option = document.createElement('option');
  option.value = String(index);
  option.textContent = `${entry.name}: add(${entry.args.join(', ')}) → ${entry.expected}`;
  preset.append(option);
}

function selectPreset() {
  const entry = corpus[Number(preset.value)];
  if (!entry) return;
  first.value = String(entry.args[0]);
  second.value = String(entry.args[1]);
  output.textContent = `Ready: ${entry.name}`;
}

function setPending(value) {
  pending = value;
  runButton.disabled = value;
  cancelButton.disabled = !value;
}

preset.addEventListener('change', selectPreset);
for (const input of [first, second]) input.addEventListener('input', () => {
  preset.value = '';
});

form.addEventListener('submit', async event => {
  event.preventDefault();
  if (pending) return;
  let args;
  try { args = [parseI32(first.value), parseI32(second.value)]; }
  catch (error) { output.textContent = error.message; return; }
  setPending(true);
  output.textContent = 'Running the fixed browser component…';
  try {
    const result = await runner.run(args);
    const entry = corpus.find(item => item.args[0] === args[0] && item.args[1] === args[1]);
    output.textContent = `add(${args.join(', ')}) = ${result.value}\n` +
      (entry ? `Fixed corpus: ${result.value === entry.expected ? 'match' : 'MISMATCH'}\n` :
        'Custom scalar arguments\n') +
      `Component SHA-256: ${result.componentSha256}`;
  } catch (error) { output.textContent = error.message; }
  finally { setPending(false); }
});

cancelButton.addEventListener('click', () => {
  if (runner.stop()) output.textContent = 'Cancelled; worker terminated';
});

selectPreset();
