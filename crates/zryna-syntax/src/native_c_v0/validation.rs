use std::collections::BTreeSet;

use super::{
    DecodeError,
    raw::{AbiType, DeclarationSet, Owner},
    require,
};

fn bound(length: usize, maximum: usize, metric: &'static str) -> Result<(), DecodeError> {
    require(length <= maximum, "ZRYNA-C4107", metric)
}

fn text(value: &str, maximum: usize, metric: &'static str) -> Result<(), DecodeError> {
    bound(value.len(), maximum, metric)?;
    require(!value.is_empty() && value.is_ascii(), "ZRYNA-C4101", "ascii-string")
}

fn name(value: &str) -> Result<(), DecodeError> {
    text(value, 128, "name-bytes")?;
    require(
        value.as_bytes()[0].is_ascii_alphabetic() || value.starts_with('_'),
        "ZRYNA-C4101",
        "name",
    )?;
    require(
        value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
        "ZRYNA-C4101",
        "name",
    )
}

fn key(value: &str) -> Result<(), DecodeError> {
    text(value, 257, "key-bytes")?;
    require(
        value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_/@.-".contains(&byte)),
        "ZRYNA-C4101",
        "key",
    )
}

fn path(value: &str) -> Result<(), DecodeError> {
    text(value, 256, "source-path-bytes")?;
    require(
        value.strip_suffix(".zry").is_some_and(|prefix| {
            !prefix.is_empty()
                && prefix.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_/-".contains(&byte))
        }),
        "ZRYNA-C4101",
        "source-path",
    )
}

fn digest(value: &str) -> Result<(), DecodeError> {
    require(
        value.len() == 64
            && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "ZRYNA-C4101",
        "sha256",
    )
}

fn slots(values: &[u8], maximum: usize, range: u8) -> Result<(), DecodeError> {
    bound(values.len(), maximum, "slot-count")?;
    require(
        values.iter().all(|value| *value < range)
            && values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "ZRYNA-C4101",
        "slots",
    )
}

pub(super) fn check(declarations: &DeclarationSet) -> Result<(), DecodeError> {
    require(
        declarations.format == "zryna.native-c-declarations.v0"
            && declarations.version == 0
            && declarations.abi == "zryna-native-c-interop-v0"
            && declarations.convention == "sysv-amd64-c-v0"
            && declarations.carriers == "native-c-interop-v0-carriers"
            && declarations.ownership == "native-c-interop-v0-resources"
            && declarations.runtime_contract == "zryna-ownership-runtime-v1",
        "ZRYNA-C4101",
        "contract-tuple",
    )?;
    bound(declarations.sources.len(), 256, "sources")?;
    bound(declarations.libraries.len(), 16, "libraries")?;
    bound(declarations.operations.len(), 256, "operations")?;
    bound(declarations.sites.len(), 4096, "sites")?;
    require(
        !declarations.sources.is_empty() && !declarations.operations.is_empty(),
        "ZRYNA-C4101",
        "empty-set",
    )?;
    for source in &declarations.sources {
        path(&source.path)?;
        digest(&source.sha256)?;
    }
    for library in &declarations.libraries {
        check_library(library)?;
    }
    for operation in &declarations.operations {
        check_operation(operation)?;
    }
    for site in &declarations.sites {
        check_site(site)?;
    }
    Ok(())
}

fn check_library(library: &super::raw::Library) -> Result<(), DecodeError> {
    text(&library.id, 128, "library-id-bytes")?;
    text(&library.version, 128, "library-version-bytes")?;
    let parts = library.id.split('@').collect::<Vec<_>>();
    require(
        parts.len() == 2
            && !parts[0].is_empty()
            && !parts[1].is_empty()
            && parts[0].as_bytes()[0].is_ascii_alphabetic()
            && parts[0].bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            && parts[1].bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
            && library
                .version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte)),
        "ZRYNA-C4101",
        "library-id-or-version",
    )?;
    digest(&library.header_sha256)?;
    digest(&library.policy_sha256)?;
    bound(library.kinds.len(), 16, "kinds")?;
    bound(library.allocators.len(), 16, "allocators")?;
    require(
        library.kinds.iter().collect::<BTreeSet<_>>().len() == library.kinds.len(),
        "ZRYNA-C4101",
        "duplicate-kind",
    )?;
    for kind in &library.kinds {
        key(kind)?;
    }
    for allocator in &library.allocators {
        for value in [&allocator.id, &allocator.kind, &allocator.create, &allocator.release] {
            key(value)?;
        }
    }
    Ok(())
}

fn check_operation(operation: &super::raw::Operation) -> Result<(), DecodeError> {
    key(&operation.key)?;
    key(&operation.library)?;
    name(&operation.logical_name)?;
    name(&operation.symbol)?;
    bound(operation.parameters.len(), 16, "parameters")?;
    bound(operation.resources.len(), 8, "resources")?;
    bound(operation.statuses.len(), 16, "statuses")?;
    require(
        matches!(operation.result, AbiType::CI32 | AbiType::CInt | AbiType::Bool32 | AbiType::Unit),
        "ZRYNA-C4101",
        "result-carrier",
    )?;
    for parameter in &operation.parameters {
        name(&parameter.name)?;
        require(
            parameter.abi != AbiType::Unit && parameter.resource.is_none_or(|index| index < 8),
            "ZRYNA-C4101",
            "parameter-carrier-or-resource",
        )?;
    }
    for resource in &operation.resources {
        for value in [&resource.kind, &resource.allocator, &resource.release] {
            key(value)?;
        }
        slots(&resource.slots, 4, 16)?;
        require(
            !resource.slots.is_empty()
                && resource.owner_before != Owner::Consumed
                && resource.max_bytes.is_none_or(|bytes| bytes <= 4096)
                && resource.expected_length_slot.is_none_or(|slot| slot < 16),
            "ZRYNA-C4101",
            "resource-shape",
        )?;
    }
    for status in &operation.statuses {
        require(
            status.code <= 2_147_483_647 && status.preserves_inputs,
            "ZRYNA-C4101",
            "status-shape",
        )?;
        slots(&status.initialized, 16, 16)?;
        slots(&status.new_owners, 8, 8)?;
    }
    path(&operation.source_binding.path)?;
    digest(&operation.source_binding.sha256)?;
    Ok(())
}

fn check_site(site: &super::raw::Site) -> Result<(), DecodeError> {
    if let Some(operation) = &site.operation {
        key(operation)?;
    }
    path(&site.path)?;
    digest(&site.source_sha256)?;
    bound(site.spelling.len(), 4096, "primitive-spelling-bytes")?;
    require(
        site.spelling.strip_prefix("Ffi.").is_some_and(|suffix| {
            suffix.split_once('(').is_some_and(|(name, _)| {
                !name.is_empty()
                    && name.as_bytes()[0].is_ascii_alphabetic()
                    && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
        }),
        "ZRYNA-C4101",
        "primitive-spelling",
    )?;
    Ok(())
}
