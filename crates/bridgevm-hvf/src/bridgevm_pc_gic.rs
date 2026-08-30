//! Fail-closed Apple GIC geometry validation for BridgeVM Virtual ARM PC v1.

use crate::machine::{bridgevm_pc as board, Region};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppleGicGeometry {
    pub distributor_size: u64,
    pub distributor_alignment: u64,
    pub redistributor_region_size: u64,
    pub redistributor_size: u64,
    pub redistributor_alignment: u64,
    pub msi_region_size: u64,
    pub msi_region_alignment: u64,
    pub spi_intid_base: u32,
    pub spi_intid_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeVmPcGicPlan {
    pub distributor: Region,
    pub redistributors: Region,
    pub msi_frame: Region,
    pub msi_intid_base: u32,
    pub msi_intid_count: u32,
    pub vcpu_count: u64,
    pub host_geometry: AppleGicGeometry,
}

fn aligned(base: u64, alignment: u64) -> bool {
    alignment != 0 && base % alignment == 0
}

fn contains_range(base: u32, count: u32, wanted_base: u32, wanted_count: u32) -> bool {
    let Some(end) = base.checked_add(count) else {
        return false;
    };
    let Some(wanted_end) = wanted_base.checked_add(wanted_count) else {
        return false;
    };
    wanted_base >= base && wanted_end <= end
}

pub fn plan_for_geometry(
    geometry: AppleGicGeometry,
    vcpu_count: u64,
) -> Result<BridgeVmPcGicPlan, String> {
    if !(1..=board::MAX_CPUS).contains(&vcpu_count) {
        return Err(format!(
            "vCPU count {vcpu_count} is outside board range 1..={}",
            board::MAX_CPUS
        ));
    }
    let regions_fit = geometry.distributor_size <= board::GIC_DIST.size
        && geometry.redistributor_region_size <= board::GIC_REDIST.size
        && geometry.msi_region_size <= board::GIC_MSI_FRAME.size;
    if !regions_fit {
        return Err(format!("host GIC regions do not fit v1: {geometry:?}"));
    }
    let bases_align = aligned(board::GIC_DIST.base, geometry.distributor_alignment)
        && aligned(board::GIC_REDIST.base, geometry.redistributor_alignment)
        && aligned(board::GIC_MSI_FRAME.base, geometry.msi_region_alignment);
    if !bases_align {
        return Err("v1 GIC bases do not satisfy host alignment".to_string());
    }
    if geometry.redistributor_size != board::GICV3_REDIST_STRIDE
        || vcpu_count * geometry.redistributor_size > geometry.redistributor_region_size
    {
        return Err("host redistributor geometry does not match the v1 CPU contract".to_string());
    }
    if !contains_range(
        geometry.spi_intid_base,
        geometry.spi_intid_count,
        board::GIC_MSI_INTID_BASE,
        board::GIC_MSI_INTID_COUNT,
    ) {
        return Err("v1 MSI INTIDs are outside the host SPI range".to_string());
    }
    for spi in [
        board::SPI_UART,
        board::SPI_RTC,
        board::SPI_TPM,
        board::SPI_PCIE_INTA,
    ] {
        let intid = board::spi_to_intid(spi);
        if !contains_range(geometry.spi_intid_base, geometry.spi_intid_count, intid, 1)
            || contains_range(
                board::GIC_MSI_INTID_BASE,
                board::GIC_MSI_INTID_COUNT,
                intid,
                1,
            )
        {
            return Err(format!("device INTID {intid} is not a usable host SPI"));
        }
    }
    Ok(BridgeVmPcGicPlan {
        distributor: board::GIC_DIST,
        redistributors: board::GIC_REDIST,
        msi_frame: board::GIC_MSI_FRAME,
        msi_intid_base: board::GIC_MSI_INTID_BASE,
        msi_intid_count: board::GIC_MSI_INTID_COUNT,
        vcpu_count,
        host_geometry: geometry,
    })
}

#[cfg(test)]
#[path = "bridgevm_pc_gic_tests.rs"]
mod tests;
