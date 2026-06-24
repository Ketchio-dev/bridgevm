use std::path::PathBuf;

use super::{
    print_checkpoint, RamfbCheckpoint, RamfbSampleEnvError, RamfbSampleSchedule,
    RamfbShellObservation,
};
use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    ramfb::{RamfbConfig, DRM_FORMAT_XRGB8888},
};

struct TestRam {
    base: u64,
    bytes: Vec<u8>,
}

impl GuestMemoryMut for TestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
        else {
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

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())?;
        let end = start.checked_add(len)?;
        self.bytes.get(start..end).map(<[u8]>::to_vec)
    }
}

fn checkpoint_test_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "bridgevm-ramfb-checkpoint-test-{}",
        std::process::id()
    ))
}

fn field_value<'a>(line: &'a str, name: &str) -> &'a str {
    assert_parseable_fields(line);
    let prefix = format!("{name}=");
    line.split_whitespace()
        .find_map(|field| field.strip_prefix(&prefix))
        .unwrap()
}

fn assert_parseable_fields(line: &str) {
    assert_eq!(line.split_whitespace().next(), Some("ramfb"));
    assert_eq!(line.split_whitespace().nth(1), Some("checkpoint:"));
    assert!(line.split_whitespace().skip(2).all(|field| {
        let Some((key, value)) = field.split_once('=') else {
            return false;
        };
        !key.is_empty() && !value.is_empty()
    }));
}

#[path = "ramfb_dump_tests/checkpoints.rs"]
mod checkpoints;
#[path = "ramfb_dump_tests/sample_schedule.rs"]
mod sample_schedule;
