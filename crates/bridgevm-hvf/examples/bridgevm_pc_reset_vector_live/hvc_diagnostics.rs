type HvReturn = i32;

const HV_SUCCESS: HvReturn = 0;

#[link(name = "Hypervisor", kind = "framework")]
unsafe extern "C" {
    fn hv_vcpu_get_reg(vcpu: u64, reg: u32, value: *mut u64) -> HvReturn;
}

pub unsafe fn print_hvc_arguments(vcpu: u64, esr: u64) -> Result<(), String> {
    let mut args = [0_u64; 3];
    for (register, value) in args.iter_mut().enumerate() {
        let status = hv_vcpu_get_reg(vcpu, register as u32, value);
        if status != HV_SUCCESS {
            return Err(format!("read HVC argument failed: {status:#x}"));
        }
    }
    eprintln!("hvc_iss={:#x} args={args:x?}", esr & 0xffff);
    Ok(())
}
