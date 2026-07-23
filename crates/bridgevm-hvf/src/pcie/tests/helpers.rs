//! Split test module.

use super::super::*;
use std::sync::Mutex;
use std::sync::OnceLock;

/// Build a raw ECAM offset for a (bus, dev, fn, reg) tuple, the way the run
/// loop derives it from a guest fault address minus the window base.
pub(super) fn ecam_offset(bus: u8, dev: u8, func: u8, reg: u16) -> u64 {
    (u64::from(bus) << SHIFT_BUS)
        | (u64::from(dev) << SHIFT_DEV)
        | (u64::from(func) << SHIFT_FN)
        | u64::from(reg)
}

pub(super) fn bdf_ecam_offset(bdf: (u8, u8, u8), reg: u16) -> u64 {
    ecam_offset(bdf.0, bdf.1, bdf.2, reg)
}

pub(super) fn read_config_bytes(ecam: &PcieEcam, bdf: (u8, u8, u8), len: usize) -> Vec<u8> {
    (0..len)
        .map(|reg| {
            u8::try_from(ecam.cfg_read(bdf_ecam_offset(bdf, reg as u16), 1))
                .expect("single-byte config read fits in u8")
        })
        .collect()
}

pub(super) fn find_vendor_cfg_type(
    ecam: &PcieEcam,
    bdf: (u8, u8, u8),
    cfg_type: u8,
) -> Option<u16> {
    let mut cap = ecam.cfg_read(bdf_ecam_offset(bdf, REG_CAP_PTR), 1) as u8;
    for _ in 0..32 {
        if cap == 0 {
            return None;
        }
        let cap_id = ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap)), 1) as u8;
        let next = ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap) + 1), 1) as u8;
        if cap_id == 0x09
            && ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap) + 3), 1) as u8 == cfg_type
        {
            return Some(u16::from(cap));
        }
        cap = next;
    }
    None
}

pub(super) fn cap_chain_contains_vendor_cfg_type(
    ecam: &PcieEcam,
    bdf: (u8, u8, u8),
    cfg_type: u8,
) -> bool {
    find_vendor_cfg_type(ecam, bdf, cfg_type).is_some()
}

pub(super) fn with_hostmem_mib_env<R>(value: &str, f: impl FnOnce() -> R) -> R {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let previous = std::env::var_os("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB");
    std::env::set_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", value);
    let result = f();
    match previous {
        Some(previous) => std::env::set_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", previous),
        None => std::env::remove_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB"),
    }
    result
}
