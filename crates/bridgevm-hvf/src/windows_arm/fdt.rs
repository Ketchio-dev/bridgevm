//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FdtRegRange {
    pub(crate) base_ipa: u64,
    pub(crate) bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FdtInterruptSpec {
    pub(crate) interrupt_type: u32,
    pub(crate) interrupt_number: u32,
    pub(crate) trigger: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsArmPlatformFdtSummary {
    pub(crate) fdt_magic: u32,
    pub(crate) memory_node_base_ipa: Option<u64>,
    pub(crate) cpu_count: u8,
    pub(crate) root_interrupt_parent: Option<u32>,
    pub(crate) gic_phandle: Option<u32>,
    pub(crate) gic_interrupt_controller: bool,
    pub(crate) gic_distributor: Option<FdtRegRange>,
    pub(crate) gic_redistributor: Option<FdtRegRange>,
    pub(crate) arch_timer_interrupts: Vec<FdtInterruptSpec>,
    pub(crate) pl011: Option<FdtRegRange>,
    pub(crate) pl011_interrupt: Option<FdtInterruptSpec>,
    pub(crate) pl031: Option<FdtRegRange>,
    pub(crate) pl031_interrupt: Option<FdtInterruptSpec>,
    pub(crate) virtio_installer_iso: Option<FdtRegRange>,
    pub(crate) virtio_installer_iso_interrupt: Option<FdtInterruptSpec>,
    pub(crate) virtio_target_disk: Option<FdtRegRange>,
    pub(crate) virtio_target_disk_interrupt: Option<FdtInterruptSpec>,
    pub(crate) blockers: Vec<String>,
}

#[derive(Default)]
pub(crate) struct FdtBlobBuilder {
    pub(crate) structure: Vec<u8>,
    pub(crate) strings: Vec<u8>,
}

impl FdtBlobBuilder {
    pub(crate) fn begin_node(&mut self, name: &str) {
        push_be_u32(&mut self.structure, FDT_BEGIN_NODE);
        self.structure.extend_from_slice(name.as_bytes());
        self.structure.push(0);
        pad_to_4(&mut self.structure);
    }

    pub(crate) fn end_node(&mut self) {
        push_be_u32(&mut self.structure, FDT_END_NODE);
    }

    pub(crate) fn prop_raw(&mut self, name: &str, data: &[u8]) {
        let name_offset = self.add_string(name);
        push_be_u32(&mut self.structure, FDT_PROP);
        push_be_u32(&mut self.structure, data.len() as u32);
        push_be_u32(&mut self.structure, name_offset);
        self.structure.extend_from_slice(data);
        pad_to_4(&mut self.structure);
    }

    pub(crate) fn prop_u32(&mut self, name: &str, value: u32) {
        self.prop_raw(name, &value.to_be_bytes());
    }

    pub(crate) fn prop_empty(&mut self, name: &str) {
        self.prop_raw(name, &[]);
    }

    pub(crate) fn prop_u32_list(&mut self, name: &str, values: &[u32]) {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        self.prop_raw(name, &data);
    }

    pub(crate) fn prop_string(&mut self, name: &str, value: &str) {
        let mut data = Vec::with_capacity(value.len() + 1);
        data.extend_from_slice(value.as_bytes());
        data.push(0);
        self.prop_raw(name, &data);
    }

    pub(crate) fn prop_string_list(&mut self, name: &str, values: &[&str]) {
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(value.as_bytes());
            data.push(0);
        }
        self.prop_raw(name, &data);
    }

    pub(crate) fn prop_reg64(&mut self, base_ipa: u64, bytes: u64) {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&base_ipa.to_be_bytes());
        data.extend_from_slice(&bytes.to_be_bytes());
        self.prop_raw("reg", &data);
    }

    pub(crate) fn prop_reg64_pairs(&mut self, ranges: &[(u64, u64)]) {
        let mut data = Vec::with_capacity(ranges.len() * 16);
        for (base_ipa, bytes) in ranges {
            data.extend_from_slice(&base_ipa.to_be_bytes());
            data.extend_from_slice(&bytes.to_be_bytes());
        }
        self.prop_raw("reg", &data);
    }

    pub(crate) fn prop_gic_interrupt(
        &mut self,
        interrupt_type: u32,
        interrupt_number: u32,
        trigger: u32,
    ) {
        self.prop_u32_list("interrupts", &[interrupt_type, interrupt_number, trigger]);
    }

    pub(crate) fn add_string(&mut self, name: &str) -> u32 {
        let offset = self.strings.len() as u32;
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        offset
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        push_be_u32(&mut self.structure, FDT_END);
        pad_to_4(&mut self.structure);

        let header_bytes = 40_u32;
        let mem_rsvmap_bytes = 16_u32;
        let off_mem_rsvmap = header_bytes;
        let off_dt_struct = off_mem_rsvmap + mem_rsvmap_bytes;
        let off_dt_strings = off_dt_struct + self.structure.len() as u32;
        let totalsize = off_dt_strings + self.strings.len() as u32;

        let mut blob = Vec::with_capacity(totalsize as usize);
        push_be_u32(&mut blob, FDT_MAGIC);
        push_be_u32(&mut blob, totalsize);
        push_be_u32(&mut blob, off_dt_struct);
        push_be_u32(&mut blob, off_dt_strings);
        push_be_u32(&mut blob, off_mem_rsvmap);
        push_be_u32(&mut blob, 17);
        push_be_u32(&mut blob, 16);
        push_be_u32(&mut blob, 0);
        push_be_u32(&mut blob, self.strings.len() as u32);
        push_be_u32(&mut blob, self.structure.len() as u32);
        push_be_u64(&mut blob, 0);
        push_be_u64(&mut blob, 0);
        blob.extend_from_slice(&self.structure);
        blob.extend_from_slice(&self.strings);
        blob
    }
}

pub(crate) fn build_windows_arm_platform_fdt_blob(
    options: &WindowsArmPlatformDescriptionOptions,
) -> Vec<u8> {
    let mut builder = FdtBlobBuilder::default();

    builder.begin_node("");
    builder.prop_string("compatible", "bridgevm,windows-arm-hvf");
    builder.prop_string("model", "BridgeVM Windows 11 Arm HVF");
    builder.prop_u32("#address-cells", 2);
    builder.prop_u32("#size-cells", 2);
    builder.prop_u32("interrupt-parent", WINDOWS_ARM_GIC_PHANDLE);

    builder.begin_node("chosen");
    builder.end_node();

    builder.begin_node(&format!("memory@{:x}", WINDOWS_ARM_GUEST_RAM_IPA));
    builder.prop_string("device_type", "memory");
    builder.prop_reg64(WINDOWS_ARM_GUEST_RAM_IPA, options.guest_ram_bytes);
    builder.end_node();

    builder.begin_node("cpus");
    builder.prop_u32("#address-cells", 1);
    builder.prop_u32("#size-cells", 0);
    for cpu_index in 0..options.vcpu_count {
        builder.begin_node(&format!("cpu@{cpu_index:x}"));
        builder.prop_string("device_type", "cpu");
        builder.prop_string("compatible", "arm,arm-v8");
        builder.prop_u32("reg", u32::from(cpu_index));
        builder.end_node();
    }
    builder.end_node();

    builder.begin_node("timer");
    builder.prop_string("compatible", "arm,armv8-timer");
    builder.prop_u32_list(
        "interrupts",
        &[
            GIC_PPI,
            13,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            14,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            11,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            10,
            IRQ_TYPE_LEVEL_HIGH,
        ],
    );
    builder.prop_empty("always-on");
    builder.end_node();

    builder.begin_node("intc@10010000");
    builder.prop_string("compatible", "arm,gic-v3");
    builder.prop_empty("interrupt-controller");
    builder.prop_u32("#interrupt-cells", 3);
    builder.prop_u32("#address-cells", 2);
    builder.prop_u32("#size-cells", 2);
    builder.prop_u32("phandle", WINDOWS_ARM_GIC_PHANDLE);
    builder.prop_reg64_pairs(&[
        (
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA,
            WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES,
        ),
        (
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA,
            windows_arm_gic_redistributor_fdt_bytes(options.vcpu_count),
        ),
    ]);
    builder.end_node();

    builder.begin_node("serial@10000000");
    builder.prop_string_list("compatible", &["arm,pl011", "arm,primecell"]);
    builder.prop_reg64(WINDOWS_ARM_PL011_MMIO_IPA, PL011_REGISTER_WINDOW_BYTES);
    builder.prop_gic_interrupt(GIC_SPI, WINDOWS_ARM_PL011_SPI, IRQ_TYPE_LEVEL_HIGH);
    builder.end_node();

    builder.begin_node("rtc@10001000");
    builder.prop_string_list("compatible", &["arm,pl031", "arm,primecell"]);
    builder.prop_reg64(WINDOWS_ARM_PL031_MMIO_IPA, PL031_REGISTER_WINDOW_BYTES);
    builder.prop_gic_interrupt(GIC_SPI, WINDOWS_ARM_PL031_SPI, IRQ_TYPE_LEVEL_HIGH);
    builder.end_node();

    builder.begin_node("virtio_mmio@10002000");
    builder.prop_string("compatible", "virtio,mmio");
    builder.prop_reg64(
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA,
        VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
    );
    builder.prop_gic_interrupt(
        GIC_SPI,
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI,
        IRQ_TYPE_LEVEL_HIGH,
    );
    builder.end_node();

    builder.begin_node("virtio_mmio@10003000");
    builder.prop_string("compatible", "virtio,mmio");
    builder.prop_reg64(
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA,
        VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
    );
    builder.prop_gic_interrupt(
        GIC_SPI,
        WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI,
        IRQ_TYPE_LEVEL_HIGH,
    );
    builder.end_node();

    builder.end_node();
    builder.finish()
}

pub(crate) fn build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes: u64) -> Vec<u8> {
    build_windows_arm_platform_fdt_blob(&WindowsArmPlatformDescriptionOptions {
        guest_ram_bytes,
        vcpu_count: WINDOWS_ARM_FIRMWARE_RUN_LOOP_FDT_VCPU_COUNT,
    })
}

pub(crate) fn windows_arm_firmware_run_loop_dtb_metadata(
    guest_ram_bytes: u64,
) -> (usize, u32, bool) {
    let blob = build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes);
    let magic = read_be_u32(&blob, 0).unwrap_or(0);
    (blob.len(), magic, magic == FDT_MAGIC)
}

pub(crate) fn inspect_windows_arm_platform_fdt_blob(blob: &[u8]) -> WindowsArmPlatformFdtSummary {
    let mut blockers = Vec::new();
    let fdt_magic = read_be_u32(blob, 0).unwrap_or(0);
    let totalsize = read_be_u32(blob, 4).unwrap_or(0) as usize;
    let off_dt_struct = read_be_u32(blob, 8).unwrap_or(0) as usize;
    let off_dt_strings = read_be_u32(blob, 12).unwrap_or(0) as usize;
    let size_dt_strings = read_be_u32(blob, 32).unwrap_or(0) as usize;
    let size_dt_struct = read_be_u32(blob, 36).unwrap_or(0) as usize;
    let mut summary = WindowsArmPlatformFdtSummary {
        fdt_magic,
        memory_node_base_ipa: None,
        cpu_count: 0,
        root_interrupt_parent: None,
        gic_phandle: None,
        gic_interrupt_controller: false,
        gic_distributor: None,
        gic_redistributor: None,
        arch_timer_interrupts: Vec::new(),
        pl011: None,
        pl011_interrupt: None,
        pl031: None,
        pl031_interrupt: None,
        virtio_installer_iso: None,
        virtio_installer_iso_interrupt: None,
        virtio_target_disk: None,
        virtio_target_disk_interrupt: None,
        blockers: Vec::new(),
    };

    if blob.len() < 40 {
        summary
            .blockers
            .push("FDT blob is shorter than the header".to_string());
        return summary;
    }
    if totalsize > blob.len() {
        blockers.push("FDT totalsize exceeds blob length".to_string());
    }
    let Some(struct_end) = off_dt_struct.checked_add(size_dt_struct) else {
        blockers.push("FDT structure block range overflowed".to_string());
        summary.blockers = blockers;
        return summary;
    };
    let Some(strings_end) = off_dt_strings.checked_add(size_dt_strings) else {
        blockers.push("FDT strings block range overflowed".to_string());
        summary.blockers = blockers;
        return summary;
    };
    if struct_end > blob.len() || strings_end > blob.len() {
        blockers.push("FDT block offsets exceed blob length".to_string());
        summary.blockers = blockers;
        return summary;
    }

    let structure = &blob[off_dt_struct..struct_end];
    let strings = &blob[off_dt_strings..strings_end];
    let mut offset = 0_usize;
    let mut path: Vec<String> = Vec::new();

    while offset + 4 <= structure.len() {
        let Some(token) = read_be_u32(structure, offset) else {
            blockers.push("FDT structure token read failed".to_string());
            break;
        };
        offset += 4;

        match token {
            FDT_BEGIN_NODE => {
                let Some((name, next_offset)) = read_fdt_node_name(structure, offset) else {
                    blockers.push("FDT node name read failed".to_string());
                    break;
                };
                offset = next_offset;
                if !name.is_empty() {
                    if path.len() == 1 && path[0] == "cpus" && name.starts_with("cpu@") {
                        summary.cpu_count = summary.cpu_count.saturating_add(1);
                    }
                    path.push(name);
                }
            }
            FDT_END_NODE => {
                let _ = path.pop();
            }
            FDT_PROP => {
                if offset + 8 > structure.len() {
                    blockers.push("FDT property header is truncated".to_string());
                    break;
                }
                let len = read_be_u32(structure, offset).unwrap_or(0) as usize;
                let name_offset = read_be_u32(structure, offset + 4).unwrap_or(0) as usize;
                offset += 8;
                let Some(data_end) = offset.checked_add(len) else {
                    blockers.push("FDT property data range overflowed".to_string());
                    break;
                };
                if data_end > structure.len() {
                    blockers.push("FDT property data is truncated".to_string());
                    break;
                }
                let data = &structure[offset..data_end];
                offset = align_up_to_4(data_end);
                let Some(name) = read_fdt_string(strings, name_offset) else {
                    blockers.push("FDT property name offset is invalid".to_string());
                    continue;
                };
                match name {
                    "reg" => record_windows_arm_fdt_reg(&path, data, &mut summary),
                    "interrupt-parent" if path.is_empty() => {
                        summary.root_interrupt_parent = read_fdt_u32(data);
                    }
                    "phandle" if path.last().is_some_and(|node| node == "intc@10010000") => {
                        summary.gic_phandle = read_fdt_u32(data);
                    }
                    "interrupt-controller"
                        if path.last().is_some_and(|node| node == "intc@10010000") =>
                    {
                        summary.gic_interrupt_controller = true;
                    }
                    "interrupts" => {
                        record_windows_arm_fdt_interrupts(&path, data, &mut summary);
                    }
                    _ => {}
                }
            }
            FDT_END => break,
            _ => {
                blockers.push(format!("unsupported FDT structure token {token:#x}"));
                break;
            }
        }
    }

    if summary.memory_node_base_ipa.is_none() {
        blockers.push("FDT memory node reg was not found".to_string());
    }
    if summary.pl011.is_none() {
        blockers.push("FDT PL011 node reg was not found".to_string());
    }
    if summary.pl031.is_none() {
        blockers.push("FDT PL031 node reg was not found".to_string());
    }
    if summary.virtio_installer_iso.is_none() {
        blockers.push("FDT VirtIO-MMIO installer ISO node reg was not found".to_string());
    }
    if summary.virtio_target_disk.is_none() {
        blockers.push("FDT VirtIO-MMIO target disk node reg was not found".to_string());
    }
    summary.blockers = blockers;
    summary
}

pub(crate) fn record_windows_arm_fdt_reg(
    path: &[String],
    data: &[u8],
    summary: &mut WindowsArmPlatformFdtSummary,
) {
    let Some(node) = path.last().map(String::as_str) else {
        return;
    };
    if path.len() == 2 && path[0] == "cpus" && node.starts_with("cpu@") {
        return;
    }
    if node == "intc@10010000" {
        let ranges = read_fdt_reg64_pairs(data);
        summary.gic_distributor = ranges.first().copied();
        summary.gic_redistributor = ranges.get(1).copied();
        return;
    }
    let Some(range) = read_fdt_reg64(data) else {
        return;
    };

    match node {
        name if name.starts_with("memory@") => {
            summary.memory_node_base_ipa = Some(range.base_ipa);
        }
        "serial@10000000" => summary.pl011 = Some(range),
        "rtc@10001000" => summary.pl031 = Some(range),
        "virtio_mmio@10002000" => summary.virtio_installer_iso = Some(range),
        "virtio_mmio@10003000" => summary.virtio_target_disk = Some(range),
        _ => {}
    }
}

pub(crate) fn record_windows_arm_fdt_interrupts(
    path: &[String],
    data: &[u8],
    summary: &mut WindowsArmPlatformFdtSummary,
) {
    let Some(node) = path.last().map(String::as_str) else {
        return;
    };
    let interrupts = read_fdt_interrupts(data);
    match node {
        "timer" => summary.arch_timer_interrupts = interrupts,
        "serial@10000000" => summary.pl011_interrupt = interrupts.first().copied(),
        "rtc@10001000" => summary.pl031_interrupt = interrupts.first().copied(),
        "virtio_mmio@10002000" => {
            summary.virtio_installer_iso_interrupt = interrupts.first().copied();
        }
        "virtio_mmio@10003000" => {
            summary.virtio_target_disk_interrupt = interrupts.first().copied();
        }
        _ => {}
    }
}

pub(crate) fn fdt_range_inside_device_window(range: FdtRegRange) -> bool {
    let Some(end) = range.base_ipa.checked_add(range.bytes) else {
        return false;
    };
    range.base_ipa >= WINDOWS_ARM_DEVICE_MMIO_IPA
        && end <= WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES)
}

pub(crate) fn read_fdt_reg64(data: &[u8]) -> Option<FdtRegRange> {
    Some(FdtRegRange {
        base_ipa: read_be_u64(data, 0)?,
        bytes: read_be_u64(data, 8)?,
    })
}

pub(crate) fn read_fdt_reg64_pairs(data: &[u8]) -> Vec<FdtRegRange> {
    let mut ranges = Vec::new();
    for chunk in data.chunks_exact(16) {
        if let Some(range) = read_fdt_reg64(chunk) {
            ranges.push(range);
        }
    }
    ranges
}

pub(crate) fn read_fdt_u32(data: &[u8]) -> Option<u32> {
    read_be_u32(data, 0)
}

pub(crate) fn read_fdt_interrupts(data: &[u8]) -> Vec<FdtInterruptSpec> {
    let mut interrupts = Vec::new();
    for chunk in data.chunks_exact(12) {
        if let (Some(interrupt_type), Some(interrupt_number), Some(trigger)) = (
            read_be_u32(chunk, 0),
            read_be_u32(chunk, 4),
            read_be_u32(chunk, 8),
        ) {
            interrupts.push(FdtInterruptSpec {
                interrupt_type,
                interrupt_number,
                trigger,
            });
        }
    }
    interrupts
}

pub(crate) fn read_fdt_node_name(data: &[u8], offset: usize) -> Option<(String, usize)> {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    let name = std::str::from_utf8(&data[offset..end]).ok()?.to_string();
    Some((name, align_up_to_4(end + 1)))
}

pub(crate) fn read_fdt_string(data: &[u8], offset: usize) -> Option<&str> {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    std::str::from_utf8(&data[offset..end]).ok()
}

pub(crate) fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub(crate) fn read_be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

pub(crate) fn push_be_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

pub(crate) fn push_be_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

pub(crate) fn pad_to_4(output: &mut Vec<u8>) {
    while output.len() % 4 != 0 {
        output.push(0);
    }
}

pub(crate) fn align_up_to_4(value: usize) -> usize {
    (value + 3) & !3
}
