# Vendored firmware

Two firmware artifacts are retained deliberately:

- `edk2-aarch64-code.fd` is the earlier known-good NVMe-boot build. Its exact
  original build revision was not recorded, so it remains as a compatibility
  and regression artifact instead of being silently replaced.
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
