//! Direct SPI route matching must remain identical to target resolution.

use super::*;

#[test]
fn direct_spi_route_match_agrees_with_target_resolution() {
    let mut gic = UserspaceGic::new(16);
    let intid = 40;
    let routes = [
        IROUTER_IRM,
        0,
        1,
        2,
        7,
        15,
        1 << 8,
        (0xff << 8) | 0xff,
        0xabcd_0000_0000_0005,
    ];

    for route in routes {
        gic.dist.route[intid] = route;
        let target = gic.route_target(intid);
        for cpu in 0..=gic.num_cpus() {
            assert_eq!(
                gic.spi_routes_to_cpu(intid, cpu),
                target == Some(cpu),
                "route={route:#x} cpu={cpu} target={target:?}"
            );
        }
    }
}

#[test]
fn spi_candidate_uses_explicit_and_irm_routes_without_cross_cpu_delivery() {
    let mut gic = UserspaceGic::new(4);
    let intid = 40;
    let (reg, bit) = Distributor::bit(intid);
    gic.dist.ctlr = GICD_CTLR_ENABLE_G1NS;
    gic.dist.group[reg] |= bit;
    gic.dist.enabled[reg] |= bit;
    gic.dist.pending[reg] |= bit;
    gic.dist.priority[intid] = 0x20;

    gic.dist.route[intid] = 2;
    for cpu in 0..4 {
        assert_eq!(
            gic.spi_candidate_for_cpu(cpu, 0xff)
                .map(|candidate| candidate.intid),
            (cpu == 2).then_some(intid as u32)
        );
    }

    gic.dist.route[intid] = IROUTER_IRM;
    for cpu in 0..4 {
        assert_eq!(
            gic.spi_candidate_for_cpu(cpu, 0xff)
                .map(|candidate| candidate.intid),
            (cpu == 0).then_some(intid as u32)
        );
    }
}
