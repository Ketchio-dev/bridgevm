//! Descriptor-access regression for console RX scatter.

use super::super::*;
use super::helpers::TestMem;

#[test]
fn rx_scatter_validates_the_full_chain_before_writing() {
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let first_addr = 0x4000_4000;
    let second_addr = 0x4000_4100;
    let mut descs = [
        Descriptor {
            addr: first_addr,
            len: 4,
            flags: DESC_F_WRITE,
            next: 0,
        },
        Descriptor {
            addr: second_addr,
            len: 4,
            flags: DESC_F_WRITE,
            next: 0,
        },
    ];

    assert_eq!(
        VirtioConsole::scatter_write_partial_slices(&mut mem, &descs, b"ABCD", b"EFGH"),
        Some(8)
    );
    assert_eq!(mem.read(first_addr, 4), b"ABCD");
    assert_eq!(mem.read(second_addr, 4), b"EFGH");
    // At 4cd4de70^, this mutation wrote the first buffer before the second descriptor failed.
    mem.write(first_addr, b"1234");
    mem.write(second_addr, b"abcd");
    descs[1].flags = 0;
    assert_eq!(
        VirtioConsole::scatter_write_partial_slices(&mut mem, &descs, b"ABCD", b"EFGH"),
        None
    );
    assert_eq!(mem.read(first_addr, 4), b"1234");
    assert_eq!(mem.read(second_addr, 4), b"abcd");
}
