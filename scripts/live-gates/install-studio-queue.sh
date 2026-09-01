#!/usr/bin/env bash
# Install the Studio live-gate LaunchAgent for the current user. Idempotent.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
LABEL="com.ketchio.bridgevm-live"
AGENTS="$HOME/Library/LaunchAgents"
PLIST="$AGENTS/$LABEL.plist"
TEMPLATE="$REPO/scripts/live-gates/$LABEL.plist"
WORKER="$REPO/scripts/live-gates/bridgevm-live-worker.sh"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
LOGDIR="$QUEUE_ROOT/logs"
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

free_gib="$(df -g "$HOME" | awk 'NR==2 {print $4}')"
if [ "$free_gib" -lt "$MIN_FREE_GIB" ]; then
    # A warning, not a failure: installing the agent is still correct, but a
    # job will refuse rather than delete canonical media to make room.
    echo "warning: ${free_gib}GiB free, below the ${MIN_FREE_GIB}GiB job guard"
else
    echo "free space: ${free_gib}GiB (guard ${MIN_FREE_GIB}GiB)"
fi

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
mkdir -p "$AGENTS" "$QUEUE_ROOT"/{queued,running,done} "$LOGDIR"
# The queue can name private image paths in raw receipts; keep it to this user.
chmod 700 "$QUEUE_ROOT"

sed -e "s|__WORKER__|$WORKER|g" -e "s|__LOGDIR__|$LOGDIR|g" \
    -e "s|__HOME__|$HOME|g" -e "s|__USER__|$(id -un)|g" "$TEMPLATE" > "$PLIST"
plutil -lint "$PLIST" >/dev/null || fail "generated plist is malformed"

# Idempotent: unload an older revision before loading this one.
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
launchctl enable "gui/$(id -u)/$LABEL"

echo "== installed =="
echo "submit a job:  scripts/live-gates/bridgevm-live submit t1-vtimer"
echo "watch it:      scripts/live-gates/bridgevm-live status"
echo "worker logs:   $LOGDIR/worker.err.log"
echo
echo "One-time user action, if not already granted: the first live gate will"
echo "ask for Screen Recording / Accessibility permission for the terminal"
echo "that runs it. No credentials are stored by this installer."
