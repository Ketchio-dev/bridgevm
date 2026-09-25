#!/usr/bin/env bash
# Cargo's reported executable, not a stale default target, must be packaged.
set -euo pipefail
unset CARGO_BUILD_TARGET

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
store="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-cargo-provenance.XXXXXX")" && pwd -P)"
trap 'rm -rf "$store"' EXIT
fixture="$store/repo"
fake_bin="$store/bin"
mkdir -p "$fixture/apps/macos/scripts" "$fixture/target/release/examples" "$fake_bin" "$store/elsewhere" "$store/output"
cp "$ROOT/apps/macos/scripts/"{build-sign-hvf-runner.sh,build-sign-hvf-windows-probe.sh,cargo-built-artifact.py,package-hvf-product-e2e.sh} \
  "$fixture/apps/macos/scripts/"
printf '[workspace]\n' > "$fixture/Cargo.toml"
printf 'entitlements\n' > "$fixture/apps/macos/HvfRunner.entitlements"
printf 'entitlements\n' > "$fixture/apps/macos/HvfRunner.release.entitlements"
for name in hvf-runner bridgevm; do
  printf 'stale:%s\n' "$name" > "$fixture/target/release/$name"
  chmod +x "$fixture/target/release/$name"
done
printf 'stale:hvf_gic_boot_probe\n' > "$fixture/target/release/examples/hvf_gic_boot_probe"
chmod +x "$fixture/target/release/examples/hvf_gic_boot_probe"
printf 'stale:snapshot_pair_cli\n' > "$fixture/target/release/examples/snapshot_pair_cli"
chmod +x "$fixture/target/release/examples/snapshot_pair_cli"
cat > "$fixture/apps/macos/scripts/package-product-e2e-helper-app.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ -d "$1/Contents/MacOS" && -x "$2/BridgeVMProductE2E" ]]
SH
chmod +x "$fixture/apps/macos/scripts/package-product-e2e-helper-app.sh"

cat > "$fake_bin/cargo" <<'PY'
#!/usr/bin/env python3
import json
import os
from pathlib import Path
import struct
import sys

args = sys.argv[1:]
root = Path(os.environ['EXPECTED_ROOT']).resolve()
assert Path.cwd() == root, 'Cargo ran outside the exact repository root'
assert args[0] in ('build', 'metadata')
assert args[args.index('--manifest-path') + 1] == str(root / 'Cargo.toml')
if args[0] == 'metadata':
    assert '--no-deps' in args and '--format-version' in args
    packages = []
    for name, targets in (
        ('hvf-runner', [('hvf-runner', 'bin')]),
        ('bridgevm-cli', [('bridgevm', 'bin')]),
        ('bridgevm-hvf', [('hvf_gic_boot_probe', 'example'), ('snapshot_pair_cli', 'example')]),
        ('wrong-package', [('hvf-runner', 'bin')]),
    ):
        packages.append({'name': name, 'id': 'fake-id-' + name,
                         'targets': [{'name': target, 'kind': [kind]} for target, kind in targets]})
    print(json.dumps({'packages': packages}))
    sys.exit(0)
assert '--message-format=json' in args
with open(os.environ['FAKE_CARGO_LOG'], 'a') as log:
    log.write('called\n')
if os.environ.get('FAKE_CARGO_FAIL'):
    print('synthetic Cargo failure', file=sys.stderr)
    sys.exit(43)
package = args[args.index('-p') + 1]
if package == 'bridgevm-hvf':
    name, kind = args[args.index('--example') + 1], 'example'
else:
    name, kind = {'hvf-runner': ('hvf-runner', 'bin'),
                  'bridgevm-cli': ('bridgevm', 'bin')}[package]
target_dir = Path(os.environ['CARGO_TARGET_DIR'])
if not target_dir.is_absolute():
    target_dir = root / target_dir
profile = 'release' if '--release' in args else 'debug'
path = target_dir / profile / ('examples' if kind == 'example' else '') / name
path.parent.mkdir(parents=True, exist_ok=True)
cpu = 0x01000007 if os.environ.get('FAKE_CARGO_X86') else 0x0100000c
path.write_bytes(struct.pack('<IIIIIIII', 0xfeedfacf, cpu, 0, 2, 0, 0, 0, 0) + name.encode())
path.chmod(0o755)
message = {'reason': 'compiler-artifact', 'target': {'name': name, 'kind': [kind]},
           'package_id': 'fake-id-' + ('wrong-package' if os.environ.get('FAKE_CARGO_WRONG_PACKAGE') else package),
           'executable': str(path)}
print(json.dumps(message))
if os.environ.get('FAKE_CARGO_DUPLICATE'):
    print(json.dumps(message))
PY
cat > "$fake_bin/codesign" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  --force) bin="${*: -1}"; [[ -x "$bin" ]]; printf '%s\n' "$bin" >> "$CODESIGN_LOG" ;;
  --verify) bin="${*: -1}"; [[ -x "$bin" ]] ;;
  -d) printf '<key>com.apple.security.hypervisor</key><true/>\n' ;;
  *) exit 1 ;;
esac
SH
chmod +x "$fake_bin/cargo" "$fake_bin/codesign"
export PATH="$fake_bin:$PATH" EXPECTED_ROOT="$fixture"
export FAKE_CARGO_LOG="$store/cargo.log" CODESIGN_LOG="$store/codesign.log"
runner_script="$fixture/apps/macos/scripts/build-sign-hvf-runner.sh"
probe_script="$fixture/apps/macos/scripts/build-sign-hvf-windows-probe.sh"
helper="$fixture/apps/macos/scripts/cargo-built-artifact.py"
fail() { echo "macOS Cargo artifact provenance: FAIL ($*)" >&2; exit 1; }

runner_output="$store/output/hvf-runner"
(cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/isolated-runner" \
  "$runner_script" --release --output "$runner_output") > "$store/runner.stdout" || fail 'runner signer failed'
cmp -s "$runner_output" "$store/isolated-runner/release/hvf-runner" || fail 'runner installed stale executable'
[[ "$(cat "$store/runner.stdout")" == "$runner_output" ]] || fail 'runner reported wrong output'
rg -Fxq "$store/isolated-runner/release/hvf-runner" "$CODESIGN_LOG" || fail 'built runner was not signed'

probe_output="$store/output/hvf_gic_boot_probe"
(cd "$store/elsewhere" && CARGO_TARGET_DIR=isolated-probe \
  "$probe_script" --release --output "$probe_output") > "$store/probe.stdout" || fail 'probe signer failed'
cmp -s "$probe_output" "$fixture/isolated-probe/release/examples/hvf_gic_boot_probe" || fail 'probe installed stale executable'
rg -Fxq "$fixture/isolated-probe/release/examples/hvf_gic_boot_probe" "$CODESIGN_LOG" || fail 'built probe was not signed'

cli="$(cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/isolated-cli" python3 "$helper" \
  --root "$fixture" --target bridgevm --kind bin -- build --locked --release -p bridgevm-cli)" || fail 'CLI artifact selection failed'
[[ "$cli" == "$store/isolated-cli/release/bridgevm" ]] || fail 'CLI selected stale executable'
! cmp -s "$cli" "$fixture/target/release/bridgevm" || fail 'CLI selected stale executable'
package="$ROOT/apps/macos/scripts/package-hvf-control-app.sh"
rg -Fq 'bridgevm_cli_bin="$(python3 "$MACOS_DIR/scripts/cargo-built-artifact.py"' "$package" || fail 'package does not use exact Cargo artifact'
rg -Fq 'install -m 755 "$bridgevm_cli_bin"' "$package" || fail 'package does not install exact CLI artifact'

staged_app="$store/output/BridgeVM.app"
swift_bin="$store/swift-bin"
mkdir -p "$staged_app/Contents/MacOS" "$staged_app/Contents/Resources/target/release/examples" "$swift_bin"
printf 'fixture\n' > "$swift_bin/BridgeVMProductE2E"
chmod +x "$swift_bin/BridgeVMProductE2E"
(cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/isolated-snapshot" \
  "$fixture/apps/macos/scripts/package-hvf-product-e2e.sh" "$staged_app" "$swift_bin" -) || fail 'snapshot package failed'
snapshot="$staged_app/Contents/Resources/target/release/examples/snapshot_pair_cli"
cmp -s "$snapshot" "$store/isolated-snapshot/release/examples/snapshot_pair_cli" || fail 'snapshot package installed stale executable'
rg -Fxq "$snapshot" "$CODESIGN_LOG" || fail 'isolated snapshot output was not signed'
rg -Fq "$fixture/target/release/" "$CODESIGN_LOG" && fail 'a stale default-target executable was signed'

before_cargo="$(wc -l < "$FAKE_CARGO_LOG")"
before_sign="$(wc -l < "$CODESIGN_LOG")"
if (cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/rejected" CARGO_BUILD_TARGET=x86_64-apple-darwin \
  "$runner_script" --release --output "$store/output/rejected") > "$store/rejected.stdout" 2> "$store/rejected.stderr"; then
  fail 'cross-target override was accepted'
fi
rg -Fq 'CARGO_BUILD_TARGET is unsupported' "$store/rejected.stderr" || fail 'cross-target diagnostic missing'
[[ "$(wc -l < "$FAKE_CARGO_LOG")" == "$before_cargo" && "$(wc -l < "$CODESIGN_LOG")" == "$before_sign" ]] || fail 'cross-target attempted build/sign'
[[ ! -e "$store/output/rejected" ]] || fail 'cross-target produced output'

if (cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/failed" FAKE_CARGO_FAIL=1 \
  "$runner_script" --release --output "$store/output/failed") > "$store/failed.stdout" 2> "$store/failed.stderr"; then
  fail 'failed Cargo build was accepted'
else
  result=$?
  [[ "$result" == 43 ]] || fail "Cargo exit status was lost: $result"
fi
rg -Fq 'synthetic Cargo failure' "$store/failed.stderr" || fail 'Cargo stderr was lost'
[[ "$(wc -l < "$CODESIGN_LOG")" == "$before_sign" && ! -e "$store/output/failed" ]] || fail 'failed Cargo build was signed or installed'

if (cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/duplicate" FAKE_CARGO_DUPLICATE=1 \
  python3 "$helper" --root "$fixture" --target hvf-runner --kind bin -- build --locked --release -p hvf-runner) \
  > "$store/duplicate.stdout" 2> "$store/duplicate.stderr"; then
  fail 'ambiguous Cargo artifacts were accepted'
fi
rg -Fq 'expected one bin executable named hvf-runner; found 2' "$store/duplicate.stderr" || fail 'ambiguous artifact diagnostic missing'

before_sign="$(wc -l < "$CODESIGN_LOG")"
if (cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/wrong-package" FAKE_CARGO_WRONG_PACKAGE=1 \
  "$runner_script" --release --output "$store/output/wrong-package") \
  > "$store/wrong-package.stdout" 2> "$store/wrong-package.stderr"; then
  fail 'wrong-package artifact was accepted'
fi
rg -Fq 'expected one bin executable named hvf-runner; found 0' "$store/wrong-package.stderr" || fail 'wrong-package diagnostic missing'
[[ ! -e "$store/output/wrong-package" ]] || fail 'wrong-package artifact was installed'
[[ "$(wc -l < "$CODESIGN_LOG")" == "$before_sign" ]] || fail 'wrong-package artifact was signed'

if (cd "$store/elsewhere" && CARGO_TARGET_DIR="$store/x86-only" FAKE_CARGO_X86=1 \
  "$runner_script" --release --output "$store/output/x86-only") \
  > "$store/x86-only.stdout" 2> "$store/x86-only.stderr"; then
  fail 'x86-only artifact was accepted'
fi
rg -Fq 'requires an arm64 Mach-O slice' "$store/x86-only.stderr" || fail 'x86-only diagnostic missing'
[[ ! -e "$store/output/x86-only" ]] || fail 'x86-only artifact was installed'
[[ "$(wc -l < "$CODESIGN_LOG")" == "$before_sign" ]] || fail 'x86-only artifact was signed'

echo 'macOS Cargo artifact provenance: PASS (isolated exact outputs signed/installed; stale target refused)'
