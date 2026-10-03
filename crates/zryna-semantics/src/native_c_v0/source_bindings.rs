use super::{DeclarationError, identity::sha256, require};
use std::collections::{BTreeMap, BTreeSet};
use zryna_source::Span;
use zryna_syntax::{
    native_c_source_v0::{
        AuthenticatedForeignSources,
        raw::{ExpressionKind, StatementKind, Type},
    },
    native_c_v0::raw::{AbiType, DeclarationSet, Direction, Mode, Primitive},
};

fn carrier(carrier: AbiType) -> Option<Type> {
    match carrier {
        AbiType::CI32 | AbiType::CInt => Some(Type::I32),
        AbiType::Bool32 => Some(Type::Bool),
        _ => None,
    }
}

pub(super) fn check(
    document: &DeclarationSet,
    syntax: &AuthenticatedForeignSources,
) -> Result<Vec<Span>, DeclarationError> {
    require(document.sources.len() == syntax.files().len(), "ZRYNA-C4106", "exact-source-set")?;
    let operations: BTreeMap<_, _> =
        document.operations.iter().map(|operation| (operation.key.as_str(), operation)).collect();
    let mut actual_sites = Vec::new();
    let mut source_digests = BTreeMap::new();
    for (claimed, file) in document.sources.iter().zip(syntax.files()) {
        require(claimed.path == file.path(), "ZRYNA-C4106", "source-path-identity")?;
        let text = syntax
            .source_text(file.file_id())
            .ok_or(DeclarationError { code: "ZRYNA-C4106", detail: "source-file" })?;
        let digest = sha256(text.as_bytes());
        require(digest == claimed.sha256, "ZRYNA-C4106", "source-bytes")?;
        source_digests.insert(file.file_id(), digest);
        for site in file.sites() {
            let function = &file.functions()[site.function_index()];
            let expression = &function.expressions[site.expression_index()];
            let ExpressionKind::Intrinsic(_, args) = &expression.kind else {
                return Err(DeclarationError { code: "ZRYNA-C4106", detail: "intrinsic-node" });
            };
            if let Some(key) = site.operation() {
                let operation = operations
                    .get(key)
                    .ok_or(DeclarationError { code: "ZRYNA-C4106", detail: "unknown-operation" })?;
                require(
                    operation.direction == Direction::Import,
                    "ZRYNA-C4106",
                    "import-intrinsic",
                )?;
                match site.primitive() {
                    Primitive::RawCall => require(
                        args.len() == operation.parameters.len() + 1,
                        "ZRYNA-C4104",
                        "raw-arity",
                    )?,
                    Primitive::Release => require(
                        operation.mode == Mode::Void && operation.parameters.len() == 1,
                        "ZRYNA-C4105",
                        "release-call",
                    )?,
                    Primitive::ForeignError => {
                        require(operation.mode == Mode::Status, "ZRYNA-C4105", "error-call")?;
                    }
                    _ => {
                        return Err(DeclarationError {
                            code: "ZRYNA-C4106",
                            detail: "operation-primitive",
                        });
                    }
                }
            }
            if site.primitive() == Primitive::OutHandle {
                let ExpressionKind::Key(kind) = &function.expressions[args[0]].kind else {
                    return Err(DeclarationError { code: "ZRYNA-C4106", detail: "literal-kind" });
                };
                require(
                    document.libraries.iter().any(|library| {
                        library.allocators.iter().any(|allocator| {
                            allocator.category == zryna_syntax::native_c_v0::raw::Category::Handle
                                && allocator.kind == *kind
                        })
                    }),
                    "ZRYNA-C4105",
                    "unknown-handle-kind",
                )?;
            }
            actual_sites.push((file, site));
        }
    }
    require(
        actual_sites.len() == document.sites.len(),
        "ZRYNA-C4106",
        "complete-primitive-site-set",
    )?;
    for ((file, site), claim) in actual_sites.iter().zip(&document.sites) {
        let span = site.span();
        let text = syntax
            .source_text(file.file_id())
            .ok_or(DeclarationError { code: "ZRYNA-C4106", detail: "source-file" })?;
        require(
            claim.path == file.path()
                && source_digests.get(&file.file_id()) == Some(&claim.source_sha256)
                && claim.start == u64::from(span.start())
                && claim.end == u64::from(span.end())
                && claim.primitive == site.primitive()
                && claim.safety == site.safety()
                && claim.operation.as_deref() == site.operation()
                && text.get(span.start() as usize..span.end() as usize)
                    == Some(claim.spelling.as_str()),
            "ZRYNA-C4106",
            "parsed-primitive-site",
        )?;
    }
    bind_operations(document, syntax, &source_digests)
}

fn bind_operations(
    document: &DeclarationSet,
    syntax: &AuthenticatedForeignSources,
    source_digests: &BTreeMap<zryna_source::FileId, String>,
) -> Result<Vec<Span>, DeclarationError> {
    let mut spans = Vec::new();
    let mut exports = BTreeSet::new();
    for (ordinal, operation) in document.operations.iter().enumerate() {
        let binding = &operation.source_binding;
        let file = syntax
            .files()
            .iter()
            .find(|file| file.path() == binding.path)
            .ok_or(DeclarationError { code: "ZRYNA-C4106", detail: "binding-source" })?;
        require(
            binding.ordinal == ordinal as u64
                && source_digests.get(&file.file_id()) == Some(&binding.sha256),
            "ZRYNA-C4106",
            "operation-source-identity",
        )?;
        let span = if operation.direction == Direction::Import {
            file.sites()
                .iter()
                .find(|site| {
                    u64::from(site.span().start()) == binding.start
                        && u64::from(site.span().end()) == binding.end
                        && site.operation() == Some(operation.key.as_str())
                        && matches!(site.primitive(), Primitive::RawCall | Primitive::Release)
                })
                .ok_or(DeclarationError { code: "ZRYNA-C4106", detail: "parsed-import-binding" })?
                .span()
        } else {
            let function = file
                .functions()
                .iter()
                .find(|function| {
                    function.exported
                        && u64::from(function.range.start) == binding.start
                        && u64::from(function.range.end) == binding.end
                        && function.name == operation.logical_name
                })
                .ok_or(DeclarationError { code: "ZRYNA-C4106", detail: "parsed-export-binding" })?;
            require(
                function.parameters.len() == operation.parameters.len()
                    && function
                        .parameters
                        .iter()
                        .zip(&operation.parameters)
                        .all(|(source, declaration)| carrier(declaration.abi) == Some(source.ty))
                    && carrier(operation.result) == Some(function.result),
                "ZRYNA-C4104",
                "export-source-signature",
            )?;
            // Exclude effects and non-scalar source constructs. Scalar expression names and types
            // still require a separately verified body; this syntactic restriction is no body seal.
            require(
                function.statements.len() == 1
                    && matches!(function.statements[0].kind, StatementKind::Return(_))
                    && function.expressions.iter().all(|expression| {
                        matches!(
                            expression.kind,
                            ExpressionKind::I32(_)
                                | ExpressionKind::Bool(_)
                                | ExpressionKind::Local(_)
                                | ExpressionKind::Add(_, _)
                        )
                    }),
                "ZRYNA-C4104",
                "export-source-surface",
            )?;
            require(
                exports.insert((file.file_id(), function.range.start, function.range.end)),
                "ZRYNA-C4102",
                "duplicate-export-binding",
            )?;
            syntax
                .span(file.file_id(), function.range)
                .map_err(|_| DeclarationError { code: "ZRYNA-C4106", detail: "export-span" })?
        };
        spans.push(span);
    }
    for file in syntax.files() {
        for function in file.functions().iter().filter(|function| function.exported) {
            require(
                exports.contains(&(file.file_id(), function.range.start, function.range.end)),
                "ZRYNA-C4106",
                "unbound-export",
            )?;
        }
    }
    Ok(spans)
}
