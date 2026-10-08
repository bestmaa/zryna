//! Carry the fixed v4 module closure through the existing private staging protocol.

use zryna_diagnostics::Diagnostic;

use super::{
    super::capture::{CapturedToolingClosure, V4_MODULES},
    ROOT, ToolingStage, create_directory, stage_changed, stage_file,
};

pub(super) const V4: &str = "v4";
pub(super) const BOUNDARY: &str = "v4/boundary";
pub(super) const SYNTAX: &str = "v4/syntax";

pub(super) fn stage(
    stage: &mut ToolingStage,
    captured: &CapturedToolingClosure,
) -> Result<(), Diagnostic> {
    if captured.v4_modules.len() != V4_MODULES.len() {
        return Err(stage_changed());
    }
    create_directory(&mut stage.directories, ROOT, "v4", V4)?;
    create_directory(&mut stage.directories, V4, "boundary", BOUNDARY)?;
    create_directory(&mut stage.directories, V4, "syntax", SYNTAX)?;
    for (module, file) in V4_MODULES.iter().zip(&captured.v4_modules) {
        stage_file(&stage.directories, &mut stage.files, module.directory, module.name, file)?;
    }
    Ok(())
}
