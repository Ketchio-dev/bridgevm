#!/usr/bin/env bash
# Sourced by install-verify-smoke.sh after the good fixture is available.
PRERELEASE="$WORK/prerelease"; cp -R "$GOOD_ASSETS" "$PRERELEASE"
python3 -c 'import json,sys;p=sys.argv[1];v=json.load(open(p));v["prerelease"]=True;open(p,"w").write(json.dumps(v))' "$PRERELEASE/github-release.json"
check "exact-version prerelease refuses" fail run_installer "$PRERELEASE" "$DEST"
WRONG_COMMIT="$WORK/wrong-commit"; cp -R "$GOOD_ASSETS" "$WRONG_COMMIT"
python3 -c 'import json,sys;p=sys.argv[1];v=json.load(open(p));v["source_commit"]="b"*40;open(p,"w").write(json.dumps(v))' "$WRONG_COMMIT/$RELEASE_MANIFEST"
write_sums "$WRONG_COMMIT"
check "manifest source commit must match release tag" fail \
  run_installer "$WRONG_COMMIT" "$DEST"
