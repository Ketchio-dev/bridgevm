#!/usr/bin/env bash

prepare_media_file() {
  local src="$1" dst="$2"
  mkdir -p "$(dirname "$dst")"
  rm -f "$dst"
  if cp -c "$src" "$dst" 2>/dev/null; then
    :
  elif [[ "$COPY_MEDIA" == "1" ]]; then
    cp "$src" "$dst"
  else
    fail "failed to clone media with 'cp -c': $src -> $dst; use --copy-media for a full copy or --no-clone-media to reuse media"
  fi
  chmod u+rw "$dst" 2>/dev/null || true
}

verify_matrix_media() {
  local run_dir="$1" run_target="$2" run_vars="$3" run="$4" smp="$5"
  local expected_target="$6" expected_vars="$7" actual_target actual_vars
  [[ -n "$expected_target" ]] || return 0
  actual_target="$(openssl dgst -sha256 -r "$run_target" | cut -d' ' -f1)"
  actual_vars="$(openssl dgst -sha256 -r "$run_vars" | cut -d' ' -f1)"
  printf 'target_sha256=%s\nvars_sha256=%s\n' "$actual_target" "$actual_vars" > "$run_dir/media-integrity.txt"
  [[ "$actual_target" == "$expected_target" ]] || fail "run $run SMP $smp target clone hash mismatch"
  [[ "$actual_vars" == "$expected_vars" ]] || fail "run $run SMP $smp vars clone hash mismatch"
}
