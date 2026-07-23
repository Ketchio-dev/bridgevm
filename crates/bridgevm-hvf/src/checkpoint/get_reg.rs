//! Split out of checkpoint.rs to keep files under 850 lines.

use super::*;

use std::io::{self};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn get_reg(vcpu: HvVcpu, reg: u32, value: &mut u64) -> io::Result<()> {
    hv(
        unsafe { hv_vcpu_get_reg(vcpu, reg, value) },
        "hv_vcpu_get_reg",
    )
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> io::Result<()> {
    hv(
        unsafe { hv_vcpu_set_reg(vcpu, reg, value) },
        "hv_vcpu_set_reg",
    )
}

pub(crate) fn hv(status: i32, operation: &str) -> io::Result<()> {
    if status == HV_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("{operation} failed with HVF status {status:#x}"),
        ))
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub(crate) fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "HVF checkpointing requires macOS on arm64",
    )
}
