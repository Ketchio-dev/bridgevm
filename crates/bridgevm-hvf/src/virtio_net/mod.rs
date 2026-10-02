//! virtio_net, decomposed by responsibility.
#[cfg(test)]
mod tests;

mod backend;
mod datapath;
mod device_state;
mod msix_bridge;
mod pci_transport;
mod protocol;
mod queue_pending;
mod register_codec;
mod snapshot;
mod transport_regs;
mod tx_gather;
mod virtqueue;

use crate::virtio_queue::address::read_descriptor;
use crate::virtio_queue::address::read_u16_at as read_u16;
use crate::virtio_queue::address::write_used;
use crate::virtio_queue::address::DESC_SIZE;
pub use backend::*;
pub use device_state::*;
pub(crate) use msix_bridge::*;
pub use pci_transport::*;
pub(crate) use protocol::*;
pub(crate) use register_codec::*;
pub use transport_regs::*;
pub(crate) use virtqueue::*;
