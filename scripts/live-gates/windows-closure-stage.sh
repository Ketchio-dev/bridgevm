#!/usr/bin/env bash
# Called only for the private per-job injection stage; canonical inputs stay immutable.
stage_closure_inputs() {
  cp -c "$IMAGE" "$STAGE/disk.raw" || return 1
  cp "$INJECTOR_VARS" "$STAGE/vars.fd" || return 1
  cp -c "$INJECTOR" "$STAGE/injector.raw" || return 1
  chmod 600 "$STAGE/disk.raw" "$STAGE/vars.fd" "$STAGE/injector.raw" || return 1
}
