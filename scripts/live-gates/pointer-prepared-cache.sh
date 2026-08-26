#!/usr/bin/env bash
# Identity predicate for immutable B4 prepared sources.
pointer_metadata_value() {
  awk -F= -v key="$2" '$1==key{print substr($0,index($0,"=")+1)}' "$1"
}
pointer_write_result() {
  local dir="$1" image="$2" vars="$3" out="$4" package="$5" umd="$6" inf="$7" commit="$8"
  printf 'target=%s\nvars=%s\nimage_sha256=%s\nvars_sha256=%s\nviogpu_package_sha256=%s\ndriver_version=120.50.0.0\ninstalled_umd_sha256=%s\ninstalled_inf_sha256=%s\nprepared_commit=%s\n' \
    "$dir/disk.raw" "$dir/vars.fd" "$image" "$vars" "$package" "$umd" "$inf" "$commit" > "$out/source.env"
}

pointer_cache_matches() {
  local meta="$1" source_image="$2" source_vars="$3" package="$4" umd="$5" inf="$6" commit="$7"
  [[ -f "$meta" && ! -L "$meta" ]] || return 1
  [[ $(pointer_metadata_value "$meta" firstboot_ready) == true \
    && $(pointer_metadata_value "$meta" installed_package_verified) == true \
    && $(pointer_metadata_value "$meta" driver_version) == 120.50.0.0 \
    && $(pointer_metadata_value "$meta" source_image_sha256) == "$source_image" \
    && $(pointer_metadata_value "$meta" source_vars_sha256) == "$source_vars" \
    && $(pointer_metadata_value "$meta" viogpu_package_sha256) == "$package" \
    && $(pointer_metadata_value "$meta" installed_umd_sha256) == "$umd" \
    && $(pointer_metadata_value "$meta" installed_inf_sha256) == "$inf" \
    && $(pointer_metadata_value "$meta" prepared_commit) == "$commit" ]]
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  set -euo pipefail
  work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
  meta="$work/retained.env"
  {
    echo firstboot_ready=true; echo installed_package_verified=true
    echo driver_version=120.50.0.0; echo source_image_sha256=image
    echo source_vars_sha256=vars; echo viogpu_package_sha256=package-a; echo prepared_commit=code-a
    echo installed_umd_sha256=umd; echo installed_inf_sha256=inf
  } > "$meta"
  pointer_cache_matches "$meta" image vars package-a umd inf code-a
  ! pointer_cache_matches "$meta" image vars package-b umd inf code-a
  ! pointer_cache_matches "$meta" image vars package-a umd inf code-b
  sed '/viogpu_package_sha256/d' "$meta" > "$work/legacy.env"
  ! pointer_cache_matches "$work/legacy.env" image vars package-a umd inf code-a
  printf 'PASS: pointer prepared cache binds package and preparation code\n'
fi
