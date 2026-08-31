use super::{
    contract, hv_vcpu_get_reg, hv_vcpu_run, hv_vcpu_set_reg, hv_vcpus_exit, hvc_diagnostics,
    status, BridgeVmPcPlatform, HvVcpu, HvVcpuExit, EXIT_EXCEPTION, HV_REG_PC,
};
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};
use std::sync::mpsc;
use std::time::Duration;

const EC_HVC: u64 = 0x16;
const EC_DATA_ABORT: u64 = 0x24;
const ISV: u64 = 1 << 24;
const WRITE: u64 = 1 << 6;
const SIGN_EXTEND: u64 = 1 << 21;
const MAX_MMIO_EXITS: usize = 64;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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

fn extend_read(value: u64, access: DataAbort) -> u64 {
    let bits = u32::from(access.size) * 8;
    let mask = u64::MAX >> (64 - bits);
    let value = value & mask;
    if !access.sign_extend || value & (1 << (bits - 1)) == 0 {
        return value;
    }
    let extended = value | !mask;
    if access.register_is_64_bit {
        extended
    } else {
        extended & u64::from(u32::MAX)
    }
}

unsafe fn emulate(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    platform: &mut BridgeVmPcPlatform,
) -> Result<(), String> {
    let syndrome = (*exit).exception.syndrome;
    let access = decode(syndrome)?;
    let ipa = (*exit).exception.physical_address;
    if !board::PCIE_ECAM.contains(ipa) {
        return Err(format!("firmware MMIO IPA {ipa:#x} is outside PCIe ECAM"));
    }
    let op = if access.write {
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
    match (platform.on_mmio(ipa, op), op) {
        (MmioOutcome::ReadValue(value), MmioOp::Read { .. }) => {
            if access.register != 31 {
                status(
                    "write MMIO result register",
                    hv_vcpu_set_reg(vcpu, access.register, extend_read(value, access)),
                )?;
            }
        }
        (MmioOutcome::WriteAck, MmioOp::Write { .. }) => {}
        (outcome, _) => return Err(format!("firmware PCIe MMIO was not handled: {outcome:?}")),
    }
    let mut pc = 0;
    status("read MMIO PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
    status("advance MMIO PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4))
}

pub(super) unsafe fn run_vcpu(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    platform: &mut BridgeVmPcPlatform,
) -> Result<(), String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if stop_rx.recv_timeout(Duration::from_secs(2)).is_err() {
            let _ = hv_vcpus_exit(&vcpu, 1);
        }
    });
    let run_result = (|| {
        let mut mmio_exits = 0;
        loop {
            status("run vCPU", hv_vcpu_run(vcpu))?;
            if (*exit).reason != EXIT_EXCEPTION {
                return Err(format!("unexpected vCPU exit reason {}", (*exit).reason));
            }
            let syndrome = (*exit).exception.syndrome;
            match (syndrome >> 26) & 0x3f {
                EC_DATA_ABORT if mmio_exits < MAX_MMIO_EXITS => {
                    emulate(vcpu, exit, platform)?;
                    mmio_exits += 1;
                }
                EC_HVC if mmio_exits == contract::pcie_function_count() => {
                    hvc_diagnostics::print_hvc_arguments(vcpu, syndrome)?;
                    return Ok(());
                }
                ec => {
                    return Err(format!(
                        "unexpected firmware exception EC={ec:#x} ESR={syndrome:#x} MMIO exits={mmio_exits}"
                    ))
                }
            }
        }
    })();
    let _ = stop_tx.send(());
    watchdog
        .join()
        .map_err(|_| "firmware PCIe watchdog panicked".to_string())?;
    run_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_unsigned_word_read_and_write() {
        let read = decode(0x9180_8005).unwrap();
        assert_eq!(read.size, 4);
        assert_eq!(read.register, 0);
        assert!(!read.write);
        let write = decode(0x9189_8045).unwrap();
        assert_eq!(write.size, 4);
        assert_eq!(write.register, 9);
        assert!(write.write);
    }

    #[test]
    fn rejects_an_abort_without_instruction_syndrome() {
        assert!(decode(EC_DATA_ABORT << 26).is_err());
    }
}
