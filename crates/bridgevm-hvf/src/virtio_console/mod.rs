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
mod notify;
mod pci_transport;
mod poll;
mod protocol;
mod queue_pending;
pub(crate) mod snapshot;
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
pub use agent_inbound::{AGENT_INBOUND_HOOK, MAX_HOST_INBOUND_LEN};
pub(crate) use control_plane::*;
pub use device_state::*;
pub use pci_transport::*;
pub use protocol::*;
pub(crate) use trace::*;
pub(crate) use virtqueue::*;
