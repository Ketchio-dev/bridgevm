use super::*;

fn supported_geometry() -> AppleGicGeometry {
    AppleGicGeometry {
        distributor_size: 0x1_0000,
        distributor_alignment: 0x1_0000,
        redistributor_region_size: 0x100_0000,
        redistributor_size: 0x2_0000,
        redistributor_alignment: 0x1_0000,
        msi_region_size: 0x1000,
        msi_region_alignment: 0x1000,
        spi_intid_base: 32,
        spi_intid_count: 960,
    }
}

#[test]
fn supported_host_geometry_produces_the_versioned_plan() {
    let plan = plan_for_geometry(supported_geometry(), 64).expect("supported plan");
    assert_eq!(plan.distributor, board::GIC_DIST);
    assert_eq!(plan.redistributors, board::GIC_REDIST);
    assert_eq!(plan.msi_frame, board::GIC_MSI_FRAME);
    assert_eq!(plan.vcpu_count, 64);
}

#[test]
fn plan_refuses_geometry_that_would_redefine_the_contract() {
    let mut geometry = supported_geometry();
    geometry.redistributor_size = 0x4_0000;
    assert!(plan_for_geometry(geometry, 4)
        .unwrap_err()
        .contains("redistributor geometry"));

    let mut geometry = supported_geometry();
    geometry.msi_region_alignment = 0x2000_0000;
    assert!(plan_for_geometry(geometry, 4)
        .unwrap_err()
        .contains("alignment"));
}

#[test]
fn plan_refuses_cpu_and_interrupt_ranges_outside_host_support() {
    assert!(plan_for_geometry(supported_geometry(), 0)
        .unwrap_err()
        .contains("vCPU count"));
    let mut geometry = supported_geometry();
    geometry.spi_intid_count = 64;
    assert!(plan_for_geometry(geometry, 4)
        .unwrap_err()
        .contains("MSI INTIDs"));
}
