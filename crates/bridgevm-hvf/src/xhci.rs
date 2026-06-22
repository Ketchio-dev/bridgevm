use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::{XHCI_MSIX_PBA_OFFSET, XHCI_MSIX_TABLE_OFFSET, XHCI_MSIX_VECTOR_COUNT};

mod commands;
mod device_context;
mod event;
mod interrupts;
mod mmio;
mod ports;
mod trace;
mod transfers;
mod usb;

use mmio::{checked_region_offset, mask_to_size, merge_dword};
use ports::{initial_ports, port_reg, PortState, XHCI_PORT_COUNT};

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
    slot1_ep0_dequeue: u64,
}

impl Default for XhciController {
    fn default() -> Self {
        Self::new()
    }
}

impl XhciController {
    pub fn new() -> Self {
        Self {
            msix: MsixTable::new(XHCI_MSIX_VECTOR_COUNT),
            ports: initial_ports(),
            usb_command: 0,
            dnctrl: 0,
            crcr: 0,
            command_dequeue: 0,
            command_cycle: false,
            dcbaap: 0,
            config: 0,
            iman0: 0,
            imod0: 0,
            erstsz0: 0,
            erstba0: 0,
            erdp0: 0,
            event_handler_busy: false,
            event_enqueue: 0,
            event_cycle: true,
            slot1_ep0_dequeue: 0,
        }
    }

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
        mask_to_size(value, size)
    }

    pub fn mmio_write(&mut self, offset: u64, size: u8, value: u64) {
        if let Some(table_offset) = checked_region_offset(
            offset,
            u64::from(XHCI_MSIX_TABLE_OFFSET),
            self.msix.table_byte_len(),
        ) {
            self.msix.table_write(table_offset, size, value);
            return;
        }
        if let Some(pba_offset) = checked_region_offset(
            offset,
            u64::from(XHCI_MSIX_PBA_OFFSET),
            self.msix.pba_byte_len(),
        ) {
            self.msix.pba_write(pba_offset, size, value);
            return;
        }

        let mut consumed = 0u8;
        while consumed < size.min(8) {
            let current = offset + u64::from(consumed);
            let aligned = current & !0x3;
            let chunk = (4 - (current & 0x3) as u8).min(size.min(8) - consumed);
            let old = self.read_dword(aligned);
            let part = value >> (u32::from(consumed) * 8);
            self.write_dword(aligned, merge_dword(old, current, chunk, part));
            consumed += chunk;
        }
    }

    pub fn mmio_write_with_mem(
        &mut self,
        offset: u64,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.mmio_write(offset, size, value);
        if commands::is_command_doorbell(offset, size) {
            return self.process_command_doorbell(mem);
        }
        if transfers::is_slot_doorbell(offset, size) {
            return self.process_slot_doorbell(offset, value, mem);
        }
        false
    }

    fn register_byte(&self, offset: u64) -> u8 {
        let shift = ((offset & 0x3) * 8) as u32;
        ((self.read_dword(offset & !0x3) >> shift) & 0xff) as u8
    }

    fn read_dword(&self, offset: u64) -> u32 {
        if let Some((port, reg)) = port_reg(offset) {
            return if reg == 0x0 {
                self.ports[port].portsc()
            } else {
                0
            };
        }
        match offset {
            0x00 => 0x0100_0040,
            0x04 => 0x0800_1040,
            0x08 => 0x0000_000f,
            0x0c => 0x0000_0000,
            0x10 => 0x0008_7001,
            0x14 => 0x0000_2000,
            0x18 => 0x0000_1000,
            0x1c => 0x0000_0000,
            0x20 => 0x0200_0402,
            0x24 => 0x2042_5355,
            0x28 => 0x0000_0405,
            0x2c => 0x0000_0000,
            0x30 => 0x0300_0002,
            0x34 => 0x2042_5355,
            0x38 => 0x0000_0401,
            0x3c => 0x0000_0000,
            0x40 => self.usb_command,
            0x44 => self.usb_status(),
            0x48 => 0x0000_0001,
            0x54 => self.dnctrl,
            0x58 => self.crcr as u32,
            0x5c => (self.crcr >> 32) as u32,
            0x70 => self.dcbaap as u32,
            0x74 => (self.dcbaap >> 32) as u32,
            0x78 => self.config,
            0x1000 => 0x0000_0003,
            0x1020 => self.iman0,
            0x1024 => self.imod0,
            0x1028 => self.erstsz0,
            0x1030 => self.erstba0 as u32,
            0x1034 => (self.erstba0 >> 32) as u32,
            0x1038 => self.erdp_low(),
            0x103c => (self.erdp0 >> 32) as u32,
            _ => 0,
        }
    }

    fn write_dword(&mut self, offset: u64, value: u32) {
        if let Some((port, reg)) = port_reg(offset) {
            if reg == 0x0 {
                self.ports[port].write_portsc(value);
            }
            return;
        }
        match offset {
            0x40 => {
                if value & USB_CMD_HCRST != 0 {
                    self.ports = initial_ports();
                    self.reset_programmed_state();
                }
                self.usb_command = value & !USB_CMD_HCRST;
            }
            0x54 => self.dnctrl = value,
            0x58 => self.write_crcr_low(value),
            0x5c => self.write_crcr_high(value),
            0x70 => self.dcbaap = (self.dcbaap & !0xffff_ffff) | u64::from(value),
            0x74 => self.dcbaap = (self.dcbaap & 0xffff_ffff) | (u64::from(value) << 32),
            0x78 => self.config = value,
            0x1020 => self.write_iman0(value),
            0x1024 => self.imod0 = value,
            0x1028 => {
                self.erstsz0 = value;
                self.reset_event_ring();
            }
            0x1030 => {
                self.erstba0 = (self.erstba0 & !0xffff_ffff) | u64::from(value);
                self.reset_event_ring();
            }
            0x1034 => {
                self.erstba0 = (self.erstba0 & 0xffff_ffff) | (u64::from(value) << 32);
                self.reset_event_ring();
            }
            0x1038 => self.write_erdp_low(value),
            0x103c => self.write_erdp_high(value),
            _ => {}
        }
    }

    fn reset_programmed_state(&mut self) {
        self.crcr = 0;
        self.command_dequeue = 0;
        self.command_cycle = false;
        self.dcbaap = 0;
        self.config = 0;
        self.iman0 = 0;
        self.imod0 = 0;
        self.erstsz0 = 0;
        self.erstba0 = 0;
        self.erdp0 = 0;
        self.slot1_ep0_dequeue = 0;
        self.reset_event_ring();
    }
}

#[cfg(test)]
mod address_context_tests;

#[cfg(test)]
mod command_tests;

#[cfg(test)]
mod config_descriptor_tests;

#[cfg(test)]
mod event_tests;

#[cfg(test)]
mod msix_tests;

#[cfg(test)]
mod platform_tests;

#[cfg(test)]
mod platform_test_support;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod transfer_prefix_tests;

#[cfg(test)]
mod transfer_tests;
