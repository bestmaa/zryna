#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) const SYMBOLS: [&str; 17] = [
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
pub(super) const HARNESS: &str = r#"
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
pub(super) const FAILURE_HARNESS: &str = r#"
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
