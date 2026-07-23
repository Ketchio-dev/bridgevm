//! The virtio-console control protocol: encoding, control-TX handling, port open/name.

use super::*;
use crate::fwcfg::GuestMemoryMut;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Control {
    pub(crate) id: u32,
    pub(crate) event: u16,
    pub(crate) value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingControlMessage {
    pub(crate) len: usize,
    pub(crate) bytes: [u8; MAX_CONTROL_MESSAGE_LEN],
}

impl VirtioConsole {
    pub(crate) fn process_control_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_CONTROL_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return false;
        };
        self.queues[queue_index].last_avail_seen = avail_idx;
        let mut progressed = false;
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut bytes = std::mem::take(&mut self.read_scratch);
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                break;
            };
            if Self::read_chain_into(
                mem,
                &queue,
                head,
                &mut descs,
                &mut bytes,
                MAX_CONTROL_MESSAGE_LEN,
            ) {
                self.handle_control_tx(&bytes);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.queues[queue_index].used_produced =
                self.queues[queue_index].used_produced.saturating_add(1);
            self.mark_queue_interrupt(queue_index);
            progressed = true;
        }
        descs.clear();
        bytes.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = bytes;
        progressed
    }

    pub(crate) fn handle_control_tx(&mut self, bytes: &[u8]) {
        let Some(control) = Control::parse(bytes) else {
            return;
        };
        console_trace!(
            "ctrl<-guest id={} event={} value={}",
            control.id,
            control.event,
            control.value
        );
        match control.event {
            VIRTIO_CONSOLE_DEVICE_READY if control.value == 1 => {
                self.enqueue_control(Control::new(0, VIRTIO_CONSOLE_DEVICE_ADD, 0));
                self.enqueue_control(Control::new(1, VIRTIO_CONSOLE_DEVICE_ADD, 0));
            }
            VIRTIO_CONSOLE_PORT_READY if control.value == 1 => {
                if let Some(port) = self.ports.get_mut(control.id as usize) {
                    port.ready = true;
                }
                if control.id == AGENT_PORT_ID {
                    self.enqueue_control(Self::agent_port_name_message());
                    self.ports[1].host_open = true;
                    // A fresh PORT_READY means the port (re)entered D0; vioser
                    // clears HostConnected on every D0 exit, so treat this as a
                    // new connection epoch and resume re-asserting PORT_NAME +
                    // PORT_OPEN.
                    self.agent_connected_confirmed = false;
                    self.enqueue_control(Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1));
                }
            }
            VIRTIO_CONSOLE_PORT_OPEN => {
                if let Some(port) = self.ports.get_mut(control.id as usize) {
                    port.guest_open = control.value != 0;
                }
            }
            VIRTIO_CONSOLE_DEVICE_READY
            | VIRTIO_CONSOLE_DEVICE_ADD
            | VIRTIO_CONSOLE_DEVICE_REMOVE
            | VIRTIO_CONSOLE_CONSOLE_PORT
            | VIRTIO_CONSOLE_RESIZE
            | VIRTIO_CONSOLE_PORT_NAME => {}
            _ => {}
        }
    }

    pub(crate) fn enqueue_control(&mut self, message: impl Into<PendingControlMessage>) {
        let message = message.into();
        if console_trace_enabled() {
            if let Some(control) = Control::parse(message.as_slice()) {
                eprintln!(
                    "[vcon] ctrl->guest id={} event={} value={} bytes={}",
                    control.id,
                    control.event,
                    control.value,
                    message.len()
                );
            }
        }
        self.pending_control.push_back(message);
    }

    /// Re-assert PORT_OPEN(host) toward the agent port while the connection is
    /// still unconfirmed. This replaces the old fixed-count resend budget: a
    /// one-shot burst permanently gives up if the guest's port only stabilizes
    /// (e.g. after a PnP resource rebalance / D0 bounce) *after* the burst is
    /// spent, which strands HostConnected=FALSE forever. Instead we keep
    /// re-asserting on every control-RX rearm and every host->guest send, but
    /// only ever leave one re-assert in flight (no flooding) and stop entirely
    /// once the guest proves the link by sending a TX byte. vioser's PORT_OPEN
    /// handler is idempotent (`if HostConnected != Connected`), so a redundant
    /// PORT_OPEN after the latch is a harmless no-op.
    pub(crate) fn maybe_reassert_host_open(&mut self) {
        let port = self.ports[AGENT_PORT_ID as usize];
        if !port.ready || !port.host_open || self.agent_connected_confirmed {
            return;
        }
        if self.host_open_reassert_pending() {
            return;
        }
        // Re-send PORT_NAME as well as PORT_OPEN. vioser's VIOSerialFindPortById
        // drops control messages that arrive before the port PDO fully resolves;
        // the same race that made the first PORT_OPEN need re-sending also drops
        // the first PORT_NAME. If PORT_NAME is lost, vioser never sets the port's
        // NameString, so it never creates the `\DosDevices\<name>` symbolic link
        // and the guest agent's CreateFile on `\\.\<name>` fails (the port has
        // only its default `vportNpM` desc, no friendly name). vioser's
        // VIOSerialPortCreateName is idempotent (`if (!NameString.Buffer)`), so a
        // redundant PORT_NAME after the name is set is a harmless no-op. Bounded
        // to the PING heartbeat with one pair in flight, so no control-queue flood.
        self.enqueue_control(Self::agent_port_name_message());
        self.enqueue_control(Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1));
    }

    /// The PORT_NAME control message for the agent port: an 8-byte control
    /// header followed by the port name bytes (no trailing NUL — vioser's
    /// VIOSerialPortCreateName derives the length from the used-ring length and
    /// appends its own NUL, per virtio 1.2 5.3).
    pub(crate) fn agent_port_name_message() -> PendingControlMessage {
        PendingControlMessage::agent_port_name()
    }

    pub(crate) fn host_open_reassert_pending(&self) -> bool {
        let target =
            PendingControlMessage::from(Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1));
        self.pending_control
            .iter()
            .any(|message| message.as_slice() == target.as_slice())
    }

    pub(crate) fn flush_pending_control(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut progressed = false;
        while let Some(message) = self.pending_control.pop_front() {
            if self.deliver_to_rx_queue(QUEUE_CONTROL_RX, message.as_slice(), mem) {
                progressed = true;
            } else {
                self.pending_control.push_front(message);
                break;
            }
        }
        progressed
    }
}

impl Control {
    pub(crate) const fn new(id: u32, event: u16, value: u16) -> Self {
        Self { id, event, value }
    }

    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CONTROL_LEN {
            return None;
        }
        Some(Self {
            id: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            event: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            value: u16::from_le_bytes(bytes[6..8].try_into().ok()?),
        })
    }

    pub(crate) fn bytes(self) -> [u8; CONTROL_LEN] {
        let mut out = [0u8; CONTROL_LEN];
        out[0..4].copy_from_slice(&self.id.to_le_bytes());
        out[4..6].copy_from_slice(&self.event.to_le_bytes());
        out[6..8].copy_from_slice(&self.value.to_le_bytes());
        out
    }
}

impl PendingControlMessage {
    pub(crate) fn from_slice(bytes: &[u8]) -> Self {
        assert!(bytes.len() <= MAX_CONTROL_MESSAGE_LEN);
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..bytes.len()].copy_from_slice(bytes);
        Self {
            len: bytes.len(),
            bytes: out,
        }
    }

    pub(crate) fn agent_port_name() -> Self {
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..CONTROL_LEN]
            .copy_from_slice(&Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_NAME, 0).bytes());
        out[CONTROL_LEN..MAX_CONTROL_MESSAGE_LEN].copy_from_slice(AGENT_PORT_NAME);
        Self {
            len: MAX_CONTROL_MESSAGE_LEN,
            bytes: out,
        }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

impl From<Control> for PendingControlMessage {
    fn from(control: Control) -> Self {
        Self::from_slice(&control.bytes())
    }
}
