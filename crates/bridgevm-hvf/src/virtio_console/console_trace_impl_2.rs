//! Continuation of the `part` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;

impl VirtioConsole {
    pub(crate) fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if (8..12).contains(&offset) && size <= 4 {
            let current = self.emerg_wr;
            let merged = insert_u32(current, offset - 8, size, value);
            self.emerg_wr = merged;
        }
    }

    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        if let Some(queue) = self.queues.get_mut(usize::from(queue_index)) {
            queue.notify_count = queue.notify_count.saturating_add(1);
        }
        if console_trace_enabled() {
            let avail = self
                .queues
                .get(usize::from(queue_index))
                .and_then(|queue| read_u16(mem, queue.driver + 2));
            let last = self
                .queues
                .get(usize::from(queue_index))
                .map(|queue| queue.last_avail_idx);
            eprintln!("[vcon] notify q{queue_index} avail_idx={avail:?} last_consumed={last:?}");
        }
        match usize::from(queue_index) {
            QUEUE_CONTROL_RX => {
                // Only drain what is genuinely queued. We must NOT synthesize a
                // PORT_OPEN re-assert here: delivering one makes vioser consume
                // and refill the control-RX ring, which kicks this very queue,
                // which would re-assert again -> a self-sustaining MSI-X storm
                // that livelocks the guest. Re-assertion is driven only by the
                // bounded triggers (agent_send heartbeat, PORT_READY epoch).
                self.flush_pending_control(mem);
            }
            QUEUE_CONTROL_TX => {
                self.process_control_tx_queue(mem);
                self.flush_pending_control(mem);
            }
            QUEUE_AGENT_RX => {
                self.deliver_agent_rx(mem);
            }
            QUEUE_AGENT_TX => {
                self.process_agent_tx_queue(mem);
            }
            _ => {}
        }
    }

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

    pub(crate) fn deliver_agent_rx(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        // Deliver host->guest bytes whenever the guest has posted receive
        // buffers on the agent RX queue. We intentionally do NOT gate on
        // ports[1].guest_open: the real vioser driver does not reliably emit a
        // guest PORT_OPEN we can observe (its VIOSerialPortCreate can
        // short-circuit), yet it still posts RX buffers once the app opens the
        // port. Gating on an unobservable guest_open deadlocked the channel.
        if self.host_to_guest.is_empty() {
            return false;
        }
        let (front, back) = self.host_to_guest.as_slices();
        let Some(written) = Self::deliver_partial_slices_to_rx_queue(
            RxQueueDeliveryState {
                queues: &mut self.queues,
                pending_msix_queue_bits: &mut self.pending_msix_queue_bits,
                interrupt_status: &mut self.interrupt_status,
                descriptor_scratch: &mut self.descriptor_scratch,
            },
            QUEUE_AGENT_RX,
            front,
            back,
            mem,
        ) else {
            return false;
        };
        self.host_to_guest.drain(..written);
        true
    }

    pub(crate) fn process_agent_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_AGENT_TX;
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
                MAX_AGENT_MESSAGE_LEN,
            ) {
                // Any guest TX proves vioser latched HostConnected (its
                // WillWriteBlock gate blocks writes until then), so we can stop
                // re-asserting PORT_OPEN from here on.
                if !bytes.is_empty() {
                    self.agent_connected_confirmed = true;
                }
                console_trace!("agent-tx<-guest len={}", bytes.len());
                self.host_inbound.extend_from_slice(&bytes);
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

    pub(crate) fn deliver_to_rx_queue(
        &mut self,
        queue_index: usize,
        bytes: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.deliver_partial_to_rx_queue(queue_index, bytes, mem)
            .is_some_and(|written| written == bytes.len())
    }

    pub(crate) fn deliver_partial_to_rx_queue(
        &mut self,
        queue_index: usize,
        bytes: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<usize> {
        Self::deliver_partial_slices_to_rx_queue(
            RxQueueDeliveryState {
                queues: &mut self.queues,
                pending_msix_queue_bits: &mut self.pending_msix_queue_bits,
                interrupt_status: &mut self.interrupt_status,
                descriptor_scratch: &mut self.descriptor_scratch,
            },
            queue_index,
            bytes,
            &[],
            mem,
        )
    }

    pub(crate) fn deliver_partial_slices_to_rx_queue(
        state: RxQueueDeliveryState<'_>,
        queue_index: usize,
        first: &[u8],
        second: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        let queue = state.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 || bytes_len == 0 {
            return None;
        }
        let avail_idx = read_u16(mem, queue.driver + 2)?;
        state.queues[queue_index].last_avail_seen = avail_idx;
        let last_avail_idx = state.queues[queue_index].last_avail_idx;
        if last_avail_idx == avail_idx {
            // The guest has not published a fresh avail buffer since we last
            // consumed. If this keeps firing while notify_count / last_avail_seen
            // stay flat, the guest stopped replenishing (not our consume path).
            state.queues[queue_index].rx_no_buffers =
                state.queues[queue_index].rx_no_buffers.saturating_add(1);
            console_trace!(
                "rx q{queue_index} NO-BUFFERS last_consumed={last_avail_idx} avail_idx={avail_idx} bytes={}",
                bytes_len
            );
            return None;
        }
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let head = read_u16(mem, queue.driver + ring_off)?;
        let mut descs = std::mem::take(state.descriptor_scratch);
        if !Self::descriptor_chain_into(mem, &queue, head, &mut descs) {
            descs.clear();
            *state.descriptor_scratch = descs;
            // avail advanced but we could not walk the chain (head >= size, or a
            // bad next link). This is the "replenished buffers are invisible to
            // us" signature -> our consume path or a size mismatch.
            console_trace!(
                "rx q{queue_index} CHAIN-FAIL head={head} size={} last_consumed={last_avail_idx} avail_idx={avail_idx}",
                queue.size
            );
            return None;
        }
        let Some(written) = Self::scatter_write_partial_slices(mem, &descs, first, second) else {
            console_trace!(
                "rx q{queue_index} SCATTER-FAIL head={head} descs={}",
                descs.len()
            );
            descs.clear();
            *state.descriptor_scratch = descs;
            return None;
        };
        descs.clear();
        *state.descriptor_scratch = descs;
        Self::write_used(
            mem,
            &queue,
            head,
            u32::try_from(written).unwrap_or(u32::MAX),
        );
        state.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        state.queues[queue_index].used_produced =
            state.queues[queue_index].used_produced.saturating_add(1);
        state.queues[queue_index].pending_msix = true;
        if let Some(bit) = queue_bit(queue_index) {
            *state.pending_msix_queue_bits |= bit;
        }
        *state.interrupt_status |= 1;
        console_trace!(
            "rx q{queue_index} DELIVER head={head} len={written} last_consumed->{} avail_idx={avail_idx}",
            last_avail_idx.wrapping_add(1)
        );
        Some(written)
    }

    pub(crate) fn read_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        out: &mut Vec<u8>,
        max_len: usize,
    ) -> bool {
        out.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        for desc in descs.iter() {
            if desc.flags & DESC_F_WRITE != 0 {
                return false;
            }
            let start = out.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                return false;
            };
            if end > max_len {
                return false;
            }
            // `read_bytes` validates the guest range before allocating in the
            // live RAM implementation. Only append after that validation so an
            // unbacked, oversized descriptor cannot resize reusable scratch.
            let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) else {
                return false;
            };
            out.extend_from_slice(&bytes);
        }
        true
    }

    pub(crate) fn scatter_write_partial_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        first: &[u8],
        second: &[u8],
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        let mut offset = 0usize;
        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                return None;
            }
            let mut desc_addr = desc.addr;
            let mut desc_remaining = desc.len as usize;
            while desc_remaining > 0 && offset < bytes_len {
                let chunk = Self::slice_pair_chunk(first, second, offset)?;
                let writable = desc_remaining.min(chunk.len());
                if writable == 0 {
                    break;
                }
                if !mem.write_bytes(desc_addr, &chunk[..writable]) {
                    return None;
                }
                offset += writable;
                desc_addr = desc_addr.checked_add(writable as u64)?;
                desc_remaining -= writable;
            }
            if offset == bytes_len {
                break;
            }
        }
        (offset > 0).then_some(offset)
    }

    pub(crate) fn slice_pair_chunk<'a>(
        first: &'a [u8],
        second: &'a [u8],
        offset: usize,
    ) -> Option<&'a [u8]> {
        if offset < first.len() {
            return Some(&first[offset..]);
        }
        let second_offset = offset.checked_sub(first.len())?;
        (second_offset < second.len()).then_some(&second[second_offset..])
    }

    pub(crate) fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
    }

    pub(crate) fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        if head >= queue.size {
            return false;
        }
        let mut index = head;
        for _ in 0..queue.size {
            let Some(desc) = Descriptor::read(mem, queue.desc + u64::from(index) * DESC_SIZE)
            else {
                out.clear();
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue.size {
                out.clear();
                return false;
            }
        }
        out.clear();
        false
    }

    pub(crate) fn write_used(
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        id: u16,
        len: u32,
    ) {
        if queue.size == 0 || queue.device == 0 {
            return;
        }
        let Some(used_idx) = read_u16(mem, queue.device + 2) else {
            return;
        };
        let elem = queue.device + 4 + u64::from(used_idx % queue.size) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(queue.device + 2, &used_idx.wrapping_add(1).to_le_bytes());
    }
}
