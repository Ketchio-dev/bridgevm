#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HvfSupport {
    Available,
    Unavailable,
}

pub fn detect_hvf_support() -> HvfSupport {
    if cfg!(target_os = "macos") {
        HvfSupport::Available
    } else {
        HvfSupport::Unavailable
    }
}
