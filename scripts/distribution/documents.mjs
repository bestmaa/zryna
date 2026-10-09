import { requireValue } from './canonical.mjs';
import { targetPaths } from './inventory.mjs';

export function installationDocuments(version, target) {
  requireValue(version === '0.2.3', 'unapproved documentation version');
  targetPaths(target);
  const windows = target === 'x86_64-pc-windows-msvc';
  const command = windows ? '& $zryna' : '"$zryna"';
  const selectCompiler = windows ? "$zryna = 'C:\\path\\to\\installation\\bin\\zryna.exe'"
    : "zryna='/absolute/path/to/installation/bin/zryna'";
  const shell = windows ? 'powershell' : 'sh';
  const platform = windows ? 'Windows Server 2022 x86-64' : 'Ubuntu 24.04 x86-64';
  const readme = `# Zryna ${version} beta

Target platform: ${platform}.

Authenticate this complete archive using the release verification procedure before running
any executable. Keep every file under this directory together when moving the installation.
The compiler supplies its own Node.js runtime and TypeScript provider. A source checkout,
Rust, pnpm and a separate Node.js installation are not needed for JavaScript or core WebAssembly.

Use a working directory outside the installation. Replace the compiler path below with the
absolute path to the extracted executable, then run:

\`\`\`${shell}
${selectCompiler}
${command} --version
${command} new hello
${command} build src/main.zry --project-root hello --target javascript
${command} run src/main.zry --project-root hello --target javascript --export main
${command} build src/main.zry --project-root hello --target webassembly --name main-wasm
${command} run src/main.zry --project-root hello --target webassembly --name main-wasm --export main
\`\`\`

The generated program returns 42. Outputs are written under the project's .zryna/out directory.
Build and run use separate bundle names; choose a new --name when a bundle already exists.
An explicit --project-root selects project sources, manifests and frozen package inputs.
Installed commands reject compiler-root and Node.js overrides.

Omitting --profile selects i32-v1. Existing control-flow-v1 and data-ownership-v1 projects
use the matching --profile option and must declare that profile in their package compatibility
record. Package resolution stays frozen; changing a project profile also requires a matching
lockfile. No additional language syntax or dependency-package imports are enabled by installation.

Linux native execution additionally requires the supported external GNU toolchain. Windows native
execution is unsupported. JavaScript and core WebAssembly are the portable installed targets.

A newly compiled H1 review candidate also supports the one-file command route:
${command} run pure.zry --project-root command-project --target wasi-command --profile command-h1-v1 --export main
Use a real source directory outside the installation, no --root/--node overrides, and an explicit
absolute owner-private --grant-file only for the one-key environment lookup. Existing frozen
package compatibility does not grant command authority or permit H1 dependency imports.
H1 remains under acceptance/review; existing published archives are not retroactively changed.

See SUPPORT.md for reporting problems. LICENSE, NOTICE and licenses/ retain the notices for this
compiler and its bundled materials. metadata/ records exact file, source and material identities.
`;
  const support = `# Support

This is the Zryna ${version} beta for ${platform}.

Report reproducible compiler or installation problems at https://github.com/zryna/zryna/issues.
Include the release version, target, diagnostic code, exact command and a minimal project.
Remove secrets and private paths before sharing project files or command output.
For security reports, follow https://github.com/zryna/zryna/security/policy.

If installation admission reports changed or missing files, stop using that installation.
Obtain and independently verify the complete archive again. Do not repair individual metadata
files or replace the bundled Node.js executable or provider modules.

Keep projects outside the entire installation directory.
The installation is relocatable as one complete directory; compiler and provider files must
remain unchanged. Project outputs belong to the selected project's .zryna directory.
`;
  return [
    { path: 'README.md', mode: 0o644, data: Buffer.from(readme, 'utf8') },
    { path: 'SUPPORT.md', mode: 0o644, data: Buffer.from(support, 'utf8') },
    { path: 'VERSION', mode: 0o644, data: Buffer.from(`${version}\n`, 'ascii') },
  ];
}
