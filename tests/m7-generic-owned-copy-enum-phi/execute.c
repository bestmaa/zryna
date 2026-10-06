/* Fixed source-first allocation and cleanup oracle. */
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
extern int32_t zryna_v1_e_bothReturn(uint8_t);
extern int32_t zryna_v1_e_optionEarly(uint8_t);
extern int32_t zryna_v1_e_optionNested(uint8_t);
extern int32_t zryna_v1_e_optionSingle(uint8_t);
extern int32_t zryna_v1_e_optionVariants(uint8_t);
extern int32_t zryna_v1_e_parallel(uint8_t);
extern int32_t zryna_v1_e_resultRepeated(uint8_t);
extern int32_t zryna_v1_e_resultSingle(uint8_t);
extern int32_t zryna_v1_e_resultVariants(uint8_t);
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
    positive("optionSingle",zryna_v1_e_optionSingle,1,11);
    positive("optionSingle",zryna_v1_e_optionSingle,0,13);
    positive("optionVariants",zryna_v1_e_optionVariants,1,19);
    positive("optionVariants",zryna_v1_e_optionVariants,0,23);
    positive("resultSingle",zryna_v1_e_resultSingle,1,41);
    positive("resultSingle",zryna_v1_e_resultSingle,0,29);
    positive("resultVariants",zryna_v1_e_resultVariants,1,31);
    positive("resultVariants",zryna_v1_e_resultVariants,0,43);
    positive("optionEarly",zryna_v1_e_optionEarly,1,47);
    positive("optionEarly",zryna_v1_e_optionEarly,0,49);
    positive("optionNested",zryna_v1_e_optionNested,1,59);
    positive("optionNested",zryna_v1_e_optionNested,0,61);
    positive("resultRepeated",zryna_v1_e_resultRepeated,1,69);
    positive("resultRepeated",zryna_v1_e_resultRepeated,0,41);
    positive("parallel",zryna_v1_e_parallel,1,660);
    positive("parallel",zryna_v1_e_parallel,0,648);
    positive("bothReturn",zryna_v1_e_bothReturn,1,89);
    positive("bothReturn",zryna_v1_e_bothReturn,0,97);
    failure("optionSingle",zryna_v1_e_optionSingle,1,1,11);
    failure("optionSingle",zryna_v1_e_optionSingle,1,2,11);
    failure("optionSingle",zryna_v1_e_optionSingle,1,3,11);
    failure("optionSingle",zryna_v1_e_optionSingle,1,4,11);
    failure("optionSingle",zryna_v1_e_optionSingle,0,1,13);
    failure("optionSingle",zryna_v1_e_optionSingle,0,2,13);
    failure("optionSingle",zryna_v1_e_optionSingle,0,3,13);
    failure("optionVariants",zryna_v1_e_optionVariants,1,1,19);
    failure("optionVariants",zryna_v1_e_optionVariants,1,2,19);
    failure("optionVariants",zryna_v1_e_optionVariants,1,3,19);
    failure("optionVariants",zryna_v1_e_optionVariants,1,4,19);
    failure("optionVariants",zryna_v1_e_optionVariants,0,1,23);
    failure("optionVariants",zryna_v1_e_optionVariants,0,2,23);
    failure("optionVariants",zryna_v1_e_optionVariants,0,3,23);
    failure("optionVariants",zryna_v1_e_optionVariants,0,4,23);
    failure("resultSingle",zryna_v1_e_resultSingle,1,1,41);
    failure("resultSingle",zryna_v1_e_resultSingle,1,2,41);
    failure("resultSingle",zryna_v1_e_resultSingle,1,3,41);
    failure("resultSingle",zryna_v1_e_resultSingle,1,4,41);
    failure("resultSingle",zryna_v1_e_resultSingle,0,1,29);
    failure("resultSingle",zryna_v1_e_resultSingle,0,2,29);
    failure("resultSingle",zryna_v1_e_resultSingle,0,3,29);
    failure("resultVariants",zryna_v1_e_resultVariants,1,1,31);
    failure("resultVariants",zryna_v1_e_resultVariants,1,2,31);
    failure("resultVariants",zryna_v1_e_resultVariants,1,3,31);
    failure("resultVariants",zryna_v1_e_resultVariants,1,4,31);
    failure("resultVariants",zryna_v1_e_resultVariants,0,1,43);
    failure("resultVariants",zryna_v1_e_resultVariants,0,2,43);
    failure("resultVariants",zryna_v1_e_resultVariants,0,3,43);
    failure("resultVariants",zryna_v1_e_resultVariants,0,4,43);
    failure("optionEarly",zryna_v1_e_optionEarly,1,1,47);
    failure("optionEarly",zryna_v1_e_optionEarly,1,2,47);
    failure("optionEarly",zryna_v1_e_optionEarly,1,3,47);
    failure("optionEarly",zryna_v1_e_optionEarly,0,1,49);
    failure("optionEarly",zryna_v1_e_optionEarly,0,2,49);
    failure("optionEarly",zryna_v1_e_optionEarly,0,3,49);
    failure("optionEarly",zryna_v1_e_optionEarly,0,4,49);
    failure("optionNested",zryna_v1_e_optionNested,1,1,59);
    failure("optionNested",zryna_v1_e_optionNested,1,2,59);
    failure("optionNested",zryna_v1_e_optionNested,1,3,59);
    failure("optionNested",zryna_v1_e_optionNested,1,4,59);
    failure("optionNested",zryna_v1_e_optionNested,1,5,59);
    failure("optionNested",zryna_v1_e_optionNested,1,6,59);
    failure("optionNested",zryna_v1_e_optionNested,0,1,61);
    failure("optionNested",zryna_v1_e_optionNested,0,2,61);
    failure("optionNested",zryna_v1_e_optionNested,0,3,61);
    failure("optionNested",zryna_v1_e_optionNested,0,4,61);
    failure("resultRepeated",zryna_v1_e_resultRepeated,1,1,69);
    failure("resultRepeated",zryna_v1_e_resultRepeated,1,2,69);
    failure("resultRepeated",zryna_v1_e_resultRepeated,1,3,69);
    failure("resultRepeated",zryna_v1_e_resultRepeated,1,4,69);
    failure("resultRepeated",zryna_v1_e_resultRepeated,1,5,69);
    failure("resultRepeated",zryna_v1_e_resultRepeated,0,1,41);
    failure("resultRepeated",zryna_v1_e_resultRepeated,0,2,41);
    failure("resultRepeated",zryna_v1_e_resultRepeated,0,3,41);
    failure("resultRepeated",zryna_v1_e_resultRepeated,0,4,41);
    failure("parallel",zryna_v1_e_parallel,1,1,660);
    failure("parallel",zryna_v1_e_parallel,1,2,660);
    failure("parallel",zryna_v1_e_parallel,1,3,660);
    failure("parallel",zryna_v1_e_parallel,1,4,660);
    failure("parallel",zryna_v1_e_parallel,0,1,648);
    failure("parallel",zryna_v1_e_parallel,0,2,648);
    failure("parallel",zryna_v1_e_parallel,0,3,648);
    failure("parallel",zryna_v1_e_parallel,0,4,648);
    failure("bothReturn",zryna_v1_e_bothReturn,1,1,89);
    failure("bothReturn",zryna_v1_e_bothReturn,1,2,89);
    failure("bothReturn",zryna_v1_e_bothReturn,1,3,89);
    failure("bothReturn",zryna_v1_e_bothReturn,1,4,89);
    failure("bothReturn",zryna_v1_e_bothReturn,0,1,97);
    failure("bothReturn",zryna_v1_e_bothReturn,0,2,97);
    failure("bothReturn",zryna_v1_e_bothReturn,0,3,97);
    failure("bothReturn",zryna_v1_e_bothReturn,0,4,97);
    return 0;
}
