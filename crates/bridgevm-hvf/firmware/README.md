# Vendored firmware

Two firmware artifacts are retained deliberately:

- `edk2-aarch64-code.fd` is the earlier known-good NVMe-boot build. Its exact
  original build revision was not recorded, so it remains as a compatibility
  and regression artifact instead of being silently replaced. It is
  development-only and is not installed by the macOS product packagers.
- `edk2-aarch64-secure-code.fd` is the product default. It is built from
  tianocore/edk2 `edk2-stable202605`, commit
  `b03a21a63e3bd001f52c527e5a57feddb53a690b`, with Secure Boot, TPM2, and the
  TPM2 configuration UI enabled. Its adjacent `.build.json` is the
  machine-readable provenance receipt.

Both are `ArmVirtPkg/ArmVirtQemu.dsc` RELEASE AARCH64 firmware volumes. They
replace Homebrew qemu's older firmware, whose NvmExpressDxe/PciBus does **not**
bind BridgeVM's NVMe endpoint.

The loader maps this volume at `FLASH_CODE` (0x0); the region beyond the volume
is zero-filled. The variable store starts from the stock ArmVirtQemu template
and is copied per VM. Fresh Windows installs enroll BridgeVM's pinned
Microsoft-only Secure Boot policy; existing VM varstores are never rewritten
automatically.

edk2 is licensed BSD-2-Clause-Patent.

## Experimental BridgeVM Virtual ARM PC package

`BridgeVmPcPkg/` is separate source for the future independent board. It does
not use the compatibility board's platform package, firmware configuration
transport, or device identifiers. Its first module is a fail-closed DXE
consumer for `BridgeVM boot-info v1`: it validates the header, ACPI table set
and SMBIOS structure stream before publishing the standard UEFI ACPI 2.0 and
SMBIOS 3.0 configuration-table GUIDs.

The module builds from the same pinned, BSD-2-Clause-Patent TianoCore revision
using only `MdePkg` plus BridgeVM-owned sources:

```sh
scripts/build-bridgevm-pc-edk2-consumer.sh /path/to/edk2 /path/to/output
```

The build script works offline, rejects the wrong source/submodule/toolchain
versions, rejects compatibility-platform references and zeroes PE debug paths
before verifying the reproducible digest. Its output is a development-only DXE
driver, not a bootable firmware volume. It does not establish reset-vector,
UEFI-service, installer or Windows boot support.

The pinned generic DXE Core can separately be packaged into a reproducible
1 MiB PI firmware volume:

```sh
scripts/build-bridgevm-pc-dxe-core-fv.sh /path/to/edk2 /path/to/output
```

That output is a build-only loader input. It is not connected to the reset
image and does not prove DXE entry, UEFI services or Windows boot.

The same package now contains the first BridgeVM-owned reset entry. It builds a
64 MiB development flash image with executable code at the board's fixed GPA
zero:

```sh
scripts/build-bridgevm-pc-reset-vector.sh /path/to/output
```

The reset code masks interrupts, parks non-boot CPUs, establishes a stack in
system RAM and enters a freestanding BridgeVM SEC C function. SEC validates the
complete boot-info header, checksum, table ranges, RAM contract and CPU count,
then constructs a bounded PI HOB list containing the PHIT, system-memory
resource, SEC stack allocation, CPU and end HOBs. This image contains no
firmware volume, PEI, DXE core, UEFI services, variables, boot manager or
Windows loader.

## Rebuild the product firmware
```
git clone --recurse-submodules --branch edk2-stable202605 \
  https://github.com/tianocore/edk2.git /path/to/edk2
git -C /path/to/edk2 checkout b03a21a63e3bd001f52c527e5a57feddb53a690b
brew install aarch64-elf-gcc acpica
scripts/build-hvf-edk2-secure-firmware.sh /path/to/edk2
```

The script pins `SOURCE_DATE_EPOCH`, GCC 16.1.0, iasl 20260408, and the final
SHA-256. It rejects a dirty/different source revision or mismatched submodules,
requires ArmVirtQemu to bind `Tcg2PhysicalPresenceLibQemu`, then checks that the
resulting firmware contains the Secure Boot and TPM2 DXE modules before
installing it. The build receipt records both the verified modules and that
library instance; BridgeVM's matching `etc/tpm/config` record is what lets the
library discover the PPI page at runtime.
