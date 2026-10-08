//! Exact protocol-v4 source closure; uses the existing bounded, no-follow capture.

use cap_std::fs::Dir;
use zryna_diagnostics::Diagnostic;

use super::{CapturedFile, MAX_WORKER_BYTES, capture_file, execution_error, open_dir};

pub(in super::super) struct V4Module {
    pub(in super::super) path: &'static str,
    pub(in super::super) directory: &'static str,
    pub(in super::super) name: &'static str,
    expected: &'static [u8],
}

macro_rules! v4_module {
    ($directory:literal, $name:literal) => {
        V4Module {
            path: concat!("v4/", $directory, "/", $name, ".mjs"),
            directory: concat!("v4/", $directory),
            name: concat!($name, ".mjs"),
            expected: include_bytes!(concat!(
                "../../../../../../adapters/typescript-6/src/v4/",
                $directory,
                "/",
                $name,
                ".mjs"
            )),
        }
    };
}

pub(in super::super) const V4_MODULES: [V4Module; 19] = [
    v4_module!("boundary", "configuration"),
    v4_module!("boundary", "dispatch"),
    v4_module!("boundary", "errors"),
    v4_module!("boundary", "request"),
    v4_module!("boundary", "transport"),
    v4_module!("syntax", "constructions"),
    v4_module!("syntax", "data-declarations"),
    v4_module!("syntax", "diagnostics"),
    v4_module!("syntax", "expression-arena"),
    v4_module!("syntax", "expressions"),
    v4_module!("syntax", "functions"),
    v4_module!("syntax", "imports"),
    v4_module!("syntax", "matches"),
    v4_module!("syntax", "names"),
    v4_module!("syntax", "source"),
    v4_module!("syntax", "spans"),
    v4_module!("syntax", "statements"),
    v4_module!("syntax", "tokens"),
    v4_module!("syntax", "types"),
];

pub(super) fn capture(
    root: &Dir,
    prefix: &[&str],
    worker: &CapturedFile,
    limits: &CapturedFile,
) -> Result<Vec<CapturedFile>, Diagnostic> {
    if worker.bytes != include_bytes!("../../../../../../adapters/typescript-6/src/worker-v4.mjs")
        || limits.bytes
            != include_bytes!("../../../../../../adapters/typescript-6/src/limits-v4.mjs")
    {
        return Err(execution_error("protocol-v4 tooling worker differs from this tooling build"));
    }
    let source = open_dir(root, prefix)?;
    let mut total = worker.bytes.len();
    let mut modules = Vec::with_capacity(V4_MODULES.len());
    for module in &V4_MODULES {
        let components = module.path.split('/').collect::<Vec<_>>();
        let file = capture_file(&source, &components, MAX_WORKER_BYTES)?;
        if file.bytes != module.expected {
            return Err(execution_error(
                "protocol-v4 tooling worker differs from this tooling build",
            ));
        }
        total = total
            .checked_add(file.bytes.len())
            .ok_or_else(|| execution_error("tooling executable closure byte count overflowed"))?;
        if total > MAX_WORKER_BYTES {
            return Err(execution_error("tooling executable source exceeds its byte limit"));
        }
        modules.push(file);
    }
    Ok(modules)
}
