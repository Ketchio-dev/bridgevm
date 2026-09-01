//! Descriptor-access regressions for console TX gather and RX scatter.

use super::super::*;
use super::helpers::{write_desc, TestMem};

#[test]
fn tx_gather_rejects_writable_descriptor_without_retaining_a_prefix() {
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let table = 0x4000_1000;
    mem.write(0x4000_4000, b"prefix");
    mem.write(0x4000_4100, b"writable");
    mem.write(0x4000_4200, b"suffix");
    write_desc(&mut mem, table, 0, 0x4000_4000, 6, DESC_F_NEXT, 1);
    write_desc(
        &mut mem,
        table,
        1,
        0x4000_4100,
        8,
        DESC_F_WRITE | DESC_F_NEXT,
        2,
    );
    write_desc(&mut mem, table, 2, 0x4000_4200, 6, 0, 0);
    let mut queue = VirtioConsoleQueue::new(0);
    queue.size = 3;
    queue.desc = table;
    let mut descs = Vec::new();
    let mut bytes = b"stale".to_vec();

    assert!(!VirtioConsole::read_chain_into(
        &mem, &queue, 0, &mut descs, &mut bytes, 64,
    ));
    assert!(bytes.is_empty(), "rejected TX must clear reusable scratch");
}

#[test]
fn rx_scatter_rejects_mixed_access_before_any_guest_write() {
    for first_len in [4, 8] {
        let mut mem = TestMem::new(0x4000_0000, 0x10000);
        let first_addr = 0x4000_4000;
        let second_addr = 0x4000_4100;
        mem.write(first_addr, b"12345678");
        mem.write(second_addr, b"abcdefgh");
        let descs = [
            Descriptor {
                addr: first_addr,
                len: first_len,
                flags: DESC_F_WRITE,
                next: 0,
            },
            Descriptor {
                addr: second_addr,
                len: 8,
                flags: 0,
                next: 0,
            },
        ];

        assert_eq!(
            VirtioConsole::scatter_write_partial_slices(&mut mem, &descs, b"ABCD", b"EFGH",),
            None
        );
        assert_eq!(mem.read(first_addr, 8), b"12345678");
        assert_eq!(mem.read(second_addr, 8), b"abcdefgh");
    }
}
