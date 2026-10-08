//! Exact protocol-v4 source closure; uses the existing bounded, no-follow capture.

use cap_std::fs::Dir;
use zryna_diagnostics::Diagnostic;

use super::{CapturedFile, MAX_WORKER_BYTES, capture_file, execution_error, open_dir};

// Exact worker previously pinned by this build and shipped in v0.2.3; installed only.
const LEGACY_WORKER_SHA256: [u8; 32] = [
    0x80, 0xce, 0xea, 0x20, 0xee, 0x79, 0x53, 0xa1, 0xeb, 0x8f, 0x85, 0xa4, 0x72, 0x40, 0x75, 0xa6,
    0xec, 0x80, 0x19, 0x83, 0xce, 0xb1, 0x6e, 0xe8, 0x37, 0x52, 0x31, 0x3b, 0xb3, 0x74, 0x0a, 0x8a,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum V4Layout {
    Legacy,
    Modular,
}

pub(in super::super) enum CapturedV4 {
    Legacy,
    Modular(Vec<CapturedFile>),
}

impl CapturedV4 {
    pub(in super::super) fn modules(&self) -> &[CapturedFile] {
        match self {
            Self::Legacy => &[],
            Self::Modular(modules) => modules,
        }
    }
}

pub(super) fn capture_installed(
    bootstrap: &Dir,
    worker: &CapturedFile,
    limits: &CapturedFile,
) -> Result<CapturedV4, Diagnostic> {
    verify_limits(limits)?;
    if worker.sha256 == LEGACY_WORKER_SHA256 {
        // A legacy worker never imports a module tree. Reject mixed forms, even empty trees.
        return match bootstrap.symlink_metadata("v4") {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(CapturedV4::Legacy),
            _ => Err(execution_error("protocol-v4 installed tooling closure mixes worker forms")),
        };
    }
    capture(bootstrap, &[], worker, limits)
}

fn verify_limits(limits: &CapturedFile) -> Result<(), Diagnostic> {
    if limits.bytes != include_bytes!("../../../../../../adapters/typescript-6/src/limits-v4.mjs") {
        return Err(execution_error("protocol-v4 tooling worker differs from this tooling build"));
    }
    Ok(())
}

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
) -> Result<CapturedV4, Diagnostic> {
    verify_limits(limits)?;
    if worker.bytes != include_bytes!("../../../../../../adapters/typescript-6/src/worker-v4.mjs") {
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
    Ok(CapturedV4::Modular(modules))
}
