//! Data-abort (MMIO) decode and emulation, routing GIC-window accesses to the
//! userspace GIC and everything else to the platform device bus.

use super::hvf::*;
use super::memory::GuestRam;
use super::us_gic::UsGic;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

const ISV: u64 = 1 << 24;
const WRITE: u64 = 1 << 6;
const SIGN_EXTEND: u64 = 1 << 21;
const EC_DATA_ABORT: u64 = 0x24;

#[derive(Clone, Copy)]
struct DataAbort {
    size: u8,
    register: u32,
    write: bool,
    sign_extend: bool,
    register_is_64_bit: bool,
}

fn decode(syndrome: u64) -> Result<DataAbort, String> {
    if (syndrome >> 26) & 0x3f != EC_DATA_ABORT || syndrome & ISV == 0 {
        return Err(format!("undecodable MMIO data abort ESR={syndrome:#x}"));
    }
    Ok(DataAbort {
        size: 1 << ((syndrome >> 22) & 3),
        register: ((syndrome >> 16) & 0x1f) as u32,
        write: syndrome & WRITE != 0,
        sign_extend: syndrome & SIGN_EXTEND != 0,
        register_is_64_bit: syndrome & (1 << 15) != 0,
    })
}

fn extend(value: u64, access: DataAbort) -> u64 {
    let bits = u32::from(access.size) * 8;
    let mask = u64::MAX >> (64 - bits);
    let value = value & mask;
    if !access.sign_extend || value & (1 << (bits - 1)) == 0 {
        return value;
    }
    let value = value | !mask;
    if access.register_is_64_bit {
        value
    } else {
        value & u64::from(u32::MAX)
    }
}

unsafe fn source_value(vcpu: HvVcpu, access: DataAbort) -> Result<u64, String> {
    if !access.write || access.register == 31 {
        return Ok(0);
    }
    let mut value = 0;
    status(
        "read MMIO source register",
        hv_vcpu_get_reg(vcpu, access.register, &mut value),
    )?;
    Ok(value)
}

unsafe fn store_result(vcpu: HvVcpu, access: DataAbort, value: u64) -> Result<(), String> {
    if !access.write && access.register != 31 {
        status(
            "write MMIO result register",
            hv_vcpu_set_reg(vcpu, access.register, extend(value, access)),
        )?;
    }
    let mut pc = 0;
    status("read MMIO PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
    status("advance MMIO PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4))
}

/// Handle a write fault on the write-watched page (mapped read+exec so writes
/// trap): log the writer's PC when it hits the watched word, then complete the
/// store into guest RAM so the guest is unaffected.
pub(super) unsafe fn watched_write(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    ram: &mut GuestRam<'_>,
    watch_gpa: u64,
) -> Result<(), String> {
    let address = (*exit).exception.physical_address;
    let Ok(access) = decode((*exit).exception.syndrome) else {
        // Store without a valid instruction syndrome (ISV=0). We cannot emulate
        // the byte-exact write, but the page is otherwise data: advance past it
        // so the watch keeps observing later, syndrome-valid stores.
        if address == watch_gpa {
            let mut pc = 0;
            status("read watch PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
            eprintln!("WWATCH(isv0) gpa={address:#x} pc={pc:#x}");
        }
        let mut pc = 0;
        status("read PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
        return status("advance PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4));
    };
    let source = source_value(vcpu, access)?;
    if address == watch_gpa {
        let mut pc = 0;
        status("read watch PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
        eprintln!(
            "WWATCH gpa={address:#x} val={source:#x} size={} pc={pc:#x}",
            access.size
        );
    }
    ram.write_bytes(address, &source.to_le_bytes()[..access.size as usize]);
    store_result(vcpu, access, 0)
}

pub(super) unsafe fn emulate(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    platform: &mut BridgeVmPcPlatform,
    gic: &mut UsGic,
    ram: &mut GuestRam<'_>,
) -> Result<(), String> {
    let access = decode((*exit).exception.syndrome)?;
    let address = (*exit).exception.physical_address;
    let source = source_value(vcpu, access)?;
    if UsGic::owns(address) {
        let write = access.write.then_some(source);
        let value = gic.mmio(address, access.size, write);
        return store_result(vcpu, access, value);
    }
    let operation = if access.write {
        MmioOp::Write {
            size: access.size,
            value: source,
        }
    } else {
        MmioOp::Read { size: access.size }
    };
    let value = match (platform.on_mmio(address, operation, ram), operation) {
        (MmioOutcome::ReadValue(value), MmioOp::Read { .. }) => value,
        (MmioOutcome::WriteAck, MmioOp::Write { .. }) => 0,
        (outcome, _) => {
            return Err(format!(
                "firmware MMIO {address:#x} was not handled: {outcome:?}"
            ))
        }
    };
    store_result(vcpu, access, value)
}
