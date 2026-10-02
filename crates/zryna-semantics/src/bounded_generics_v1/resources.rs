use zryna_diagnostics::Diagnostic;
use zryna_source::MAX_SOURCE_FILES;
use zryna_syntax::v4;

use super::SemanticInput;

#[derive(Clone, Copy, Debug)]
pub(super) enum Metric {
    Modules,
    SourceBytes,
    ImportsPerModule,
    Imports,
    NamesPerImport,
    ImportedNames,
    DataPerModule,
    Data,
    FunctionsPerModule,
    Functions,
    TypesPerModule,
    Types,
}

impl Metric {
    pub(super) const fn limit(self) -> usize {
        match self {
            Self::Modules => MAX_SOURCE_FILES,
            Self::SourceBytes => v4::MAX_AGGREGATE_SOURCE_BYTES,
            Self::ImportsPerModule => v4::MAX_IMPORTS_PER_MODULE,
            Self::Imports => v4::MAX_IMPORTS_PER_PROJECT,
            Self::NamesPerImport => v4::MAX_IMPORTED_NAMES_PER_DECLARATION,
            Self::ImportedNames => v4::MAX_IMPORTED_NAMES_PER_PROJECT,
            Self::DataPerModule => v4::MAX_DATA_DECLARATIONS_PER_MODULE,
            Self::Data => v4::MAX_DATA_DECLARATIONS_PER_PROJECT,
            Self::FunctionsPerModule => v4::MAX_FUNCTIONS_PER_MODULE,
            Self::Functions => v4::MAX_FUNCTIONS_PER_PROJECT,
            Self::TypesPerModule => v4::MAX_TYPE_NODES_PER_MODULE,
            Self::Types => v4::MAX_TYPE_NODES_PER_PROJECT,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Modules => "modules",
            Self::SourceBytes => "source bytes",
            Self::ImportsPerModule => "imports per module",
            Self::Imports => "project imports",
            Self::NamesPerImport => "names per import",
            Self::ImportedNames => "project imported names",
            Self::DataPerModule => "data declarations per module",
            Self::Data => "project data declarations",
            Self::FunctionsPerModule => "functions per module",
            Self::Functions => "project functions",
            Self::TypesPerModule => "type occurrences per module",
            Self::Types => "project type occurrences",
        }
    }
}

#[derive(Debug)]
pub(super) struct LimitFailure {
    metric: Metric,
    count: Option<usize>,
}

impl LimitFailure {
    pub(super) fn diagnostic(self) -> Diagnostic {
        let count = self.count.map_or_else(|| "checked-add overflow".into(), |n| n.to_string());
        Diagnostic::error(
            "ZRYNA-M7201",
            None,
            format!("{} limit {}; rejected count {count}", self.metric.name(), self.metric.limit()),
            "reduce the complete authenticated declaration graph",
        )
    }
}

pub(super) fn add(total: usize, amount: usize, metric: Metric) -> Result<usize, LimitFailure> {
    let count = total.checked_add(amount);
    match count {
        Some(value) if value <= metric.limit() => Ok(value),
        _ => Err(LimitFailure { metric, count }),
    }
}

pub(super) fn preflight(input: SemanticInput<'_>) -> Result<(), LimitFailure> {
    add(0, input.syntax().files().len(), Metric::Modules)?;
    let (mut bytes, mut imports, mut names, mut data, mut functions, mut types) =
        (0, 0, 0, 0, 0, 0);
    for unit in input.syntax().files() {
        let source = input
            .sources()
            .verify_file_id(unit.id)
            .ok()
            .and_then(|id| input.sources().source(id))
            .expect("authenticated source file");
        bytes = add(bytes, source.text().len(), Metric::SourceBytes)?;
        add(0, unit.imports.len(), Metric::ImportsPerModule)?;
        imports = add(imports, unit.imports.len(), Metric::Imports)?;
        for import in &unit.imports {
            add(0, import.bindings.len(), Metric::NamesPerImport)?;
            names = add(names, import.bindings.len(), Metric::ImportedNames)?;
        }
        add(0, unit.data_declarations.len(), Metric::DataPerModule)?;
        data = add(data, unit.data_declarations.len(), Metric::Data)?;
        add(0, unit.functions.len(), Metric::FunctionsPerModule)?;
        functions = add(functions, unit.functions.len(), Metric::Functions)?;
        add(0, unit.type_syntax.len(), Metric::TypesPerModule)?;
        types = add(types, unit.type_syntax.len(), Metric::Types)?;
    }
    Ok(())
}
