use super::pcie::PcieProof;
use std::fmt;

pub(super) struct PcieDevices<'a>(pub(super) &'a PcieProof);

impl fmt::Display for PcieDevices<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.0.nvme, self.0.nvme_block)
    }
}
