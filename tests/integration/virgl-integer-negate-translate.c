/* Regression fixture: execute the real TGSI translator without a GL context. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "config.h"
#include "vrend/vrend_shader.h"
#include "tgsi/tgsi_text.h"
#include "pipe/p_shader_tokens.h"

struct fixture { const char *name, *decl, *op, *source, *expected; };

static int check(const struct fixture *test, const char *directory)
{
    char source[2048], path[4096];
    snprintf(source, sizeof(source),
        "FRAG\nDCL OUT[0], COLOR\nDCL TEMP[0]\n%s"
        "IMM[0] UINT32 {1, 2, 3, 4}\nIMM[1] UINT32 {8, 8, 8, 8}\n"
        "0: MOV TEMP[0], IMM[0]\n1: %s OUT[0], %s%s\n2: END\n",
        test->decl, test->op, test->source,
        strcmp(test->op, "MOV") ? ", IMM[1]" : "");
    struct tgsi_token tokens[512];
    if (!tgsi_text_translate(source, tokens, 512)) return 1;
    struct vrend_shader_cfg cfg = {0};
    cfg.glsl_version = 140; cfg.use_core_profile = 1; cfg.max_draw_buffers = 1;
    struct vrend_shader_key key = {0};
    struct vrend_shader_info info = {0};
    struct vrend_variable_shader_info var = {0};
    struct vrend_strarray out = {0};
    if (!strarray_alloc(&out, SHADER_MAX_STRINGS)) return 1;
    if (!vrend_convert_shader(NULL, &cfg, tokens, 0, &key, &info, &var, &out)) {
        strarray_free(&out, true); return 1;
    }
    snprintf(path, sizeof(path), "%s/%s.glsl", directory, test->name);
    FILE *file = fopen(path, "wx");
    if (!file) { perror(path); strarray_free(&out, true); return 1; }
    int found = 0, failed = 0;
    for (int i = 0; i < out.num_strings; ++i) {
        const char *text = out.strings[i].buf;
        found |= strstr(text, test->expected) != NULL;
        if (fputs(text, file) == EOF) failed = 1;
    }
    if (fclose(file)) failed = 1;
    strarray_free(&out, true);
    printf("%s: %s\n", test->name, found && !failed ? "PASS" : "FAIL");
    return !found || failed;
}

int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    const struct fixture tests[] = {
        {"unsigned-temp", "", "UADD", "-TEMP[0]", "-(floatBitsToUint(vec4(temp0)"},
        {"signed-temp", "", "IMAX", "-TEMP[0]", "-(floatBitsToInt(vec4(temp0)"},
        {"unsigned-input", "DCL IN[0], GENERIC[1], CONSTANT\n", "UADD", "-IN[0]", "-(floatBitsToUint("},
        {"unsigned-ubo", "DCL CONST[1][0]\n", "UADD", "-CONST[1][0]", "-(floatBitsToUint("},
        {"float-control", "", "ADD", "-TEMP[0]", "vec4(-temp0)"},
        {"untyped-control", "", "MOV", "-TEMP[0]", "vec4(-temp0)"},
    };
    int failures = 0;
    for (size_t i = 0; i < sizeof(tests) / sizeof(tests[0]); ++i)
        failures += check(&tests[i], argv[1]);
    printf("translator failures=%d; no GPU execution or criterion claim\n", failures);
    return failures ? 1 : 0;
}
