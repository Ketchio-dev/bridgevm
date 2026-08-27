//! Regression for allocation-free HDA playback DMA reads.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BDL_GPA: u64 = 0x1000;
const PCM_GPA: u64 = 0x2000;

struct ReadIntoOnlyMem {
    bytes: Vec<u8>,
    read_into_calls: Cell<usize>,
}

impl ReadIntoOnlyMem {
    fn new(pcm: &[u8]) -> Self {
        let mut bytes = vec![0u8; 0x4000];
        let bdl = BDL_GPA as usize;
        bytes[bdl..bdl + 8].copy_from_slice(&PCM_GPA.to_le_bytes());
        bytes[bdl + 8..bdl + 12].copy_from_slice(&u32::try_from(pcm.len()).unwrap().to_le_bytes());
        let pcm_start = PCM_GPA as usize;
        bytes[pcm_start..pcm_start + pcm.len()].copy_from_slice(pcm);
        Self {
            bytes,
            read_into_calls: Cell::new(0),
        }
    }
}

impl GuestMemoryMut for ReadIntoOnlyMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Ok(start) = usize::try_from(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        let Some(dst) = self.bytes.get_mut(start..end) else {
            return false;
        };
        dst.copy_from_slice(data);
        true
    }

    fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
        panic!("HDA playback DMA must not allocate through read_bytes")
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Ok(start) = usize::try_from(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(dst.len()) else {
            return false;
        };
        let Some(src) = self.bytes.get(start..end) else {
            return false;
        };
        dst.copy_from_slice(src);
        self.read_into_calls
            .set(self.read_into_calls.get().saturating_add(1));
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedPcm {
    bytes: Vec<u8>,
    rate: u32,
    channels: u8,
    bits: u8,
}

struct CaptureSink(Arc<Mutex<Vec<CapturedPcm>>>);

impl HdaPcmSink for CaptureSink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        self.0.lock().unwrap().push(CapturedPcm {
            bytes: samples.to_vec(),
            rate,
            channels,
            bits,
        });
    }
}

fn write(ctrl: &mut HdaController, mem: &mut ReadIntoOnlyMem, offset: u64, size: u8, value: u64) {
    ctrl.mmio_write(offset, size, value, mem);
}

#[test]
fn playback_dma_reuses_read_into_scratch_across_polls() {
    let expected: Vec<u8> = (0..192).map(|value| value as u8).collect();
    let mut mem = ReadIntoOnlyMem::new(&expected);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut ctrl = HdaController::with_pcm_output_path::<&std::path::Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(CaptureSink(Arc::clone(&writes)))));

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, BDL_GPA);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, expected.len() as u64);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, u64::from(SDCTL_RUN));

    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));
    let scratch_capacity = ctrl.pcm_scratch.capacity();
    let scratch_ptr = ctrl.pcm_scratch.as_ptr();
    assert!(scratch_capacity >= expected.len());

    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    assert_eq!(ctrl.pcm_scratch.capacity(), scratch_capacity);
    assert_eq!(ctrl.pcm_scratch.as_ptr(), scratch_ptr);
    assert_eq!(mem.read_into_calls.get(), 4);
    assert_eq!(
        *writes.lock().unwrap(),
        vec![
            CapturedPcm {
                bytes: expected.clone(),
                rate: 48_000,
                channels: 2,
                bits: 16,
            },
            CapturedPcm {
                bytes: expected,
                rate: 48_000,
                channels: 2,
                bits: 16,
            },
        ]
    );
}
