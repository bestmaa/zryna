"""Resolve the requested pnpm Node entry in local or pinned setup-action layouts."""
import json
from pathlib import Path


def resolve(shim, home=None):
    shim = Path(shim).absolute()
    candidates = [shim.resolve()]
    for parent in (shim.parent.parent/'pnpm/bin',):
        candidates.extend(parent/name for name in ('pnpm.mjs','pnpm.cjs'))
    if home is not None:
        managed = Path(home)/'global/v11'
        if managed.is_dir():
            roots = list(managed.iterdir())
            assert len(roots) <= 64, 'unexpected package manager installation inventory'
            for root in roots:
                candidates.extend(root/'node_modules/pnpm/bin'/name for name in ('pnpm.mjs','pnpm.cjs'))
    selected = set()
    for entry in candidates:
        if not entry.is_file() or entry.name not in ('pnpm.mjs','pnpm.cjs'):
            continue
        entry = entry.resolve(strict=True)
        manifest = entry.parent.parent/'package.json'
        if not manifest.is_file():
            continue
        value = json.loads(manifest.read_bytes())
        if (value.get('name') == 'pnpm' and value.get('version') == '11.18.0'
                and value.get('bin',{}).get('pnpm') == entry.relative_to(manifest.parent).as_posix()):
            selected.add(entry)
    assert len(selected) == 1, 'exact requested pnpm Node entry is missing or ambiguous'
    return selected.pop()
