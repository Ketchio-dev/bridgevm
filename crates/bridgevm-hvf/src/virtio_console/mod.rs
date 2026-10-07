//! virtio_console, decomposed by responsibility.
#[macro_use]
#[cfg(test)]
mod tests;

#[macro_use]
mod trace;
mod agent_data_path;
mod agent_inbound;
mod control_message;
mod control_plane;
mod device_state;
mod msix_bridge;
mod notify;
mod pci_transport;
mod protocol;
mod queue_pending;
mod register_codec;
pub(crate) mod snapshot;
mod transport_regs;
mod tx_gather;
mod virtqueue;

use crate::virtio_queue::address::read_descriptor;
use crate::virtio_queue::address::read_u16_at as read_u16;
use crate::virtio_queue::address::write_used;
use crate::virtio_queue::address::DESC_SIZE;
pub(crate) use control_plane::*;
pub use device_state::*;
pub(crate) use msix_bridge::*;
pub use pci_transport::*;
pub use protocol::*;
pub(crate) use register_codec::*;
pub(crate) use trace::*;
pub(crate) use virtqueue::*;
