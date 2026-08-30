//! Bounded live probe: ask Hypervisor.framework for its GIC geometry and create
//! that GIC at the BridgeVM Virtual ARM PC v1 addresses. No firmware or vCPU is
//! started, so success proves only the host GIC placement boundary.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple {
    use bridgevm_hvf::bridgevm_pc_gic::{plan_for_geometry, AppleGicGeometry};
    use std::ffi::c_void;

    type HvReturn = i32;
    type HvGicConfig = *mut c_void;
    const HV_SUCCESS: HvReturn = 0;

    #[link(name = "Hypervisor", kind = "framework")]
    unsafe extern "C" {
        fn hv_vm_config_create() -> *mut c_void;
        fn hv_vm_config_get_max_ipa_size(bits: *mut u32) -> HvReturn;
        fn hv_vm_config_set_ipa_size(config: *mut c_void, bits: u32) -> HvReturn;
        fn hv_vm_create(config: *mut c_void) -> HvReturn;
        fn hv_vm_destroy() -> HvReturn;
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

    unsafe fn query_geometry() -> Result<AppleGicGeometry, String> {
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

    unsafe fn run_unsafe() -> Result<(), String> {
        let geometry = query_geometry()?;
        let plan = plan_for_geometry(geometry, 4)?;
        let vm_config = hv_vm_config_create();
        if vm_config.is_null() {
            return Err("hv_vm_config_create returned null".to_string());
        }
        let mut max_ipa = 0;
        status("max IPA query", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
        status("set max IPA", hv_vm_config_set_ipa_size(vm_config, max_ipa))?;
        status("create VM", hv_vm_create(vm_config))?;

        let result = (|| {
            let gic = hv_gic_config_create();
            if gic.is_null() {
                return Err("hv_gic_config_create returned null".to_string());
            }
            status(
                "set distributor",
                hv_gic_config_set_distributor_base(gic, plan.distributor.base),
            )?;
            status(
                "set redistributors",
                hv_gic_config_set_redistributor_base(gic, plan.redistributors.base),
            )?;
            status(
                "set MSI frame",
                hv_gic_config_set_msi_region_base(gic, plan.msi_frame.base),
            )?;
            status(
                "set MSI INTIDs",
                hv_gic_config_set_msi_interrupt_range(
                    gic,
                    plan.msi_intid_base,
                    plan.msi_intid_count,
                ),
            )?;
            status("create GIC", hv_gic_create(gic))?;
            println!("BridgeVM Virtual ARM PC GIC probe: PASS");
            println!("geometry={geometry:?}");
            println!("plan={plan:?}");
            Ok(())
        })();
        let destroy = hv_vm_destroy();
        result?;
        status("destroy VM", destroy)
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
    eprintln!("BridgeVM Virtual ARM PC GIC probe requires Apple Silicon macOS");
}
