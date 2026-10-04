/* Independent observation client: no access to the private runtime's static allocation list. */
static void *owners[128];
static uintptr_t owner_addresses[128];
static size_t owner_sizes[128], owner_releases[128], owner_count;
static size_t attempts, fail_at, releases;
void *__real_malloc(size_t);
void __real_free(void *);
void *__wrap_malloc(size_t size) {
  ++attempts;
  if (fail_at && attempts == fail_at) return NULL;
  void *owner = __real_malloc(size);
  assert(owner != NULL && owner_count < 128);
  owners[owner_count] = owner; owner_addresses[owner_count] = (uintptr_t)owner;
  owner_sizes[owner_count] = size;
  owner_releases[owner_count++] = 0;
  return owner;
}
void __wrap_free(void *owner) {
  assert(owner != NULL);
  size_t index = owner_count;
  while (index && (owner_releases[index-1] || owners[index-1] != owner)) --index;
  assert(index != 0 && owner_releases[index-1] == 0);
  owner_releases[index-1] = ++releases;
  __real_free(owner);
}
static size_t live_count(void) {
  size_t result = 0;
  for (size_t i=0;i<owner_count;++i) result += owner_releases[i] == 0;
  return result;
}
static size_t foreign_count(size_t length, int live) {
  size_t result = 0;
  for (size_t i=0;i<owner_count;++i)
    if (owner_sizes[i] == length && (owner_releases[i] == 0) == live) ++result;
  return result;
}
static void reset_observation(void) {
  assert(live_count() == 0);
  owner_count = attempts = releases = fail_at = 0;
}
static void private_pointer_is_distinct_from_foreign(uintptr_t pointer,size_t length) {
  for (size_t i=0;i<owner_count;++i)
    if (owner_sizes[i]==length) assert(pointer!=owner_addresses[i]);
}
static void foreign_releases_precede(size_t earlier, size_t later) {
  size_t first=0, second=0;
  for (size_t i=0;i<owner_count;++i) {
    if (owner_sizes[i]==earlier) { assert(first==0); first=owner_releases[i]; }
    if (owner_sizes[i]==later) { assert(second==0); second=owner_releases[i]; }
  }
  assert(first!=0 && second>first);
}
static void input_bytes(struct zryna_c_v0_inputs *inputs, size_t length) {
  zryna_rt_o1_handle input = {0};
  assert(zryna_rt_o1_vec_allocate(1,length ? length : 1,&input) == 0);
  int32_t *data = (int32_t *)input.pointer;
  for (size_t i=0;i<length;++i) data[i] = (int32_t)(i % 256);
  input.length = length;
  memcpy(&inputs->owned[0],&input,sizeof(input));
}
static void release_result(struct zryna_c_v0_outcome *outcome) {
  zryna_rt_o1_handle returned;
  memcpy(&returned,&outcome->owned,sizeof(returned));
  assert(zryna_rt_o1_vec_release_storage(1,&returned) == 0);
}
