use super::{DeclarationError, LibraryMaterial, MAX_MATERIAL_BYTES, require};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use zryna_syntax::native_c_v0::raw::{DeclarationSet, Direction};

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn declaration_digest(bytes: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"ZRYNA-NATIVE-C-DECLARATION-V0\0");
    digest.update(bytes);
    digest.finalize().into()
}

pub(super) fn canonical(mut value: Value) -> Result<Vec<u8>, DeclarationError> {
    value.sort_all_objects();
    let mut bytes = serde_json::to_vec(&value)
        .map_err(|_| DeclarationError { code: "ZRYNA-C4101", detail: "policy-encoding" })?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn ordered(
    values: impl IntoIterator<Item = impl AsRef<str>>,
    detail: &'static str,
) -> Result<(), DeclarationError> {
    let mut previous: Option<String> = None;
    for value in values {
        let value = value.as_ref();
        require(previous.as_deref().is_none_or(|before| before < value), "ZRYNA-C4102", detail)?;
        previous = Some(value.to_owned());
    }
    Ok(())
}

pub(super) fn policy_bytes(
    document: &DeclarationSet,
    library: &str,
) -> Result<Vec<u8>, DeclarationError> {
    let library = document
        .libraries
        .iter()
        .find(|item| item.id == library)
        .ok_or(DeclarationError { code: "ZRYNA-C4102", detail: "library-identity" })?;
    let mut operations = Vec::new();
    for operation in document.operations.iter().filter(|operation| operation.library == library.id)
    {
        let mut value = serde_json::to_value(operation)
            .map_err(|_| DeclarationError { code: "ZRYNA-C4101", detail: "operation-encoding" })?;
        value
            .as_object_mut()
            .ok_or(DeclarationError { code: "ZRYNA-C4101", detail: "operation-object" })?
            .remove("sourceBinding");
        operations.push(value);
    }
    canonical(
        serde_json::json!({"allocators": library.allocators, "kinds": library.kinds, "operations": operations}),
    )
}

pub(super) fn check(
    document: &DeclarationSet,
    materials: &[LibraryMaterial<'_>],
) -> Result<(), DeclarationError> {
    ordered(document.sources.iter().map(|source| &source.path), "source-order")?;
    ordered(document.libraries.iter().map(|library| &library.id), "library-order")?;
    ordered(document.operations.iter().map(|operation| &operation.key), "operation-order")?;
    require(
        materials.len() == document.libraries.len(),
        "ZRYNA-C4102",
        "exact-library-material-set",
    )?;
    let mut material_ids = BTreeSet::new();
    for material in materials {
        require(
            material_ids.insert(material.library_id),
            "ZRYNA-C4102",
            "duplicate-library-material",
        )?;
        require(
            material.header_bytes.len() <= MAX_MATERIAL_BYTES
                && material.policy_bytes.len() <= MAX_MATERIAL_BYTES,
            "ZRYNA-C4107",
            "library-material-bytes",
        )?;
        require(
            document.libraries.iter().any(|library| library.id == material.library_id),
            "ZRYNA-C4102",
            "unknown-library-material",
        )?;
    }
    for library in &document.libraries {
        let (name, version) = library
            .id
            .split_once('@')
            .ok_or(DeclarationError { code: "ZRYNA-C4102", detail: "library-version" })?;
        require(
            !name.is_empty() && version == library.version && !version.contains('@'),
            "ZRYNA-C4102",
            "library-version",
        )?;
        ordered(&library.kinds, "kind-order")?;
        ordered(library.allocators.iter().map(|allocator| &allocator.id), "allocator-order")?;
        let material = materials
            .iter()
            .find(|material| material.library_id == library.id)
            .ok_or(DeclarationError { code: "ZRYNA-C4102", detail: "missing-library-material" })?;
        require(
            sha256(material.header_bytes) == library.header_sha256,
            "ZRYNA-C4102",
            "header-digest",
        )?;
        let projected = policy_bytes(document, &library.id)?;
        // Compare complete independently captured bytes, not just a producer's matching digest.
        require(
            material.policy_bytes == projected
                && sha256(material.policy_bytes) == library.policy_sha256,
            "ZRYNA-C4102",
            "policy-material-or-digest",
        )?;
    }
    let mut symbols = BTreeSet::new();
    for operation in &document.operations {
        require(
            symbols.insert(operation.symbol.to_ascii_lowercase()),
            "ZRYNA-C4102",
            "symbol-collision",
        )?;
        if operation.direction == Direction::Import {
            require(
                document.libraries.iter().any(|library| library.id == operation.library)
                    && operation.key == format!("{}/{}", operation.library, operation.symbol),
                "ZRYNA-C4102",
                "import-library",
            )?;
        } else {
            require(
                !document.libraries.iter().any(|library| library.id == operation.library)
                    && operation.key == format!("{}/{}", operation.library, operation.logical_name),
                "ZRYNA-C4102",
                "export-module-key",
            )?;
            require(
                operation.symbol == format!("zryna_c_v0_e_{}", operation.logical_name),
                "ZRYNA-C4102",
                "export-symbol",
            )?;
        }
    }
    Ok(())
}
