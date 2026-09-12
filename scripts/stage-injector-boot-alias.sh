#!/usr/bin/env bash
# Stage the loader path used by existing injector Boot0003, without editing vars.
set -euo pipefail
[[ $# -eq 1 && "$1" == /* ]] || { echo 'FAIL: absolute staging volume required' >&2; exit 2; }
volume="$1"
for relative in '' efi efi/boot efi/microsoft efi/microsoft/boot; do
  [[ -d "$volume/$relative" && ! -L "${volume}${relative:+/$relative}" ]] || {
    echo 'FAIL: real EFI staging directories required' >&2; exit 1;
  }
done
source="$volume/efi/boot/bootaa64.efi"
destination="$volume/efi/microsoft/boot/bootmgfw.efi"
[[ -f "$source" && -s "$source" && ! -L "$source" ]] || {
  echo 'FAIL: regular nonempty ARM64 media loader required' >&2; exit 1;
}
if [[ -e "$destination" || -L "$destination" ]]; then
  [[ -f "$destination" && -s "$destination" && ! -L "$destination" ]] || {
    echo 'FAIL: invalid existing boot manager' >&2; exit 1;
  }
  echo 'Injector boot manager already present; preserving ISO bytes.'
  exit 0
fi
/bin/cp -X "$source" "$destination"
cmp -s "$source" "$destination" || { echo 'FAIL: boot loader copy differs' >&2; exit 1; }
/usr/sbin/dot_clean -m "$volume/efi/microsoft/boot"
echo 'Staged byte-identical injector Boot0003 loader alias; boot not proven by staging.'
