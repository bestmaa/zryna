//! Immutable open-document overlays for the authenticated ownership module closure.

use std::time::Duration;

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_frontend::VerifiedFrontendProviderV4;
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

use crate::{
    MAX_MODULE_DISCOVERY_WALL_TIME, ModuleClosureError, WorkspaceSourceRoot,
    source_session::{ModuleSourceRoot, ModuleSourceSession},
    workspace_source::{StableSource, WorkspaceSourceSession},
};

use super::{VerifiedOwnershipModuleClosure, discover_with_clock_limit};

struct OverlayRoot<'a> {
    root: &'a WorkspaceSourceRoot,
    overlays: &'a SourceMap,
}

struct OverlaySession<'a> {
    disk: WorkspaceSourceSession<'a>,
    overlays: &'a SourceMap,
}

impl ModuleSourceRoot for OverlayRoot<'_> {
    type Session<'root>
        = OverlaySession<'root>
    where
        Self: 'root;

    fn begin(&self) -> Result<Self::Session<'_>, Diagnostic> {
        Ok(OverlaySession { disk: self.root.begin_discovery()?, overlays: self.overlays })
    }
}

impl ModuleSourceSession for OverlaySession<'_> {
    fn read_source(&mut self, path: &NormalizedSourcePath) -> Result<StableSource, Diagnostic> {
        if let Some(id) = self.overlays.file_id(path) {
            let source = self.overlays.source(id).ok_or_else(overlay_changed)?;
            let sha256: [u8; 32] = Sha256::digest(source.text().as_bytes()).into();
            Ok(StableSource { text: source.text().to_owned(), sha256 })
        } else {
            self.disk.read_source(path)
        }
    }

    fn validate_provider_batch(&mut self, sources: &SourceMap) -> Result<(), Diagnostic> {
        let mut disk = Vec::new();
        for raw in 0..sources.len() {
            let id = sources
                .verify_file_id(u32::try_from(raw).map_err(|_| overlay_changed())?)
                .map_err(|_| overlay_changed())?;
            let source = sources.source(id).ok_or_else(overlay_changed)?;
            if let Some(overlay_id) = self.overlays.file_id(source.path()) {
                let overlay = self.overlays.source(overlay_id).ok_or_else(overlay_changed)?;
                if source.text() != overlay.text() {
                    return Err(overlay_changed());
                }
            } else {
                disk.push(SourceFileInput {
                    path: source.path().as_str().to_owned(),
                    text: source.text().to_owned(),
                });
            }
        }
        if disk.is_empty() {
            self.disk.revalidate_all()
        } else {
            let map = SourceMap::build(disk).map_err(|_| overlay_changed())?;
            self.disk.validate_provider_batch(&map)
        }
    }

    fn revalidate_all(&mut self) -> Result<(), Diagnostic> {
        self.disk.revalidate_all()
    }
}

fn overlay_changed() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-D3302",
        None,
        "open-document overlay differs from the admitted source authority",
        "retry from one immutable open-document revision",
    )
}

/// Discovers a bounded M3 graph from exact open buffers and authenticated saved imports.
///
/// An open buffer shadows its saved file. Every unopened import uses the retained, no-follow
/// workspace source session. The final source map contains only reachable modules.
///
/// # Errors
/// Rejects changed disk authority, unsafe paths, invalid imports, provider failure or budgets.
pub fn discover_ownership_module_closure_with_overlays<
    Provider: VerifiedFrontendProviderV4 + ?Sized,
>(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
    overlays: &SourceMap,
    frontend: &Provider,
) -> Result<VerifiedOwnershipModuleClosure, ModuleClosureError> {
    discover_ownership_module_closure_with_overlays_bounded(
        root,
        entrypoint,
        overlays,
        frontend,
        MAX_MODULE_DISCOVERY_WALL_TIME,
    )
}

pub(crate) fn discover_ownership_module_closure_with_overlays_bounded<
    Provider: VerifiedFrontendProviderV4 + ?Sized,
>(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
    overlays: &SourceMap,
    frontend: &Provider,
    limit: Duration,
) -> Result<VerifiedOwnershipModuleClosure, ModuleClosureError> {
    if overlays.file_id(&entrypoint).is_none() {
        return Err(ModuleClosureError::Rejected(vec![overlay_changed()]));
    }
    discover_with_clock_limit(
        &OverlayRoot { root, overlays },
        entrypoint,
        frontend,
        std::time::Instant::now,
        limit,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        ffi::OsString,
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use zryna_frontend::{ProviderExpectationV4, WorkerFrontendV4, WorkerLimitsV4, WorkerSpecV4};

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn frontend() -> WorkerFrontendV4 {
        let output =
            crate::process_spawn::output(Command::new("node").args(["-p", "process.execPath"]))
                .expect("node path");
        assert!(output.status.success());
        let node = PathBuf::from(String::from_utf8(output.stdout).expect("node UTF-8").trim());
        WorkerFrontendV4::new(
            WorkerSpecV4::new(
                node,
                vec![OsString::from("src/worker-v4.mjs")],
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters/typescript-6"),
                ProviderExpectationV4::new("typescript-6", "6.0.3").expect("provider"),
                WorkerLimitsV4::default(),
            )
            .expect("worker"),
        )
    }

    #[test]
    fn open_entry_uses_saved_import_and_never_reads_saved_entry() {
        let path = env::temp_dir().join(format!(
            "zryna-m3-overlay-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("fixture root");
        fs::write(path.join("main.zry"), "invalid saved entry").expect("saved entry");
        fs::write(
            path.join("math.zry"),
            "export function double(value: i32): i32 { return value + value; }",
        )
        .expect("saved import");
        let root = WorkspaceSourceRoot::capture(&path).expect("captured root");
        let overlays = SourceMap::build(vec![SourceFileInput {
            path: "main.zry".to_owned(),
            text: "import { double } from \"./math.zry\"; export function score(value: i32): i32 { return double(value); }".to_owned(),
        }]).expect("overlay");
        let entry = NormalizedSourcePath::new("main.zry").expect("entry");
        let closure = discover_ownership_module_closure_with_overlays(
            &root,
            entry.clone(),
            &overlays,
            &frontend(),
        )
        .expect("complete closure");
        assert_eq!(closure.sources().len(), 2);
        closure.lower_data_ownership_v1().expect("verified semantics");
        let original = closure
            .sources()
            .source(closure.sources().file_id(&entry).expect("entry id"))
            .expect("entry source");
        assert_eq!(
            original.text(),
            overlays
                .source(overlays.file_id(&entry).expect("overlay id"))
                .expect("overlay source")
                .text()
        );
        drop(closure);
        drop(root);
        fs::remove_dir_all(path).expect("fixture cleanup");
    }
}
