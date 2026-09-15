# Native app CLI

`bridgevm app` queries the same `vm.json` registrations as the native macOS app.
It does not open an app window, start or stop a VM, repair registrations, or
change saved resources. Runtime state is reported as `unobserved`; saved CPU and
memory values are not utilization measurements.

```sh
bridgevm app list
bridgevm app list --json
bridgevm app inspect 개발-vm --json
bridgevm app readiness 개발-vm --json
bridgevm app list --library "/absolute/path/to/native-library"
```

Use the exact ID returned by `list`, including Korean IDs. `--library` accepts an
absolute path without `..`; omitting it uses the native app's default library.
`--json` and `--library` can follow the command or appear before it within the
`app` namespace. `bridgevm app --help` and each subcommand's `--help` work without
finding or opening the installed app.

The older root commands, such as `bridgevm --store PATH list`, use a separate
`manifest.yaml` store. `--store` and `--socket` are rejected for `bridgevm app`
before a store is opened or a daemon connection is attempted. `bridgevm hvf`
continues to expose local queries and explicitly enabled probes; it does not
replace native library inventory commands.

## Installing the command

Use a current `BridgeVM.app` or `BridgeVMControl.app` package containing the native CLI protocol
`bridgevm-native-cli-v1`. The package includes the Rust command at
`Contents/Resources/target/release/bridgevm`, paired with
`Contents/MacOS/BridgeVMControl`. A package can be queried directly:

```sh
/Applications/BridgeVM.app/Contents/Resources/target/release/bridgevm app list
```

If you want a short shell command, create a link in a directory already on your
PATH. These commands are optional; BridgeVM does not change shell configuration:

```sh
mkdir -p "$HOME/.local/bin"
ln -s /Applications/BridgeVM.app/Contents/Resources/target/release/bridgevm \
  "$HOME/.local/bin/bridgevm"
```

`ln -s` refuses an existing destination. Ensure the chosen directory is on your
PATH yourself, or use the full path. The executable's canonical location is used
to find its paired app even when invoked through a link. Otherwise discovery
checks `BridgeVM.app`, then `BridgeVMControl.app` in `/Applications`, followed by
those names in the current account's `~/Applications`. It does not use repository, PATH, or helper
environment overrides. The first existing candidate must be compatible;
BridgeVM reports an incompatible installation instead of silently choosing a
different copy.

Discovery reads a bounded regular executable file for the protocol marker before
execution. This is a version-compatibility check, **not signature verification**.
An older app without the marker is refused so that ignoring `--cli` cannot open
its GUI through this command. Forwarding uses direct Unix `exec` with separate
arguments and inherited standard streams; the native command's exit status and
signals are preserved.

## Results and exit codes

`list` and `inspect` JSON use `schema: "bridgevm.app-library.v1"`. The top-level
fields are `schema`, `libraryPath`, `records`, `issues`, and `complete`. Records
include the canonical ID, display name, backend, saved CPU/memory values,
installation-pending flag, configuration and bundle paths, unobserved runtime
state, and recovery-record observations. Optional saved values may be omitted.
Inventory issues remain visible; recovery markers are reported without running
recovery or interpreting guest media.

`readiness` uses `bridgevm.app-readiness.v1` and checks launch inputs without
starting a VM. Release capability limitations remain separate from these launch
prerequisites. A ready result does not prove a guest boot or close a release gate.
Its JSON separates `launchBlockers`, `releaseBlockers`, and `productLimitations`.
`engineChecksPerformed: false` means those engine checks were not evaluated;
an empty release-blocker list in that case is not evidence of release readiness.

| Exit | Inventory (`list` / `inspect`) | `readiness` |
| --- | --- | --- |
| 0 | Complete query | Launch prerequisites ready |
| 1 | Unavailable or incomplete inventory; discovery/exec failure | Blocked, unavailable configuration, or unsupported backend |
| 2 | Invalid command, option, path or ID | Invalid usage |

An existing but empty library returns an empty complete inventory. A missing
library also returns an empty `list` result without creating a directory;
`inspect` for a missing ID reports failure. JSON success is a query result, not
evidence that a VM is running.
