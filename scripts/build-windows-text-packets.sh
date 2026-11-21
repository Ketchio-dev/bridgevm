#!/usr/bin/env bash
# Build the project's own ARM64 user-mode text helper; no CRT or driver is linked.
set -euo pipefail
[[ $# -eq 1 && "$1" = /* && ! -e "$1" ]] || { echo 'requires a fresh absolute output path' >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
work=$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-text-packets.XXXXXX")
trap 'rm -rf "$work"' EXIT
ZIG=${ZIG:-zig}
"$ZIG" cc -target aarch64-windows-gnu -c -Os -fno-builtin -std=c11 -Wall -Wextra -Werror \
  "$ROOT/scripts/win-assets/bvagent-text-packets.c" -o "$work/packets.o"
"$ZIG" build-lib -target aarch64-windows-gnu -dynamic -fno-compiler-rt -fentry=DllMain -fstrip \
  "$work/packets.o" -lkernel32 -luser32 -femit-bin="$work/packets.dll"
# The linker stamps the COFF/export headers. Normalize these metadata fields;
# machine code and imports remain exactly as linked. This helper is unsigned.
python3 - "$work/packets.dll" "$1" <<'PY'
from pathlib import Path
import struct
import sys
source, output = map(Path, sys.argv[1:])
data = bytearray(source.read_bytes())
pe = struct.unpack_from('<I', data, 0x3c)[0]
assert data[pe:pe + 4] == b'PE\0\0'
assert struct.unpack_from('<H', data, pe + 4)[0] == 0xaa64
optional = pe + 24
assert struct.unpack_from('<H', data, optional)[0] == 0x20b
section_count = struct.unpack_from('<H', data, pe + 6)[0]
sections = optional + struct.unpack_from('<H', data, pe + 20)[0]
def offset(rva):
    for index in range(section_count):
        entry = sections + index * 40
        size, address, raw_size, raw = struct.unpack_from('<IIII', data, entry + 8)
        if address <= rva < address + max(size, raw_size):
            return raw + rva - address
    raise ValueError('unmapped PE directory')
struct.pack_into('<I', data, pe + 8, 0)
struct.pack_into('<I', data, optional + 64, 0)  # Optional image checksum.
exports = struct.unpack_from('<I', data, optional + 112)[0]
assert exports
struct.pack_into('<I', data, offset(exports) + 4, 0)
imports = struct.unpack_from('<I', data, optional + 120)[0]
names = []
entry = offset(imports)
while any(data[entry:entry + 20]):
    name_rva = struct.unpack_from('<I', data, entry + 12)[0]
    start = offset(name_rva)
    end = data.index(0, start)
    names.append(data[start:end].decode('ascii').lower())
    entry += 20
assert set(names) == {'kernel32.dll', 'user32.dll'}, names
with output.open('xb') as stream:
    stream.write(data)
print('ARM64 text helper built (project code; kernel32/user32 imports only)')
PY
