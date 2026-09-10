#!/usr/bin/env bash
# Deterministic contract: neither submission nor dispatch may run t0 on a Mac.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "${WORK:?}"' EXIT
export BRIDGEVM_LIVE_ROOT="$WORK/queue"
export VENUE_MARKER="$WORK/project-check-ran"
checks=0
fail() { echo "FAIL: $*" >&2; exit 1; }
refused() {
    local status=0 log="$WORK/refusal-$checks.log"
    "$@" > "$log" 2>&1 || status=$?
    [[ "$status" -eq 2 ]] || fail "expected refusal status 2, got $status"
    grep -Fq 'deterministic checks belong on GitHub-hosted Actions' "$log" ||
        fail "refusal did not explain the execution venue"
    checks=$((checks + 2))
}
refused "$REPO/scripts/live-gates/bridgevm-live" submit t0-check --job-id Venue.deterministic
[[ ! -e "$BRIDGEVM_LIVE_ROOT" ]] || fail "refused submission created queue state"
checks=$((checks + 1))

# Run a copy with a harmless sentinel instead of the real project check.
# A regression must not recursively invoke the test suite or start a VM.
mkdir -p "$WORK/repo/scripts/live-gates"
cp "$REPO/scripts/live-gates/run-tier.sh" "$WORK/repo/scripts/live-gates/"
cat > "$WORK/repo/scripts/check-project.sh" <<'SH'
#!/usr/bin/env bash
printf 'unexpected project check\n' > "${VENUE_MARKER:?}"
SH
chmod +x "$WORK/repo/scripts/check-project.sh"
refused bash "$WORK/repo/scripts/live-gates/run-tier.sh" t0-check --out "$WORK/result"
[[ ! -e "$VENUE_MARKER" && ! -e "$WORK/result" ]] ||
    fail "refused dispatch executed the check or created a result directory"
checks=$((checks + 1))
python3 "$REPO/tests/integration/live-worker-venue-contract.py" && printf 'PASS: deterministic venue contract (%s checks plus worker contract)\n' "$checks"
