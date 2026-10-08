//! Audited source for the Linux x86-64 `OwnershipRuntimeAbiV1` object.

use std::{collections::BTreeSet, fmt::Write as _};

pub(crate) const SOURCE: &[u8] = include_bytes!("../../../runtime/native/ownership_runtime_v1.c");

const LAYOUT_MARKER: &str = "/* ZRYNA_RT_O1_ELEMENT_LAYOUT_CASES */";

pub(crate) fn render_source(
    program: &zryna_native_mir::data_ownership_v1::VerifiedMirModule,
) -> Vec<u8> {
    let mut elements = BTreeSet::new();
    for ty in program.types() {
        if ty.category() == zryna_native_mir::data_ownership_v1::raw::TypeCategory::Vec
            && let Some(element) = ty.referenced_type()
        {
            elements.insert(element);
        }
    }
    let layouts = elements.into_iter().filter_map(|id| {
        program.types().find(|ty| ty.id() == id).and_then(|ty| {
            align_up(ty.size(), ty.alignment()).map(|stride| (id, stride, ty.alignment()))
        })
    });
    let mut source = b"#define ZRYNA_M3_OBSERVATION 1\n".to_vec();
    source.extend(render_layouts(layouts));
    source
}

fn render_layouts(layouts: impl IntoIterator<Item = (u32, u64, u64)>) -> Vec<u8> {
    let mut cases = String::new();
    for (id, stride, alignment) in layouts {
        writeln!(
            cases,
            "  case {id}U: *stride = UINT64_C({stride}); *alignment = {alignment}U; return 1;"
        )
        .expect("write to String");
    }
    std::str::from_utf8(SOURCE)
        .expect("checked runtime source is UTF-8")
        .replacen(LAYOUT_MARKER, &cases, 1)
        .into_bytes()
}

/// Exact checked runtime implementation with element cases from the retained native C issuer.
/// Rendering these required bytes grants no runtime object, native recipe or host execution.
pub(crate) fn render_native_c_source(
    program: &zryna_native_mir::native_c_v0::VerifiedMirProgram,
) -> Vec<u8> {
    let target = program.source().native_layouts().target();
    render_layouts(
        program
            .source()
            .runtime_abi()
            .element_layouts()
            .filter(|element| element.target() == target)
            .map(|element| (element.element().index(), element.stride(), element.alignment())),
    )
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    value.checked_add(alignment.checked_sub(1)?).map(|value| value & !(alignment - 1))
}

#[cfg(test)]
mod tests;
