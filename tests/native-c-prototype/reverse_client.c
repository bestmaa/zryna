/* Link only with a real independently audited Zryna export object. */
#include "prototype_exports.h"

#include <assert.h>
#include <limits.h>
#include <stdio.h>

_Static_assert(_Generic(&zryna_c_v0_e_add, int32_t (*)(int32_t, int32_t): 1, default: 0),
               "exact reverse export signature");

int main(void) {
  assert(zryna_c_v0_e_add(20, 22) == 42);
  assert(zryna_c_v0_e_add(20, 22) == 42);
  assert(zryna_c_v0_e_add(INT32_MAX, 1) == INT32_MIN);
  assert(zryna_c_v0_e_add(INT32_MIN, -1) == INT32_MAX);
  assert(zryna_c_v0_e_add(INT32_MIN, INT32_MIN) == 0);
  puts("native-c-prototype: reverse scalar observations passed");
  return 0;
}
