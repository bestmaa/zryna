#include "candidate.h"

#include <limits.h>
#include <stdbool.h>

#if !defined(__x86_64__) || !defined(__linux__) || !defined(__LP64__) || defined(__ILP32__)
#error This candidate requires the Linux x86-64 LP64 target.
#endif

#define CHECK_LAYOUT(type, width, alignment) \
  _Static_assert(sizeof(type) == (width) && _Alignof(type) == (alignment), #type " layout")
#define CHECK_FUNCTION(name, signature) \
  _Static_assert(_Generic(&(name), signature: 1, default: 0), #name " signature")

_Static_assert(CHAR_BIT == 8, "eight-bit C bytes");
_Static_assert(CHAR_MIN < 0, "plain char is signed on this target");
CHECK_LAYOUT(int8_t, 1, 1);
CHECK_LAYOUT(uint8_t, 1, 1);
CHECK_LAYOUT(char, 1, 1);
CHECK_LAYOUT(int16_t, 2, 2);
CHECK_LAYOUT(uint16_t, 2, 2);
CHECK_LAYOUT(int32_t, 4, 4);
CHECK_LAYOUT(uint32_t, 4, 4);
CHECK_LAYOUT(int, 4, 4);
CHECK_LAYOUT(unsigned int, 4, 4);
CHECK_LAYOUT(int64_t, 8, 8);
CHECK_LAYOUT(uint64_t, 8, 8);
CHECK_LAYOUT(long, 8, 8);
CHECK_LAYOUT(unsigned long, 8, 8);
CHECK_LAYOUT(long long, 8, 8);
CHECK_LAYOUT(unsigned long long, 8, 8);
CHECK_LAYOUT(size_t, 8, 8);
CHECK_LAYOUT(_Bool, 1, 1);
CHECK_LAYOUT(bool, 1, 1);
CHECK_LAYOUT(void *, 8, 8);
CHECK_LAYOUT(uint8_t *, 8, 8);
CHECK_LAYOUT(struct fixture_handle *, 8, 8);

_Static_assert((int32_t)-1 < 0 && (int64_t)-1 < 0, "signed fixed integers");
_Static_assert((uint8_t)-1 > 0 && (uint32_t)-1 > 0 && (uint64_t)-1 > 0,
               "unsigned fixed integers");
_Static_assert((size_t)-1 > 0 && SIZE_MAX == UINT64_MAX, "unsigned LP64 size_t");
_Static_assert(_Generic((uint32_t)0, _Bool: 0, default: 1),
               "32-bit Boolean shim is not C _Bool");

CHECK_FUNCTION(add, int32_t (*)(int32_t, int32_t));
CHECK_FUNCTION(sum_bytes, int32_t (*)(const uint8_t *, size_t, int32_t *));
CHECK_FUNCTION(fixture_open, int32_t (*)(int32_t, struct fixture_handle **));
CHECK_FUNCTION(fixture_read, int32_t (*)(struct fixture_handle *, int32_t *));
CHECK_FUNCTION(fixture_close, void (*)(struct fixture_handle *));
CHECK_FUNCTION(zryna_c_v0_e_add, int32_t (*)(int32_t, int32_t));

#if defined(REJECT_EXPORT_ARITY)
int32_t zryna_c_v0_e_add(int32_t left);
#elif defined(REJECT_EXPORT_BOOL_WIDTH)
_Bool zryna_c_v0_e_add(int32_t left, int32_t right);
#elif defined(REJECT_BUFFER_LENGTH_WIDTH)
int32_t sum_bytes(const uint8_t *bytes, uint32_t length, int32_t *out);
#elif defined(REJECT_HANDLE_POINTER_LEVEL)
int32_t fixture_open(int32_t seed, struct fixture_handle *out);
#endif
