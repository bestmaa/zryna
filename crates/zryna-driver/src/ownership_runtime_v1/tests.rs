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
mod fixtures;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use fixtures::{FAILURE_HARNESS, HARNESS, SYMBOLS};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn runtime_compiles_with_strict_c_and_executes_transitions() {
    compile_and_run(HARNESS, &[], "transitions");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn allocation_failure_is_atomic_and_recoverable() {
    compile_and_run(FAILURE_HARNESS, &["-DZRYNA_RT_O1_FAIL_ALLOCATION_AT=2"], "allocation-failure");
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
    let output = crate::process_spawn::output(
        Command::new("/usr/bin/gcc")
            .args(["-std=c11", "-pedantic", "-Wall", "-Wextra", "-Werror", "-O2", "-fno-common"])
            .arg(fixture)
            .arg("-o")
            .arg(&executable),
    )
    .expect("compile capacity probe");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let status = crate::process_spawn::status(&mut Command::new(&executable))
        .expect("execute capacity probe");
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
    let output = crate::process_spawn::output(&mut command).expect("compile runtime test");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let status =
        crate::process_spawn::status(&mut Command::new(&executable)).expect("execute runtime test");
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
        include_str!("../../../zryna-ownership-runtime-abi/include/zryna_ownership_runtime_v1.h");
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
    let output = crate::process_spawn::output(
        Command::new("/usr/bin/gcc")
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
            .arg(&object_path),
    )
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
