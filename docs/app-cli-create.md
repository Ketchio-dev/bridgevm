# Creating an own-HVF Windows VM from the CLI

`bridgevm app create-windows` creates an installation-pending `hvf-engine` /
`windows-hvf` registration. It does not install Windows or start a VM.

```sh
bridgevm app create-windows "Dev VM" \
  --iso "/absolute/path/Windows 11 ARM.iso"
```

The canonical ID may differ from the name or gain a collision suffix. Inspect it:
```sh
bridgevm app inspect dev-vm
```

**Add `--fetch-drivers` to make the entry installable:** the command then
downloads (once) the pinned Microsoft-signed Red Hat virtio-win archive, refuses
it unless its length and SHA-256 match, builds the storage, serial and network
payload and manifest from it, and seals them in the new VM bundle exactly as the
app does. The text output then ends with `Next: bridgevm app install <ID>`. A
failed download exits 1 before any library entry is created.

**Without `--fetch-drivers`, installation is blocked for CLI-created entries:**
the command does not stage the required signed ARM64 guest driver payload or its
manifest. Neither the app
nor CLI can add those inputs to an existing entry. Open the entry in the app
and choose **New VM…** to reuse its ISO. In the new VM form, choose **Download
Drivers** or select your signed Arm64 driver folder and SHA-256 manifest. See
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
--fetch-drivers
```

Defaults: 64 GiB, 6144 MiB, 4 CPUs, 1440x900, shared NAT; CPUs must fit the host.
`--resolution` is the Windows resolution at startup, the same setting as
**Resolution** in the VM's settings. With the Windows display driver installed,
Windows then follows the window size.
Duplicate/unknown flags, relative paths, `..`, invalid values/names, directories,
missing media and final-component symlinks fail before library creation.

No password, recovery key, unattended answer file, payload path, driver override or
experimental graphics option is accepted. JSON `bridgevm.app-create-windows.v2`
contains ID/name, saved resources, pending state, backend/boot mode, network and
`guestDriversSealed`—not ISO paths or media identity;
`installPending: true` is not installation readiness.

Exit 0 confirms saved registration read-back, not installability. Exit 1 means
creation/read-back failed; inspect before retrying since publication may have
succeeded. Exit 2 means invalid usage/input. Success proves owned files and
registration only—not installation, guest boot/health, graphics or shutdown.
