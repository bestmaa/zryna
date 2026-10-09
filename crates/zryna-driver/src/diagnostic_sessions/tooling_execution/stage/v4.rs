//! Carry the fixed v4 module closure through the existing private staging protocol.

use zryna_diagnostics::Diagnostic;

use super::{
    super::capture::{CapturedToolingClosure, CapturedV4, V4_MODULES, V4Layout},
    ROOT, ToolingStage, create_directory, stage_changed, stage_file,
};

pub(super) const V4: &str = "v4";
pub(super) const BOUNDARY: &str = "v4/boundary";
pub(super) const SYNTAX: &str = "v4/syntax";

pub(super) fn layout(captured: &CapturedV4) -> Result<V4Layout, Diagnostic> {
    match captured {
        CapturedV4::Legacy => Ok(V4Layout::Legacy),
        CapturedV4::Modular(modules) if modules.len() == V4_MODULES.len() => Ok(V4Layout::Modular),
        CapturedV4::Modular(_) => Err(stage_changed()),
    }
}

pub(super) fn stage(
    stage: &mut ToolingStage,
    captured: &CapturedToolingClosure,
) -> Result<(), Diagnostic> {
    let CapturedV4::Modular(modules) = &captured.v4 else { return Ok(()) };
    create_directory(&mut stage.directories, ROOT, "v4", V4)?;
    create_directory(&mut stage.directories, V4, "boundary", BOUNDARY)?;
    create_directory(&mut stage.directories, V4, "syntax", SYNTAX)?;
    for (module, file) in V4_MODULES.iter().zip(modules) {
        stage_file(&stage.directories, &mut stage.files, module.directory, module.name, file)?;
    }
    Ok(())
}
