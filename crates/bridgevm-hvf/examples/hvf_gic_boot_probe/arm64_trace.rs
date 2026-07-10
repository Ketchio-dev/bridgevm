use bridgevm_hvf::fwcfg::GuestMemoryMut;

fn sign_extend(value: u64, bits: u8) -> i64 {
    let shift = 64 - bits;
    ((value << shift) as i64) >> shift
}

fn branch_delta_string(delta: i64) -> String {
    if delta < 0 {
        format!("-{:#x}", delta.unsigned_abs())
    } else {
        format!("+{delta:#x}")
    }
}

fn condition_name(cond: u32) -> &'static str {
    match cond & 0xf {
        0x0 => "eq",
        0x1 => "ne",
        0x2 => "cs",
        0x3 => "cc",
        0x4 => "mi",
        0x5 => "pl",
        0x6 => "vs",
        0x7 => "vc",
        0x8 => "hi",
        0x9 => "ls",
        0xa => "ge",
        0xb => "lt",
        0xc => "gt",
        0xd => "le",
        0xe => "al",
        _ => "nv",
    }
}

fn describe_arm64_instruction_word(pc: u64, word: u32) -> String {
    if (word & 0x7c00_0000) == 0x1400_0000 {
        let delta = sign_extend(u64::from(word & 0x03ff_ffff), 26) << 2;
        let target = pc.wrapping_add_signed(delta);
        let mnemonic = if (word & 0x8000_0000) == 0 { "b" } else { "bl" };
        return format!("{mnemonic} {} -> {target:#x}", branch_delta_string(delta));
    }
    if (word & 0xff00_0010) == 0x5400_0000 {
        let delta = sign_extend(u64::from((word >> 5) & 0x7ffff), 19) << 2;
        let target = pc.wrapping_add_signed(delta);
        return format!(
            "b.{} {} -> {target:#x}",
            condition_name(word & 0xf),
            branch_delta_string(delta)
        );
    }
    if (word & 0x7e00_0000) == 0x3400_0000 {
        let delta = sign_extend(u64::from((word >> 5) & 0x7ffff), 19) << 2;
        let target = pc.wrapping_add_signed(delta);
        let width = if (word & 0x8000_0000) == 0 { "w" } else { "x" };
        let mnemonic = if (word & 0x0100_0000) == 0 {
            "cbz"
        } else {
            "cbnz"
        };
        return format!(
            "{mnemonic} {width}{}, {} -> {target:#x}",
            word & 0x1f,
            branch_delta_string(delta)
        );
    }
    if (word & 0x7e00_0000) == 0x3600_0000 {
        let delta = sign_extend(u64::from((word >> 5) & 0x3fff), 14) << 2;
        let target = pc.wrapping_add_signed(delta);
        let bit = ((word >> 19) & 0x1f) | ((word >> 26) & 0x20);
        let width = if bit < 32 { "w" } else { "x" };
        let mnemonic = if (word & 0x0100_0000) == 0 {
            "tbz"
        } else {
            "tbnz"
        };
        return format!(
            "{mnemonic} {width}{}, #{bit}, {} -> {target:#x}",
            word & 0x1f,
            branch_delta_string(delta)
        );
    }
    match word {
        0xd503_201f => "nop".to_string(),
        0xd503_207f => "wfi".to_string(),
        0xd503_205f => "wfe".to_string(),
        0xd503_3f9f => "dsb sy".to_string(),
        0xd503_3fdf => "isb sy".to_string(),
        _ if (word & 0xffff_fc1f) == 0xd65f_0000 => format!("ret x{}", (word >> 5) & 0x1f),
        _ if (word & 0xffff_fc1f) == 0xd61f_0000 => format!("br x{}", (word >> 5) & 0x1f),
        _ if (word & 0xffff_fc1f) == 0xd63f_0000 => format!("blr x{}", (word >> 5) & 0x1f),
        _ => "-".to_string(),
    }
}

pub(super) fn print_translated_instruction_words(
    mem: &dyn GuestMemoryMut,
    label: &str,
    center_va: u64,
    center_ipa: Option<u64>,
    before: u64,
    len: usize,
) {
    let Some(center_ipa) = center_ipa else {
        return;
    };
    let (Some(base_va), Some(base_ipa)) = (
        center_va.checked_sub(before),
        center_ipa.checked_sub(before),
    ) else {
        return;
    };
    let aligned_len = len & !3;
    if aligned_len == 0 {
        return;
    }
    let mut bytes = vec![0u8; aligned_len];
    if !mem.read_into(base_ipa, &mut bytes) {
        println!("INSN[{label}->ipa]@{base_ipa:#x}: <not in guest RAM view>");
        return;
    }
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let Ok(index) = u64::try_from(index) else {
            break;
        };
        let Some(off) = index.checked_mul(4) else {
            break;
        };
        let va = base_va + off;
        let ipa = base_ipa + off;
        let Ok(word_bytes) = <[u8; 4]>::try_from(chunk) else {
            continue;
        };
        let word = u32::from_le_bytes(word_bytes);
        let desc = describe_arm64_instruction_word(va, word);
        println!("INSN[{label}->ipa]: va={va:#x} ipa={ipa:#x} word={word:#010x} {desc}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_branch_immediates() {
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0x1400_0001),
            "b +0x4 -> 0x1004"
        );
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0x97ff_ffff),
            "bl -0x4 -> 0xffc"
        );
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0x5400_0020),
            "b.eq +0x4 -> 0x1004"
        );
    }

    #[test]
    fn describes_register_branches_and_waits() {
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0xd65f_03c0),
            "ret x30"
        );
        assert_eq!(describe_arm64_instruction_word(0x1000, 0xd503_207f), "wfi");
    }

    #[test]
    fn describes_test_and_compare_branches() {
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0x3500_0020),
            "cbnz w0, +0x4 -> 0x1004"
        );
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0x3600_0020),
            "tbz w0, #0, +0x4 -> 0x1004"
        );
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0x3710_0068),
            "tbnz w8, #2, +0xc -> 0x100c"
        );
        assert_eq!(
            describe_arm64_instruction_word(0x1000, 0xb600_0020),
            "tbz x0, #32, +0x4 -> 0x1004"
        );
    }
}
