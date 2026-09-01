//! Descriptor-access regression for console TX gathering.

use super::super::*;
use super::helpers::{write_desc, TestMem};

#[test]
fn tx_gather_rejects_a_writable_middle_descriptor_and_reuses_scratch() {
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let table = 0x4000_1000;
    mem.write(0x4000_4000, b"prefix");
    mem.write(0x4000_4100, b"middle");
    mem.write(0x4000_4200, b"suffix");
    write_desc(&mut mem, table, 0, 0x4000_4000, 6, DESC_F_NEXT, 1);
    write_desc(&mut mem, table, 1, 0x4000_4100, 6, DESC_F_NEXT, 2);
    write_desc(&mut mem, table, 2, 0x4000_4200, 6, 0, 0);
    let mut queue = VirtioConsoleQueue::new(0);
    queue.size = 3;
    queue.desc = table;
    let mut descs = Vec::with_capacity(3);
    let mut bytes = Vec::with_capacity(18);

    assert!(VirtioConsole::read_chain_into(
        &mem, &queue, 0, &mut descs, &mut bytes, 64,
    ));
    assert_eq!(bytes, b"prefixmiddlesuffix");
    let desc_capacity = descs.capacity();
    let byte_capacity = bytes.capacity();
    // At 4cd4de70^, this mutation rejected only after retaining the gathered prefix.
    write_desc(
        &mut mem,
        table,
        1,
        0x4000_4100,
        6,
        DESC_F_WRITE | DESC_F_NEXT,
        2,
    );
    assert!(!VirtioConsole::read_chain_into(
        &mem, &queue, 0, &mut descs, &mut bytes, 64,
    ));
    assert!(bytes.is_empty());
    assert_eq!(descs.capacity(), desc_capacity);
    assert_eq!(bytes.capacity(), byte_capacity);

    write_desc(&mut mem, table, 1, 0x4000_4100, 6, DESC_F_NEXT, 2);
    assert!(VirtioConsole::read_chain_into(
        &mem, &queue, 0, &mut descs, &mut bytes, 64,
    ));
    assert_eq!(bytes, b"prefixmiddlesuffix");
    assert_eq!(descs.capacity(), desc_capacity);
    assert_eq!(bytes.capacity(), byte_capacity);
}
