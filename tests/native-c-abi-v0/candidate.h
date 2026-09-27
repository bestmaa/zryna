#ifndef ZRYNA_NATIVE_C_V0_CANDIDATE_H
#define ZRYNA_NATIVE_C_V0_CANDIDATE_H

#include <stddef.h>
#include <stdint.h>

/* Proposed fixture declarations only; no foreign code is linked here. */
struct fixture_handle;

int32_t add(int32_t left, int32_t right);
int32_t sum_bytes(const uint8_t *bytes, size_t length, int32_t *out);
int32_t fixture_open(int32_t seed, struct fixture_handle **out);
int32_t fixture_read(struct fixture_handle *handle, int32_t *out);
void fixture_close(struct fixture_handle *handle);
int32_t fixture_copy_bytes(const uint8_t *bytes, size_t length,
                           uint8_t **out_bytes, size_t *out_length);
void fixture_release_bytes(uint8_t *bytes);
int32_t zryna_c_v0_e_add(int32_t left, int32_t right);

#endif
