use bridgevm_hvf::machine::bridgevm_pc as board;

pub(super) fn contains(ipa: u64) -> bool {
    board::PCIE_ECAM.contains(ipa)
        || board::PCIE_MMIO_32.contains(ipa)
        || board::PCIE_MMIO_64.contains(ipa)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_config_and_both_bar_apertures_only() {
        assert!(contains(board::PCIE_ECAM.base));
        assert!(contains(board::PCIE_MMIO_32.base));
        assert!(contains(board::PCIE_MMIO_64.end() - 1));
        assert!(!contains(board::RAM_BASE));
    }
}
