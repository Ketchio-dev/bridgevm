use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::{XHCI_MSIX_PBA_OFFSET, XHCI_MSIX_TABLE_OFFSET};

mod commands;
mod dci3_endpoint_state;
mod dci3_rearm;
mod device_context;
mod device_context_mem;
mod event;
mod interrupt_in;
mod interrupt_trb;
mod interrupts;
mod lifecycle;
mod mmio;
mod ports;
mod registers;
mod reset;
mod setup_input_report;
pub(crate) mod trace;
mod trace_dci3_drain;
mod trace_dci3_input_capture;
mod trace_host_controller_reset;
mod trace_mmio;
mod transfers;
mod usb;

use mmio::{checked_region_offset, mask_to_size, merge_dword};
use ports::{PortState, XHCI_PORT_COUNT};
pub use setup_input_report::{
    SetupInputAction, XhciSetupInputQueueError, XhciSetupInputReportStats,
};

pub const XHCI_CAP_LENGTH: u8 = 0x40;

const USB_CMD_RS: u32 = 1 << 0;
const USB_CMD_HCRST: u32 = 1 << 1;
const USB_STS_HCH: u32 = 1 << 0;

#[derive(Debug, Clone)]
pub struct XhciController {
    msix: MsixTable,
    ports: [PortState; XHCI_PORT_COUNT],
    usb_command: u32,
    dnctrl: u32,
    crcr: u64,
    command_dequeue: u64,
    command_cycle: bool,
    dcbaap: u64,
    config: u32,
    iman0: u32,
    imod0: u32,
    erstsz0: u32,
    erstba0: u64,
    erdp0: u64,
    event_handler_busy: bool,
    event_enqueue: u32,
    event_cycle: bool,
    port_status_change_pending: bool,
    slot1_ep0_dequeue: u64,
    slot1_ep0_dcs: bool,
    slot1_dci3_dequeue: u64,
    slot1_dci3_ring_base: u64,
    slot1_dci3_dcs: bool,
    slot1_dci3_two_entry_queue_rearm: bool,
    slot1_dci3_last_dequeue: u64,
    slot1_dci3_last_dcs: bool,
    slot1_dci3_last_ring_base: u64,
    slot1_dci3_last_ring_dcs: bool,
    slot1_dci3_last_reusable: bool,
    boot_keyboard_report_queue: setup_input_report::BootKeyboardReportQueue,
    setup_input_report_stats: XhciSetupInputReportStats,
    usb_configuration: u8,
}

impl XhciController {
    pub fn mmio_read(&self, offset: u64, size: u8) -> u64 {
        if let Some(table_offset) = checked_region_offset(
            offset,
            u64::from(XHCI_MSIX_TABLE_OFFSET),
            self.msix.table_byte_len(),
        ) {
            return self.msix.table_read(table_offset, size);
        }
        if let Some(pba_offset) = checked_region_offset(
            offset,
            u64::from(XHCI_MSIX_PBA_OFFSET),
            self.msix.pba_byte_len(),
        ) {
            return self.msix.pba_read(pba_offset, size);
        }

        let mut value = 0u64;
        for byte in 0..usize::from(size.min(8)) {
            value |= u64::from(self.register_byte(offset + byte as u64)) << (byte * 8);
        }
        let value = mask_to_size(value, size);
        trace_mmio::mmio_read(offset, size, value);
        value
    }

    pub fn mmio_write(&mut self, offset: u64, size: u8, value: u64) -> bool {
        trace_mmio::mmio_write(offset, size, value);
        if let Some(table_offset) = checked_region_offset(
            offset,
            u64::from(XHCI_MSIX_TABLE_OFFSET),
            self.msix.table_byte_len(),
        ) {
            self.msix.table_write(table_offset, size, value);
            return false;
        }
        if let Some(pba_offset) = checked_region_offset(
            offset,
            u64::from(XHCI_MSIX_PBA_OFFSET),
            self.msix.pba_byte_len(),
        ) {
            self.msix.pba_write(pba_offset, size, value);
            return false;
        }

        let mut consumed = 0u8;
        let mut port_status_change_generated = false;
        while consumed < size.min(8) {
            let current = offset + u64::from(consumed);
            let aligned = current & !0x3;
            let chunk = (4 - (current & 0x3) as u8).min(size.min(8) - consumed);
            let old = self.read_dword(aligned);
            let part = value >> (u32::from(consumed) * 8);
            port_status_change_generated |=
                self.write_dword(aligned, merge_dword(old, current, chunk, part));
            consumed += chunk;
        }
        port_status_change_generated
    }

    pub fn mmio_write_with_mem(
        &mut self,
        offset: u64,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        let event_ring_programming_write = event::is_event_ring_programming_write(offset, size);
        let port_status_change_generated = self.mmio_write(offset, size, value);
        if port_status_change_generated {
            self.mark_port_status_change_pending();
        }
        let mut interrupt = if event_ring_programming_write || port_status_change_generated {
            self.post_pending_port_status_change_event(mem)
        } else {
            false
        };
        if commands::is_command_doorbell(offset, size) {
            interrupt |= self.process_command_doorbell(mem);
            return interrupt;
        }
        if transfers::is_slot_doorbell(offset, size) {
            interrupt |= self.process_slot_doorbell(offset, value, mem);
            return interrupt;
        }
        if self.has_queued_setup_input_report() {
            interrupt |= self.process_dci3_interrupt_in_transfer(mem);
            return interrupt;
        }
        interrupt
    }
}

#[cfg(test)]
mod address_context_bsr_tests;
#[cfg(test)]
mod address_context_tests;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod config_descriptor_tests;
#[cfg(test)]
mod configure_endpoint_setup_input_no_endpoint_tests;
#[cfg(test)]
mod configure_endpoint_setup_input_post_hcrst_tests;
#[cfg(test)]
mod configure_endpoint_setup_input_readdress_tests;
#[cfg(test)]
mod configure_endpoint_setup_input_tests;
#[cfg(test)]
mod configure_endpoint_tests;
#[cfg(test)]
mod disable_slot_tests;
#[cfg(test)]
mod ep0_enumeration_tests;
#[cfg(test)]
mod ep0_overflow_tests;
#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod hid_report_descriptor_tests;
#[cfg(test)]
mod msix_tests;
#[cfg(test)]
mod platform_setup_input_cycle_drain_tests;
#[cfg(test)]
mod platform_setup_input_late_drain_tests;
#[cfg(test)]
mod platform_setup_input_post_fire_kick_tests;
#[cfg(test)]
mod platform_setup_input_support;
#[cfg(test)]
mod platform_setup_input_tests;
#[cfg(test)]
mod platform_test_support;
#[cfg(test)]
mod platform_tests;
#[cfg(test)]
mod port_link_state_tests;
#[cfg(test)]
mod port_reset_change_tests;
#[cfg(test)]
mod set_configuration_msix_tests;
#[cfg(test)]
mod set_configuration_tests;
#[cfg(test)]
mod set_protocol_tests;
#[cfg(test)]
mod stop_endpoint_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod trace_tests;
#[cfg(test)]
mod transfer_prefix_tests;
#[cfg(test)]
mod transfer_tests;
