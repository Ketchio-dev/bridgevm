use crate::xhci_hid_input::SetupInputHostWake;
use crate::{watchdog_generation_matches, HvVcpuT};
use bridgevm_hvf::platform_virt::VirtPlatform;
use std::sync::{atomic::AtomicU64, Arc};

pub(super) fn arm_pointer_deadline(
    platform: &VirtPlatform, wake: &mut SetupInputHostWake, vcpu: HvVcpuT,
    generation: &Arc<AtomicU64>, boot_generation: u64,
) {
    let Some(deadline) = platform.xhci_pointer_report_deadline() else { return; };
    let generation = Arc::clone(generation);
    wake.arm(deadline, move || {
        if watchdog_generation_matches(&generation, boot_generation) {
            super::input_control_wake::exit_vcpu(vcpu);
        }
    });
}
