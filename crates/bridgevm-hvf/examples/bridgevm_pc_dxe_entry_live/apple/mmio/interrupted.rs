use super::super::{hv_vcpu_get_reg, image_diagnostics, status, HvVcpu, HV_REG_PC};

pub(super) unsafe fn describe(
    vcpu: HvVcpu,
    reason: u32,
    mmio_exits: usize,
    ram: &[u8],
) -> Result<String, String> {
    let mut pc = 0;
    let mut x0 = 0;
    let mut x1 = 0;
    let mut fp = 0;
    let mut lr = 0;
    status(
        "read interrupted PC",
        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc),
    )?;
    status("read interrupted x0", hv_vcpu_get_reg(vcpu, 0, &mut x0))?;
    status("read interrupted x1", hv_vcpu_get_reg(vcpu, 1, &mut x1))?;
    status("read interrupted fp", hv_vcpu_get_reg(vcpu, 29, &mut fp))?;
    status("read interrupted lr", hv_vcpu_get_reg(vcpu, 30, &mut lr))?;
    let image = image_diagnostics::loaded_image(ram, pc);
    let caller = image_diagnostics::frame_return_address(ram, fp)
        .map(|address| {
            format!(
                "caller={address:#x} {}",
                image_diagnostics::loaded_image(ram, address)
            )
        })
        .unwrap_or_else(|| "caller=unresolved".to_string());
    let exception = image_diagnostics::exception_context(ram, fp)
        .unwrap_or_else(|| "exception_context=unresolved".to_string());
    Ok(format!(
        "unexpected vCPU exit reason {reason} PC={pc:#x} LR={lr:#x} FP={fp:#x} x0={x0:#x} x1={x1:#x} MMIO exits={mmio_exits} {image} {caller} {exception}"
    ))
}
