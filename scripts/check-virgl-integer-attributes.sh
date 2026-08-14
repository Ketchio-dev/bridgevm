#!/usr/bin/env bash
# Deterministic guard for the macOS integer vertex-attribute rule carried in
# scripts/patches/virglrenderer-macos-venus.patch.
#
# Apple's GL binds pure-integer vertex formats through glVertexAttribIPointer,
# so a shader that consumes such an attribute as an integer must declare it with
# an integer GLSL type. The rule that restores Windows text rendering has three
# parts, each of which was individually disproved in its broader or narrower
# form during live A/B testing:
#
#   1. it must not depend on VIRGL_USE_INTEGER (global integer mode regresses DWM),
#   2. it must key off proven integer consumption (UARL/I2F/U2F provenance),
#   3. it must promote per shader, not per input (the glyph vertex shader
#      forwards untyped attributes as raw bits that the fragment shader reads
#      back with floatBitsToUint).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATCH="$ROOT/scripts/patches/virglrenderer-macos-venus.patch"

[[ -f "$PATCH" ]] || {
  echo "virglrenderer patch is missing: $PATCH" >&2
  exit 1
}

errors=0

require() {
  local description="$1" pattern="$2"
  if ! grep -qw -- "$pattern" "$PATCH"; then
    printf 'missing %s (expected %s)\n' "$description" "$pattern" >&2
    errors=$((errors + 1))
  fi
}

reject() {
  local description="$1" pattern="$2"
  if grep -qw -- "$pattern" "$PATCH"; then
    printf 'rejected %s (found %s)\n' "$description" "$pattern" >&2
    errors=$((errors + 1))
  fi
}

# The provenance scan itself, and its three sink opcodes.
require "provenance scanner" "vrend_shader_integer_input_mask"
require "UARL provenance mask" "TGSI_OPCODE_UARL"
require "I2F provenance mask" "TGSI_OPCODE_I2F"
require "U2F provenance mask" "TGSI_OPCODE_U2F"

# Provenance must flow through temporaries: the Windows UMD reaches UARL via a
# MOV into a temporary, so a direct input-to-sink test alone finds nothing.
require "temporary taint tracking" "temp_taint"

# Promotion must be shader-wide once integer consumption is proven.
require "shader-wide promotion" "integer_consumer"

# The float-value UARL workaround must survive for shaders whose address source
# is genuinely a float attribute; only integer-typed attributes take the direct
# path.
require "dual UARL reconstruction" "intBitsToFloat"
require "integer-typed UARL path" "attrib_signed_int_bitmask"

# Rules that live testing disproved and must not come back.
reject "exact DirectWrite layout signature" "R16G16_SINT"
reject "unconditional integer mode" "vrend_state\.use_integer = true"

if [[ $errors -ne 0 ]]; then
  printf 'virgl integer attribute policy: FAIL (%d)\n' "$errors" >&2
  exit 1
fi

printf 'virgl integer attribute policy: PASS\n'
