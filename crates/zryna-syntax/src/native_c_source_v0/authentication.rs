use sha2::{Digest, Sha256};
use zryna_source::{FileId, SourceMap, SourceMapIdentity, Span};

use super::{
    MAX_FILES, MAX_PROJECT_EXPRESSIONS, MAX_PROJECT_FUNCTIONS, MAX_PROJECT_STATEMENTS,
    MAX_SOURCE_BYTES, SourceAuthError, error, limit, parser,
    raw::{self, ExpressionKind},
};
use crate::native_c_v0::raw::{Primitive, Safety};

/// Independently parsed intrinsic bound to a complete immutable source file.
#[derive(Clone, Debug)]
pub struct IntrinsicSite {
    pub(super) span: Span,
    primitive: Primitive,
    operation: Option<String>,
    function: usize,
    expression: usize,
}

impl IntrinsicSite {
    /// Exact source-map-issued call span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// Independently parsed reserved primitive identity.
    #[must_use]
    pub const fn primitive(&self) -> Primitive {
        self.primitive
    }
    /// Safety marker derived from the actual reserved source spelling.
    #[must_use]
    pub const fn safety(&self) -> Safety {
        if matches!(self.primitive, Primitive::RawCall) { Safety::UnsafeRaw } else { Safety::Safe }
    }
    /// Exact parsed operation key for operation-bearing intrinsics.
    #[must_use]
    pub fn operation(&self) -> Option<&str> {
        self.operation.as_deref()
    }
    /// Owning function index within its authenticated file.
    #[must_use]
    pub const fn function_index(&self) -> usize {
        self.function
    }
    /// Intrinsic expression index within its function arena.
    #[must_use]
    pub const fn expression_index(&self) -> usize {
        self.expression
    }
}

/// Complete parsed source file; its fields cannot be supplied by a syntax producer.
#[derive(Clone, Debug)]
pub struct AuthenticatedFile {
    file: FileId,
    path: String,
    sha256: [u8; 32],
    functions: Vec<raw::Function>,
    sites: Vec<IntrinsicSite>,
}

impl AuthenticatedFile {
    /// Exact immutable source-map identity of this file.
    #[must_use]
    pub const fn file_id(&self) -> FileId {
        self.file
    }
    /// Complete portable path, in bytewise project order.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Actual SHA-256 of complete source bytes, computed inside authentication.
    #[must_use]
    pub const fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    /// Complete parsed functions. Cloning syntax cannot construct an authority.
    #[must_use]
    pub fn functions(&self) -> &[raw::Function] {
        &self.functions
    }
    /// Complete independently collected intrinsic inventory, in byte span order.
    #[must_use]
    pub fn sites(&self) -> &[IntrinsicSite] {
        &self.sites
    }
}

/// Opaque complete restricted-source authority, distinct from executable language authority.
#[derive(Clone, Debug)]
pub struct AuthenticatedForeignSources {
    sources: SourceMap,
    files: Vec<AuthenticatedFile>,
}

impl AuthenticatedForeignSources {
    /// Exact immutable source-map identity retained by this authority.
    #[must_use]
    pub const fn source_map_identity(&self) -> SourceMapIdentity {
        self.sources.identity()
    }
    /// Checks the original map identity; identical bytes in a rebuilt map do not suffice.
    #[must_use]
    pub fn belongs_to(&self, sources: &SourceMap) -> bool {
        self.sources.identity() == sources.identity()
    }
    /// Complete bytewise ordered source inventory.
    #[must_use]
    pub fn files(&self) -> &[AuthenticatedFile] {
        &self.files
    }
    /// Returns complete original bytes for an authenticated file, rejecting foreign file ids.
    #[must_use]
    pub fn source_text(&self, file: FileId) -> Option<&str> {
        self.sources.source(file).map(zryna_source::SourceFile::text)
    }
    /// Constructs an exact span through the retained immutable source map.
    ///
    /// # Errors
    /// Rejects foreign file identities and invalid UTF-8 ranges.
    pub fn span(&self, file: FileId, range: raw::Range) -> Result<Span, SourceAuthError> {
        self.sources.span(file, range.start, range.end).map_err(|_| error("source-span", range))
    }
}

/// Parses and authenticates every complete file from an independently captured immutable map.
///
/// No provider AST, site inventory, claimed digest, ambient filesystem or recovery flag enters
/// this constructor. The restricted grammar admits typed functions, initialized `const`, returns,
/// canonical terminal status guards, scalar literals/addition and directly spelled intrinsics.
/// It rejects all other complete-input occupants. Name resolution, type checking, status dominance,
/// token flow, cleanup, totality and arbitrary C behavior remain separate unproved obligations.
///
/// # Errors
/// Rejects unsupported source grammar, paths and exact first-extra source/arena budgets without
/// returning a partial parsed project.
pub fn authenticate_sources(
    sources: &SourceMap,
) -> Result<AuthenticatedForeignSources, SourceAuthError> {
    let origin = raw::Range { start: 0, end: 0 };
    if sources.len() > MAX_FILES {
        return Err(limit("sources", origin));
    }
    let mut bytes = 0;
    let mut functions = 0;
    let mut sites = 0;
    let mut expressions = 0;
    let mut statements = 0;
    let mut files = Vec::new();
    for index in 0..sources.len() {
        let file = sources
            .verify_file_id(u32::try_from(index).map_err(|_| limit("sources", origin))?)
            .map_err(|_| error("source-file", origin))?;
        let source = sources.source(file).ok_or_else(|| error("source-file", origin))?;
        let path = source.path().as_str();
        if !path.is_ascii()
            || path.len() > 256
            || std::path::Path::new(path).extension() != Some(std::ffi::OsStr::new("zry"))
            || !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"_/.-".contains(&b))
        {
            return Err(error("source-path", origin));
        }
        bytes += source.text().len();
        if bytes > MAX_SOURCE_BYTES {
            return Err(limit("aggregate-source-bytes", origin));
        }
        let parsed = parser::parse(source.text())?;
        functions += parsed.len();
        if functions > MAX_PROJECT_FUNCTIONS {
            return Err(limit("project-functions", origin));
        }
        let mut intrinsic_sites = Vec::new();
        for (function_index, function) in parsed.iter().enumerate() {
            expressions += function.expressions.len();
            statements += function.statements.len();
            if expressions > MAX_PROJECT_EXPRESSIONS {
                return Err(limit("project-expressions", function.range));
            }
            if statements > MAX_PROJECT_STATEMENTS {
                return Err(limit("project-statements", function.range));
            }
            validate_range(sources, file, function.range, "function-span")?;
            for expression in &function.expressions {
                validate_range(sources, file, expression.range, "expression-span")?;
            }
            for (expression_index, expression) in function.expressions.iter().enumerate() {
                if let ExpressionKind::Intrinsic(primitive, args) = &expression.kind {
                    if sites == 4096 {
                        return Err(limit("sites", expression.range));
                    }
                    sites += 1;
                    let operation = if matches!(
                        primitive,
                        Primitive::RawCall | Primitive::Release | Primitive::ForeignError
                    ) {
                        match &function.expressions[args[0]].kind {
                            ExpressionKind::Key(key) => Some(key.clone()),
                            _ => return Err(error("literal-operation", expression.range)),
                        }
                    } else {
                        None
                    };
                    let span = sources
                        .span(file, expression.range.start, expression.range.end)
                        .map_err(|_| error("site-span", expression.range))?;
                    intrinsic_sites.push(IntrinsicSite {
                        span,
                        primitive: *primitive,
                        operation,
                        function: function_index,
                        expression: expression_index,
                    });
                }
            }
        }
        intrinsic_sites.sort_by_key(|site| (site.span.start(), site.span.end()));
        files.push(AuthenticatedFile {
            file,
            path: path.to_owned(),
            sha256: Sha256::digest(source.text().as_bytes()).into(),
            functions: parsed,
            sites: intrinsic_sites,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(AuthenticatedForeignSources { sources: sources.clone(), files })
}

// Authentication needs exact map ownership and UTF-8 range validity here, not rendered columns.
// Retained intrinsic sites and the public span issuer still use SourceMap::span unchanged.
fn validate_range(
    sources: &SourceMap,
    file: FileId,
    range: raw::Range,
    detail: &'static str,
) -> Result<(), SourceAuthError> {
    let reject = || error(detail, range);
    let source = sources.source(file).ok_or_else(reject)?;
    let length = u32::try_from(source.text().len()).map_err(|_| reject())?;
    if range.start > range.end || range.end > length {
        return Err(reject());
    }
    let start = usize::try_from(range.start).map_err(|_| reject())?;
    let end = usize::try_from(range.end).map_err(|_| reject())?;
    source.text().get(start..end).ok_or_else(reject)?;
    Ok(())
}

#[cfg(test)]
mod range_tests {
    use super::*;
    use zryna_source::{NormalizedSourcePath, SourceFileInput};

    #[test]
    fn bounds_validation_matches_span_issuance_without_discarding_map_identity() {
        let path = NormalizedSourcePath::new("f.zry").expect("path");
        for text in ["", "a\r\nb", "aé😀\nβ"] {
            let sources =
                SourceMap::build(vec![SourceFileInput { path: "f.zry".into(), text: text.into() }])
                    .expect("source map");
            let file = sources.file_id(&path).expect("issued file");
            for start in 0..=u32::try_from(text.len()).expect("fixture length") + 1 {
                for end in 0..=u32::try_from(text.len()).expect("fixture length") + 1 {
                    let range = raw::Range { start, end };
                    let result = validate_range(&sources, file, range, "expression-span");
                    assert_eq!(
                        result.is_ok(),
                        sources.span(file, start, end).is_ok(),
                        "{text:?} {range:?}"
                    );
                    if let Err(error) = result {
                        assert_eq!(error.code(), "ZRYNA-C4106");
                        assert_eq!(error.detail(), "expression-span");
                        assert_eq!(error.range(), range);
                    }
                }
            }
            let foreign =
                SourceMap::build(vec![SourceFileInput { path: "f.zry".into(), text: text.into() }])
                    .expect("identical bytes with another issuer");
            let range = raw::Range { start: 0, end: 0 };
            assert!(foreign.span(file, 0, 0).is_err());
            assert_eq!(
                validate_range(&foreign, file, range, "function-span")
                    .expect_err("foreign file id")
                    .detail(),
                "function-span"
            );
        }
    }
}
