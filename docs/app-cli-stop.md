# Stop an app-owned native VM

```sh
bridgevm app stop 개발-vm
bridgevm app stop 개발-vm --json --library "/absolute/path/to/native-library"
```

Use the exact VM ID from `bridgevm app list`. The app must already manage the
selected library. See [native CLI setup and commands](app-cli.md).

`stop` uses `bridgevm.app-stop.v1`. It selects one app-owned HVF run and binds the
request to the app instance, library identity, VM ID, accepted registration
digest, run token and retained runner PID. The app checks this identity again
before admitting the stop. A changed or missing saved registration does not
retarget the request. Attached observations and legacy wrapper launches cannot
be stopped through this command; ambiguous targets are refused.

The command waits for the accepted operation. When the guest control service is
ready, the app attempts its existing `shutdown.exe /p /f` command using the file
identity retained at launch. It allows up to 180 seconds before requesting
runner cancellation, followed by up to 12 seconds to observe cleanup. If the
guest control file was replaced, the app requests runner cancellation without
writing to the replacement. Each retained helper and TPM process gets a TERM
grace period, then KILL and reap observation if needed. While cleanup is unconfirmed, an exhausted deadline blocks another launch or
destructive library work. A later fully validated cleanup can release ownership,
but the original operation keeps its missed-deadline failure.

Exit 0 requires both a validated supervisor completion record and the retained
runner's exit, with the output channel fully drained. This confirms owned child
cleanup and media lease release; it does not prove normal Windows shutdown or
that guest applications saved their work. Abrupt runner death, malformed output,
missing cleanup evidence and missed operation deadlines return exit 1.

The CLI keeps one overall 210-second waiting deadline and retries a lost response
with the same target and operation ID. Individual exchanges have a two-second
deadline. Repeating `stop` while the same run remains retained returns its
existing operation, including a completed one. Disconnecting the CLI does not
cancel an admitted stop. The app keeps at most 256 operation records during its
lifetime and refuses new admissions at capacity; existing records remain
queryable. Restarting the app does not restore old operation tickets.

The JSON result includes `schema`, `scope`, `vmID`, `libraryPath`, `target`,
`operationID`, `complete`, `observation`, and `unavailableReason`; absent optional
values may be omitted. The observation carries the stop phase, deadlines,
supervisor cleanup counts and lease disposition, retained runner exit, and any
failure. The existing status protocol remains version 1; stop control uses a
separate strict version 2 message on the same private endpoint.

Exit codes are 0 for confirmed cleanup, 1 for unavailable/refused/unconfirmed
results, and 2 for invalid usage. CLI start is not implemented.

The [native runtime contract](reference/native-owned-runtime.md) describes the
engineering wire formats and retained ownership rules.
