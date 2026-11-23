//! virtio_net, decomposed by responsibility.
#[cfg(test)]
mod tests;

mod backend;
mod datapath;
mod device_state;
mod pci_transport;
mod protocol;
mod queue_pending;
mod rx_delivery;
mod snapshot;
mod transport_regs;
mod tx_gather;
mod virtqueue;

use crate::virtio_pci::*;
use crate::virtio_queue::address::descriptor_chain_into;
use crate::virtio_queue::address::read_avail_idx;
use crate::virtio_queue::address::read_u16_at as read_u16;
use crate::virtio_queue::address::write_used;
use crate::virtio_queue::address::Descriptor;
use crate::virtio_queue::address::DESC_F_WRITE;
#[cfg(test)]
use crate::virtio_queue::address::{DESC_F_NEXT, DESC_SIZE};
pub use backend::*;
pub use device_state::*;
pub use pci_transport::*;
pub(crate) use protocol::*;
