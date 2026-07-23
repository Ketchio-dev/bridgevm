//! mmio_devices, split by responsibility.

mod rtc;
mod serial;

pub use rtc::*;
pub use serial::*;
