/* Exercise the actual patched TGSI translator; no GL context is required. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "config.h"
#include "vrend/vrend_shader.h"
#include "tgsi/tgsi_text.h"
#include "pipe/p_shader_tokens.h"

static int check(const char *declaration, int inferred, const char *expected)
{
    char source[1024];
    snprintf(source, sizeof(source),
        "FRAG\nDCL OUT[0], COLOR\nDCL SAMP[0]\n%s"
        "IMM[0] UINT32 {0, 0, 0, 0}\n"
        "0: TXF OUT[0], IMM[0], SAMP[0], BUFFER\n1: END\n", declaration);
    struct tgsi_token tokens[256];
    if (!tgsi_text_translate(source, tokens, 256)) return 1;
    struct vrend_shader_cfg cfg = {0};
    cfg.glsl_version = 140; cfg.use_core_profile = 1; cfg.max_draw_buffers = 1;
    struct vrend_shader_key key = {0};
    if (inferred == 1) key.sampler_buffer_uint_mask[0] = 1;
    if (inferred == 2) key.sampler_buffer_sint_mask[0] = 1;
    struct vrend_shader_info info = {0};
    struct vrend_variable_shader_info var = {0};
    struct vrend_strarray out = {0};
    if (!strarray_alloc(&out, SHADER_MAX_STRINGS)) return 1;
    if (!vrend_convert_shader(NULL, &cfg, tokens, 0, &key, &info, &var, &out)) return 1;
    int found = 0;
    for (int i = 0; i < out.num_strings; ++i)
        found |= strstr(out.strings[i].buf, expected) != NULL;
    strarray_free(&out, true);
    if (!found) fprintf(stderr, "wrong sampler type: declared=%s inferred=%d expected=%s\n", declaration, inferred, expected);
    return !found;
}

int main(void)
{
    int failures = 0;
    failures += check("", 1, "usamplerBuffer");
    failures += check("", 2, "isamplerBuffer");
    failures += check("", 0, " samplerBuffer");
    failures += check("DCL SVIEW[0], BUFFER, FLOAT\n", 1, " samplerBuffer");
    failures += check("DCL SVIEW[0], BUFFER, UINT\n", 2, "usamplerBuffer");
    failures += check("DCL SVIEW[0], BUFFER, SINT\n", 1, "isamplerBuffer");
    if (failures) return 1;
    puts("PASS: legacy buffer inference and explicit sampler type precedence");
    return 0;
}
