## @file
# Build description for the BridgeVM-owned firmware modules.
#
# This intentionally contains no compatibility-board package dependency. The
# It builds only BridgeVM-owned consumers and probe libraries.
#
# SPDX-License-Identifier: Apache-2.0
##
[Defines]
  PLATFORM_NAME                  = BridgeVmPc
  PLATFORM_GUID                  = 7AFA93B6-7311-466B-AB29-50131A268326
  PLATFORM_VERSION               = 0.1
  DSC_SPECIFICATION              = 0x00010005
  OUTPUT_DIRECTORY              = Build/BridgeVmPc
  SUPPORTED_ARCHITECTURES        = AARCH64
  BUILD_TARGETS                  = DEBUG|RELEASE
  SKUID_IDENTIFIER              = DEFAULT
[LibraryClasses]
  BaseLib|MdePkg/Library/BaseLib/BaseLib.inf
  BaseMemoryLib|MdePkg/Library/BaseMemoryLib/BaseMemoryLib.inf
  BridgeVmPcPciProbeLib|BridgeVmPcPkg/Library/PciProbeLib/PciProbeLib.inf
  DebugLib|MdePkg/Library/BaseDebugLibNull/BaseDebugLibNull.inf
  PcdLib|MdePkg/Library/BasePcdLibNull/BasePcdLibNull.inf
  StackCheckLib|MdePkg/Library/StackCheckLibNull/StackCheckLibNull.inf
  UefiBootServicesTableLib|MdePkg/Library/UefiBootServicesTableLib/UefiBootServicesTableLib.inf
  UefiDriverEntryPoint|MdePkg/Library/UefiDriverEntryPoint/UefiDriverEntryPoint.inf
[Components]
  BridgeVmPcPkg/Drivers/PlatformTablesDxe/PlatformTablesDxe.inf
  BridgeVmPcPkg/Drivers/DxeProbe/DxeProbe.inf
