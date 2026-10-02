use zryna_source::FileId;

/// Original module identity, branded by its immutable source map.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModuleIdentity {
    source: FileId,
}

impl ModuleIdentity {
    pub(super) const fn new(source: FileId) -> Self {
        Self { source }
    }

    /// Returns the authoritative source file for this module.
    #[must_use]
    pub const fn source_file(self) -> FileId {
        self.source
    }

    /// Returns the source-map-ordered module index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.source.index()
    }
}

/// Source declaration kind; this is not a closed type or instance kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DeclarationKind {
    /// Original nominal struct declaration.
    Struct,
    /// Original nominal enum declaration.
    Enum,
    /// Original function declaration.
    Function,
}

/// Original declaration identity with separate function and data source indices.
///
/// Identities and contexts cannot be constructed by a caller.
/// ```compile_fail
/// use zryna_semantics::bounded_generics_v1::{DeclarationContext, DeclarationIdentity};
/// let identity = DeclarationIdentity { module: todo!(), kind: todo!(), source_index: 0 };
/// let context = DeclarationContext { input: todo!(), modules: Vec::new() };
/// ```
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeclarationIdentity {
    module: ModuleIdentity,
    kind: DeclarationKind,
    source_index: u32,
}

impl DeclarationIdentity {
    pub(super) fn new(module: ModuleIdentity, kind: DeclarationKind, source_index: usize) -> Self {
        Self {
            module,
            kind,
            source_index: u32::try_from(source_index).expect("bounded original declaration index"),
        }
    }

    /// Returns the issuing original module.
    #[must_use]
    pub const fn module(self) -> ModuleIdentity {
        self.module
    }

    /// Returns the original declaration kind.
    #[must_use]
    pub const fn kind(self) -> DeclarationKind {
        self.kind
    }

    /// Returns the source index among data declarations or among functions.
    #[must_use]
    pub const fn source_index(self) -> u32 {
        self.source_index
    }
}

/// Type parameter identity confined to one original declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeParameterIdentity {
    declaration: DeclarationIdentity,
    index: u32,
}

impl TypeParameterIdentity {
    pub(super) fn new(declaration: DeclarationIdentity, index: usize) -> Self {
        Self { declaration, index: u32::try_from(index).expect("bounded type parameter index") }
    }

    /// Returns the owning original declaration.
    #[must_use]
    pub const fn declaration(self) -> DeclarationIdentity {
        self.declaration
    }

    /// Returns the parameter's source-order index within its owner.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
