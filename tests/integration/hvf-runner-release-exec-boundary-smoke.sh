#!/usr/bin/env bash
# Exercise the actual release binary: legacy repository and PATH execution must fail closed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
cargo +1.97.0 build --locked --release -p hvf-runner --quiet
cargo +1.97.0 build --locked -p hvf-runner --quiet
runner="${CARGO_TARGET_DIR:-$ROOT/target}/release/hvf-runner"
debug_runner="${CARGO_TARGET_DIR:-$ROOT/target}/debug/hvf-runner"
[[ -x "$runner" ]] || { echo "release hvf-runner missing: $runner" >&2; exit 1; }
[[ -x "$debug_runner" ]] || { echo "debug hvf-runner missing: $debug_runner" >&2; exit 1; }

store="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-runner-release.XXXXXX")"
trap 'rm -rf "$store"' EXIT
fake_repo="$store/fake-repo"
fake_bin="$store/bin"
marker="$store/executed"
mkdir -p "$fake_repo/scripts" "$fake_bin"
cat > "$fake_repo/scripts/run-hvf-windows-installed-boot.sh" <<'WRAPPER'
#!/usr/bin/env bash
printf 'wrapper\n' >> "$EXEC_MARKER"
WRAPPER
cat > "$fake_bin/bridgevm-fake-helper" <<'HELPER'
#!/usr/bin/env bash
printf 'path-helper\n' >> "$EXEC_MARKER"
HELPER
chmod +x "$fake_repo/scripts/run-hvf-windows-installed-boot.sh" "$fake_bin/bridgevm-fake-helper"
printf 'image' > "$store/disk.raw"
printf 'image' > "$store/vars.fd"
manifest="$store/launch.json"
printf '{"version":1,"disk":"%s","uefi_vars":"%s","ram_mib":1024,"vcpus":1}\n' \
  "$store/disk.raw" "$store/vars.fd" > "$manifest"

fail() { echo "release runner execution boundary: FAIL ($*)" >&2; exit 1; }
expect_refused() {
  local label="$1" expected="$2" output
  shift 2
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded: $output"
  fi
  [[ "$output" == *"$expected"* ]] || fail "$label missed '$expected': $output"
  [[ ! -e "$marker" ]] || fail "$label executed a repository/PATH command"
}

# A debug control proves this valid manifest reaches PATH-based swtpm execution.
mkdir -p "$store/tpm-control"
if debug_output="$(env EXEC_MARKER="$marker" PATH="$fake_bin:$PATH" "$debug_runner" \
  --launch-spec "$manifest" --helper /usr/bin/true \
  --helper-vtpm-state "$store/tpm-control" --helper-swtpm-bin bridgevm-fake-helper 2>&1)"; then
  fail "debug swtpm control unexpectedly completed: $debug_output"
fi
[[ -f "$marker" && "$(cat "$marker")" == 'path-helper' ]] || fail 'debug swtpm PATH control did not execute'
rm "$marker"

launch_args=(--launch --target "$store/disk.raw" --vars "$store/vars.fd" --evidence-dir "$store/evidence")
expect_refused 'explicit repo root' 'unavailable in release builds' \
  env EXEC_MARKER="$marker" "$runner" "${launch_args[@]}" --repo-root "$fake_repo"
expect_refused 'environment repo root' 'unavailable in release builds' \
  env EXEC_MARKER="$marker" BRIDGEVM_REPO_ROOT="$fake_repo" "$runner" "${launch_args[@]}"
expect_refused 'repo override before typed dispatch' 'unavailable in release builds' \
  "$runner" --repo-root "$fake_repo" --launch-spec "$store/absent.json"
expect_refused 'legacy launch before typed dispatch' 'unavailable in release builds' \
  "$runner" --launch --launch-spec "$store/absent.json"
expect_refused 'PATH supervise' 'unavailable in release builds' \
  env EXEC_MARKER="$marker" PATH="$fake_bin:$PATH" "$runner" --supervise bridgevm-fake-helper
expect_refused 'PATH helper' 'release helper executable path must be absolute: --helper' \
  env EXEC_MARKER="$marker" PATH="$fake_bin:$PATH" "$runner" \
    --launch-spec "$manifest" --helper bridgevm-fake-helper
expect_refused 'PATH swtpm' 'release helper executable path must be absolute: --helper-swtpm-bin' \
  env EXEC_MARKER="$marker" PATH="$fake_bin:$PATH" "$runner" \
    --launch-spec "$manifest" --helper /usr/bin/true \
    --helper-vtpm-state "$store/tpm" --helper-swtpm-bin bridgevm-fake-helper
expect_refused 'typed route with absolute helper' 'read launch manifest' \
  "$runner" --launch-spec "$store/absent.json" --helper /usr/bin/true

echo 'release runner execution boundary: PASS (actual release binary; no VM)'
