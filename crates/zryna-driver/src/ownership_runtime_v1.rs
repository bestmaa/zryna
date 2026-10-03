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
mod tests {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    use std::{
        collections::BTreeSet,
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::SOURCE;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    use super::render_layouts;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    use object::{Object, ObjectSymbol};

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const SYMBOLS: [&str; 17] = [
        "zryna_rt_o1_allocate",
        "zryna_rt_o1_grow",
        "zryna_rt_o1_release",
        "zryna_rt_o1_string_from_utf8_copy",
        "zryna_rt_o1_string_clone",
        "zryna_rt_o1_string_concat",
        "zryna_rt_o1_string_release",
        "zryna_rt_o1_vec_allocate",
        "zryna_rt_o1_vec_reserve",
        "zryna_rt_o1_vec_release_storage",
        "zryna_rt_o1_strong_clone",
        "zryna_rt_o1_weak_downgrade",
        "zryna_rt_o1_weak_clone",
        "zryna_rt_o1_weak_upgrade",
        "zryna_rt_o1_strong_release_begin",
        "zryna_rt_o1_strong_release_finish",
        "zryna_rt_o1_weak_release",
    ];
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const HARNESS: &str = r#"
#include <stdint.h>
#include <string.h>
#include "zryna_ownership_runtime_v1.h"

int main(void) {
  uintptr_t pointer = 0, grown = 0;
  zryna_rt_o1_handle text = {0, 0, 0};
  zryna_rt_o1_handle copy = {0, 0, 0};
  zryna_rt_o1_handle joined = {0, 0, 0};
  zryna_rt_o1_handle rejected = {99, 99, 99};
  uint8_t valid[] = {'o', 'k'};
  uint8_t invalid[] = {0xc0, 0x80};
  uint32_t last = 99, deallocated = 99;
  uint32_t *counts;
  if (zryna_rt_o1_allocate(8, 8, &pointer) != 0 || pointer == 0) return 1;
  if (zryna_rt_o1_release(1, 8, 8) != 255) return 14;
  memset((void *)pointer, 0x5a, 8);
  if (zryna_rt_o1_grow(pointer, 8, 16, 8, &grown) != 0 || grown == 0) return 2;
  if (((uint8_t *)grown)[0] != 0x5a || zryna_rt_o1_release(grown, 16, 8) != 0) return 3;
  if (zryna_rt_o1_allocate(UINT64_C(67108865), 8, &pointer) != 1 || pointer != 0) return 17;
  if (zryna_rt_o1_allocate(8, 3, &pointer) != 255 || pointer != 0) return 18;
  if (zryna_rt_o1_string_from_utf8_copy(invalid, 2, &text) != 4 || text.pointer != 0) return 4;
  if (zryna_rt_o1_string_from_utf8_copy(valid, 2, &text) != 0) return 19;
  if (zryna_rt_o1_string_clone(&text, &text) != 255 || text.length != 2) return 20;
  if (zryna_rt_o1_string_clone(&text, &copy) != 0 || copy.length != 2) return 21;
  if (zryna_rt_o1_string_concat(&text, &copy, &joined) != 0 || joined.length != 4) return 22;
  if (zryna_rt_o1_string_release(&joined) != 0 || zryna_rt_o1_string_release(&copy) != 0 ||
      zryna_rt_o1_string_release(&text) != 0) return 23;
  if (zryna_rt_o1_vec_allocate(7, UINT64_C(1048577), &rejected) != 2 ||
      rejected.pointer != 0 || rejected.length != 0 || rejected.capacity != 0) return 24;
  if (zryna_rt_o1_vec_allocate(7, 2, &text) != 0 || text.pointer == 0 || text.capacity != 2) return 12;
  text.length = 2;
  if (zryna_rt_o1_vec_reserve(7, &text, 2, &copy) != 0 ||
      copy.pointer != text.pointer || copy.length != 2 || copy.capacity != 2) return 25;
  if (zryna_rt_o1_vec_reserve(7, &text, 2, &text) != 255 || text.length != 2) return 26;
  if (zryna_rt_o1_vec_release_storage(7, &text) != 0) return 13;
  if (zryna_rt_o1_allocate(1, 1, &pointer) != 0 || pointer == 0) return 27;
  rejected = (zryna_rt_o1_handle){pointer, 2, 2};
  if (zryna_rt_o1_string_clone(&rejected, &copy) != 255 || copy.pointer != 0) return 28;
  rejected = (zryna_rt_o1_handle){pointer, 1, 1};
  if (zryna_rt_o1_vec_reserve(7, &rejected, 1, &copy) != 255 || copy.pointer != 0) return 29;
  if (zryna_rt_o1_strong_clone(pointer) != 255) return 30;
  if (zryna_rt_o1_release(pointer, 1, 1) != 0) return 31;
  if (zryna_rt_o1_allocate(16, 4, &pointer) != 0) return 5;
  counts = (uint32_t *)pointer;
  counts[0] = 1; counts[1] = 1;
  if (zryna_rt_o1_strong_clone(1) != 255) return 15;
  counts[0] = UINT32_MAX;
  if (zryna_rt_o1_strong_clone(pointer) != 3 || counts[0] != UINT32_MAX) return 16;
  counts[0] = 1;
  if (zryna_rt_o1_strong_clone(pointer) != 0 || counts[0] != 2) return 6;
  if (zryna_rt_o1_weak_downgrade(pointer) != 0 || counts[1] != 2) return 7;
  if (zryna_rt_o1_strong_release_begin(pointer, &last) != 0 || last != 0) return 8;
  if (zryna_rt_o1_strong_release_begin(pointer, &last) != 0 || last != 1) return 9;
  if (zryna_rt_o1_strong_release_finish(pointer) != 0 || counts[1] != 1) return 10;
  if (zryna_rt_o1_weak_release(pointer, &deallocated) != 0 || deallocated != 1) return 11;
  return 0;
}
"#;

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const FAILURE_HARNESS: &str = r#"
#include <stdint.h>
#include "zryna_ownership_runtime_v1.h"

int main(void) {
  uintptr_t pointer = 99;
  zryna_rt_o1_handle vector = {99, 99, 99};
  zryna_rt_o1_handle grown = {88, 88, 88};
  if (zryna_rt_o1_vec_allocate(7, 1, &vector) != 0) return 1;
  vector.length = 1;
  if (zryna_rt_o1_vec_reserve(7, &vector, 2, &grown) != 1) return 2;
  if (grown.pointer != 0 || grown.length != 0 || grown.capacity != 0) return 3;
  if (vector.pointer == 0 || vector.length != 1 || vector.capacity != 1) return 4;
  if (zryna_rt_o1_vec_release_storage(7, &vector) != 0) return 5;
  if (zryna_rt_o1_allocate(8, 8, &pointer) != 0 || pointer == 0) return 6;
  if (zryna_rt_o1_release(pointer, 8, 8) != 0) return 7;
  return 0;
}
"#;

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn runtime_compiles_with_strict_c_and_executes_transitions() {
        compile_and_run(HARNESS, &[], "transitions");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn allocation_failure_is_atomic_and_recoverable() {
        compile_and_run(
            FAILURE_HARNESS,
            &["-DZRYNA_RT_O1_FAIL_ALLOCATION_AT=2"],
            "allocation-failure",
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn universal_capacity_and_native_exhaustion_keep_distinct_statuses() {
        let sequence = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("zryna-runtime-v1-capacity-{}-{sequence}", std::process::id()));
        fs::create_dir(&root).expect("capacity probe directory");
        let executable = root.join("capacity-probe");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/m4-fixtures/allocation-core/native-capacity.c");
        let output = Command::new("/usr/bin/gcc")
            .args(["-std=c11", "-pedantic", "-Wall", "-Wextra", "-Werror", "-O2", "-fno-common"])
            .arg(fixture)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("compile capacity probe");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let status = Command::new(&executable).status().expect("execute capacity probe");
        assert!(status.success(), "capacity probe status {status}");
        fs::remove_dir_all(root).expect("capacity probe cleanup");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn compile_and_run(harness_text: &str, defines: &[&str], label: &str) {
        let sequence = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("zryna-runtime-v1-{label}-{}-{sequence}", std::process::id()));
        fs::create_dir(&root).expect("runtime test directory");
        let source = root.join("runtime.c");
        let harness = root.join("harness.c");
        let executable = root.join("runtime-test");
        fs::write(&source, render_layouts([(7, 8, 8)])).expect("runtime source");
        fs::write(&harness, harness_text).expect("runtime harness");
        let include = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../zryna-ownership-runtime-abi/include");
        let mut command = Command::new("/usr/bin/gcc");
        command.args([
            "-std=c11",
            "-pedantic",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-O2",
            "-fno-common",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
        ]);
        command
            .args(defines)
            .arg("-I")
            .arg(include)
            .arg(&source)
            .arg(&harness)
            .arg("-o")
            .arg(&executable);
        let output = command.output().expect("compile runtime test");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let status = Command::new(&executable).status().expect("execute runtime test");
        assert!(status.success(), "runtime harness status {status}");
        fs::remove_dir_all(root).expect("runtime test cleanup");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn candidate_scratch_finalization_cannot_hide_owned_or_pending_control_leaks() {
        const HARNESS: &str = r#"
#include <stdint.h>
#include "zryna_ownership_runtime_v1.h"
extern uint32_t zryna_m3_allocate_record(uint64_t, uint32_t, uintptr_t *);
extern uint32_t zryna_m3_finish_invocation(void);
static uint32_t attempt, fail_at;
uint32_t zryna_m3_observe(uint32_t command) {
  return command == UINT32_C(0x10000006) && ++attempt == fail_at ? 2 : 0;
}
int main(void) {
  uintptr_t scratch = 0, owned = 0;
  uint32_t last = 0;
  if (zryna_m3_allocate_record(24, 8, &scratch) != 0) return 1;
  if (zryna_m3_finish_invocation() != 0) return 2;
  if (zryna_m3_allocate_record(24, 8, &scratch) != 0) return 3;
  if (zryna_rt_o1_release(scratch, 24, 8) != 0 || zryna_m3_finish_invocation() != 0) return 4;
  if (zryna_m3_allocate_record(24, 8, &scratch) != 0 || zryna_rt_o1_allocate(8, 8, &owned) != 0) return 5;
  if (zryna_m3_finish_invocation() != 255) return 6;
  if (zryna_rt_o1_release(owned, 8, 8) != 0 || zryna_m3_finish_invocation() != 0) return 7;
  if (zryna_rt_o1_allocate(16, 4, &owned) != 0) return 8;
  ((uint32_t *)owned)[0] = 1; ((uint32_t *)owned)[1] = 1;
  if (zryna_rt_o1_strong_release_begin(owned, &last) != 0 || last != 1) return 9;
  if (zryna_m3_finish_invocation() != 255) return 10;
  if (zryna_rt_o1_strong_release_finish(owned) != 0 || zryna_m3_finish_invocation() != 0) return 11;
  if (zryna_m3_allocate_record(24, 8, &scratch) != 0) return 12;
  fail_at = attempt + 1;
  if (zryna_rt_o1_allocate(8, 8, &owned) != 1 || owned != 0) return 13;
  if (zryna_m3_finish_invocation() != 0) return 14;
  return 0;
}
"#;
        compile_and_run(HARNESS, &["-DZRYNA_M3_OBSERVATION=1"], "scratch-ledger");
    }

    #[test]
    fn runtime_source_exports_only_the_sealed_inventory() {
        let source = std::str::from_utf8(SOURCE).expect("UTF-8 runtime source");
        let declarations =
            include_str!("../../zryna-ownership-runtime-abi/include/zryna_ownership_runtime_v1.h");
        for symbol in declarations
            .split_ascii_whitespace()
            .filter(|word| word.contains('('))
            .filter_map(|word| word.strip_prefix("zryna_rt_o1_"))
            .filter_map(|word| word.split('(').next())
        {
            assert!(source.contains(&format!("zryna_rt_o1_{symbol}(")));
        }
        assert_eq!(source.matches("uint32_t zryna_rt_o1_").count(), 17);
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn compiled_runtime_has_exact_exports_and_ambient_imports() {
        let sequence = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("zryna-runtime-v1-audit-{}-{sequence}", std::process::id()));
        fs::create_dir(&root).expect("runtime audit directory");
        let source = root.join("runtime.c");
        let object_path = root.join("runtime.o");
        fs::write(&source, render_layouts([(7, 8, 8)])).expect("runtime source");
        let output = Command::new("/usr/bin/gcc")
            .args([
                "-std=c11",
                "-pedantic",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-O2",
                "-fno-stack-protector",
                "-c",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&object_path)
            .output()
            .expect("compile runtime object");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let bytes = fs::read(&object_path).expect("runtime object");
        let object = object::File::parse(bytes.as_slice()).expect("ELF runtime object");
        let defined = object
            .symbols()
            .filter(|symbol| symbol.is_global() && !symbol.is_undefined())
            .filter_map(|symbol| symbol.name().ok())
            .collect::<BTreeSet<_>>();
        assert_eq!(defined, SYMBOLS.into_iter().collect());
        let ambient = object
            .symbols()
            .filter(ObjectSymbol::is_undefined)
            .filter_map(|symbol| symbol.name().ok())
            .collect::<BTreeSet<_>>();
        assert!(
            ambient.iter().all(|name| ["free", "malloc", "memcpy", "memset"].contains(name)),
            "unexpected ambient imports: {ambient:?}"
        );
        fs::remove_dir_all(root).expect("runtime audit cleanup");
    }
}
