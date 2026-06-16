# BridgeVM Tools for macOS Guests

This is a later-stage helper for macOS guest integration.

The shared agent protocol already carries the cross-guest P0 contract in
`crates/bridgevm-agent-protocol`: authenticated hello with agent version and
feature capability advertisement plus per-VM tools token auth, heartbeat, host
time sync, guest IP reporting,
clipboard text sync, display resize, shared-folder mount commands,
application/window control scaffolding, guest metrics, optional command
`request_id`s, and `CommandResult` acknowledgements.
