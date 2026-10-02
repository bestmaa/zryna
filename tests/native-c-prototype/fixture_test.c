#include "../native-c-abi-v0/candidate.h"
#include "fixture_control.h"

#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static void scalar_and_sum(void) {
  fixture_test_reset();
  assert(add(20, 22) == 42);
  assert(add(20, 22) == 42);
  assert(add(INT32_MAX, 1) == INT32_MIN);
  assert(add(INT32_MIN, -1) == INT32_MAX);
  assert(add(-1, 1) == 0);
  assert(add(INT32_MIN, INT32_MIN) == 0);
  int32_t out = -123;
  assert(sum_bytes(NULL, 0, &out) == 0 && out == 0);
  const uint8_t small[] = {1, 2, 3};
  assert(sum_bytes(small, sizeof(small), &out) == 0 && out == 6);
  uint8_t large[4097];
  memset(large, 255, sizeof(large));
  assert(sum_bytes(large, 4096, &out) == 0 && out == 1044480);
  struct fixture_test_observation before = fixture_test_observe();
  out = -123;
  assert(sum_bytes(large, sizeof(large), &out) == 1 && out == -123);
  struct fixture_test_observation after = fixture_test_observe();
  assert(after.i32_writes == before.i32_writes);
  assert(after.allocations == 0 && after.live == 0);
  for (size_t index = 0; index < sizeof(large); ++index) assert(large[index] == 255);
}

static void handle_negative_and_allocation_failure(void) {
  fixture_test_reset();
  struct fixture_handle *first = NULL;
  assert(fixture_open(7, &first) == 0 && first != NULL);
  int32_t value = 0;
  assert(fixture_read(first, &value) == 0 && value == 7);
  struct fixture_handle *failed = first;
  struct fixture_test_observation before = fixture_test_observe();
  fixture_test_fail_allocation(before.allocation_attempts + 1);
  assert(fixture_open(-1, &failed) == 1 && failed == first);
  struct fixture_test_observation negative = fixture_test_observe();
  assert(negative.allocation_attempts == before.allocation_attempts);
  assert(negative.handle_writes == before.handle_writes);
  assert(negative.live == 1 && negative.allocations == 1);
  assert(fixture_open(8, &failed) == 2 && failed == first);
  struct fixture_test_observation allocation = fixture_test_observe();
  assert(allocation.allocation_attempts == before.allocation_attempts + 1);
  assert(allocation.handle_writes == before.handle_writes);
  assert(allocation.allocations == 1 && allocation.live == 1);
  assert(fixture_read(first, &value) == 0 && value == 7);
  fixture_close(first);
  assert(fixture_test_observe().live == 0 && fixture_test_observe().releases == 1);
  assert(fixture_test_release_at(0).acquisition == 1);
  assert(fixture_test_release_at(0).kind == FIXTURE_TEST_HANDLE);
}

static void byte_copy_and_failure(void) {
  fixture_test_reset();
  const uint8_t input[] = {1, 2, 3};
  uint8_t sentinel = 99;
  uint8_t *out = &sentinel;
  size_t length = 77;
  assert(fixture_copy_bytes(NULL, 0, &out, &length) == 0 && out == NULL && length == 0);
  assert(fixture_test_observe().allocation_attempts == 0);
  assert(fixture_copy_bytes(input, sizeof(input), &out, &length) == 0);
  assert(out != NULL && out != input && length == sizeof(input));
  assert(memcmp(out, input, sizeof(input)) == 0);
  fixture_release_bytes(out);
  struct fixture_handle *handle = NULL;
  assert(fixture_open(7, &handle) == 0);
  struct fixture_test_observation before = fixture_test_observe();
  fixture_test_fail_allocation(before.allocation_attempts + 1);
  out = &sentinel;
  length = 77;
  assert(fixture_copy_bytes(input, sizeof(input), &out, &length) == 1);
  assert(out == &sentinel && length == 77);
  struct fixture_test_observation after = fixture_test_observe();
  assert(after.pointer_writes == before.pointer_writes && after.count_writes == before.count_writes);
  assert(after.allocations == before.allocations && after.live == 1);
  assert(memcmp(input, (uint8_t[]){1, 2, 3}, sizeof(input)) == 0);
  fixture_close(handle);
  assert(fixture_test_observe().releases == 2 && fixture_test_observe().live == 0);
  assert(fixture_test_release_at(0).kind == FIXTURE_TEST_BYTES);
  assert(fixture_test_release_at(1).kind == FIXTURE_TEST_HANDLE);
}

static void byte_bound_and_reverse_prefix(void) {
  fixture_test_reset();
  uint8_t input[4097];
  memset(input, 255, sizeof(input));
  uint8_t *out = NULL;
  size_t length = 0;
  assert(fixture_copy_bytes(input, 4096, &out, &length) == 0 && length == 4096);
  assert(memcmp(input, out, 4096) == 0);
  fixture_release_bytes(out);
  out = input;
  length = 4097;
  struct fixture_test_observation before = fixture_test_observe();
  assert(fixture_copy_bytes(input, sizeof(input), &out, &length) == 1);
  struct fixture_test_observation after = fixture_test_observe();
  assert(out == input && length == sizeof(input));
  assert(after.allocation_attempts == before.allocation_attempts);
  assert(after.pointer_writes == before.pointer_writes && after.count_writes == before.count_writes);
  fixture_test_reset();
  struct fixture_handle *first = NULL;
  struct fixture_handle *second = NULL;
  assert(fixture_open(1, &first) == 0);
  assert(fixture_copy_bytes(input, 3, &out, &length) == 0);
  assert(fixture_open(2, &second) == 0);
  fixture_close(second);
  fixture_release_bytes(out);
  fixture_close(first);
  assert(fixture_test_observe().live == 0 && fixture_test_observe().releases == 3);
  for (size_t index = 0; index < 3; ++index)
    assert(fixture_test_release_at(index).acquisition == 3 - index);
}

int main(void) {
  scalar_and_sum();
  handle_negative_and_allocation_failure();
  byte_copy_and_failure();
  byte_bound_and_reverse_prefix();
  puts("native-c-prototype: four raw fixture groups passed");
  return 0;
}
