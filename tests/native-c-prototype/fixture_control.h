#ifndef ZRYNA_NATIVE_C_PROTOTYPE_CONTROL_H
#define ZRYNA_NATIVE_C_PROTOTYPE_CONTROL_H

#include <stddef.h>
#include <stdint.h>

/* Test instrumentation only; these symbols are not foreign declarations. */
enum fixture_test_kind { FIXTURE_TEST_HANDLE = 1, FIXTURE_TEST_BYTES = 2 };

struct fixture_test_observation {
  size_t calls;
  size_t allocation_attempts;
  size_t allocations;
  size_t live;
  size_t handle_writes;
  size_t i32_writes;
  size_t pointer_writes;
  size_t count_writes;
  size_t releases;
};

struct fixture_test_release {
  size_t acquisition;
  enum fixture_test_kind kind;
};

void fixture_test_reset(void);
void fixture_test_fail_allocation(size_t attempt);
struct fixture_test_observation fixture_test_observe(void);
struct fixture_test_release fixture_test_release_at(size_t index);

#endif
