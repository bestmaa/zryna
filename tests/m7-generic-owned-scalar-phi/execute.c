/* Actual allocation identities and fixed branch cleanup observations, unchanged ABI. */
#include "ownership-runtime-v1.h"
#include <assert.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
extern int32_t zryna_v1_e_single(uint8_t);
extern int32_t zryna_v1_e_double(uint8_t);
extern int32_t zryna_v1_e_early(uint8_t);
extern int32_t zryna_v1_e_nested(uint8_t);
extern int32_t zryna_v1_e_looped(uint8_t);
extern int32_t zryna_v1_e_optional(uint8_t);
typedef struct { uintptr_t pointer; uint64_t length; int live; } allocation_record;
static allocation_record records[16];
static size_t count, calls, fail_at, live;
static void reset(size_t fail) { assert(live==0);count=0;calls=0;fail_at=fail; }
static size_t find(const zryna_rt_o1_handle *value) {
    assert(value->capacity==value->length);
    for(size_t i=0;i<count;i++)if(records[i].pointer==value->pointer&&records[i].live){assert(records[i].length==value->length);return i;}
    assert(!"duplicate or unavailable runtime handle");return 0;
}
uint32_t zryna_rt_o1_string_from_utf8_copy(const uint8_t *bytes,uint64_t length,zryna_rt_o1_handle *out) {
    calls++;if(calls==fail_at){printf("fail:%zu\n",calls);return 1;}
    assert(count<16&&length<65536);void *pointer=malloc(length?length:1);assert(pointer);memcpy(pointer,bytes,length);
    records[count++]=(allocation_record){(uintptr_t)pointer,length,1};live++;printf("alloc:%zu:%.*s\n",count,(int)length,(const char *)pointer);
    *out=(zryna_rt_o1_handle){(uintptr_t)pointer,length,length};return 0;
}
uint32_t zryna_rt_o1_string_clone(const zryna_rt_o1_handle *source,zryna_rt_o1_handle *out) {
    (void)find(source);return zryna_rt_o1_string_from_utf8_copy((const uint8_t *)source->pointer,source->length,out);
}
uint32_t zryna_rt_o1_string_release(const zryna_rt_o1_handle *value) {
    size_t index=find(value);printf("drop:%zu:%.*s\n",index+1,(int)value->length,(const char *)value->pointer);
    memset((void *)value->pointer,0,value->length);free((void *)value->pointer);records[index].live=0;assert(live>0);live--;return 0;
}

static void positive(const char *name,int32_t (*function)(uint8_t),uint8_t flag,int32_t expected) {
    reset(0);printf("call:%s:%u\n",name,flag);assert(function(flag)==expected);assert(live==0);
    for(size_t i=0;i<count;i++)assert(!records[i].live);
}
static void failure(const char *name,int32_t (*function)(uint8_t),uint8_t flag,size_t number,int32_t expected) {
    reset(number);printf("fault:%s:%u:%zu\n",name,flag,number);pid_t child=fork();assert(child>=0);
    if(child==0){(void)function(flag);_exit(99);}
    int status;assert(waitpid(child,&status,0)==child);assert(WIFSIGNALED(status)&&WTERMSIG(status)==SIGILL);
    positive(name,function,flag,expected);
}
int main(void) {
    const struct rlimit no_core={0,0};assert(setrlimit(RLIMIT_CORE,&no_core)==0);assert(setvbuf(stdout,NULL,_IONBF,0)==0);
    positive("single",zryna_v1_e_single,1,11);
    positive("single",zryna_v1_e_single,0,13);
    positive("double",zryna_v1_e_double,1,7);
    positive("double",zryna_v1_e_double,0,9);
    positive("early",zryna_v1_e_early,1,17);
    positive("early",zryna_v1_e_early,0,19);
    positive("nested",zryna_v1_e_nested,1,24);
    positive("nested",zryna_v1_e_nested,0,29);
    positive("looped",zryna_v1_e_looped,1,1);
    positive("looped",zryna_v1_e_looped,0,5);
    positive("optional",zryna_v1_e_optional,1,31);
    positive("optional",zryna_v1_e_optional,0,41);
    failure("single",zryna_v1_e_single,1,1,11);
    failure("single",zryna_v1_e_single,1,2,11);
    failure("single",zryna_v1_e_single,1,3,11);
    failure("single",zryna_v1_e_single,0,1,13);
    failure("single",zryna_v1_e_single,0,2,13);
    failure("double",zryna_v1_e_double,1,1,7);
    failure("double",zryna_v1_e_double,1,2,7);
    failure("double",zryna_v1_e_double,1,3,7);
    failure("double",zryna_v1_e_double,0,1,9);
    failure("double",zryna_v1_e_double,0,2,9);
    failure("double",zryna_v1_e_double,0,3,9);
    failure("early",zryna_v1_e_early,1,1,17);
    failure("early",zryna_v1_e_early,1,2,17);
    failure("early",zryna_v1_e_early,0,1,19);
    failure("early",zryna_v1_e_early,0,2,19);
    failure("early",zryna_v1_e_early,0,3,19);
    failure("nested",zryna_v1_e_nested,1,1,24);
    failure("nested",zryna_v1_e_nested,1,2,24);
    failure("nested",zryna_v1_e_nested,1,3,24);
    failure("nested",zryna_v1_e_nested,0,1,29);
    failure("nested",zryna_v1_e_nested,0,2,29);
    failure("nested",zryna_v1_e_nested,0,3,29);
    failure("looped",zryna_v1_e_looped,1,1,1);
    failure("looped",zryna_v1_e_looped,1,2,1);
    failure("looped",zryna_v1_e_looped,1,3,1);
    failure("looped",zryna_v1_e_looped,0,1,5);
    failure("looped",zryna_v1_e_looped,0,2,5);
    failure("looped",zryna_v1_e_looped,0,3,5);
    failure("looped",zryna_v1_e_looped,0,4,5);
    failure("optional",zryna_v1_e_optional,1,1,31);
    failure("optional",zryna_v1_e_optional,1,2,31);
    failure("optional",zryna_v1_e_optional,1,3,31);
    failure("optional",zryna_v1_e_optional,0,1,41);
    failure("optional",zryna_v1_e_optional,0,2,41);
    failure("optional",zryna_v1_e_optional,0,3,41);
    return 0;
}
