# BridgeVM Tools for Linux

Planned Linux guest daemon responsibilities:

- Heartbeat
- Time sync with host-provided `TimeSync`
- Guest IP reporting with `GuestIpChanged` after network changes and initial connect
- Clipboard sync
- Dynamic resolution
- Shared folders
- Application and window metadata

The shared protocol lives in `crates/bridgevm-agent-protocol`. Current P0
messages validate the authenticated `GuestHello` handshake: protocol version,
non-empty guest OS, optional non-empty agent version, one or more advertised
feature capabilities, and a non-empty per-VM tools auth token. The wire shape
keeps auth optional for forward-compatible decoding, but validation requires it.
The same protocol validates usable guest IP reports, non-zero display resize
dimensions, non-empty share tokens, and bounded guest CPU metrics. Host commands
can carry an optional envelope `request_id`; guests report accepted or failed
work with `CommandResult`.
