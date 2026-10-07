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
extern int32_t zryna_v1_e_optionNone(uint8_t);
extern int32_t zryna_v1_e_optionInnerNone(uint8_t);
extern int32_t zryna_v1_e_optionInnerSome(uint8_t);
extern int32_t zryna_v1_e_resultOkNone(uint8_t);
extern int32_t zryna_v1_e_resultOkSome(uint8_t);
extern int32_t zryna_v1_e_resultErrNone(uint8_t);
extern int32_t zryna_v1_e_resultErrSome(uint8_t);
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
    positive("optionNone",zryna_v1_e_optionNone,1,3);
    positive("optionNone",zryna_v1_e_optionNone,0,3);
    positive("optionInnerNone",zryna_v1_e_optionInnerNone,1,5);
    positive("optionInnerNone",zryna_v1_e_optionInnerNone,0,5);
    positive("optionInnerSome",zryna_v1_e_optionInnerSome,1,7);
    positive("optionInnerSome",zryna_v1_e_optionInnerSome,0,7);
    positive("resultOkNone",zryna_v1_e_resultOkNone,1,11);
    positive("resultOkNone",zryna_v1_e_resultOkNone,0,11);
    positive("resultOkSome",zryna_v1_e_resultOkSome,1,13);
    positive("resultOkSome",zryna_v1_e_resultOkSome,0,13);
    positive("resultErrNone",zryna_v1_e_resultErrNone,1,17);
    positive("resultErrNone",zryna_v1_e_resultErrNone,0,17);
    positive("resultErrSome",zryna_v1_e_resultErrSome,1,19);
    positive("resultErrSome",zryna_v1_e_resultErrSome,0,19);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,1,1,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,1,2,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,1,3,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,1,4,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,0,1,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,0,2,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,0,3,7);
    failure("optionInnerSome",zryna_v1_e_optionInnerSome,0,4,7);
    failure("resultOkSome",zryna_v1_e_resultOkSome,1,1,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,1,2,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,1,3,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,1,4,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,0,1,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,0,2,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,0,3,13);
    failure("resultOkSome",zryna_v1_e_resultOkSome,0,4,13);
    failure("resultErrSome",zryna_v1_e_resultErrSome,1,1,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,1,2,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,1,3,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,1,4,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,0,1,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,0,2,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,0,3,19);
    failure("resultErrSome",zryna_v1_e_resultErrSome,0,4,19);
    return 0;
}
