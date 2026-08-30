/* BridgeVM Virtual ARM PC reset-vector link contract.
 * SPDX-License-Identifier: Apache-2.0
 */

OUTPUT_ARCH(aarch64)
ENTRY(_start)

SECTIONS
{
  . = 0;
  .text : ALIGN(4)
  {
    KEEP(*(.text.reset))
    *(.text*)
  }
  .rodata : ALIGN(8) { *(.rodata*) }

  /DISCARD/ :
  {
    *(.comment)
    *(.note*)
    *(.eh_frame*)
  }
}

ASSERT(_start == 0, "BridgeVM PC reset vector must start at flash offset zero")
ASSERT(SIZEOF(.text) <= 0x1000, "BridgeVM PC reset vector exceeds its v1 page")
