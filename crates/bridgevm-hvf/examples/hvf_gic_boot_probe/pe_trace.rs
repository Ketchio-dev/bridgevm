use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::stage1::{self, Stage1Context};

#[path = "pe_trace/pe_image.rs"]
mod pe_image;
use pe_image::{find_pe_owner, read_le_u64};
#[cfg(test)]
#[path = "pe_trace/tests.rs"]
mod tests;

pub(crate) fn print_translated_pe_owner(mem: &dyn GuestMemoryMut, label: &str, ipa: Option<u64>) {
    if let Some(ipa) = ipa {
        print_pe_owner(mem, &format!("{label}->ipa"), ipa);
    }
}

pub(crate) fn print_pe_owner(mem: &dyn GuestMemoryMut, label: &str, addr: u64) {
    println!("{}", pe_owner_line(mem, label, addr));
}

fn pe_owner_line(mem: &dyn GuestMemoryMut, label: &str, addr: u64) -> String {
    if addr < machine::RAM_BASE {
        return format!("IMAGE[{label}]: {addr:#x}: outside RAM");
    }
    match find_pe_owner(mem, addr, 512 * 1024 * 1024) {
        Some(owner) => {
            let rva = addr - owner.base;
            let pdb = owner.pdb_path.as_deref().unwrap_or("-");
            format!(
                "IMAGE[{label}]: addr={addr:#x} base={:#x} size={:#x} rva={rva:#x} entry={:#x} machine={:#x} preferred_base={:#x} pdb={pdb}",
                owner.base,
                owner.size,
                owner.entry_rva,
                owner.machine,
                owner.preferred_base
            )
        }
        None => format!("IMAGE[{label}]: {addr:#x}: no PE owner found within 512 MiB below"),
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
