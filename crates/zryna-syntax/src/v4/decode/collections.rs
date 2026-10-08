use super::*;

fn bounded<'de, D, T, const MAX: usize>(d: D, label: &'static str) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct B<T, const MAX: usize>(&'static str, PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for B<T, MAX> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "at most {MAX} {}", self.0)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX));
            while out.len() < MAX {
                match seq.next_element()? {
                    Some(v) => out.push(v),
                    None => return Ok(out),
                }
            }
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom(format_args!("{} exceeds limit {MAX}", self.0)));
            }
            Ok(out)
        }
    }
    d.deserialize_seq(B::<T, MAX>(label, PhantomData))
}
macro_rules! bounded_fn {
    ($name:ident, $ty:ty, $max:expr, $label:literal) => {
        pub(in crate::v4) fn $name<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<$ty>, D::Error> {
            bounded::<D, $ty, $max>(d, $label)
        }
    };
}
bounded_fn!(files, RawSourceUnit, MAX_SOURCE_FILES, "files");
bounded_fn!(diagnostics, RawProviderDiagnostic, MAX_PROVIDER_DIAGNOSTICS, "diagnostics");
bounded_fn!(imports, RawImportSyntax, MAX_IMPORTS_PER_MODULE, "imports");
bounded_fn!(bindings, RawImportBindingSyntax, MAX_IMPORTED_NAMES_PER_DECLARATION, "bindings");
bounded_fn!(types, RawTypeSyntax, MAX_TYPE_NODES_PER_MODULE, "type nodes");
bounded_fn!(
    declarations,
    RawDataDeclaration,
    MAX_DATA_DECLARATIONS_PER_MODULE,
    "data declarations"
);
bounded_fn!(fields, RawDataField, MAX_MEMBERS_PER_DECLARATION, "fields");
bounded_fn!(variants, RawEnumVariant, MAX_MEMBERS_PER_DECLARATION, "variants");
bounded_fn!(functions, RawFunctionSyntax, MAX_FUNCTIONS_PER_MODULE, "functions");
bounded_fn!(parameters, RawParameterSyntax, MAX_PARAMETERS_PER_FUNCTION, "parameters");
bounded_fn!(blocks, RawBlockSyntax, MAX_BLOCKS_PER_FUNCTION, "blocks");
bounded_fn!(statements, RawStatementSyntax, MAX_STATEMENTS_PER_FUNCTION, "statements");
bounded_fn!(statement_ids, u32, MAX_STATEMENTS_PER_FUNCTION, "statement ids");
bounded_fn!(expressions, RawExpressionSyntax, MAX_EXPRESSIONS_PER_FUNCTION, "expressions");
bounded_fn!(arguments, u32, MAX_PARAMETERS_PER_FUNCTION, "arguments");
bounded_fn!(
    initializers,
    RawFieldInitializer,
    MAX_INITIALIZERS_PER_CONSTRUCTION,
    "field initializers"
);
bounded_fn!(elements, u32, MAX_ELEMENTS_PER_CONSTRUCTION, "elements");
bounded_fn!(arms, RawMatchArm, MAX_MATCH_ARMS_PER_EXPRESSION, "match arms");
