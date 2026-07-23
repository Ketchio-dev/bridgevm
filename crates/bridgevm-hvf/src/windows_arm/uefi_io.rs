//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

pub(crate) fn verify_uefi_firmware_file(
    path: &PathBuf,
    slot_bytes: u64,
) -> Result<UefiFirmwareFileVerification, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let bytes = file.metadata().map_err(|error| error.to_string())?.len();
    if bytes == 0 {
        return Err("file is empty".to_string());
    }
    if bytes > slot_bytes {
        return Err(format!(
            "file is larger than the planned pflash slot ({bytes:#x} > {slot_bytes:#x})"
        ));
    }
    let len: usize = bytes
        .try_into()
        .map_err(|_| "file is too large to inspect on this host".to_string())?;
    let mut contents = vec![0_u8; len];
    file.read_exact(&mut contents)
        .map_err(|error| error.to_string())?;
    let volume = detect_uefi_firmware_volume(&contents)?;
    Ok(UefiFirmwareFileVerification { bytes, volume })
}

pub(crate) fn load_uefi_pflash_slot(
    name: &'static str,
    path: &PathBuf,
    ipa_start: u64,
    slot_bytes: u64,
    writable: bool,
) -> Result<WindowsArmUefiPflashSlotMap, String> {
    let slot_len: usize = slot_bytes
        .try_into()
        .map_err(|_| "pflash slot is too large to allocate on this host".to_string())?;
    let source = media::read_bounded_file(path, slot_len).map_err(|error| error.to_string())?;
    if source.is_empty() {
        return Err("file is empty".to_string());
    }
    let source_bytes =
        u64::try_from(source.len()).map_err(|_| "file is too large to map".to_string())?;
    let mut slot = vec![0_u8; slot_len];
    slot[..source.len()].copy_from_slice(&source);
    let prefix_verified = slot[..source.len()] == source[..];
    let padding_zeroed = slot[source.len()..].iter().all(|byte| *byte == 0);

    Ok(WindowsArmUefiPflashSlotMap {
        name,
        path: path.clone(),
        ipa_start,
        slot_bytes,
        source_bytes,
        copied_bytes: source_bytes,
        zero_padding_bytes: slot_bytes - source_bytes,
        writable,
        prefix_verified,
        padding_zeroed,
    })
}

pub(crate) fn copy_uefi_vars_template(
    template_path: &PathBuf,
    vars_path: &PathBuf,
) -> Result<(), String> {
    std::fs::copy(template_path, vars_path).map_err(|error| error.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(vars_path)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

pub(crate) fn detect_uefi_firmware_volume(
    bytes: &[u8],
) -> Result<UefiFirmwareVolumeMetadata, String> {
    if bytes.len() < UEFI_FV_MIN_HEADER_BYTES {
        return Err("file is too small for a UEFI firmware volume header".to_string());
    }
    let search_end = bytes.len().min(64 * 1024);
    for signature_offset in UEFI_FV_SIGNATURE_OFFSET..search_end.saturating_sub(4) {
        if &bytes[signature_offset..signature_offset + 4] != UEFI_FV_SIGNATURE {
            continue;
        }
        let offset = signature_offset - UEFI_FV_SIGNATURE_OFFSET;
        if offset + UEFI_FV_MIN_HEADER_BYTES > bytes.len() {
            continue;
        }
        let length_bytes = u64_from_le(bytes, offset + UEFI_FV_LENGTH_OFFSET)?;
        let header_length = u16_from_le(bytes, offset + UEFI_FV_HEADER_LENGTH_OFFSET)?;
        let header_length_usize = usize::from(header_length);
        if header_length_usize < UEFI_FV_MIN_HEADER_BYTES {
            return Err("UEFI firmware volume header length is too small".to_string());
        }
        if header_length_usize % 2 != 0 {
            return Err("UEFI firmware volume header length is not 16-bit aligned".to_string());
        }
        if offset + header_length_usize > bytes.len() {
            return Err("UEFI firmware volume header extends past the file".to_string());
        }
        let length_usize: usize = length_bytes
            .try_into()
            .map_err(|_| "UEFI firmware volume length is too large to inspect".to_string())?;
        if length_usize < header_length_usize {
            return Err("UEFI firmware volume length is smaller than its header".to_string());
        }
        if offset + length_usize > bytes.len() {
            return Err("UEFI firmware volume length extends past the file".to_string());
        }
        let header = &bytes[offset..offset + header_length_usize];
        if uefi_checksum16(header) != 0 {
            return Err("UEFI firmware volume header checksum verification failed".to_string());
        }
        return Ok(UefiFirmwareVolumeMetadata {
            offset: offset as u64,
            length_bytes,
            header_length,
            checksum_verified: true,
        });
    }
    Err("UEFI firmware volume signature _FVH was not found".to_string())
}

pub(crate) fn render_uefi_volume_metadata(
    label: &str,
    volume: &Option<UefiFirmwareVolumeMetadata>,
    output: &mut String,
) {
    match volume {
        Some(volume) => {
            output.push_str(&format!("{label} detected: true\n"));
            output.push_str(&format!("{label} offset: {:#x}\n", volume.offset));
            output.push_str(&format!(
                "{label} length bytes: {:#x}\n",
                volume.length_bytes
            ));
            output.push_str(&format!(
                "{label} header length: {:#x}\n",
                volume.header_length
            ));
            output.push_str(&format!(
                "{label} checksum verified: {}\n",
                volume.checksum_verified
            ));
        }
        None => output.push_str(&format!("{label} detected: false\n")),
    }
}

pub(crate) fn render_uefi_pflash_slot(
    label: &str,
    slot: &Option<WindowsArmUefiPflashSlotMap>,
    output: &mut String,
) {
    match slot {
        Some(slot) => {
            output.push_str(&format!("{label} loaded: true\n"));
            output.push_str(&format!("{label} name: {}\n", slot.name));
            output.push_str(&format!("{label} path: {}\n", slot.path.display()));
            output.push_str(&format!(
                "{label} IPA range: {:#x}..{:#x}\n",
                slot.ipa_start,
                slot.ipa_end_exclusive()
            ));
            output.push_str(&format!("{label} slot bytes: {:#x}\n", slot.slot_bytes));
            output.push_str(&format!("{label} source bytes: {:#x}\n", slot.source_bytes));
            output.push_str(&format!("{label} copied bytes: {:#x}\n", slot.copied_bytes));
            output.push_str(&format!(
                "{label} zero padding bytes: {:#x}\n",
                slot.zero_padding_bytes
            ));
            output.push_str(&format!("{label} writable: {}\n", slot.writable));
            output.push_str(&format!(
                "{label} prefix verified: {}\n",
                slot.prefix_verified
            ));
            output.push_str(&format!(
                "{label} padding zeroed: {}\n",
                slot.padding_zeroed
            ));
        }
        None => output.push_str(&format!("{label} loaded: false\n")),
    }
}

pub(crate) fn ipa_ranges_overlap(
    left_start: u64,
    left_size: u64,
    right_start: u64,
    right_size: u64,
) -> bool {
    let left_end = left_start.saturating_add(left_size);
    let right_end = right_start.saturating_add(right_size);
    left_start < right_end && right_start < left_end
}

pub(crate) fn decode_gpt_partition_name(bytes: &[u8]) -> String {
    let mut units = Vec::new();
    for chunk in bytes.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16_lossy(&units)
}

pub(crate) fn stable_guid_bytes(path: &Path, label: &str, disk_size_bytes: u64) -> [u8; 16] {
    let mut first = fnv1a64(0xcbf2_9ce4_8422_2325, label.as_bytes());
    first = fnv1a64(first, path.to_string_lossy().as_bytes());
    first = fnv1a64(first, &disk_size_bytes.to_le_bytes());
    let mut second = fnv1a64(0x8422_2325_cbf2_9ce4, path.to_string_lossy().as_bytes());
    second = fnv1a64(second, label.as_bytes());
    second = fnv1a64(second, &disk_size_bytes.to_be_bytes());
    let mut output = [0_u8; 16];
    output[0..8].copy_from_slice(&first.to_le_bytes());
    output[8..16].copy_from_slice(&second.to_le_bytes());
    output[6] = (output[6] & 0x0f) | 0x40;
    output[8] = (output[8] & 0x3f) | 0x80;
    output
}

pub(crate) fn fnv1a64(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn write_all_at(file: &mut File, offset: u64, bytes: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())
}

pub(crate) fn read_exact_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut bytes = vec![0_u8; len];
    file.read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

pub(crate) fn u32_from_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "u32 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u32 field parse failed".to_string())?,
    ))
}

pub(crate) fn u16_from_le(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| "u16 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u16 field parse failed".to_string())?,
    ))
}

pub(crate) fn u64_from_le(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| "u64 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u64 field parse failed".to_string())?,
    ))
}

pub(crate) fn uefi_checksum16(bytes: &[u8]) -> u16 {
    let mut sum = 0_u16;
    for chunk in bytes.chunks_exact(2) {
        sum = sum.wrapping_add(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    sum
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
