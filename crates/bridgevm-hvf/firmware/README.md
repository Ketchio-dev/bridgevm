# Vendored firmware

`edk2-aarch64-code.fd` is an `ArmVirtPkg/ArmVirtQemu.dsc` firmware volume built
from current [tianocore/edk2](https://github.com/tianocore/edk2) (RELEASE,
`-a AARCH64 -t GCC`). It replaces Homebrew qemu's older `edk2-aarch64-code.fd`,
whose NvmExpressDxe/PciBus does **not** bind BridgeVM's NVMe endpoint. A current
edk2 binds it and boots Windows 11 ARM64 from NVMe.

The loader maps this volume at `FLASH_CODE` (0x0); the region beyond the volume
is zero-filled. The variable store still uses the (version-insensitive) stock
ArmVirtQemu vars template (see `DEFAULT_QEMU_AARCH64_VARS`).

edk2 is licensed BSD-2-Clause-Patent.

## Rebuild
```
git clone --depth 1 --recurse-submodules --shallow-submodules \
    https://github.com/tianocore/edk2.git && cd edk2
make -C BaseTools
export GCC_AARCH64_PREFIX=aarch64-elf-   # brew: aarch64-elf-gcc
source ./edksetup.sh BaseTools
build -a AARCH64 -t GCC -p ArmVirtPkg/ArmVirtQemu.dsc -b RELEASE
# -> Build/ArmVirtQemu-AArch64/RELEASE_GCC/FV/QEMU_EFI.fd
```
