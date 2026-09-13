#define _POSIX_C_SOURCE 200809L
#include "wasm3.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

m3ApiRawFunction(host_print) {
    m3ApiGetArgMem(const char *, text)
    m3ApiGetArg(uint32_t, len)
    m3ApiCheckMem(text, len)
    fwrite(text, 1, len, stdout); putchar('\n');
    m3ApiSuccess();
}
m3ApiRawFunction(host_abort) { m3ApiTrap(m3Err_trapAbort); }
static void check(M3Result error, const char *stage) {
    if (error) { fprintf(stderr, "%s: %s\n", stage, error); exit(1); }
}
static double now(void) {
    struct timespec time; clock_gettime(CLOCK_MONOTONIC, &time);
    return time.tv_sec + time.tv_nsec / 1e9;
}
static unsigned char *read_file(const char *path, size_t *size) {
    FILE *file = fopen(path, "rb"); if (!file) {perror(path);exit(1);}
    fseek(file, 0, SEEK_END); long n = ftell(file); rewind(file);
    if (n < 0) exit(1); *size = (size_t)n;
    unsigned char *bytes = malloc(*size);
    if (!bytes || fread(bytes, 1, *size, file) != *size) exit(1);
    fclose(file); return bytes;
}
int main(int argc, char **argv) {
    if (argc < 2) {fprintf(stderr,"usage: wasm3-benchmark module.wasm [archive.cbr index fuel]\n");return 2;}
    setvbuf(stdout, NULL, _IOLBF, 0);
    size_t size; unsigned char *bytes = read_file(argv[1], &size);
    IM3Environment env = m3_NewEnvironment();
    IM3Runtime runtime = m3_NewRuntime(env, 200*1024, NULL);
    IM3Module module; double start = now();
    check(m3_ParseModule(env, &module, bytes, (uint32_t)size), "parse");
    check(m3_LoadModule(runtime, module), "load");
    check(m3_LinkRawFunction(module, "env", "print", "v(ii)", host_print), "link print");
    check(m3_LinkRawFunction(module, "env", "abort", "v()", host_abort), "link abort");
    printf("wasm_bytes=%zu load_ms=%.3f initial_memory=%u stack=204800\n", size, 1000*(now()-start), m3_GetMemorySize(runtime));
    if (argc >= 3) {
        size_t input_size; unsigned char *input = read_file(argv[2], &input_size);
        int32_t index = argc > 3 ? atoi(argv[3]) : 2;
        uint64_t fuel = argc > 4 ? strtoull(argv[4], NULL, 10) : 500000000;
        IM3Function alloc, run; uint32_t ptr, result;
        check(m3_FindFunction(&alloc, runtime, "probe_alloc"), "find allocator");
        check(m3_FindFunction(&run, runtime, "probe_run"), "find run");
        check(m3_CallV(alloc, (uint32_t)input_size), "allocate");
        check(m3_GetResultsV(alloc, &ptr), "input pointer");
        uint32_t memory_size; uint8_t *memory = m3_GetMemory(runtime, &memory_size, 0);
        if ((uint64_t)ptr+input_size > memory_size) exit(1);
        memcpy(memory+ptr, input, input_size); free(input);
        start=now();check(m3_CallV(run, ptr, (uint32_t)input_size, index, fuel), "run");
        check(m3_GetResultsV(run, &result), "result");
        printf("archive=%s index=%d bytes=%u fuel_limit=%llu ms=%.3f outer_memory=%u\n",argv[2],index,result,(unsigned long long)fuel,1000*(now()-start),m3_GetMemorySize(runtime));
    } else {
        IM3Function benchmark;check(m3_FindFunction(&benchmark,runtime,"rar_fixture_benchmark"),"find benchmark");
        for(uint32_t fixture=0;fixture<4;fixture++) for(int i=0;i<2;i++) {
            int32_t index=i?2:-1; uint32_t result;start=now();
            check(m3_CallV(benchmark,fixture,index),"benchmark");check(m3_GetResultsV(benchmark,&result),"result");
            printf("fixture=%u index=%d bytes=%u ms=%.3f outer_memory=%u\n",fixture,index,result,1000*(now()-start),m3_GetMemorySize(runtime));
        }
    }
    m3_FreeRuntime(runtime);m3_FreeEnvironment(env);free(bytes);
    return 0;
}
