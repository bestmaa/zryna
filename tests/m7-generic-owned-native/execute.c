/* Actual malloc/copy/free with immutable ABI records, fixed UTF-8 cleanup oracles and trap children. */
#include "ownership-runtime-v1.h"
#include <assert.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
extern int32_t zryna_v1_e_some(int32_t);
extern int32_t zryna_v1_e_none(int32_t);
extern int32_t zryna_v1_e_ok(int32_t);
extern int32_t zryna_v1_e_err(int32_t);
extern int32_t zryna_v1_e_exclusive(int32_t);
extern int32_t zryna_v1_e_clones(int32_t);
extern int32_t zryna_v1_e_lexical(int32_t);
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
static void positive(const char *name,int32_t (*function)(int32_t)) {
    reset(0);printf("call:%s\n",name);assert(function(7)==7);assert(live==0);
    for(size_t i=0;i<count;i++)assert(!records[i].live);
}
static void failure(const char *name,int32_t (*function)(int32_t),size_t number) {
    reset(number);printf("fault:%s:%zu\n",name,number);pid_t child=fork();assert(child>=0);
    if(child==0){(void)function(7);_exit(99);}
    int status;assert(waitpid(child,&status,0)==child);assert(WIFSIGNALED(status)&&WTERMSIG(status)==SIGILL);
}
int main(void) {
    const struct rlimit no_core={0,0};assert(setrlimit(RLIMIT_CORE,&no_core)==0);assert(setvbuf(stdout,NULL,_IONBF,0)==0);
    positive("some",zryna_v1_e_some);positive("none",zryna_v1_e_none);positive("ok",zryna_v1_e_ok);positive("err",zryna_v1_e_err);
    positive("exclusive",zryna_v1_e_exclusive);positive("clones",zryna_v1_e_clones);positive("lexical",zryna_v1_e_lexical);
    for(size_t i=1;i<=3;i++){failure("some",zryna_v1_e_some,i);positive("some",zryna_v1_e_some);}
    failure("clones",zryna_v1_e_clones,2);positive("clones",zryna_v1_e_clones);return 0;
}
