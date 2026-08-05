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
    kept = [x for x in order if x != drop]
    if len(kept) == len(order):
        print('BootOrder already clean:', [hex(x) for x in order])
        return
    # Keep the record length identical: pad by repeating the final entry, which
    # firmware treats as an already-tried option rather than a new one.
    while len(kept) < len(order):
        kept.append(kept[-1])
    struct.pack_into('<%dH' % len(kept), d, start, *kept)
    open(path, 'wb').write(d)
    print('BootOrder', [hex(x) for x in order], '->', [hex(x) for x in kept])

if __name__ == '__main__':
    main(sys.argv[1])
