//! Private reuse of the captured scalar frontend by the separate browser compiler facade.

use zryna_source::SourceMap;

use super::ToolingCompiler;
use crate::{SourceToIrError, SourceToIrSuccess};

impl ToolingCompiler {
    pub(crate) fn compile_scalar_source(
        &self,
        sources: &SourceMap,
    ) -> Result<SourceToIrSuccess, SourceToIrError> {
        self.node.revalidate().map_err(|error| SourceToIrError::Rejected(vec![error]))?;
        self.execution.revalidate().map_err(|error| SourceToIrError::Rejected(vec![error]))?;
        let result = crate::compile_to_verified_ir(&self.frontend, sources);
        self.execution.revalidate().map_err(|error| SourceToIrError::Rejected(vec![error]))?;
        self.node.revalidate().map_err(|error| SourceToIrError::Rejected(vec![error]))?;
        result
    }
}
