//! Bounded decoder for the firmware's pre-StartImage service-call ring.

const OFFSET: usize = 0x3400;
const MAGIC: u64 = 0x4341_5254_5342_4d42;
const VERSION: u32 = 1;
const CAPACITY: u32 = 126;
const HEADER: usize = 16;
const ENTRY: usize = 24;

const NAMES: [&str; 12] = [
    "HandleProtocol",
    "OpenProtocol",
    "LocateProtocol",
    "LocateHandleBuffer",
    "LocateHandle",
    "GetMemoryMap",
    "AllocatePages",
    "AllocatePool",
    "Exit",
    "SetWatchdogTimer",
    "ExitBootServices",
    "GetVariable",
];

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

pub(super) fn write(ram: &[u8]) {
    let Some(bytes) = ram.get(OFFSET..OFFSET + HEADER + CAPACITY as usize * ENTRY) else {
        return;
    };
    if u64_at(bytes, 0) != Some(MAGIC) {
        return;
    }
    if u32_at(bytes, 8) != Some(VERSION) {
        println!("bs_trace_error=unknown version");
        return;
    }
    let count = u32_at(bytes, 12).unwrap_or(0);
    println!("bs_trace_count={count}");
    let first = count.saturating_sub(CAPACITY);
    for sequence in first..count {
        let base = HEADER + (sequence % CAPACITY) as usize * ENTRY;
        let id = u32_at(bytes, base).unwrap_or(0);
        let name = match id {
            1..=12 => NAMES[id as usize - 1],
            _ => "Unknown",
        };
        println!(
            "bs_call=seq:{},fn:{},arg:{:#x},detail:{:#x},status:{:#x}",
            sequence,
            name,
            u32_at(bytes, base + 4).unwrap_or(0),
            u64_at(bytes, base + 8).unwrap_or(0),
            u64_at(bytes, base + 16).unwrap_or(0)
        );
    }
}
