# Stop an app-owned native VM

```sh
bridgevm app stop dev-vm
bridgevm app stop dev-vm --json --library "/absolute/path/to/native-library"
```

Use the exact VM ID from `bridgevm app list`. The app must already manage the
selected library. See [native CLI setup and commands](app-cli.md).

`stop` uses `bridgevm.app-stop.v2`. It selects one app-owned HVF run and binds the
request to the app instance, library identity, VM ID, run token and retained
runner PID. The app stops only the run with exactly that token and PID for that
VM ID. Saved settings play no part, so editing or removing the saved registration
neither retargets nor refuses the request. Attached observations and legacy
wrapper launches cannot be stopped through this command; ambiguous targets are
refused.

The command waits for the accepted operation. When the guest control service is
ready, the app sends `shutdown.exe /s /t 0` using the file identity retained at
launch; Windows asks its apps to close, so an app with unsaved work can hold the
shutdown on Windows' own screen. When the service is not ready (the login screen,
early boot), the app presses the guest's ACPI power button once instead, by
appending `POWERBUTTON` to the live input channel, and Windows shuts down the
same way; the agent's command still follows if the service becomes ready first.
The command allows up to 180 seconds before
requesting runner cancellation, followed by up to 12 seconds to observe cleanup.
If the guest control file was replaced, the app requests runner cancellation
without writing to the replacement. Each retained helper and TPM process gets a
TERM grace period, then KILL and reap observation if needed; on TERM the HVF
helper stops its vCPUs and saves the UEFI vars and disk before it exits. If the
app's own Shut Down is already waiting for Windows, the command joins that operation,
which does not end by itself after 180 seconds: the app offers Force Stop instead. While cleanup is unconfirmed, an exhausted deadline blocks another launch or
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
cancel an admitted stop. The app keeps the newest 256 finished operation records
for status queries; operations still in progress are always kept. Restarting the
app does not restore old operation tickets. A refused stop prints one plain
sentence saying why and what to do (for example that the VM is not running in
the app, or that the app only observes it).

The JSON result includes `schema`, `scope`, `vmID`, `libraryPath`, `target`,
`operationID`, `complete`, `observation`, and `unavailableReason`; absent optional
values may be omitted. The observation carries the stop phase, deadlines,
supervisor cleanup counts and lease disposition, retained runner exit, and any
failure. The existing status protocol remains version 1; stop control uses a
separate version 4 message on the same private endpoint.

Exit codes are 0 for confirmed cleanup, 1 for unavailable/refused/unconfirmed
results, and 2 for invalid usage. See [CLI start](app-cli-start.md) for startup.

The [native runtime contract](reference/native-owned-runtime.md) describes the
engineering wire formats and retained ownership rules.
