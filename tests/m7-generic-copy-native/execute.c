/* Fixed Linux/System V scalar observations; no Zryna runtime or profile activation. */
#include <assert.h>
#include <inttypes.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

extern int32_t zryna_v1_e_score(int32_t);
extern int32_t zryna_v1_e_unwrap(int32_t);
extern int32_t zryna_v1_e_flag(int32_t);
extern int32_t zryna_v1_e_add(int32_t);
extern int32_t zryna_v1_e_fallback(void);
extern int32_t zryna_v1_e_error(void);

int main(void) {
    const struct rlimit no_core = { 0, 0 };
    assert(setrlimit(RLIMIT_CORE, &no_core) == 0);
    const int32_t input[] = { INT32_MIN, -1, 0, 7, INT32_MAX };
    for (size_t i = 0; i < sizeof(input) / sizeof(input[0]); ++i) {
        assert(zryna_v1_e_score(input[i]) == input[i]);
        assert(zryna_v1_e_unwrap(input[i]) == input[i]);
        printf("score:%" PRId32 "\nunwrap:%" PRId32 "\n", input[i], input[i]);
    }
    assert(zryna_v1_e_flag(0) == 0); assert(zryna_v1_e_flag(1) == 1);
    assert(zryna_v1_e_add(INT32_MAX) == INT32_MIN);
    assert(zryna_v1_e_add(-1) == 0);
    assert(zryna_v1_e_fallback() == 11); assert(zryna_v1_e_error() == 23);
    puts("flag:1\nflag:0\nadd:-2147483648\nadd:0\nfallback:11\nerror:23");
    const int32_t invalid[] = { INT32_MIN, -1, 2, INT32_MAX };
    for (size_t i = 0; i < sizeof(invalid) / sizeof(invalid[0]); ++i) {
        pid_t child = fork(); assert(child >= 0);
        if (child == 0) { (void)zryna_v1_e_flag(invalid[i]); _exit(99); }
        int status; assert(waitpid(child, &status, 0) == child);
        assert(WIFSIGNALED(status) && WTERMSIG(status) == SIGILL);
        assert(zryna_v1_e_flag(1) == 1); assert(zryna_v1_e_unwrap(7) == 7);
    }
    assert(zryna_v1_e_unwrap(-1) == -1);
    puts("unwrap:7\nflag:1");
    return 0;
}
