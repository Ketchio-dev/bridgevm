fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at ${STORE:-unknown}" >&2
  exit 1
}

byte_octal() {
  printf '%03o' "$1"
}

write_bytes() {
  local path="$1"
  local offset="$2"
  local bytes="$3"
  printf '%b' "$bytes" | dd of="$path" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

write_byte() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal "$value")"
}

write_le16() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal $((value & 0xff)))\\$(byte_octal $(((value >> 8) & 0xff)))"
}

write_le32() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal $((value & 0xff)))\\$(byte_octal $(((value >> 8) & 0xff)))\\$(byte_octal $(((value >> 16) & 0xff)))\\$(byte_octal $(((value >> 24) & 0xff)))"
}

write_le64() {
  local path="$1"
  local offset="$2"
  local value="$3"
  local bytes=""
  local shift
  for shift in 0 8 16 24 32 40 48 56; do
    bytes+="\\$(byte_octal $(((value >> shift) & 0xff)))"
  done
  write_bytes "$path" "$offset" "$bytes"
}

write_uefi_fv_fixture() {
  local path="$1"
  local size="$2"
  dd if=/dev/zero of="$path" bs="$size" count=1 2>/dev/null
  write_bytes "$path" 16 '\214\214\371\141\322\113\054\117\212\211\042\115\257\334\361\157'
  write_le64 "$path" 32 "$size"
  write_bytes "$path" 40 '_FVH'
  write_le32 "$path" 44 327423
  write_le16 "$path" 48 72
  write_le16 "$path" 52 0
  write_byte "$path" 54 0
  write_byte "$path" 55 2
  write_le32 "$path" 56 1
  write_le32 "$path" 60 "$size"
  write_le32 "$path" 64 0
  write_le32 "$path" 68 0

  local sum
  sum="$(od -An -tu2 -N72 -v "$path" | awk '{ for (i = 1; i <= NF; i++) s = (s + $i) % 65536 } END { print s + 0 }')"
  local checksum=$(((65536 - sum) % 65536))
  write_le16 "$path" 50 "$checksum"
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly included '$needle'; got: $haystack" ;;
  esac
}

assert_not_matches() {
  local haystack="$1"
  local regex="$2"
  local label="$3"
  if printf '%s\n' "$haystack" | grep -Eq "$regex"; then
    fail "$label unexpectedly matched /$regex/; got: $haystack"
  fi
}

install_backend_guards() {
  local label="$1"
  mkdir -p "$FAKE_BIN"
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$FAKE_BIN/$backend" <<SH
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "\$(basename "\$0")" "\$*" >>"\${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in $label: \$(basename "\$0")" >&2
exit 99
SH
    chmod +x "$FAKE_BIN/$backend"
  done

  export PATH="$FAKE_BIN:$PATH"
  export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
  export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"
}

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend or GUI launch attempted: $(cat "$BACKEND_LOG")"
}
