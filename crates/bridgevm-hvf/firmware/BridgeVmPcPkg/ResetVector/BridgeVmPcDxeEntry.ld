/* BridgeVM Virtual ARM PC DXE-entry link contract.
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
  .vectors : ALIGN(2048) { KEEP(*(.vectors)) }

  /DISCARD/ :
  {
    *(.comment)
    *(.note*)
    *(.eh_frame*)
  }
}

ASSERT(_start == 0, "BridgeVM PC reset vector must start at flash offset zero")
ASSERT(SIZEOF(.text) <= 0x1000, "BridgeVM PC reset vector exceeds its v1 page")
ASSERT(SIZEOF(.vectors) == 0x800, "BridgeVM PC exception vector has the wrong size")
