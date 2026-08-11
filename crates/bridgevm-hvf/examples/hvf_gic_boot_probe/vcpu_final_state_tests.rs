use super::*;
use crate::vcpu_final_state::{CapturedRegister, VcpuFinalState};
use std::collections::BTreeMap;

struct TestMem {
    base: u64,
    bytes: Vec<u8>,
}

impl TestMem {
    fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0; len],
        }
    }

    fn write_u64(&mut self, gpa: u64, value: u64) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(offset) = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(end) = offset.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[offset..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let offset = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())?;
        let end = offset.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes[offset..end].to_vec())
    }
}

fn table_index(va: u64, level: u8) -> u64 {
    let shift = 39u32.saturating_sub(u32::from(level) * 9);
    (va >> shift) & 0x1ff
}

fn map_page(mem: &mut TestMem, va: u64, ipa: u64) -> Stage1Context {
    let tables = [0x1000, 0x2000, 0x3000, 0x4000];
    for level in 0..3u8 {
        let entry = tables[level as usize] + table_index(va, level) * 8;
        mem.write_u64(entry, tables[level as usize + 1] | 3);
    }
    let entry = tables[3] + table_index(va, 3) * 8;
    mem.write_u64(entry, ipa | (1 << 10) | 3);
    Stage1Context {
        sctlr_el1: 1,
        tcr_el1: 24,
        ttbr0_el1: tables[0],
        ttbr1_el1: 0,
        mair_el1: 0,
    }
}

fn node(node: u64, flink: u64, blink: u64) -> ViogpuListNode {
    ViogpuListNode {
        node,
        flink,
        blink,
        vbuffer: node - VIOGPU_LIST_ENTRY_OFFSET,
        buf: 0,
        size: 0x48,
        data_buf: 0,
        data_size: 0,
        resp_buf: 0,
        resp_size: 0x18,
        complete_cb: 0,
        complete_ctx: 0,
        auto_release: true,
        command_type: None,
        response_type: None,
    }
}

#[test]
fn strict_waiter_relation_accepts_the_observed_resource_create_shape() {
    let mut state = VcpuFinalState::test_state(0);
    state.x[0].value = 0xffff_808e_e79a_3848;
    state.x[20].value = state.x[0].value;
    state.x[28].value = 0xffff_808e_e79a_3828;
    state.x[27].value = 0x48;
    state.x[26].value = 0x18;
    state.x[2].value = 0x18;

    let waiter = viogpu_waiter(&state).unwrap();

    assert_eq!(waiter.object, state.x[28].value);
    assert_eq!(waiter.lock, state.x[0].value);
    assert_eq!(waiter.request_size, 0x48);
    assert_eq!(waiter.response_size_arg, 0x18);
    assert_eq!(waiter.response_size_saved, 0x18);
}

#[test]
fn strict_waiter_relation_rejects_an_unrelated_register_context() {
    let mut state = VcpuFinalState::test_state(2);
    state.x[0].value = 0x1000;
    state.x[20].value = 0x1000;
    state.x[28].value = 0x2000;

    assert!(viogpu_waiter(&state)
        .unwrap_err()
        .contains("strict relation failed"));
}

#[test]
fn failed_register_status_is_not_treated_as_a_zero_value() {
    let mut state = VcpuFinalState::test_state(1);
    state.x[0] = CapturedRegister {
        status: 0xfae9_4003u32 as i32,
        value: 0,
    };
    state.sctlr_el1 = state.x[0];

    assert!(state.required_x(0).unwrap_err().contains("read failed"));
    assert!(state
        .stage1_context()
        .unwrap_err()
        .contains("SCTLR_EL1 read failed"));
}

#[test]
fn list_walk_stops_at_the_real_sentinel() {
    let sentinel = 0x1000;
    let entries = BTreeMap::from([
        (0x2000, node(0x2000, 0x3000, sentinel)),
        (0x3000, node(0x3000, sentinel, 0x2000)),
    ]);
    let walk = walk_in_use_list(sentinel, 0x2000, 64, |address| {
        entries
            .get(&address)
            .cloned()
            .ok_or_else(|| "missing".to_string())
    });

    assert_eq!(walk.nodes.len(), 2);
    assert_eq!(walk.termination, ListTermination::Sentinel);
}

#[test]
fn absent_sentinel_cycle_is_detected_without_exceeding_the_bound() {
    let entries = BTreeMap::from([
        (0x2000, node(0x2000, 0x3000, 0x3000)),
        (0x3000, node(0x3000, 0x2000, 0x2000)),
    ]);
    let walk = walk_in_use_list(0x1000, 0x2000, 64, |address| {
        Ok(entries[&address].clone())
    });

    assert_eq!(walk.nodes.len(), 2);
    assert_eq!(
        walk.termination,
        ListTermination::Cycle { address: 0x2000 }
    );
}

#[test]
fn unreadable_list_node_terminates_fail_closed() {
    let walk = walk_in_use_list(0x1000, 0x2000, 64, |address| {
        Err(format!("cannot translate {address:#x}"))
    });

    assert!(walk.nodes.is_empty());
    assert!(matches!(
        walk.termination,
        ListTermination::Unreadable {
            address: 0x2000,
            ..
        }
    ));
}

#[test]
fn list_walk_never_reads_more_than_sixty_four_nodes() {
    let mut reads = 0u64;
    let walk = walk_in_use_list(0x1000, 0x2000, usize::MAX, |address| {
        reads += 1;
        Ok(node(
            address,
            address + 0x100,
            address.saturating_sub(0x100),
        ))
    });

    assert_eq!(reads, MAX_VIOGPU_LIST_NODES as u64);
    assert_eq!(walk.nodes.len(), MAX_VIOGPU_LIST_NODES);
    assert!(matches!(walk.termination, ListTermination::Truncated { .. }));
}

#[test]
fn bounded_virtual_read_crosses_a_page_without_assuming_contiguous_ipas() {
    let va = 0x0000_0000_1234_5ff8;
    let mut mem = TestMem::new(0, 0xb000);
    let context = map_page(&mut mem, va, 0x8000);
    let _ = map_page(&mut mem, va + 8, 0xa000);
    assert!(mem.write_bytes(0x8ff8, b"abcdefgh"));
    assert!(mem.write_bytes(0xa000, b"ijklmnop"));

    let bytes = read_virtual_bytes(&mem, &context, va, 16).unwrap();

    assert_eq!(bytes, b"abcdefghijklmnop");
}

#[test]
fn bounded_virtual_read_rejects_more_than_one_hundred_bytes() {
    let mem = TestMem::new(0, 0x5000);
    let context = Stage1Context {
        sctlr_el1: 1,
        tcr_el1: 24,
        ttbr0_el1: 0x1000,
        ttbr1_el1: 0,
        mair_el1: 0,
    };

    assert!(read_virtual_bytes(&mem, &context, 0x1000, 0x101)
        .unwrap_err()
        .contains("outside"));
}

#[test]
fn vbuffer_layout_exposes_callback_and_late_auto_release_identity() {
    let mut bytes = [0u8; VIOGPU_VBUFFER_BYTES];
    bytes[64..72].copy_from_slice(&0x1111u64.to_le_bytes());
    bytes[72..80].copy_from_slice(&0x2222u64.to_le_bytes());
    bytes[80] = 1;

    assert_eq!(le_u64(&bytes, 64), 0x1111);
    assert_eq!(le_u64(&bytes, 72), 0x2222);
    assert_eq!(bytes[80], 1);
}
