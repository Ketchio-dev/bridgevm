use super::hvf::*;
use super::memory::GuestRam;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};
use std::sync::mpsc;
use std::time::Duration;

const EC_WFX: u64 = 0x01;
const EC_HVC: u64 = 0x16;
const EC_DATA_ABORT: u64 = 0x24;
const ISV: u64 = 1 << 24;
const WRITE: u64 = 1 << 6;
const SIGN_EXTEND: u64 = 1 << 21;
const MAX_EXITS: usize = 100_000;

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

unsafe fn emulate(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    platform: &mut BridgeVmPcPlatform,
    ram: &mut GuestRam<'_>,
) -> Result<(), String> {
    let access = decode((*exit).exception.syndrome)?;
    let address = (*exit).exception.physical_address;
    let operation = if access.write {
        let mut value = 0;
        if access.register != 31 {
            status(
                "read MMIO source register",
                hv_vcpu_get_reg(vcpu, access.register, &mut value),
            )?;
        }
        MmioOp::Write {
            size: access.size,
            value,
        }
    } else {
        MmioOp::Read { size: access.size }
    };
    match (platform.on_mmio(address, operation, ram), operation) {
        (MmioOutcome::ReadValue(value), MmioOp::Read { .. }) => {
            if access.register != 31 {
                status(
                    "write MMIO result register",
                    hv_vcpu_set_reg(vcpu, access.register, extend(value, access)),
                )?;
            }
        }
        (MmioOutcome::WriteAck, MmioOp::Write { .. }) => {}
        (outcome, _) => {
            return Err(format!(
                "firmware MMIO {address:#x} was not handled: {outcome:?}"
            ))
        }
    }
    let mut pc = 0;
    status("read MMIO PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
    status("advance MMIO PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4))
}

pub(super) unsafe fn run(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    platform: &mut BridgeVmPcPlatform,
    ram: &mut GuestRam<'_>,
) -> Result<(usize, usize), String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if stop_rx.recv_timeout(Duration::from_secs(20)).is_err() {
            let _ = hv_vcpus_exit(&vcpu, 1);
        }
    });
    let mut mmio_exits = 0;
    let mut vtimer_exits = 0;
    let result = (|| loop {
        if mmio_exits + vtimer_exits >= MAX_EXITS {
            return Err("firmware exceeded the bounded exit count".to_string());
        }
        status("run vCPU", hv_vcpu_run(vcpu))?;
        match (*exit).reason {
            EXIT_EXCEPTION => {
                let syndrome = (*exit).exception.syndrome;
                match (syndrome >> 26) & 0x3f {
                    EC_DATA_ABORT => {
                        emulate(vcpu, exit, platform, ram)?;
                        mmio_exits += 1;
                    }
                    EC_HVC => return Ok((mmio_exits, vtimer_exits)),
                    EC_WFX => return Err(format!("firmware entered WFx ESR={syndrome:#x}")),
                    ec => {
                        return Err(format!(
                            "unexpected firmware exception EC={ec:#x} ESR={syndrome:#x}"
                        ))
                    }
                }
            }
            EXIT_VTIMER => {
                // Board uses the virtual timer; HVF auto-masks on the activation
                // exit, so unmask to keep the in-kernel GIC delivering INTID 27.
                status("unmask VTimer", hv_vcpu_set_vtimer_mask(vcpu, false))?;
                vtimer_exits += 1;
            }
            EXIT_CANCELED => return Err("firmware watchdog canceled the vCPU".to_string()),
            reason => return Err(format!("unexpected HVF exit reason {reason}")),
        }
    })();
    let _ = stop_tx.send(());
    watchdog
        .join()
        .map_err(|_| "firmware watchdog thread panicked".to_string())?;
    result
}
