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
                        super::mmio::emulate(vcpu, exit, platform, gic, ram)?;
                        mmio_exits += 1;
                    }
                    EC_SYSREG => {
                        gic.sysreg(vcpu, syndrome)?;
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
                // Virtual timer fired: re-arm CNTV_CVAL to a future deadline so
                // HVF does not immediately re-fire (nobody else re-arms it once
                // the guest installs its own vectors), make the timer PPI
                // (INTID 27) pending in the userspace GIC, then unmask.
                let deadline = mach_absolute_time().wrapping_add(VTIMER_DEADLINE_TICKS);
                status(
                    "re-arm CNTV_CVAL",
                    hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, deadline),
                )?;
                gic.vtimer_fired();
                status("unmask VTimer", hv_vcpu_set_vtimer_mask(vcpu, false))?;
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
