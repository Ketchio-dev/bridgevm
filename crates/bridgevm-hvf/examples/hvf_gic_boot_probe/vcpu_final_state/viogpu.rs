//! Strict viogpu waiter detection and bounded object/list inspection.

use crate::*;
use std::collections::BTreeSet;

const VIOGPU_LIST_ENTRY_OFFSET: u64 = 0x30;
const VIOGPU_VBUFFER_BYTES: usize = 0x58;
const VIOGPU_OBJECT_BYTES: usize = 0x30;
const VIOGPU_IN_USE_SENTINEL_OFFSET: u64 = 0x10;
const VIOGPU_LOCK_OFFSET: u64 = 0x20;
const MAX_VIRTUAL_READ_BYTES: usize = 0x100;
const MAX_VIOGPU_LIST_NODES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViogpuWaiter {
    object: u64,
    lock: u64,
    request_size: u64,
    response_size_arg: u64,
    response_size_saved: u64,
    response_pointer: u64,
}

fn viogpu_waiter(state: &VcpuFinalState) -> Result<ViogpuWaiter, String> {
    let x0 = state.required_x(0)?;
    let x2 = state.required_x(2)?;
    let x3 = state.required_x(3)?;
    let x20 = state.required_x(20)?;
    let x26 = state.required_x(26)?;
    let x27 = state.required_x(27)?;
    let x28 = state.required_x(28)?;
    let expected_lock = x28
        .checked_add(VIOGPU_LOCK_OFFSET)
        .ok_or_else(|| "x28 + lock offset overflowed".to_string())?;
    if x0 == 0
        || x0 != x20
        || x0 != expected_lock
        || x28 & 7 != 0
        || x27 == 0
        || x26 != x2
    {
        return Err(format!(
            "strict relation failed: x0={x0:#x} x20={x20:#x} x28={x28:#x} expected_lock={expected_lock:#x}"
        ));
    }
    Ok(ViogpuWaiter {
        object: x28,
        lock: x0,
        request_size: x27,
        response_size_arg: x2,
        response_size_saved: x26,
        response_pointer: x3,
    })
}

pub(crate) fn report_viogpu_waiter(
    mem: &dyn GuestMemoryMut,
    context: &Stage1Context,
    state: &VcpuFinalState,
) {
    let waiter = match viogpu_waiter(state) {
        Ok(waiter) => waiter,
        Err(reason) => {
            println!("VIOGPU-WAITER[vcpu{}]: no-match: {reason}", state.index);
            return;
        }
    };
    println!(
        "VIOGPU-WAITER[vcpu{}]: candidate object={:#x} lock={:#x} request_size={:#x} response_size_arg={:#x} response_size_saved={:#x} response_pointer={:#x}",
        state.index,
        waiter.object,
        waiter.lock,
        waiter.request_size,
        waiter.response_size_arg,
        waiter.response_size_saved,
        waiter.response_pointer
    );
    let bytes = match read_virtual_bytes(mem, context, waiter.object, VIOGPU_OBJECT_BYTES) {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("VIOGPU-OBJECT[vcpu{}]: unreadable: {reason}", state.index);
            return;
        }
    };
    let object = ViogpuObjectSnapshot::parse(&bytes);
    let sentinel = waiter.object + VIOGPU_IN_USE_SENTINEL_OFFSET;
    println!(
        "VIOGPU-OBJECT[vcpu{}]: base={:#x} free=({:#x},{:#x}) in_use_sentinel={sentinel:#x} in_use=({:#x},{:#x}) lock={:#x} count={} count_min={}",
        state.index,
        waiter.object,
        object.free_flink,
        object.free_blink,
        object.in_use_flink,
        object.in_use_blink,
        object.lock,
        object.count,
        object.count_min
    );
    let walk = walk_in_use_list(sentinel, object.in_use_flink, MAX_VIOGPU_LIST_NODES, |node| {
        read_viogpu_node(mem, context, node)
    });
    let mut previous = sentinel;
    for (index, node) in walk.nodes.iter().enumerate() {
        println!(
            "VIOGPU-INUSE[vcpu{}][{index}]: node={:#x} flink={:#x} blink={:#x} backlink_ok={} vbuf={:#x} buf={:#x} size={} data={:#x}/{} resp={:#x}/{} cb={:#x} ctx={:#x} auto_release={} cmd_type={} resp_type={}",
            state.index,
            node.node,
            node.flink,
            node.blink,
            node.blink == previous,
            node.vbuffer,
            node.buf,
            node.size,
            node.data_buf,
            node.data_size,
            node.resp_buf,
            node.resp_size,
            node.complete_cb,
            node.complete_ctx,
            node.auto_release,
            optional_hex(node.command_type),
            optional_hex(node.response_type),
        );
        previous = node.node;
    }
    println!(
        "VIOGPU-INUSE[vcpu{}]: nodes={} termination={:?} sentinel_blink={:#x} tail_matches={}",
        state.index,
        walk.nodes.len(),
        walk.termination,
        object.in_use_blink,
        previous == object.in_use_blink
    );
}

fn optional_hex(value: Option<u32>) -> String {
    value
        .map(|value| format!("{value:#x}"))
        .unwrap_or_else(|| "unreadable".to_string())
}

fn read_virtual_bytes(
    mem: &dyn GuestMemoryMut,
    context: &Stage1Context,
    va: u64,
    len: usize,
) -> Result<Vec<u8>, String> {
    if len == 0 || len > MAX_VIRTUAL_READ_BYTES {
        return Err(format!("read length {len:#x} outside 1..={MAX_VIRTUAL_READ_BYTES:#x}"));
    }
    let mut bytes = vec![0u8; len];
    let mut done = 0usize;
    while done < len {
        let current_va = va
            .checked_add(done as u64)
            .ok_or_else(|| "virtual address overflow".to_string())?;
        let translation = stage1::translate(mem, context, current_va)
            .map_err(|failure| format!("VA {current_va:#x}: {}", failure.reason))?;
        let va_page_left = 0x1000usize - (current_va as usize & 0xfff);
        let ipa_page_left = 0x1000usize - (translation.ipa as usize & 0xfff);
        let chunk = (len - done).min(va_page_left).min(ipa_page_left);
        if !mem.read_into(translation.ipa, &mut bytes[done..done + chunk]) {
            return Err(format!(
                "VA {current_va:#x} translated to unreadable IPA {:#x}",
                translation.ipa
            ));
        }
        done += chunk;
    }
    Ok(bytes)
}

fn read_virtual_u32(
    mem: &dyn GuestMemoryMut,
    context: &Stage1Context,
    va: u64,
) -> Result<u32, String> {
    let bytes = read_virtual_bytes(mem, context, va, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViogpuObjectSnapshot {
    free_flink: u64,
    free_blink: u64,
    in_use_flink: u64,
    in_use_blink: u64,
    lock: u64,
    count: u32,
    count_min: u32,
}

impl ViogpuObjectSnapshot {
    fn parse(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), VIOGPU_OBJECT_BYTES);
        Self {
            free_flink: le_u64(bytes, 0),
            free_blink: le_u64(bytes, 8),
            in_use_flink: le_u64(bytes, 16),
            in_use_blink: le_u64(bytes, 24),
            lock: le_u64(bytes, 32),
            count: le_u32(bytes, 40),
            count_min: le_u32(bytes, 44),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViogpuListNode {
    node: u64,
    flink: u64,
    blink: u64,
    vbuffer: u64,
    buf: u64,
    size: i32,
    data_buf: u64,
    data_size: u32,
    resp_buf: u64,
    resp_size: i32,
    complete_cb: u64,
    complete_ctx: u64,
    auto_release: bool,
    command_type: Option<u32>,
    response_type: Option<u32>,
}

fn read_viogpu_node(
    mem: &dyn GuestMemoryMut,
    context: &Stage1Context,
    node: u64,
) -> Result<ViogpuListNode, String> {
    let vbuffer = node
        .checked_sub(VIOGPU_LIST_ENTRY_OFFSET)
        .ok_or_else(|| format!("node {node:#x} is below list-entry offset"))?;
    let bytes = read_virtual_bytes(mem, context, vbuffer, VIOGPU_VBUFFER_BYTES)?;
    let buf = le_u64(&bytes, 0);
    let resp_buf = le_u64(&bytes, 32);
    Ok(ViogpuListNode {
        node,
        flink: le_u64(&bytes, 48),
        blink: le_u64(&bytes, 56),
        vbuffer,
        buf,
        size: le_i32(&bytes, 8),
        data_buf: le_u64(&bytes, 16),
        data_size: le_u32(&bytes, 24),
        resp_buf,
        resp_size: le_i32(&bytes, 40),
        complete_cb: le_u64(&bytes, 64),
        complete_ctx: le_u64(&bytes, 72),
        auto_release: bytes[80] != 0,
        command_type: (buf != 0)
            .then(|| read_virtual_u32(mem, context, buf).ok())
            .flatten(),
        response_type: (resp_buf != 0)
            .then(|| read_virtual_u32(mem, context, resp_buf).ok())
            .flatten(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListTermination {
    Sentinel,
    Cycle { address: u64 },
    Null,
    Unreadable { address: u64, reason: String },
    Truncated { next: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ListWalk {
    nodes: Vec<ViogpuListNode>,
    termination: ListTermination,
}

fn walk_in_use_list(
    sentinel: u64,
    first: u64,
    limit: usize,
    mut read: impl FnMut(u64) -> Result<ViogpuListNode, String>,
) -> ListWalk {
    let limit = limit.min(MAX_VIOGPU_LIST_NODES);
    let mut nodes = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = first;
    for _ in 0..limit {
        if current == sentinel {
            return ListWalk {
                nodes,
                termination: ListTermination::Sentinel,
            };
        }
        if current == 0 {
            return ListWalk {
                nodes,
                termination: ListTermination::Null,
            };
        }
        if !visited.insert(current) {
            return ListWalk {
                nodes,
                termination: ListTermination::Cycle { address: current },
            };
        }
        let node = match read(current) {
            Ok(node) => node,
            Err(reason) => {
                return ListWalk {
                    nodes,
                    termination: ListTermination::Unreadable {
                        address: current,
                        reason,
                    },
                };
            }
        };
        current = node.flink;
        nodes.push(node);
    }
    let termination = if current == sentinel {
        ListTermination::Sentinel
    } else {
        ListTermination::Truncated { next: current }
    };
    ListWalk { nodes, termination }
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64 field"))
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 field"))
}

fn le_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("i32 field"))
}

#[cfg(test)]
#[path = "../vcpu_final_state_tests.rs"]
mod tests;
