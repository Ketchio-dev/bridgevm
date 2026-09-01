use super::hvf::*;
use super::memory::GuestRam;
use super::us_gic::UsGic;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use std::sync::mpsc;
use std::time::Duration;

const EC_WFX: u64 = 0x01;
const EC_HVC: u64 = 0x16;
const EC_SYSREG: u64 = 0x18;
const EC_DATA_ABORT: u64 = 0x24;
const MAX_EXITS: usize = 4_000_000;
const MAX_WATCHDOG_SECS: u64 = 120;

fn watchdog_secs() -> u64 {
    std::env::var("BRIDGEVM_PC_WATCHDOG_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (1..=MAX_WATCHDOG_SECS).contains(seconds))
        .unwrap_or(20)
}

/// The boot-result page lives in low guest RAM, which the Windows Boot Manager
/// reuses once it runs, so capture the validated StartImage-handoff record the
/// moment it first becomes valid — before it is overwritten by deeper progress.
fn snapshot_result(ram: &GuestRam<'_>, snapshot: &mut Option<super::result::BootResult>) {
    if snapshot.is_none() {
        if let Ok(result) = super::result::validate_windows_start(ram.bytes()) {
            *snapshot = Some(result);
        }
    }
}

pub(super) unsafe fn run(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    platform: &mut BridgeVmPcPlatform,
    gic: &mut UsGic,
    ram: &mut GuestRam<'_>,
    snapshot: &mut Option<super::result::BootResult>,
    windows_diagnostic: bool,
) -> Result<(usize, usize), String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if stop_rx
            .recv_timeout(Duration::from_secs(watchdog_secs()))
            .is_err()
        {
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
                        super::mmio::emulate(vcpu, exit, platform, gic, ram)?;
                        mmio_exits += 1;
                    }
                    EC_SYSREG => {
                        gic.sysreg(vcpu, syndrome)?;
                        mmio_exits += 1;
                    }
                    EC_HVC if windows_diagnostic => match super::psci::handle(vcpu)? {
                        super::psci::Action::Resume => {}
                        super::psci::Action::SystemOff => {
                            return Err("Windows requested PSCI SYSTEM_OFF".to_string())
                        }
                        super::psci::Action::SystemReset => {
                            return Err("Windows requested PSCI SYSTEM_RESET".to_string())
                        }
                    },
                    EC_HVC => return Ok((mmio_exits, vtimer_exits)),
                    EC_WFX => {
                        let mut pc = 0;
                        status("read WFx PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
                        if syndrome & 1 == 0 && !gic.line_asserted() {
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        status("advance WFx PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4))?;
                    }
                    ec => {
                        return Err(format!(
                            "unexpected firmware exception EC={ec:#x} ESR={syndrome:#x}"
                        ))
                    }
                }
            }
            EXIT_VTIMER => {
                status("mask VTimer", hv_vcpu_set_vtimer_mask(vcpu, true))?;
                gic.vtimer_fired();
                vtimer_exits += 1;
            }
            EXIT_CANCELED => return Err("firmware watchdog canceled the vCPU".to_string()),
            reason => return Err(format!("unexpected HVF exit reason {reason}")),
        }
        gic.refresh(vcpu)?;
        snapshot_result(ram, snapshot);
    })();
    let _ = stop_tx.send(());
    watchdog
        .join()
        .map_err(|_| "firmware watchdog thread panicked".to_string())?;
    result
}
