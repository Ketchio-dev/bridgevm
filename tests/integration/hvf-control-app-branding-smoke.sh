#!/usr/bin/env bash
# Exercise actual bundle metadata and icon packaging without starting an app.
set -euo pipefail
[[ "$(uname -s)" == Darwin ]] || { echo "SKIP: macOS iconutil is required"; exit 0; }
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-branding-test.XXXXXX")"
trap 'rm -rf "$work"' EXIT
app="$work/BridgeVM Control.app"
helper="$root/apps/macos/scripts/bundle-control-app-metadata.sh"
mkdir -p "$app/Contents/Resources"
if "$helper" "$app" 'invalid version' >"$work/refusal.log" 2>&1; then
  echo "FAIL: invalid version accepted" >&2; exit 1
fi
[[ ! -e "$app/Contents/Info.plist" && ! -e "$app/Contents/Resources/BridgeVM.icns" ]]
for target in "$app/Contents/Info.plist" "$app/Contents/Resources/BridgeVM.icns"; do
  ln -s "$work/missing-owned-target" "$target"
  if "$helper" "$app" 1.2.3 >"$work/refusal.log" 2>&1; then
    echo "FAIL: dangling output link accepted" >&2; exit 1
  fi
  [[ -L "$target" && "$(readlink "$target")" == "$work/missing-owned-target" ]]
  rm "$target"
done
"$helper" "$app" 2.3.4-preview.1
/usr/bin/iconutil --convert iconset --output "$work/decoded.iconset" "$app/Contents/Resources/BridgeVM.icns"
python3 - "$app" "$work/decoded.iconset" <<'PY'
import pathlib, plistlib, struct, sys, zlib
app, decoded = map(pathlib.Path, sys.argv[1:])
with (app / 'Contents/Info.plist').open('rb') as stream:
    metadata = plistlib.load(stream)
assert metadata['CFBundleShortVersionString'] == '2.3.4-preview.1'
assert metadata['CFBundleIdentifier'] == 'dev.bridgevm.control'
assert metadata['CFBundleIconFile'] == 'BridgeVM.icns'
assert (app / 'Contents/Resources' / metadata['CFBundleIconFile']).is_file()
expected = {f'icon_{p}x{p}{"@2x" if s == 2 else ""}.png': p*s
            for p in (16, 32, 128, 256, 512) for s in (1, 2)}
assert {p.name for p in decoded.iterdir()} == set(expected)
for name, pixels in expected.items():
    data = (decoded / name).read_bytes()
    assert data[:8] == b'\x89PNG\r\n\x1a\n'
    assert struct.unpack('>II', data[16:24]) == (pixels, pixels)
    cursor, compressed = 8, bytearray()
    while cursor < len(data):
        size = struct.unpack('>I', data[cursor:cursor+4])[0]
        kind = data[cursor+4:cursor+8]
        payload = data[cursor+8:cursor+8+size]
        crc = struct.unpack('>I', data[cursor+8+size:cursor+12+size])[0]
        assert zlib.crc32(kind + payload) & 0xffffffff == crc
        if kind == b'IDAT': compressed.extend(payload)
        cursor += size + 12
    assert cursor == len(data) and zlib.decompress(compressed)
print('PASS: bundle identity, version and 10 decodable icon representations')
PY
before="$(shasum -a 256 "$app/Contents/Info.plist" "$app/Contents/Resources/BridgeVM.icns")"
if "$helper" "$app" 9.9.9 >"$work/refusal.log" 2>&1; then
  echo "FAIL: existing metadata overwritten" >&2; exit 1
fi
[[ "$before" == "$(shasum -a 256 "$app/Contents/Info.plist" "$app/Contents/Resources/BridgeVM.icns")" ]]
echo "PASS: invalid input and overwrite refusal preserve the staged bundle"
