# Native app CLI diagnostics

`bridgevm app doctor` checks the ordered executable candidates and bounded
`bridgevm-native-cli-v1` marker used by normal native commands. It does not open
the app, create a library, or contact a VM.

```sh
bridgevm app doctor
bridgevm app doctor --json
```

The report lists every bounded candidate and identifies the executable normal
app commands would select. The first installed candidate controls selection, so
an incompatible earlier installation blocks a compatible later copy and both
remain visible in the report. A ready report exits 0; a blocked report is printed
before the command exits 1.

JSON uses schema `bridgevm.app-doctor.v1`. `selectedExecutable` is null when
discovery is blocked. Candidate status is `missing`, `compatible`, `incompatible`,
`invalid`, or `unreadable`.

This check establishes only executable discovery and the native CLI protocol
marker. It does not verify the code signature, inspect VM library contents, or
measure guest health. If `--library` is supplied, the path is echoed as context
and `libraryChecked` remains false.
