#!/usr/bin/env bash
# The caller starts a trusted shell; clear loader overrides before any child.
unset LD_PRELOAD LD_LIBRARY_PATH LD_AUDIT
for bridgevm_t22_loader_name in "${!DYLD_@}"; do unset "$bridgevm_t22_loader_name"; done
unset bridgevm_t22_loader_name
checked_git() {
    /usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin \
        GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
        /usr/bin/perl -e 'alarm 30; exec @ARGV or die "fixed git unavailable"' \
        /usr/bin/git --no-optional-locks -c core.fsmonitor=false -c core.untrackedCache=false -C "$ROOT" "$@"
}
