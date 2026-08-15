# Builds and runs one XCTest-shim suite. Split out of run-xctest-shim-suites.sh
# so the driver stays about the orchestration and this stays about one suite.
#
# Requires ROOT, WORK and TARGET from the caller.

# suite <name> <sources-dir> <tests-dir> [extra swiftc args...]
# -D DEBUG matches `swift test`, which builds debug: the sources gate their
# env-var overrides on DEBUG and the tests exercise those overrides.
suite() {
    local name="$1" sources="$2" tests="$3"; shift 3
    echo "== $name =="
    # macOS bash 3.2 has no globstar; find(1) does the recursive walk.
    local lib_sources=()
    while IFS= read -r file; do lib_sources+=("$file"); done \
        < <(find "$ROOT/$sources" -name '*.swift' | sort)
    local bundle_shim=()
    if grep -rq 'Bundle\.module' "$ROOT/$sources"; then
        bundle_shim=("$WORK/bundle-module.swift")
    fi
    swiftc -D DEBUG -emit-module -emit-library -static -module-name "$name" \
        -target "$TARGET" -enable-testing -o "$WORK/lib$name.a" \
        "${lib_sources[@]}" ${bundle_shim[@]+"${bundle_shim[@]}"} "$@"
    mkdir -p "$WORK/$name"
    python3 "$ROOT/scripts/generate-xctest-manifest.py" \
        "$ROOT/$tests" "$WORK/$name/main.swift"
    swiftc -D DEBUG -target "$TARGET" -I "$WORK" -L "$WORK" -lXCTest "-l$name" \
        -o "$WORK/run-$name" "$ROOT/$tests"/*.swift "$WORK/$name/main.swift" "$@"
    BV_SHIM_RESOURCES="$ROOT/apps/macos/Sources/BridgeVMControl/Resources" \
        "$WORK/run-$name"
}
