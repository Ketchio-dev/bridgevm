# Creating an own-HVF Windows VM from the CLI

`bridgevm app create-windows` creates an installation-pending `hvf-engine` /
`windows-hvf` registration. It does not install Windows or start a VM.

```sh
bridgevm app create-windows "개발 VM" \
  --iso "/absolute/path/Windows 11 ARM.iso"
```

The canonical ID may differ from the name or gain a collision suffix. Inspect it:
```sh
bridgevm app inspect 개발-vm
```

**Installation is blocked for CLI-created entries:** this command does not stage
the required signed ARM64 guest driver payload or its manifest. Neither the app
nor CLI can add those inputs to an existing entry. For installation, create a new
VM in the app with the ISO, signed payload and manifest described in
[installation prerequisites](install.md#what-you-need-after-installing).
`bridgevm app install` controls entries that already have those sealed inputs;
retrying it cannot complete a CLI-created entry.

The ISO must be an absolute readable regular file, not a final-component symlink.
BridgeVM reserves a unique destination, copies the ISO to `disks/installer.iso`,
records its SHA-256 in the private request, and publishes `vm.json` last.
Pre-publication failure removes the partial destination; existing or malformed
library entries are never overwritten.

Optional settings use the same product choices as the app:

```text
--disk-gib 64|96|128|256|512
--memory-mib 2048|4096|6144|8192|12288|16384|24576|32768
--cpus COUNT
--resolution 1280x800|1440x900|1920x1080|2560x1440
--no-network
```

Defaults: 64 GiB, 6144 MiB, 4 CPUs, 1440x900, shared NAT; CPUs must fit the host.
Duplicate/unknown flags, relative paths, `..`, invalid values/names, directories,
missing media and final-component symlinks fail before library creation.

No password, recovery key, unattended answer file, payload, driver override or
experimental graphics option is accepted. JSON `bridgevm.app-create-windows.v1`
contains ID/name, saved resources, pending state, backend/boot mode, network and
configuration digest—not ISO paths or media identity. Its schema is unchanged;
`installPending: true` is not installation readiness.

Exit 0 confirms saved registration read-back, not installability. Exit 1 means
creation/read-back failed; inspect before retrying since publication may have
succeeded. Exit 2 means invalid usage/input. Success proves owned files and
registration only—not installation, guest boot/health, graphics or shutdown.
