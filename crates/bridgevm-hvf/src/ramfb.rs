pub const RAMFB_FW_CFG_FILE: &str = "etc/ramfb";
pub const RAMFB_CONFIG_SIZE: usize = 28;
pub const DRM_FORMAT_XRGB8888: u32 = 0x3432_5258;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamfbConfig {
    pub addr: u64,
    pub fourcc: u32,
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl RamfbConfig {
    pub fn from_be_bytes(bytes: &[u8]) -> Result<Self, RamfbParseError> {
        if bytes.len() != RAMFB_CONFIG_SIZE {
            return Err(RamfbParseError::WrongSize {
                actual: bytes.len(),
            });
        }
        Ok(Self {
            addr: u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            fourcc: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            flags: u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            width: u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            height: u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
            stride: u32::from_be_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
        })
    }

    pub const fn is_active(self) -> bool {
        self.addr != 0
            && self.fourcc != 0
            && self.width != 0
            && self.height != 0
            && self.stride != 0
    }

    pub const fn is_xrgb8888(self) -> bool {
        self.fourcc == DRM_FORMAT_XRGB8888
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamfbParseError {
    WrongSize { actual: usize },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Ramfb {
    config: Option<RamfbConfig>,
}

impl Ramfb {
    pub const fn new() -> Self {
        Self { config: None }
    }

    pub fn update_from_fw_cfg(&mut self, bytes: &[u8]) {
        self.config = match RamfbConfig::from_be_bytes(bytes) {
            Ok(config) if config.is_active() => Some(config),
            Ok(_) | Err(_) => None,
        };
    }

    pub const fn config(self) -> Option<RamfbConfig> {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xrgb_config_bytes() -> [u8; RAMFB_CONFIG_SIZE] {
        let mut bytes = [0u8; RAMFB_CONFIG_SIZE];
        bytes[0..8].copy_from_slice(&0x4008_0000u64.to_be_bytes());
        bytes[8..12].copy_from_slice(&DRM_FORMAT_XRGB8888.to_be_bytes());
        bytes[12..16].copy_from_slice(&0u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&800u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&600u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&(800u32 * 4).to_be_bytes());
        bytes
    }

    #[test]
    fn parses_qemu_big_endian_config() {
        let config = RamfbConfig::from_be_bytes(&xrgb_config_bytes()).unwrap();

        assert_eq!(config.addr, 0x4008_0000);
        assert_eq!(config.fourcc, DRM_FORMAT_XRGB8888);
        assert_eq!(config.flags, 0);
        assert_eq!(config.width, 800);
        assert_eq!(config.height, 600);
        assert_eq!(config.stride, 3200);
        assert!(config.is_active());
        assert!(config.is_xrgb8888());
    }

    #[test]
    fn zero_config_is_inactive() {
        let mut ramfb = Ramfb::new();

        ramfb.update_from_fw_cfg(&[0u8; RAMFB_CONFIG_SIZE]);

        assert_eq!(ramfb.config(), None);
    }

    #[test]
    fn wrong_size_is_rejected() {
        assert_eq!(
            RamfbConfig::from_be_bytes(&[0u8; RAMFB_CONFIG_SIZE - 1]),
            Err(RamfbParseError::WrongSize {
                actual: RAMFB_CONFIG_SIZE - 1
            })
        );
    }
}
