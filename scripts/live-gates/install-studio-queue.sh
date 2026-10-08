#!/usr/bin/env bash
# Install the Studio live-gate LaunchAgent for the current user. Idempotent.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
LABEL="com.ketchio.bridgevm-live"
AGENTS="$HOME/Library/LaunchAgents"
PLIST="$AGENTS/$LABEL.plist"
TEMPLATE="$REPO/scripts/live-gates/$LABEL.plist"
WORKER="$REPO/scripts/live-gates/bridgevm-live-worker.sh"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"; WORK_ROOT="${BRIDGEVM_LIVE_WORK:-$HOME/BridgeVM/live-work}"
LOGDIR="$HOME/Library/Logs/BridgeVM"
MIN_FREE_GIB="${BRIDGEVM_LIVE_MIN_FREE_GIB:-100}"

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

fail() { echo "preflight: $*" >&2; exit 1; }

echo "== preflight =="
REPO_PHYSICAL="$(cd "$REPO" && pwd -P)"
case "$REPO_PHYSICAL/" in
    "$HOME/Desktop/"*|"$HOME/Documents/"*|"$HOME/Downloads/"*)
        fail "clone the repository outside Desktop/Documents/Downloads; LaunchAgent privacy policy blocks $REPO_PHYSICAL"
        ;;
esac

[ -x "$WORKER" ] || fail "worker is not executable: $WORKER"
[ -f "$TEMPLATE" ] || fail "missing plist template: $TEMPLATE"

command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required (receipt redaction)"
command -v caffeinate >/dev/null && command -v taskpolicy >/dev/null || fail "caffeinate and taskpolicy are required (comparable long gates)"

if ! command -v cargo >/dev/null; then
    fail "cargo is required; install the pinned toolchain first"
fi

QUEUE_ROOT="$(python3 -I -B "$REPO/scripts/live-gates/queue-root-path.py" "$QUEUE_ROOT")" || fail "storage capacity unavailable or configuration invalid"; /bin/bash "$REPO/scripts/live-gates/installer-storage-preflight.sh" "$REPO" "$QUEUE_ROOT" "$WORK_ROOT" "$MIN_FREE_GIB"

# A registered runner on a public repo is the thing this design exists to
# avoid, so refuse to install alongside one.
if [ -d "$HOME/actions-runner" ] || pgrep -qf 'Runner.Listener' 2>/dev/null; then
    fail "a GitHub Actions runner is present; this queue must not run beside one"
fi

echo "repo:   $REPO"
echo "queue:  $QUEUE_ROOT"
echo "agent:  $PLIST"

if [ "$DRY_RUN" -eq 1 ]; then
    echo "== dry run, nothing installed =="
    exit 0
fi

echo "== install =="
bash "$REPO/scripts/live-gates/queue-directories.sh" "$QUEUE_ROOT"
mkdir -p "$AGENTS" "$LOGDIR" "$WORK_ROOT"
chmod 700 "$LOGDIR" "$WORK_ROOT" # Logs, builds and queue receipts can name private paths.

python3 -I -B "$REPO/scripts/live-gates/render-live-launchagent.py" "$TEMPLATE" "$HOME" "$(id -un)" "$WORKER" "$LOGDIR" "$QUEUE_ROOT" "$WORK_ROOT" "$MIN_FREE_GIB" > "$PLIST"
plutil -lint "$PLIST" >/dev/null || fail "generated plist is malformed"

# Idempotent: unload an older revision before loading this one.
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
launchctl enable "gui/$(id -u)/$LABEL"

echo "== installed =="
printf 'submit a job:  BRIDGEVM_LIVE_ROOT=%q scripts/live-gates/bridgevm-live submit t1-vtimer\n' "$QUEUE_ROOT"
printf 'watch it:      BRIDGEVM_LIVE_ROOT=%q scripts/live-gates/bridgevm-live status\n' "$QUEUE_ROOT"
echo "worker logs:   $LOGDIR/worker.err.log"
echo
echo "External-volume access requires operator-granted Full Disk Access for /bin/bash."
echo "Screen Recording / Accessibility for the live helpers are separate permissions."
echo "This installer grants no permissions and stores no credentials."
