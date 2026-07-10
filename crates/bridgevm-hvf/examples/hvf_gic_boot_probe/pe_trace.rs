use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::stage1::{self, Stage1Context};

pub(crate) fn print_translated_pe_owner(mem: &dyn GuestMemoryMut, label: &str, ipa: Option<u64>) {
    if let Some(ipa) = ipa {
        print_pe_owner(mem, &format!("{label}->ipa"), ipa);
    }
}

#[derive(Debug)]
struct PeImageOwner {
    base: u64,
    size: u32,
    entry_rva: u32,
    machine: u16,
    preferred_base: u64,
    pdb_path: Option<String>,
}

fn read_guest_array<const N: usize>(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<[u8; N]> {
    let mut bytes = [0u8; N];
    mem.read_into(gpa, &mut bytes).then_some(bytes)
}

fn read_le_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    Some(u16::from_le_bytes(read_guest_array(mem, gpa)?))
}

fn read_le_u32(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u32> {
    Some(u32::from_le_bytes(read_guest_array(mem, gpa)?))
}

fn read_le_u64(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u64> {
    Some(u64::from_le_bytes(read_guest_array(mem, gpa)?))
}

fn codeview_path(bytes: &[u8]) -> Option<String> {
    let start = if bytes.starts_with(b"RSDS") {
        24
    } else if bytes.starts_with(b"NB10") {
        16
    } else {
        return None;
    };
    let path = bytes.get(start..)?;
    let end = path.iter().position(|b| *b == 0).unwrap_or(path.len());
    if end == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&path[..end]).to_string();
    Some(text)
}

fn pe_debug_pdb_path(
    mem: &dyn GuestMemoryMut,
    base: u64,
    optional_header: u64,
    optional_magic: u16,
) -> Option<String> {
    let (dirs_offset, debug_dir_offset) = match optional_magic {
        0x10b => (0x5c, 0x60 + 6 * 8), // PE32
        0x20b => (0x6c, 0x70 + 6 * 8), // PE32+
        _ => return None,
    };
    if read_le_u32(mem, optional_header + dirs_offset)? <= 6 {
        return None;
    }
    let debug_rva = u64::from(read_le_u32(mem, optional_header + debug_dir_offset)?);
    let debug_size = u64::from(read_le_u32(mem, optional_header + debug_dir_offset + 4)?);
    if debug_rva == 0 || debug_size < 28 {
        return None;
    }
    let entries = (debug_size / 28).min(16);
    for i in 0..entries {
        let entry = base + debug_rva + i * 28;
        let typ = read_le_u32(mem, entry + 12)?;
        if typ != 2 {
            continue; // IMAGE_DEBUG_TYPE_CODEVIEW
        }
        let size = read_le_u32(mem, entry + 16)?;
        let addr = read_le_u32(mem, entry + 20)?;
        if size < 4 || addr == 0 {
            continue;
        }
        let len = usize::try_from(size).ok()?.min(512);
        let mut bytes = [0u8; 512];
        if !mem.read_into(base + u64::from(addr), &mut bytes[..len]) {
            return None;
        }
        if let Some(path) = codeview_path(&bytes[..len]) {
            return Some(path);
        }
    }
    None
}

fn pe_image_at(mem: &dyn GuestMemoryMut, base: u64) -> Option<PeImageOwner> {
    if read_guest_array::<2>(mem, base)? != *b"MZ" {
        return None;
    }
    let e_lfanew = u64::from(read_le_u32(mem, base + 0x3c)?);
    if !(0x40..=0x1000).contains(&e_lfanew) {
        return None;
    }
    let pe = base + e_lfanew;
    if read_guest_array::<4>(mem, pe)? != *b"PE\0\0" {
        return None;
    }
    let machine = read_le_u16(mem, pe + 4)?;
    let optional_size = u64::from(read_le_u16(mem, pe + 20)?);
    let optional = pe + 24;
    if optional_size < 0x60 {
        return None;
    }
    let magic = read_le_u16(mem, optional)?;
    let preferred_base = match magic {
        0x10b => u64::from(read_le_u32(mem, optional + 0x1c)?),
        0x20b => read_le_u64(mem, optional + 0x18)?,
        _ => return None,
    };
    let entry_rva = read_le_u32(mem, optional + 0x10)?;
    let size = read_le_u32(mem, optional + 0x38)?;
    if size == 0 || size > 128 * 1024 * 1024 {
        return None;
    }
    Some(PeImageOwner {
        base,
        size,
        entry_rva,
        machine,
        preferred_base,
        pdb_path: pe_debug_pdb_path(mem, base, optional, magic),
    })
}

fn find_pe_owner(mem: &dyn GuestMemoryMut, addr: u64, scan_limit: u64) -> Option<PeImageOwner> {
    if addr < machine::RAM_BASE {
        return None;
    }
    let min = machine::RAM_BASE.max(addr.saturating_sub(scan_limit));
    let mut base = addr & !0xfff;
    loop {
        if let Some(owner) = pe_image_at(mem, base) {
            if addr >= owner.base && addr < owner.base + u64::from(owner.size) {
                return Some(owner);
            }
        }
        if base <= min {
            break;
        }
        base = base.saturating_sub(0x1000);
    }
    None
}

pub(crate) fn print_pe_owner(mem: &dyn GuestMemoryMut, label: &str, addr: u64) {
    if addr < machine::RAM_BASE {
        println!("IMAGE[{label}]: {addr:#x}: outside RAM");
        return;
    }
    match find_pe_owner(mem, addr, 512 * 1024 * 1024) {
        Some(owner) => {
            let rva = addr - owner.base;
            let pdb = owner.pdb_path.as_deref().unwrap_or("-");
            println!(
                "IMAGE[{label}]: addr={addr:#x} base={:#x} size={:#x} rva={rva:#x} entry={:#x} machine={:#x} preferred_base={:#x} pdb={pdb}",
                owner.base,
                owner.size,
                owner.entry_rva,
                owner.machine,
                owner.preferred_base
            );
        }
        None => println!("IMAGE[{label}]: {addr:#x}: no PE owner found within 512 MiB below"),
    }
}

fn pe_owner_summary(mem: &dyn GuestMemoryMut, addr: u64) -> String {
    if addr < machine::RAM_BASE {
        return "outside RAM".to_string();
    }
    match find_pe_owner(mem, addr, 512 * 1024 * 1024) {
        Some(owner) => {
            let rva = addr - owner.base;
            let pdb = owner.pdb_path.as_deref().unwrap_or("-");
            format!(
                "base={:#x} rva={rva:#x} entry={:#x} pdb={pdb}",
                owner.base, owner.entry_rva
            )
        }
        None => "no PE owner within 512 MiB below".to_string(),
    }
}

pub(crate) fn translated_ipa(
    mem: &dyn GuestMemoryMut,
    ctx: &Stage1Context,
    va: u64,
) -> Result<u64, String> {
    stage1::translate(mem, ctx, va)
        .map(|t| t.ipa)
        .map_err(|failure| failure.reason)
}

pub(crate) fn print_frame_chain(
    mem: &dyn GuestMemoryMut,
    ctx: &Stage1Context,
    start_fp: u64,
    limit: usize,
) {
    if start_fp == 0 || limit == 0 {
        return;
    }
    println!("FRAMECHAIN: start_fp={start_fp:#x} limit={limit}");
    let mut fp = start_fp;
    for index in 0..limit {
        let fp_ipa = match translated_ipa(mem, ctx, fp) {
            Ok(ipa) => ipa,
            Err(reason) => {
                println!("  frame[{index}]: fp={fp:#x}: {reason}");
                break;
            }
        };
        let Some(next_fp) = read_le_u64(mem, fp_ipa) else {
            println!("  frame[{index}]: fp={fp:#x} fp_ipa={fp_ipa:#x}: frame unreadable");
            break;
        };
        let saved_lr = match read_le_u64(mem, fp_ipa + 8) {
            Some(value) => value,
            None => {
                println!("  frame[{index}]: fp={fp:#x} fp_ipa={fp_ipa:#x}: saved LR unreadable");
                break;
            }
        };
        let lr_ipa = if saved_lr == 0 {
            None
        } else {
            translated_ipa(mem, ctx, saved_lr).ok()
        };
        let owner = lr_ipa
            .map(|ipa| pe_owner_summary(mem, ipa))
            .unwrap_or_else(|| "-".to_string());
        match lr_ipa {
            Some(ipa) => println!(
                "  frame[{index}]: fp={fp:#x} fp_ipa={fp_ipa:#x} next_fp={next_fp:#x} lr={saved_lr:#x} lr_ipa={ipa:#x} image={owner}"
            ),
            None => println!(
                "  frame[{index}]: fp={fp:#x} fp_ipa={fp_ipa:#x} next_fp={next_fp:#x} lr={saved_lr:#x} lr_ipa=- image={owner}"
            ),
        }
        if next_fp == 0 {
            break;
        }
        if next_fp <= fp {
            println!("  frame[{index}]: stopping: next_fp is not above current fp");
            break;
        }
        if next_fp - fp > 1024 * 1024 {
            println!("  frame[{index}]: stopping: next_fp jump exceeds 1 MiB");
            break;
        }
        fp = next_fp;
    }
}
