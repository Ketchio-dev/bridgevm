#!/usr/bin/env bash
set -euo pipefail

# D2 notarization boundary. If Apple notarization credentials are present, submit
# the DMG with notarytool and staple. On a free Apple account (no Developer ID,
# no App Store Connect API key), print a labelled EXTERNAL status and exit 0 —
# the boundary is recorded, not hidden. Never a silent success.

usage() {
  cat >&2 <<'EOF'
usage: scripts/notarize-submit.sh --dmg OUT.dmg [--profile NOTARY_PROFILE]

Submits OUT.dmg to Apple notarization when credentials exist:
  --profile NAME   an `xcrun notarytool store-credentials` keychain profile, or
                   set BRIDGEVM_NOTARY_PROFILE. When neither is available the
                   command prints EXTERNAL_NOTARIZATION_REQUIRED and exits 0.
EOF
}

DMG=""
PROFILE="${BRIDGEVM_NOTARY_PROFILE:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg) [[ $# -ge 2 ]] || { usage; exit 2; }; DMG="$2"; shift 2 ;;
    --profile) [[ $# -ge 2 ]] || { usage; exit 2; }; PROFILE="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done

[[ -n "$DMG" ]] || { usage; exit 2; }
[[ -f "$DMG" ]] || { echo "dmg not found: $DMG" >&2; exit 1; }

# A real Developer ID signature must exist before notarization is even possible.
has_developer_id=0
if security find-identity -v -p codesigning 2>/dev/null | grep -q "Developer ID Application"; then
  has_developer_id=1
fi

if [[ -z "$PROFILE" || "$has_developer_id" -eq 0 ]]; then
  echo "EXTERNAL_NOTARIZATION_REQUIRED dmg=$DMG developer_id=$has_developer_id profile=${PROFILE:-<none>}"
  echo "Apple notarization needs a paid Developer ID Application certificate and a"
  echo "stored notarytool credential profile; neither is configured on this host."
  echo "This is the documented free-account boundary, not a build failure."
  exit 0
fi

echo "notarize submit dmg=$DMG profile=$PROFILE"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"
echo "NOTARIZED dmg=$DMG"
