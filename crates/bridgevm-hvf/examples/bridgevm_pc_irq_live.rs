//! Bounded live proof that the BridgeVM Virtual ARM PC v1 GIC addresses carry
//! an architected timer PPI into a minimal EL1 guest. This is an interrupt-path
//! gate only: it does not boot firmware or Windows.

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
mod guest {
    use bridgevm_hvf::machine::bridgevm_pc as board;

    pub const MEMORY_SIZE: usize = 0x1_0000;
    pub const FLAG_OFFSET: usize = 0x3000;
    pub const HANDLER_OFFSET: usize = 0x1280;

    const SETUP_TEMPLATE: [u32; 29] = [
        0xd2800020, 0xd518cca0, 0xd5033fdf, 0xd2801fe0, 0xd5184600, 0xd2800020, 0xd518cce0, 0, 0,
        0xd518c000, 0xd5033fdf, 0, 0x52800262, 0xb9000022, 0, 0x52a10002, 0xb9008022, 0xb9010022,
        0xb904183f, 0xd5033f9f, 0xd5033fdf, 0xd53be040, 0xd2a00203, 0x8b030000, 0xd51be340,
        0x52800020, 0xd51be320, 0xd50342ff, 0x14000000,
    ];
    const HANDLER_TEMPLATE: [u32; 7] = [
        0xd538cc00, 0, 0, 0xd2800022, 0xf9000022, 0xd518cc20, 0xd69f03e0,
    ];

    fn movz_address(register: u32, address: u64) -> Result<u32, String> {
        if register > 31 {
            return Err("AArch64 register is outside 0..=31".to_string());
        }
        for halfword in 0..4 {
            let shift = halfword * 16;
            let immediate = (address >> shift) & 0xffff;
            if immediate != 0 && address == immediate << shift {
                return Ok(0xd280_0000
                    | ((halfword as u32) << 21)
                    | ((immediate as u32) << 5)
                    | register);
            }
        }
        Err(format!("address {address:#x} needs more than one MOVZ"))
    }

    fn movk(register: u32, immediate: u16, shift: u32) -> Result<u32, String> {
        if register > 31 || !matches!(shift, 0 | 16 | 32 | 48) {
            return Err("invalid AArch64 MOVK encoding request".to_string());
        }
        Ok(0xf280_0000 | ((shift / 16) << 21) | (u32::from(immediate) << 5) | register)
    }

    pub fn images() -> Result<([u32; 29], [u32; 7]), String> {
        let vector_page = board::RAM_BASE
            .checked_add(0x1000)
            .ok_or_else(|| "vector page overflow".to_string())?;
        let flag = board::RAM_BASE
            .checked_add(FLAG_OFFSET as u64)
            .ok_or_else(|| "flag address overflow".to_string())?;
        let redist_sgi = board::GIC_REDIST
            .base
            .checked_add(0x1_0000)
            .ok_or_else(|| "redistributor SGI frame overflow".to_string())?;

        let mut setup = SETUP_TEMPLATE;
        setup[7] = movz_address(0, board::RAM_BASE)?;
        setup[8] = movk(0, (vector_page & 0xffff) as u16, 0)?;
        setup[11] = movz_address(1, board::GIC_DIST.base)?;
        setup[14] = movz_address(1, redist_sgi)?;

        let mut handler = HANDLER_TEMPLATE;
        handler[1] = movz_address(1, board::RAM_BASE)?;
        handler[2] = movk(1, (flag & 0xffff) as u16, 0)?;
        Ok((setup, handler))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn guest_words_encode_only_the_bridgevm_pc_contract() {
            let (setup, handler) = images().unwrap();
            assert_eq!(setup[7], 0xd2c0_0020);
            assert_eq!(setup[8], 0xf282_0000);
            assert_eq!(setup[11], 0xd2a4_0001);
            assert_eq!(setup[14], 0xd2a4_2021);
            assert_eq!(handler[1], 0xd2c0_0021);
            assert_eq!(handler[2], 0xf286_0001);
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple {
    use super::guest::{images, FLAG_OFFSET, HANDLER_OFFSET, MEMORY_SIZE};
    use bridgevm_hvf::bridgevm_pc_gic::{plan_for_geometry, AppleGicGeometry};
    use bridgevm_hvf::machine::bridgevm_pc as board;
    use std::alloc::{alloc_zeroed, dealloc, Layout};
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use std::sync::mpsc;
    use std::time::Duration;

    type HvReturn = i32;
    type HvVcpu = u64;
    type HvGicConfig = *mut c_void;
    const HV_SUCCESS: HvReturn = 0;
    const HV_REG_PC: u32 = 31;
    const HV_REG_CPSR: u32 = 34;
    const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;
    const HV_MEMORY_READ: u64 = 1;
    const HV_MEMORY_WRITE: u64 = 2;
    const HV_MEMORY_EXEC: u64 = 4;

    #[repr(C)]
    struct HvVcpuExitException {
        syndrome: u64,
        virtual_address: u64,
        physical_address: u64,
    }

    #[repr(C)]
    struct HvVcpuExit {
        reason: u32,
        exception: HvVcpuExitException,
    }

    #[link(name = "Hypervisor", kind = "framework")]
    unsafe extern "C" {
        fn hv_vm_config_create() -> *mut c_void;
        fn hv_vm_config_get_max_ipa_size(bits: *mut u32) -> HvReturn;
        fn hv_vm_config_set_ipa_size(config: *mut c_void, bits: u32) -> HvReturn;
        fn hv_vm_create(config: *mut c_void) -> HvReturn;
        fn hv_vm_destroy() -> HvReturn;
        fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
        fn hv_vm_unmap(ipa: u64, size: usize) -> HvReturn;
        fn hv_vcpu_create(
            vcpu: *mut HvVcpu,
            exit: *mut *mut HvVcpuExit,
            config: *mut c_void,
        ) -> HvReturn;
        fn hv_vcpu_destroy(vcpu: HvVcpu) -> HvReturn;
        fn hv_vcpu_run(vcpu: HvVcpu) -> HvReturn;
        fn hv_vcpus_exit(vcpus: *const HvVcpu, vcpu_count: u32) -> HvReturn;
        fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> HvReturn;
        fn hv_vcpu_set_sys_reg(vcpu: HvVcpu, reg: u16, value: u64) -> HvReturn;
        fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpu, masked: bool) -> HvReturn;
        fn hv_gic_get_distributor_size(size: *mut usize) -> HvReturn;
        fn hv_gic_get_distributor_base_alignment(alignment: *mut usize) -> HvReturn;
        fn hv_gic_get_redistributor_region_size(size: *mut usize) -> HvReturn;
        fn hv_gic_get_redistributor_size(size: *mut usize) -> HvReturn;
        fn hv_gic_get_redistributor_base_alignment(alignment: *mut usize) -> HvReturn;
        fn hv_gic_get_msi_region_size(size: *mut usize) -> HvReturn;
        fn hv_gic_get_msi_region_base_alignment(alignment: *mut usize) -> HvReturn;
        fn hv_gic_get_spi_interrupt_range(base: *mut u32, count: *mut u32) -> HvReturn;
        fn hv_gic_config_create() -> HvGicConfig;
        fn hv_gic_config_set_distributor_base(config: HvGicConfig, base: u64) -> HvReturn;
        fn hv_gic_config_set_redistributor_base(config: HvGicConfig, base: u64) -> HvReturn;
        fn hv_gic_config_set_msi_region_base(config: HvGicConfig, base: u64) -> HvReturn;
        fn hv_gic_config_set_msi_interrupt_range(
            config: HvGicConfig,
            base: u32,
            count: u32,
        ) -> HvReturn;
        fn hv_gic_create(config: HvGicConfig) -> HvReturn;
    }

    fn status(label: &str, value: HvReturn) -> Result<(), String> {
        (value == HV_SUCCESS)
            .then_some(())
            .ok_or_else(|| format!("{label} failed: {value:#x}"))
    }

    unsafe fn size(
        label: &str,
        query: unsafe extern "C" fn(*mut usize) -> HvReturn,
    ) -> Result<u64, String> {
        let mut value = 0usize;
        status(label, query(&mut value))?;
        u64::try_from(value).map_err(|_| format!("{label} does not fit u64"))
    }

    unsafe fn geometry() -> Result<AppleGicGeometry, String> {
        let mut spi_intid_base = 0;
        let mut spi_intid_count = 0;
        status(
            "SPI range query",
            hv_gic_get_spi_interrupt_range(&mut spi_intid_base, &mut spi_intid_count),
        )?;
        Ok(AppleGicGeometry {
            distributor_size: size("distributor size", hv_gic_get_distributor_size)?,
            distributor_alignment: size(
                "distributor alignment",
                hv_gic_get_distributor_base_alignment,
            )?,
            redistributor_region_size: size(
                "redistributor region size",
                hv_gic_get_redistributor_region_size,
            )?,
            redistributor_size: size("redistributor size", hv_gic_get_redistributor_size)?,
            redistributor_alignment: size(
                "redistributor alignment",
                hv_gic_get_redistributor_base_alignment,
            )?,
            msi_region_size: size("MSI region size", hv_gic_get_msi_region_size)?,
            msi_region_alignment: size("MSI alignment", hv_gic_get_msi_region_base_alignment)?,
            spi_intid_base,
            spi_intid_count,
        })
    }

    unsafe fn create_gic() -> Result<AppleGicGeometry, String> {
        let geometry = geometry()?;
        let plan = plan_for_geometry(geometry, 1)?;
        let config = hv_gic_config_create();
        if config.is_null() {
            return Err("hv_gic_config_create returned null".to_string());
        }
        status(
            "set distributor",
            hv_gic_config_set_distributor_base(config, plan.distributor.base),
        )?;
        status(
            "set redistributors",
            hv_gic_config_set_redistributor_base(config, plan.redistributors.base),
        )?;
        status(
            "set MSI frame",
            hv_gic_config_set_msi_region_base(config, plan.msi_frame.base),
        )?;
        status(
            "set MSI INTIDs",
            hv_gic_config_set_msi_interrupt_range(
                config,
                plan.msi_intid_base,
                plan.msi_intid_count,
            ),
        )?;
        status("create GIC", hv_gic_create(config))?;
        Ok(geometry)
    }

    unsafe fn run_vcpu(
        vcpu: HvVcpu,
        exit: *mut HvVcpuExit,
        flag: *const u32,
    ) -> Result<(u32, u32), String> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if stop_rx.recv_timeout(Duration::from_secs(2)).is_err() {
                let _ = hv_vcpus_exit(&vcpu, 1);
            }
        });
        let mut vtimer_exits = 0;
        let result = loop {
            if let Err(error) = status("run vCPU", hv_vcpu_run(vcpu)) {
                break Err(error);
            }
            match (*exit).reason {
                0 => break Ok(()),
                2 => {
                    vtimer_exits += 1;
                    if let Err(error) = status(
                        "mask unexpected VTimer exit",
                        hv_vcpu_set_vtimer_mask(vcpu, true),
                    ) {
                        break Err(error);
                    }
                    if vtimer_exits > 3 {
                        break Err("too many VTimer exits for in-kernel GIC delivery".to_string());
                    }
                }
                1 => {
                    let esr = (*exit).exception.syndrome;
                    break Err(format!(
                        "unexpected guest exception EC={:#x} ESR={esr:#x}",
                        (esr >> 26) & 0x3f
                    ));
                }
                reason => break Err(format!("unexpected exit reason {reason}")),
            }
        };
        let _ = stop_tx.send(());
        watchdog
            .join()
            .map_err(|_| "watchdog thread panicked".to_string())?;
        result?;
        Ok((std::ptr::read_volatile(flag), vtimer_exits))
    }

    unsafe fn run_created_vm() -> Result<(AppleGicGeometry, u32, u32), String> {
        let geometry = create_gic()?;
        let layout = Layout::from_size_align(MEMORY_SIZE, MEMORY_SIZE)
            .map_err(|error| format!("guest memory layout: {error}"))?;
        let memory = alloc_zeroed(layout);
        if memory.is_null() {
            return Err("guest memory allocation failed".to_string());
        }
        let result = (|| {
            let (setup, handler) = images()?;
            for (index, word) in setup.iter().enumerate() {
                std::ptr::write_unaligned(memory.add(index * 4).cast::<u32>(), word.to_le());
            }
            for (index, word) in handler.iter().enumerate() {
                std::ptr::write_unaligned(
                    memory.add(HANDLER_OFFSET + index * 4).cast::<u32>(),
                    word.to_le(),
                );
            }
            status(
                "map guest RAM",
                hv_vm_map(
                    memory.cast(),
                    board::RAM_BASE,
                    MEMORY_SIZE,
                    HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
                ),
            )?;
            let mapped_result = (|| {
                let mut vcpu = 0;
                let mut exit = null_mut();
                status(
                    "create vCPU",
                    hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
                )?;
                let vcpu_result = (|| {
                    status(
                        "set MPIDR_EL1",
                        hv_vcpu_set_sys_reg(
                            vcpu,
                            HV_SYS_REG_MPIDR_EL1,
                            0x8000_0000 | board::cpu_mpidr(0),
                        ),
                    )?;
                    status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, board::RAM_BASE))?;
                    status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
                    status("unmask VTimer", hv_vcpu_set_vtimer_mask(vcpu, false))?;
                    run_vcpu(vcpu, exit, memory.add(FLAG_OFFSET).cast::<u32>())
                })();
                let destroy = hv_vcpu_destroy(vcpu);
                vcpu_result.and_then(|result| {
                    status("destroy vCPU", destroy)?;
                    Ok(result)
                })
            })();
            let unmap = hv_vm_unmap(board::RAM_BASE, MEMORY_SIZE);
            mapped_result.and_then(|result| {
                status("unmap guest RAM", unmap)?;
                Ok(result)
            })
        })();
        dealloc(memory, layout);
        let (flag, vtimer_exits) = result?;
        Ok((geometry, flag, vtimer_exits))
    }

    unsafe fn run_unsafe() -> Result<(), String> {
        let config = hv_vm_config_create();
        if config.is_null() {
            return Err("hv_vm_config_create returned null".to_string());
        }
        let mut max_ipa = 0;
        status("max IPA query", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
        status("set max IPA", hv_vm_config_set_ipa_size(config, max_ipa))?;
        status("create VM", hv_vm_create(config))?;
        let result = run_created_vm();
        let destroy = hv_vm_destroy();
        let (geometry, flag, vtimer_exits) = result?;
        status("destroy VM", destroy)?;
        if flag != 1 {
            return Err(format!("guest IRQ handler flag is {flag}, expected 1"));
        }
        if vtimer_exits != 0 {
            return Err(format!("observed {vtimer_exits} unexpected VTimer exits"));
        }
        println!("BridgeVM Virtual ARM PC IRQ probe: PASS");
        println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
        println!(
            "gic_dist={:#x} gic_redist={:#x} gic_msi={:#x} ram={:#x}",
            board::GIC_DIST.base,
            board::GIC_REDIST.base,
            board::GIC_MSI_FRAME.base,
            board::RAM_BASE
        );
        println!("geometry={geometry:?}");
        println!("flag={flag} vtimer_exits={vtimer_exits}");
        println!("LIVE PROOF: BridgeVM Virtual ARM PC v1 GIC delivers the architected-timer PPI");
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        unsafe { run_unsafe() }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), String> {
    apple::run()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("BridgeVM Virtual ARM PC IRQ probe requires Apple Silicon macOS");
}
