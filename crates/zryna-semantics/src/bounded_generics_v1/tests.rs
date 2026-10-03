pub(super) mod body_fixtures;
mod fixtures;
mod identity;
mod modules;
mod scopes;

use super::{DeclarationContext, SemanticInput, resolve_declarations};
use fixtures::Project;
use zryna_diagnostics::Diagnostic;
use zryna_source::NormalizedSourcePath;

fn context<'a>(
    project: &'a Project,
    entry: &str,
) -> Result<DeclarationContext<'a>, Vec<Diagnostic>> {
    let entry = project
        .sources
        .file_id(&NormalizedSourcePath::new(entry).expect("entry path"))
        .expect("original fixture entry");
    resolve_declarations(
        SemanticInput::try_new(&project.syntax, &project.sources, entry)
            .expect("genuine original syntax and source"),
    )
}

fn source_text(project: &Project, span: zryna_source::Span) -> &str {
    let source = project.sources.source(span.file()).expect("original span file");
    &source.text()[span.start() as usize..span.end() as usize]
}

fn assert_error(project: &Project, entry: &str, code: &str, token: &str) {
    let errors = context(project, entry).expect_err("no partial declaration context");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code(), code);
    assert_eq!(
        source_text(project, errors[0].primary_span().expect("original error token")),
        token
    );
}
