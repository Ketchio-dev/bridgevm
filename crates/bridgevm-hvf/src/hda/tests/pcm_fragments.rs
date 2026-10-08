//! Existing DMA delivery can split complete PCM frames at descriptor boundaries.
use super::super::*;
use super::helpers::{write, RecordingPcmSink, RAM_BASE};
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[test]
fn aligned_budget_delivers_six_then_two_byte_stereo_fragments() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(RecordingPcmSink {
        writes: Arc::clone(&writes),
    })));
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let (bdl, pcm) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000);
    assert!(mem.write_bytes(pcm, &[1, 2, 3, 4, 5, 6, 7, 8]));
    for (index, offset, length) in [(0u64, 0u64, 6u32), (1, 6, 2)] {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&(pcm + offset).to_le_bytes());
        descriptor[8..12].copy_from_slice(&length.to_le_bytes());
        assert!(mem.write_bytes(bdl + index * 16, &descriptor));
    }
    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, 8);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 1);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, u64::from(SDCTL_RUN));
    ctrl.consume_stream(&mut mem, 8);
    let recorded = writes.lock().unwrap();
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0].samples, [1, 2, 3, 4, 5, 6]);
    assert_eq!(recorded[1].samples, [7, 8]);
    assert!(recorded
        .iter()
        .all(|r| (r.rate, r.channels, r.bits) == (48_000, 2, 16)));
}
