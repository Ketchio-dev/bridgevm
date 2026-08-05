#!/usr/bin/env python3
"""Drop the injector's Boot0003 from a UEFI vars image's BootOrder.

The injector pins Boot0003 to its own GPT partition GUID. That entry is correct
during the injector pass, but on the following installed-Windows boot the
injector disk is gone, so firmware tries Boot0003, fails to find
\\EFI\\Boot\\bootaa64.efi, and must fall back. Most boots fall back fine; some
stall there. Removing the dead entry removes the fallback entirely.
"""
import re, struct, sys

GUID = bytes.fromhex('61dfe48bca93d211aa0d00e098032b8c')  # EFI_GLOBAL_VARIABLE
NAME = 'BootOrder'.encode('utf-16-le')

def main(path, drop=3):
    d = bytearray(open(path, 'rb').read())
    last = None
    for m in re.finditer(re.escape(NAME), bytes(d)):
        off = m.start()
        if d[off-16:off] != GUID:
            continue
        namesize, datasize = struct.unpack_from('<II', d, off-24)
        if namesize != len(NAME) + 2:
            continue
        last = (off, namesize, datasize)
    if last is None:
        sys.exit('FAIL: no BootOrder variable found')
    off, namesize, datasize = last
    start = off + namesize - 2
    order = list(struct.unpack_from('<%dH' % (datasize // 2), d, start))
    # Also de-duplicate. The source image lists Boot0000 twice, so dropping
    # Boot0003 still left [0x0, 0x0]; firmware retrying the same option is what
    # this script exists to prevent.
    kept = []
    for x in order:
        if x != drop and x not in kept:
            kept.append(x)
    if kept == order:
        print('BootOrder already clean:', [hex(x) for x in order])
        return
    # Shorten the record rather than padding it. The previous version repeated
    # the final entry to keep the length, on the assumption that firmware would
    # treat a duplicate as already tried. That assumption was never tested and
    # produced BootOrder [0x0, 0x3, 0x0] -> [0x0, 0x0, 0x0]: Boot0000 listed
    # three times. datasize lives in the header 24 bytes before the name, so
    # the length can simply be corrected.
    struct.pack_into('<%dH' % len(kept), d, start, *kept)
    new_datasize = len(kept) * 2
    struct.pack_into('<I', d, off - 20, new_datasize)
    # Zero the tail so the dropped entry does not linger in the image.
    for i in range(start + new_datasize, start + datasize):
        d[i] = 0
    open(path, 'wb').write(d)
    print('BootOrder', [hex(x) for x in order], '->', [hex(x) for x in kept],
          '(datasize %d -> %d)' % (datasize, new_datasize))

if __name__ == '__main__':
    main(sys.argv[1])
