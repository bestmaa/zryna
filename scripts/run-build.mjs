import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runPreflight, validatePreflightCommands } from './run-preflight.mjs';

// Reuse the same size, formatting and boundary authorities before compilation.
export const BUILD_COMMANDS = Object.freeze([
  ...['repository-structure', 'rust-format', 'architecture-check'].map(id =>
    validatePreflightCommands().find(command => command.id === id)),
  Object.freeze({ id: 'rust-workspace-build', executable: 'cargo',
    args: Object.freeze(['build', '--locked', '--workspace']) }),
]);

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 2) throw new Error('usage: run-build.mjs (no gate-skipping options)');
    runPreflight(BUILD_COMMANDS);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
