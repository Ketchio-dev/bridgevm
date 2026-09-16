# Creating an own-HVF Windows VM from the CLI

`bridgevm app create-windows` creates the same installation-pending
`hvf-engine` / `windows-hvf` registration used by the native app's Windows
creation sheet. It does not install Windows or start a VM.

```sh
bridgevm app create-windows "개발 VM" \
  --iso "/absolute/path/Windows 11 ARM.iso"
```

The returned canonical ID may differ from the display name and may gain a numeric
suffix when a registration already owns the base ID. Use that exact ID next:

```sh
bridgevm app install 개발-vm
bridgevm app install-status 개발-vm
```

The source ISO must be an absolute, readable regular file and the final path must
not be a symbolic link. BridgeVM reserves a unique destination, clones or copies
the ISO to `disks/installer.iso`, records its SHA-256 identity in the private
pending request, and atomically publishes `vm.json` last. Failure before
publication removes the reserved partial destination. Existing and malformed
library entries are never overwritten.

Optional settings use the same product choices as the app:

```text
--disk-gib 64|96|128|256|512
--memory-mib 2048|4096|6144|8192|12288|16384|24576|32768
--cpus COUNT
--resolution 1280x800|1440x900|1920x1080|2560x1440
--no-network
```

Defaults are 64 GiB, 6144 MiB, 4 CPUs, 1440x900, and shared NAT networking. CPU
count must fit the current host limit. Duplicate flags, relative paths, `..`,
unknown settings, unsupported values, control characters in names, directories,
missing media, and final-component symlinks fail before library creation.

The command accepts no password, recovery key, unattended answer file, guest
payload, driver override, or experimental graphics option. JSON output uses
`bridgevm.app-create-windows.v1` and includes the ID, display name, saved resources,
pending state, backend, boot mode, network choice, and exact saved configuration
digest. It omits the source and managed ISO paths and their media identity.

Exit 0 means the saved registration was read back and matched the accepted request.
Exit 1 means creation or read-back confirmation failed; inspect the library before
retrying because a confirmed registration may exist when only read-back failed.
Exit 2 means invalid usage or input. A successful result proves owned host files
and registration only. It does not prove installation, guest boot, guest health,
graphics, or shutdown.
