#include "../native-c-abi-v0/candidate.h"
#include "fixture_control.h"

#include <assert.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

#if !defined(__linux__) || !defined(__x86_64__) || !defined(__LP64__) || defined(__ILP32__)
#error The prototype fixture requires Linux x86-64 LP64.
#endif

_Static_assert(CHAR_BIT == 8, "eight-bit bytes");
_Static_assert(sizeof(int32_t) == 4 && _Alignof(int32_t) == 4, "i32 carrier");
_Static_assert(sizeof(size_t) == 8 && SIZE_MAX == UINT64_MAX, "LP64 count");
_Static_assert(sizeof(void *) == 8 && _Alignof(void *) == 8, "LP64 pointer");

struct fixture_handle { int32_t seed; };
struct allocation {
  void *pointer;
  size_t acquisition;
  enum fixture_test_kind kind;
};

/* The oracle has a separate bound from the wrapper's 64-owner contract. */
#define TEST_CAPACITY 256
static struct allocation live[TEST_CAPACITY];
static struct fixture_test_release released[TEST_CAPACITY];
static struct fixture_test_observation observed;
static size_t failing_attempt;

void fixture_test_reset(void) {
  assert(observed.live == 0);
  memset(live, 0, sizeof(live));
  memset(released, 0, sizeof(released));
  memset(&observed, 0, sizeof(observed));
  failing_attempt = 0;
}

void fixture_test_fail_allocation(size_t attempt) { failing_attempt = attempt; }
struct fixture_test_observation fixture_test_observe(void) { return observed; }
struct fixture_test_release fixture_test_release_at(size_t index) {
  assert(index < observed.releases);
  return released[index];
}

static void *allocate(size_t bytes, enum fixture_test_kind kind) {
  size_t slot = 0;
  while (slot < TEST_CAPACITY && live[slot].pointer != NULL) ++slot;
  assert(slot < TEST_CAPACITY);
  ++observed.allocation_attempts;
  if (observed.allocation_attempts == failing_attempt) return NULL;
  void *pointer = malloc(bytes);
  if (pointer == NULL) return NULL;
  ++observed.allocations;
  ++observed.live;
  live[slot] = (struct allocation){pointer, observed.allocations, kind};
  return pointer;
}

static struct allocation *lookup(const void *pointer, enum fixture_test_kind kind) {
  for (size_t slot = 0; slot < TEST_CAPACITY; ++slot) {
    if (live[slot].pointer == pointer && pointer != NULL) {
      assert(live[slot].kind == kind);
      return &live[slot];
    }
  }
  /* A bad raw precondition is a test-process failure, never a C status. */
  abort();
}

static void release(void *pointer, enum fixture_test_kind kind) {
  struct allocation *entry = lookup(pointer, kind);
  assert(observed.releases < TEST_CAPACITY);
  released[observed.releases++] = (struct fixture_test_release){entry->acquisition, kind};
  entry->pointer = NULL;
  --observed.live;
  free(pointer);
}

int32_t add(int32_t left, int32_t right) {
  ++observed.calls;
  uint32_t bits = (uint32_t)left + (uint32_t)right;
  int64_t signed_value = bits <= INT32_MAX ? (int64_t)bits : (int64_t)bits - INT64_C(4294967296);
  return (int32_t)signed_value;
}

int32_t sum_bytes(const uint8_t *bytes, size_t length, int32_t *out) {
  ++observed.calls;
  assert(out != NULL && (bytes != NULL || length == 0));
  if (length > 4096) return 1;
  uint64_t sum = 0;
  for (size_t index = 0; index < length; ++index) sum += bytes[index];
  assert(sum <= INT32_MAX);
  *out = (int32_t)sum;
  ++observed.i32_writes;
  return 0;
}

int32_t fixture_open(int32_t seed, struct fixture_handle **out) {
  ++observed.calls;
  assert(out != NULL);
  if (seed < 0) return 1;
  struct fixture_handle *handle = allocate(sizeof(*handle), FIXTURE_TEST_HANDLE);
  if (handle == NULL) return 2;
  handle->seed = seed;
  *out = handle;
  ++observed.handle_writes;
  return 0;
}

int32_t fixture_read(struct fixture_handle *handle, int32_t *out) {
  ++observed.calls;
  assert(out != NULL);
  (void)lookup(handle, FIXTURE_TEST_HANDLE);
  *out = handle->seed;
  ++observed.i32_writes;
  return 0;
}

void fixture_close(struct fixture_handle *handle) {
  ++observed.calls;
  release(handle, FIXTURE_TEST_HANDLE);
}

int32_t fixture_copy_bytes(const uint8_t *bytes, size_t length,
                           uint8_t **out_bytes, size_t *out_length) {
  ++observed.calls;
  assert(out_bytes != NULL && out_length != NULL && (bytes != NULL || length == 0));
  if (length > 4096) return 1;
  uint8_t *copy = NULL;
  if (length != 0) {
    copy = allocate(length, FIXTURE_TEST_BYTES);
    if (copy == NULL) return 1;
    memcpy(copy, bytes, length);
  }
  *out_bytes = copy;
  ++observed.pointer_writes;
  *out_length = length;
  ++observed.count_writes;
  return 0;
}

void fixture_release_bytes(uint8_t *bytes) {
  ++observed.calls;
  release(bytes, FIXTURE_TEST_BYTES);
}
