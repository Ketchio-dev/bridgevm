//! Split test module.

use super::super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn test_checkpoint(device_state: Vec<u8>) -> VmCheckpoint {
    VmCheckpoint {
        ram_len: SPARSE_RAM_CHUNK_SIZE as u64,
        ram_chunks: vec![SparseRamChunk {
            offset: 0,
            bytes: vec![1, 2, 3, 4],
        }],
        vcpus: vec![VcpuRegisterBundle {
            x: [0; 31],
            pc: 0x4000_1000,
            fpcr: 0,
            fpsr: 0,
            cpsr: 0x3c5,
            sys_regs: vec![(0xc080, 1)],
            simd: [[0; 16]; 32],
            gic_icc_regs: vec![(0xc230, 1)],
            vtimer_offset: 7,
            vtimer_masked: true,
        }],
        gic_state: vec![5, 6, 7],
        device_state,
    }
}

pub(super) fn test_directory(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bridgevm-checkpoint-{label}-{}-{unique}",
        std::process::id()
    ))
}
