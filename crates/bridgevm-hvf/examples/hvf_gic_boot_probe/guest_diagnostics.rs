//! Guest memory, reset snapshot, and stage-1 translation diagnostics.

use crate::*;

pub(crate) fn print_bytes(label: &str, base: u64, bytes: &[u8]) {
    print!("{label}@{base:#x}:");
    for b in bytes {
        print!("{b:02x}");
    }
    println!();
}

pub(crate) fn dump_guest_bytes(
    mem: &dyn GuestMemoryMut,
    label: &str,
    center: u64,
    before: u64,
    len: usize,
) {
    let Some(base) = center.checked_sub(before) else {
        println!("{label}@{center:#x}: <underflow>");
        return;
    };
    match mem.read_bytes(base, len) {
        Some(bytes) => print_bytes(label, base, &bytes),
        None => println!("{label}@{base:#x}: <not in guest RAM view>"),
    }
}

pub(crate) fn dump_guest_bytes_if_mapped(
    mem: &dyn GuestMemoryMut,
    label: &str,
    center: u64,
    before: u64,
    len: usize,
) {
    let Some(base) = center.checked_sub(before) else {
        return;
    };
    if let Some(bytes) = mem.read_bytes(base, len) {
        print_bytes(label, base, &bytes);
    }
}

pub(crate) fn dump_env_guest_bytes(mem: &dyn GuestMemoryMut) {
    let Ok(extra) = std::env::var("BRIDGEVM_BOOT_PROBE_DUMP_GPA") else {
        return;
    };
    for (idx, spec) in extra
        .split([',', ';', ' ', '\n', '\t'])
        .filter(|s| !s.trim().is_empty())
        .enumerate()
    {
        let mut parts = spec.split(':');
        let Some(gpa) = parts.next().and_then(parse_u64) else {
            println!("DUMP[env:{idx}] {spec:?}: <invalid gpa>");
            continue;
        };
        let len = parts
            .next()
            .and_then(parse_u64)
            .map(|v| v.clamp(1, 0x1000) as usize)
            .unwrap_or(0x100);
        let before = parts
            .next()
            .and_then(parse_u64)
            .map(|v| v.min(0x1000))
            .unwrap_or(0);
        dump_guest_bytes(mem, &format!("DUMP[env:{idx}]"), gpa, before, len);
    }
}

/// Directory for the crash-survivable reset snapshot channel, or None if the
/// `BRIDGEVM_DUMP_ON_RESET` env is unset/empty. Forwarded by the installed-boot
/// runner (`--dump-on-reset <dir>`); the launcher strips inherited BRIDGEVM_*
/// so it only arrives via ENV_ARGS.
pub(crate) fn dump_on_reset_dir() -> Option<String> {
    std::env::var("BRIDGEVM_DUMP_ON_RESET")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// How many guest resets to snapshot (default 1 = only the first / gen1 crash).
pub(crate) fn dump_on_reset_max() -> u64 {
    std::env::var("BRIDGEVM_DUMP_ON_RESET_MAX")
        .ok()
        .and_then(|s| parse_u64(&s))
        .unwrap_or(1)
}

/// Crash-survivable observation channel. On a guest PSCI SYSTEM_RESET, snapshot
/// the vCPU register file and the ENTIRE guest RAM to disk BEFORE the reboot
/// path wipes RAM. Windows bugchecks that self-reset without ever writing a
/// crash dump (the venus KMD StartDevice crash behaves this way) are still
/// fully readable offline from this snapshot: walk TTBR1_EL1, resolve
/// KiBugCheckData, map the faulting PC/params to loaded modules. Best-effort;
/// any IO error is logged and non-fatal so it never perturbs the reboot path.
pub(crate) fn dump_guest_state_on_reset(
    dir: &str,
    index: u64,
    vcpu: HvVcpuT,
    ram_ptr: *const u8,
    ram_size: usize,
) {
    use std::io::Write as _;
    if let Err(e) = std::fs::create_dir_all(dir) {
        println!("DUMP_ON_RESET[{index}]: cannot create dir {dir:?}: {e}");
        return;
    }
    let reg = |r: u32| -> u64 {
        let mut v = 0u64;
        unsafe {
            hv_vcpu_get_reg(vcpu, r, &mut v);
        }
        v
    };
    let sreg = |r: u16| -> u64 {
        let mut v = 0u64;
        unsafe {
            hv_vcpu_get_sys_reg(vcpu, r, &mut v);
        }
        v
    };
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"index\": {index},\n"));
    json.push_str(&format!("  \"ram_base\": {},\n", machine::RAM_BASE));
    json.push_str(&format!("  \"ram_size\": {ram_size},\n"));
    json.push_str("  \"x\": [");
    for i in 0..=30u32 {
        if i > 0 {
            json.push_str(", ");
        }
        json.push_str(&format!("{}", reg(HV_REG_X0 + i)));
    }
    json.push_str("],\n");
    for (name, v) in [
        ("pc", reg(HV_REG_PC)),
        ("cpsr", reg(HV_REG_CPSR)),
        ("sctlr_el1", sreg(HV_SYS_REG_SCTLR_EL1)),
        ("ttbr0_el1", sreg(HV_SYS_REG_TTBR0_EL1)),
        ("ttbr1_el1", sreg(HV_SYS_REG_TTBR1_EL1)),
        ("tcr_el1", sreg(HV_SYS_REG_TCR_EL1)),
        ("spsr_el1", sreg(HV_SYS_REG_SPSR_EL1)),
        ("elr_el1", sreg(HV_SYS_REG_ELR_EL1)),
        ("esr_el1", sreg(HV_SYS_REG_ESR_EL1)),
        ("far_el1", sreg(HV_SYS_REG_FAR_EL1)),
        ("mair_el1", sreg(HV_SYS_REG_MAIR_EL1)),
        ("vbar_el1", sreg(HV_SYS_REG_VBAR_EL1)),
        ("sp_el0", sreg(HV_SYS_REG_SP_EL0)),
        ("sp_el1", sreg(HV_SYS_REG_SP_EL1)),
    ] {
        json.push_str(&format!("  \"{name}\": {v},\n"));
    }
    json.push_str("  \"_\": 0\n}\n");
    let regs_path = format!("{dir}/reset-{index}-regs.json");
    match std::fs::write(&regs_path, json.as_bytes()) {
        Ok(()) => println!("DUMP_ON_RESET[{index}]: wrote {regs_path}"),
        Err(e) => println!("DUMP_ON_RESET[{index}]: regs write failed: {e}"),
    }
    let ram_path = format!("{dir}/reset-{index}-ram.bin");
    match std::fs::File::create(&ram_path) {
        Ok(mut f) => {
            // SAFETY: ram_ptr/ram_size describe the live guest RAM mapping,
            // valid for the whole probe run and not concurrently mutated here
            // (all vCPUs are stopped/joined before the reset dispatcher runs).
            let slice = unsafe { std::slice::from_raw_parts(ram_ptr, ram_size) };
            let mut off = 0usize;
            let mut ok = true;
            while off < ram_size {
                let end = (off + (32usize << 20)).min(ram_size);
                if let Err(e) = f.write_all(&slice[off..end]) {
                    println!("DUMP_ON_RESET[{index}]: ram write failed at {off:#x}: {e}");
                    ok = false;
                    break;
                }
                off = end;
            }
            if ok {
                let _ = f.flush();
                println!("DUMP_ON_RESET[{index}]: wrote {ram_path} ({ram_size} bytes)");
            }
        }
        Err(e) => println!("DUMP_ON_RESET[{index}]: ram create failed: {e}"),
    }
}

pub(crate) fn print_stage1_walk_steps(label: &str, steps: &[Stage1WalkStep]) {
    if !env_flag("BRIDGEVM_TRACE_STAGE1_WALKS") {
        return;
    }
    for step in steps {
        let desc = step
            .descriptor
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        let next = step
            .next_table_ipa
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        let out = step
            .output_ipa
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  WALK[{label}]: L{} table={:#x} index={:#x} entry={:#x} desc={} kind={} next={} out={}",
            step.level, step.table_ipa, step.index, step.entry_ipa, desc, step.kind, next, out
        );
    }
}

pub(crate) fn print_stage1_translation(
    mem: &dyn GuestMemoryMut,
    ctx: &Stage1Context,
    label: &str,
    va: u64,
) -> Option<u64> {
    if va == 0 {
        return None;
    }
    match stage1::translate(mem, ctx, va) {
        Ok(t) => {
            println!(
                "XLATE[{label}]: va={va:#x} -> ipa={:#x} root={} va_bits={} start=L{} leaf=L{}:{} desc={:#x} va_base={:#x} ipa_base={:#x} attr={} ap={} sh={} af={} pxn={} uxn={}",
                t.ipa,
                t.root.label(),
                t.va_bits,
                t.start_level,
                t.leaf_level,
                t.leaf_kind,
                t.leaf_descriptor,
                t.leaf_va_base,
                t.leaf_ipa_base,
                t.attr_index,
                t.access_permissions,
                t.shareability,
                t.access_flag,
                t.pxn,
                t.uxn
            );
            print_stage1_walk_steps(label, &t.steps);
            Some(t.ipa)
        }
        Err(failure) => {
            println!("XLATE[{label}]: va={va:#x}: {}", failure.reason);
            print_stage1_walk_steps(label, &failure.steps);
            None
        }
    }
}

pub(crate) fn dump_translated_guest_bytes(
    mem: &dyn GuestMemoryMut,
    label: &str,
    ipa: Option<u64>,
    before: u64,
    len: usize,
) {
    if let Some(ipa) = ipa {
        dump_guest_bytes(mem, &format!("{label}->ipa"), ipa, before, len);
    }
}
