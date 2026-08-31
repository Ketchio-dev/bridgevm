use super::hvf::*;
use bridgevm_hvf::bridgevm_pc_gic::{plan_for_geometry, AppleGicGeometry};

unsafe fn size(
    label: &str,
    query: unsafe extern "C" fn(*mut usize) -> HvReturn,
) -> Result<u64, String> {
    let mut value = 0usize;
    status(label, query(&mut value))?;
    u64::try_from(value).map_err(|_| format!("{label} does not fit u64"))
}

pub(super) unsafe fn create() -> Result<AppleGicGeometry, String> {
    let mut spi_intid_base = 0;
    let mut spi_intid_count = 0;
    status(
        "query GIC SPI range",
        hv_gic_get_spi_interrupt_range(&mut spi_intid_base, &mut spi_intid_count),
    )?;
    let geometry = AppleGicGeometry {
        distributor_size: size("query GIC distributor size", hv_gic_get_distributor_size)?,
        distributor_alignment: size(
            "query GIC distributor alignment",
            hv_gic_get_distributor_base_alignment,
        )?,
        redistributor_region_size: size(
            "query GIC redistributor region size",
            hv_gic_get_redistributor_region_size,
        )?,
        redistributor_size: size(
            "query GIC redistributor size",
            hv_gic_get_redistributor_size,
        )?,
        redistributor_alignment: size(
            "query GIC redistributor alignment",
            hv_gic_get_redistributor_base_alignment,
        )?,
        msi_region_size: size("query GIC MSI size", hv_gic_get_msi_region_size)?,
        msi_region_alignment: size(
            "query GIC MSI alignment",
            hv_gic_get_msi_region_base_alignment,
        )?,
        spi_intid_base,
        spi_intid_count,
    };
    let plan = plan_for_geometry(geometry, 1)?;
    let config = hv_gic_config_create();
    if config.is_null() {
        return Err("hv_gic_config_create returned null".to_string());
    }
    status(
        "set GIC distributor",
        hv_gic_config_set_distributor_base(config, plan.distributor.base),
    )?;
    status(
        "set GIC redistributors",
        hv_gic_config_set_redistributor_base(config, plan.redistributors.base),
    )?;
    status(
        "set GIC MSI frame",
        hv_gic_config_set_msi_region_base(config, plan.msi_frame.base),
    )?;
    status(
        "set GIC MSI range",
        hv_gic_config_set_msi_interrupt_range(config, plan.msi_intid_base, plan.msi_intid_count),
    )?;
    status("create GIC", hv_gic_create(config))?;
    Ok(geometry)
}
