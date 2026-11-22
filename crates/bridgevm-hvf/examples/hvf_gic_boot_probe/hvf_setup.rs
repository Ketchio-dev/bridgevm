use super::*;

pub(crate) unsafe fn create_vm() {
    // Create the VM with the max IPA size: the PCIe ECAM sits at 256 GiB,
    // beyond the 36-bit default IPA window.
    let vmcfg = hv_vm_config_create();
    let mut max_ipa = 0u32;
    hv_vm_config_get_max_ipa_size(&mut max_ipa);
    hv_vm_config_set_ipa_size(vmcfg, max_ipa);
    let mut el2_supported = false;
    let el2_supported_status = hv_vm_config_get_el2_supported(&mut el2_supported);
    let mut el2_enabled_before = false;
    let el2_enabled_before_status = hv_vm_config_get_el2_enabled(vmcfg, &mut el2_enabled_before);
    let request_el2 = env_flag("BRIDGEVM_ENABLE_EL2");
    let el2_enable_status = if request_el2 && el2_supported_status == 0 && el2_supported {
        hv_vm_config_set_el2_enabled(vmcfg, true)
    } else {
        0
    };
    let mut el2_enabled_after = false;
    let el2_enabled_after_status = hv_vm_config_get_el2_enabled(vmcfg, &mut el2_enabled_after);
    println!(
        "EL2 config: requested={} supported={} status={el2_supported_status:#x}, enabled_before={} status={el2_enabled_before_status:#x}, set_true={el2_enable_status:#x}, enabled_after={} status={el2_enabled_after_status:#x}",
        request_el2, el2_supported, el2_enabled_before, el2_enabled_after
    );
    let vc = hv_vm_create(vmcfg);
    println!("hv_vm_create(ipa={max_ipa}) = {vc:#x}");
    assert_eq!(vc, 0, "hv_vm_create");
}

pub(crate) unsafe fn create_gic() {
    // In-kernel GICv3 must be created after the VM and before any vCPU.
    let gic = hv_gic_config_create();
    assert_eq!(
        hv_gic_config_set_distributor_base(gic, machine::GIC_DIST.base),
        0,
        "set dist base"
    );
    assert_eq!(
        hv_gic_config_set_redistributor_base(gic, machine::GIC_REDIST.base),
        0,
        "set redist base"
    );
    let mut spi_intid_base = 0u32;
    let mut spi_intid_count = 0u32;
    assert_eq!(
        hv_gic_get_spi_interrupt_range(&mut spi_intid_base, &mut spi_intid_count),
        0,
        "get SPI INTID range"
    );
    let msi_intid_base = machine::GIC_MSI_INTID_BASE;
    let msi_intid_count = machine::GIC_MSI_INTID_COUNT;
    assert!(
        msi_intid_base >= spi_intid_base
            && msi_intid_base + msi_intid_count <= spi_intid_base + spi_intid_count,
        "MSI INTID range {msi_intid_base}..{} outside supported SPI INTID range {spi_intid_base}..{}",
        msi_intid_base + msi_intid_count,
        spi_intid_base + spi_intid_count
    );
    assert_eq!(
        hv_gic_config_set_msi_region_base(gic, machine::GIC_ITS.base),
        0,
        "set MSI region base"
    );
    assert_eq!(
        hv_gic_config_set_msi_interrupt_range(gic, msi_intid_base, msi_intid_count),
        0,
        "set MSI INTID range"
    );
    let gic_r = hv_gic_create(gic);
    println!(
        "hv_gic_create = {gic_r:#x} (dist {:#x}, redist {:#x}, msi {:#x}+{:#x}, intids {}..{})",
        machine::GIC_DIST.base,
        machine::GIC_REDIST.base,
        machine::GIC_ITS.base,
        HV_GIC_REG_GICM_SET_SPI_NSR,
        msi_intid_base,
        msi_intid_base + msi_intid_count
    );
    assert_eq!(gic_r, 0, "hv_gic_create");
}
