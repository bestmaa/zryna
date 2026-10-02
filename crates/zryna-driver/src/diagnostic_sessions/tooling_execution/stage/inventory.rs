use std::collections::BTreeSet;

use cap_std::fs::Dir;
use zryna_diagnostics::Diagnostic;

use super::{MODULES, OLD, OLD_LIB, ROOT, SCOPE, WRAPPER, WRAPPER_LIB, stage_changed};

pub(super) fn validate_inventory(key: &str, directory: &Dir) -> Result<(), Diagnostic> {
    let expected: BTreeSet<String> = match key {
        ROOT => [
            "node_modules",
            "worker.mjs",
            "worker-v3.mjs",
            "limits-v3.mjs",
            "worker-v4.mjs",
            "limits-v4.mjs",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect(),
        MODULES => ["@typescript"].map(str::to_owned).into_iter().collect(),
        SCOPE => ["old", "typescript6"].map(str::to_owned).into_iter().collect(),
        WRAPPER | OLD => ["lib", "package.json"].map(str::to_owned).into_iter().collect(),
        WRAPPER_LIB | OLD_LIB => ["typescript.js"].map(str::to_owned).into_iter().collect(),
        _ => return Err(stage_changed()),
    };
    let actual = directory
        .entries()
        .map_err(|_| stage_changed())?
        .map(|entry| {
            entry
                .map_err(|_| stage_changed())?
                .file_name()
                .into_string()
                .map_err(|_| stage_changed())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual == expected { Ok(()) } else { Err(stage_changed()) }
}
