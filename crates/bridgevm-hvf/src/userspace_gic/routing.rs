//! SPI affinity routing shared by candidate scans and mutation paths.

use super::*;

impl UserspaceGic {
    /// Decode the Aff1:Aff0 levels modelled by BridgeVM's GICD_IROUTER.
    fn irouter_affinity(value: u64) -> u64 {
        value & 0xffff
    }

    /// Resolve an SPI; deterministic IRM chooses CPU0, otherwise use MPIDR.
    pub(super) fn route_target(&self, intid: usize) -> Option<usize> {
        let route = self.dist.route[intid];
        if route & IROUTER_IRM != 0 {
            return Some(0);
        }
        let affinity = Self::irouter_affinity(route);
        (0..self.num_cpus).find(|&cpu| machine::cpu_mpidr(cpu as u64) == affinity)
    }

    /// Match the already-known candidate CPU directly. Unlike
    /// `route_target`, this never scans every modelled CPU.
    pub(super) fn spi_routes_to_cpu(&self, intid: usize, cpu: usize) -> bool {
        if cpu >= self.num_cpus {
            return false;
        }
        let route = self.dist.route[intid];
        if route & IROUTER_IRM != 0 {
            return cpu == 0;
        }
        machine::cpu_mpidr(cpu as u64) == Self::irouter_affinity(route)
    }
}
