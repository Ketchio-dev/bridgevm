//! mmio_emulation, split by responsibility.

mod read_emulation;
mod read_exit;
mod write_emulation;

pub use read_emulation::*;
pub use read_exit::*;
pub use write_emulation::*;
