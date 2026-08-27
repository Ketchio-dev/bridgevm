//! SPI affinity routing shared by candidate scans and mutation paths.

use super::*;

impl UserspaceGic {
    fn route_affinity(route: u64) -> u64 {
        ((route >> 8) & 0xff) << 8 | (route & 0xff)
    }

    /// Which CPU a routed SPI targets. IRM (1-of-N) delivers to CPU0
    /// for QEMU parity; explicit affinity resolves against modelled CPUs.
    pub(super) fn route_target(&self, intid: usize) -> Option<usize> {
        let route = self.dist.route[intid];
        if route & IROUTER_IRM != 0 {
            return Some(0);
        }
        let affinity = Self::route_affinity(route);
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
        machine::cpu_mpidr(cpu as u64) == Self::route_affinity(route)
    }
}
