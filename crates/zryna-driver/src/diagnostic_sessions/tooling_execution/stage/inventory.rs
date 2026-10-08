use std::collections::BTreeSet;

use cap_std::fs::Dir;
use zryna_diagnostics::Diagnostic;

use super::{super::capture::V4_MODULES, v4};
use super::{MODULES, OLD, OLD_LIB, ROOT, SCOPE, WRAPPER, WRAPPER_LIB, stage_changed};

pub(super) fn cleanup_keys() -> impl Iterator<Item = &'static str> {
    [
        "old-runtime",
        "old-manifest",
        "wrapper-runtime",
        "wrapper-manifest",
        "worker-v3",
        "limits-v3",
        "worker-v4",
        "limits-v4",
        "worker",
    ]
    .into_iter()
    .chain(V4_MODULES.iter().map(|module| module.path))
}

pub(super) fn file_key(parent: &str, name: &str) -> Result<&'static str, Diagnostic> {
    let key = match (parent, name) {
        (ROOT, "worker.mjs") => "worker",
        (ROOT, "worker-v3.mjs") => "worker-v3",
        (ROOT, "limits-v3.mjs") => "limits-v3",
        (ROOT, "worker-v4.mjs") => "worker-v4",
        (ROOT, "limits-v4.mjs") => "limits-v4",
        (WRAPPER, "package.json") => "wrapper-manifest",
        (WRAPPER_LIB, "typescript.js") => "wrapper-runtime",
        (OLD, "package.json") => "old-manifest",
        (OLD_LIB, "typescript.js") => "old-runtime",
        _ => {
            return V4_MODULES
                .iter()
                .find(|module| module.directory == parent && module.name == name)
                .map(|module| module.path)
                .ok_or_else(stage_changed);
        }
    };
    Ok(key)
}

pub(super) fn file_name(key: &str) -> &'static str {
    match key {
        "worker" => "worker.mjs",
        "worker-v3" => "worker-v3.mjs",
        "limits-v3" => "limits-v3.mjs",
        "worker-v4" => "worker-v4.mjs",
        "limits-v4" => "limits-v4.mjs",
        "wrapper-manifest" | "old-manifest" => "package.json",
        "wrapper-runtime" | "old-runtime" => "typescript.js",
        _ => V4_MODULES.iter().find(|module| module.path == key).map_or("", |module| module.name),
    }
}

pub(super) fn validate_inventory(key: &str, directory: &Dir) -> Result<(), Diagnostic> {
    let expected: BTreeSet<String> = match key {
        ROOT => [
            "node_modules",
            "worker.mjs",
            "worker-v3.mjs",
            "limits-v3.mjs",
            "worker-v4.mjs",
            "limits-v4.mjs",
            "v4",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect(),
        MODULES => ["@typescript"].map(str::to_owned).into_iter().collect(),
        SCOPE => ["old", "typescript6"].map(str::to_owned).into_iter().collect(),
        WRAPPER | OLD => ["lib", "package.json"].map(str::to_owned).into_iter().collect(),
        WRAPPER_LIB | OLD_LIB => ["typescript.js"].map(str::to_owned).into_iter().collect(),
        v4::V4 => ["boundary", "syntax"].map(str::to_owned).into_iter().collect(),
        v4::BOUNDARY | v4::SYNTAX => V4_MODULES
            .iter()
            .filter(|module| module.directory == key)
            .map(|module| module.name.to_owned())
            .collect(),
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
