use bridgevm_hvf::machine::bridgevm_pc as board;

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn image_size(bytes: &[u8], base: usize) -> Option<usize> {
    if bytes.get(base..base + 2)? != b"MZ" {
        return None;
    }
    let pe = base.checked_add(u32_at(bytes, base + 0x3c)? as usize)?;
    if bytes.get(pe..pe + 4)? != b"PE\0\0" || u16_at(bytes, pe + 24)? != 0x20b {
        return None;
    }
    Some(u32_at(bytes, pe + 24 + 56)? as usize)
}

pub(super) fn loaded_image(ram: &[u8], pc: u64) -> String {
    let Some(pc_offset) = pc.checked_sub(board::RAM_BASE).map(|value| value as usize) else {
        return "image=outside-ram".to_string();
    };
    if pc_offset >= ram.len() {
        return "image=outside-ram".to_string();
    }
    let mut base = pc_offset & !0xfff;
    loop {
        let Some(size) = image_size(ram, base) else {
            if base == 0 {
                break;
            }
            base -= 0x1000;
            continue;
        };
        if pc_offset < base.saturating_add(size) {
            return format!(
                "image_base={:#x} image_offset={:#x} image_size={size:#x}",
                board::RAM_BASE + base as u64,
                pc_offset - base
            );
        }
        if base == 0 {
            break;
        }
        base -= 0x1000;
    }
    "image=unresolved".to_string()
}

pub(super) fn frame_return_address(ram: &[u8], frame_pointer: u64) -> Option<u64> {
    let offset = frame_pointer.checked_sub(board::RAM_BASE)? as usize;
    u64_at(ram, offset.checked_add(8)?)
}

pub(super) fn exception_context(ram: &[u8], deadloop_frame: u64) -> Option<String> {
    let deadloop_offset = deadloop_frame.checked_sub(board::RAM_BASE)? as usize;
    let handler_frame = u64_at(ram, deadloop_offset)?;
    let context = handler_frame.checked_add(0x160)?;
    let context_offset = context.checked_sub(board::RAM_BASE)? as usize;
    let x0 = u64_at(ram, context_offset)?;
    let x1 = u64_at(ram, context_offset.checked_add(8)?)?;
    let x16 = u64_at(ram, context_offset.checked_add(0x80)?)?;
    let lr = u64_at(ram, context_offset.checked_add(0xf0)?)?;
    let sp = u64_at(ram, context_offset.checked_add(0xf8)?)?;
    let elr = u64_at(ram, context_offset.checked_add(768)?)?;
    let esr = u64_at(ram, context_offset.checked_add(792)?)?;
    let far = u64_at(ram, context_offset.checked_add(800)?)?;
    Some(format!(
        "handler_frame={handler_frame:#x} exception_context={context:#x} ELR={elr:#x} ESR={esr:#x} FAR={far:#x} saved_LR={lr:#x} saved_SP={sp:#x} saved_x0={x0:#x} saved_x1={x1:#x} saved_x16={x16:#x} elr_{} lr_{}",
        loaded_image(ram, elr),
        loaded_image(ram, lr)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_an_unmapped_pc() {
        assert_eq!(loaded_image(&[0; 0x1000], 0), "image=outside-ram");
    }
}
