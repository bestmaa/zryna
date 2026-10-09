/* Independent tiny library, with only the seven declared strong definitions. */
#include <stdlib.h>
#include <limits.h>
_Static_assert(CHAR_BIT == 8 && sizeof(int32_t) == 4, "exact scalar carrier");
_Static_assert(sizeof(size_t) == 8 && sizeof(void *) == 8, "exact LP64 target");
struct fixture_handle { int32_t seed; };
int32_t add(int32_t left, int32_t right) {
  uint32_t bits=(uint32_t)left+(uint32_t)right;
  int64_t value=bits<=INT32_MAX ? (int64_t)bits : (int64_t)bits-INT64_C(4294967296);
  return (int32_t)value;
}
int32_t sum_bytes(const uint8_t *bytes, size_t length, int32_t *out) {
  if(length>4096) return 1;
  int32_t result=0;
  for(size_t i=0;i<length;++i) result+=(int32_t)bytes[i];
  *out=result;
  return 0;
}
int32_t fixture_open(int32_t seed,struct fixture_handle **out) {
  if(seed<0) return 1;
  struct fixture_handle *handle=malloc(sizeof(*handle));
  if(handle==NULL) return 2;
  handle->seed=seed;
  *out=handle;
  return 0;
}
int32_t fixture_read(struct fixture_handle *handle,int32_t *out) {
  *out=handle->seed;
  return 0;
}
void fixture_close(struct fixture_handle *handle) {free(handle);}
int32_t fixture_copy_bytes(const uint8_t *bytes,size_t length,uint8_t **out,size_t *count) {
  if(length>4096) return 1;
  uint8_t *copy=NULL;
  if(length>0) {
    copy=malloc(length);
    if(copy==NULL) return 1;
    for(size_t i=0;i<length;++i) copy[i]=bytes[i];
  }
  *out=copy; *count=length;
  return 0;
}
void fixture_release_bytes(uint8_t *bytes) {free(bytes);}
