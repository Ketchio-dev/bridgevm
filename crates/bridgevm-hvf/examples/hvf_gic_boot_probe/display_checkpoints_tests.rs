use std::cell::RefCell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::DisplayCheckpoints;
use crate::ramfb_dump::{self, RamfbSampleSchedule};
use crate::{BootTimer, RamfbSampleLoop, VirtPlatform};
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::ramfb::{RamfbConfig, DRM_FORMAT_XRGB8888};

const RAM_BASE: u64 = 0x4000_0000;

/// Guest RAM holding a 4x2 frame; every read checks the platform lock is free.
struct LockFreeRam<'a> {
    bytes: Vec<u8>,
    platform: &'a Mutex<VirtPlatform>,
    events: &'a RefCell<Vec<String>>,
}

impl GuestMemoryMut for LockFreeRam<'_> {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        assert!(
            self.platform.try_lock().is_ok(),
            "framebuffer read under the platform lock"
        );
        self.events.borrow_mut().push("read".into());
        let start = usize::try_from(gpa.checked_sub(RAM_BASE)?).ok()?;
        self.bytes
            .get(start..start.checked_add(len)?)
            .map(<[u8]>::to_vec)
    }
}

fn frame_config() -> RamfbConfig {
    RamfbConfig {
        addr: RAM_BASE + 0x100,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 4,
        height: 2,
        stride: 16,
    }
}

#[test]
fn due_samples_and_the_boot_timer_read_the_frame_after_the_lock_is_released() {
    let platform = Mutex::new(VirtPlatform::new(VirtFdtConfig::default()));
    let schedule = RamfbSampleSchedule::from_millis_values(&[1, 2]).unwrap();
    let mut samples = RamfbSampleLoop::with_schedule(schedule);
    let mut boot_timer = BootTimer::new(true, Instant::now(), Duration::from_secs(1), None, vec![]);
    std::thread::sleep(Duration::from_millis(3));

    let guard = platform.lock().unwrap();
    let mut checkpoints = DisplayCheckpoints::take(&guard, &mut samples, &mut boot_timer, 0, 7, 0);
    assert_eq!(checkpoints.labels, ["ramfb-sample-1ms", "ramfb-sample-2ms"]);
    assert!(checkpoints.boot.samples_display());
    // This platform has no ramfb; give the pass a programmed frame to read.
    checkpoints.frame.ramfb = Some(frame_config());

    let events = RefCell::new(Vec::new());
    let mut bytes = vec![0u8; 0x1000];
    bytes[0x100..0x120].fill(0x7f);
    let ram = LockFreeRam {
        bytes,
        platform: &platform,
        events: &events,
    };
    checkpoints.emit_after(guard, &mut boot_timer, &ram, |label, gpu, config, mem| {
        assert!(platform.try_lock().is_ok(), "{label} dumped under the lock");
        events.borrow_mut().push(label.to_owned());
        ramfb_dump::print_checkpoint_with_virtio_gpu(label, gpu, config, mem);
    });
    // Each sample reads the frame in turn; the boot timer's summary comes last.
    assert_eq!(
        events.into_inner(),
        [
            "ramfb-sample-1ms",
            "read",
            "ramfb-sample-2ms",
            "read",
            "read"
        ]
    );

    // Taken once: the next pass has nothing due and copies nothing.
    let guard = platform.lock().unwrap();
    let next = DisplayCheckpoints::take(&guard, &mut samples, &mut boot_timer, 0, 8, 0);
    assert!(next.labels.is_empty() && !next.boot.samples_display());
    assert!(next.frame.ramfb.is_none() && next.frame.gpu.is_none());
}
