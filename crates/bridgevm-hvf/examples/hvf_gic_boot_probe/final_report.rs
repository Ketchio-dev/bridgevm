macro_rules! persist_and_report_stop {
    ($platform:ident, $media:ident, $vcpu:ident, $guest_ram:ident, $last_pc:ident, $last_pre_run_pc:ident, $last_watch_pc:ident, $last_watch_lr:ident, $stop_reason:ident, $stop_reason_code:ident, $exits:ident, $vtimer_exits:ident, $psci_calls:ident, $surplus_canceled_exits:ident, $boot_timer:ident, $boot_timer_elapsed:ident, $secondary_exit_counts:ident, $drain_stats:ident, $unimpl:ident, $mmio_traces:ident, $recent_pcie_ecam:ident, $recent_pcie_mmio:ident, $recent_pcie_pio:ident, $recent_xhci:ident, $uart_triggers:ident, $xhci_hid_boot_key_triggers:ident, $xhci_setup_input_triggers:ident, $xhci_pointer_input_triggers:ident, $redist_lo:ident, $redist_hi:ident $(,)?) => {{
        let mut platform_guard = $platform.lock().expect("platform mutex");
        let $platform = &mut *platform_guard;
        let serial = $platform.uart_output().to_vec();
        let vars_writes = $media
            .flash_vars
            .persist($platform.flash_vars_image())
            .unwrap_or_else(|e| panic!("persist UEFI vars: {e}"));
        print_media_writes("UEFI vars", &vars_writes);
        if let Some(nvme) = $media.nvme_disk.as_ref() {
            let writes = persist_nvme_media($platform, nvme, NvmePersistNamespace::Primary);
            print_media_writes(NvmePersistNamespace::Primary.subject(), &writes);
        }
        if let Some(target) = $media.nvme_target.as_ref() {
            let writes = persist_nvme_media($platform, target, NvmePersistNamespace::Target);
            print_media_writes(NvmePersistNamespace::Target.subject(), &writes);
        }
        storage_effect_receipt::maybe_write_probe_storage_effect_receipt(
            $media.nvme_disk.as_ref(),
            $platform,
        );
        maybe_write_file("BRIDGEVM_BOOT_PROBE_SERIAL_OUT", &serial, "serial log");
        let symbols = symbol_lines(&serial);
        if !symbols.is_empty() {
            let text = symbols.join("\n") + "\n";
            maybe_write_file(
                "BRIDGEVM_BOOT_PROBE_SYMBOLS_OUT",
                text.as_bytes(),
                "symbol log",
            );
        }

        let fp = read_reg($vcpu, HV_REG_FP);
        let lr = read_reg($vcpu, HV_REG_LR);
        let sp_el0 = read_sys_reg($vcpu, HV_SYS_REG_SP_EL0);
        let sp_el1 = read_sys_reg($vcpu, HV_SYS_REG_SP_EL1);
        let vbar_el1 = read_sys_reg($vcpu, HV_SYS_REG_VBAR_EL1);
        let elr_el1 = read_sys_reg($vcpu, HV_SYS_REG_ELR_EL1);
        let esr_el1 = read_sys_reg($vcpu, HV_SYS_REG_ESR_EL1);
        let far_el1 = read_sys_reg($vcpu, HV_SYS_REG_FAR_EL1);
        let spsr_el1 = read_sys_reg($vcpu, HV_SYS_REG_SPSR_EL1);
        let stage1_ctx = Stage1Context {
            sctlr_el1: read_sys_reg($vcpu, HV_SYS_REG_SCTLR_EL1),
            tcr_el1: read_sys_reg($vcpu, HV_SYS_REG_TCR_EL1),
            ttbr0_el1: read_sys_reg($vcpu, HV_SYS_REG_TTBR0_EL1),
            ttbr1_el1: read_sys_reg($vcpu, HV_SYS_REG_TTBR1_EL1),
            mair_el1: read_sys_reg($vcpu, HV_SYS_REG_MAIR_EL1),
        };
        println!(
            "REGS: pc={:#x} lr={lr:#x} fp={fp:#x} sp_el0={sp_el0:#x} sp_el1={sp_el1:#x}", $last_pc
        );
        print_gpr_context($vcpu);
        println!(
            "STAGE1: SCTLR={:#x} MMU={} TCR={:#x} TTBR0={:#x} TTBR1={:#x} MAIR={:#x}",
            stage1_ctx.sctlr_el1,
            stage1_ctx.sctlr_el1 & 1 != 0,
            stage1_ctx.tcr_el1,
            stage1_ctx.ttbr0_el1,
            stage1_ctx.ttbr1_el1,
            stage1_ctx.mair_el1
        );
        let pc_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "pc", $last_pc);
        let lr_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "lr", lr);
        let elr_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "elr", elr_el1);
        let _vbar_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "vbar", vbar_el1);
        let _far_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "far", far_el1);
        let fp_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "fp", fp);
        let _sp_el0_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "sp_el0", sp_el0);
        let sp_el1_ipa = print_stage1_translation(&$guest_ram, &stage1_ctx, "sp_el1", sp_el1);
        print_pe_owner(&$guest_ram, "pc", $last_pc);
        print_pe_owner(&$guest_ram, "lr", lr);
        print_translated_pe_owner(&$guest_ram, "pc", pc_ipa);
        print_translated_pe_owner(&$guest_ram, "lr", lr_ipa);
        print_translated_pe_owner(&$guest_ram, "elr", elr_ipa);
        if $last_watch_pc != 0 {
            print_pe_owner(&$guest_ram, "watch-pc", $last_watch_pc);
            print_pe_owner(&$guest_ram, "watch-lr", $last_watch_lr);
            let watch_pc_ipa =
                print_stage1_translation(&$guest_ram, &stage1_ctx, "watch-pc", $last_watch_pc);
            let watch_lr_ipa =
                print_stage1_translation(&$guest_ram, &stage1_ctx, "watch-lr", $last_watch_lr);
            print_translated_pe_owner(&$guest_ram, "watch-pc", watch_pc_ipa);
            print_translated_pe_owner(&$guest_ram, "watch-lr", watch_lr_ipa);
        }
        dump_guest_bytes(&$guest_ram, "CODE[pc]", $last_pc, 0x20, 0x60);
        dump_guest_bytes(&$guest_ram, "CODE[lr]", lr, 0x28, 0x60);
        dump_translated_guest_bytes(&$guest_ram, "CODE[pc]", pc_ipa, 0x20, 0x60);
        dump_translated_guest_bytes(&$guest_ram, "CODE[lr]", lr_ipa, 0x28, 0x60);
        print_translated_instruction_words(&$guest_ram, "CODE[pc]", $last_pc, pc_ipa, 0x20, 0x60);
        print_translated_instruction_words(&$guest_ram, "CODE[lr]", lr, lr_ipa, 0x28, 0x60);
        if fp != 0 {
            dump_guest_bytes(&$guest_ram, "FRAME[fp]", fp, 0, 0x80);
            dump_translated_guest_bytes(&$guest_ram, "FRAME[fp]", fp_ipa, 0, 0x80);
            let frame_limit =
                usize::try_from(env_u64("BRIDGEVM_FRAME_CHAIN_LIMIT", 12)).unwrap_or(12);
            print_frame_chain(&$guest_ram, &stage1_ctx, fp, frame_limit.min(64));
        }
        if sp_el1 != 0 {
            dump_guest_bytes(&$guest_ram, "STACK[sp_el1]", sp_el1, 0, 0x100);
            dump_translated_guest_bytes(&$guest_ram, "STACK[sp_el1]", sp_el1_ipa, 0, 0x100);
        }
        dump_env_guest_bytes(&$guest_ram);

        let mut rx = [0u64; 4];
        for (i, r) in [HV_REG_X0, HV_REG_X0 + 1, HV_REG_X0 + 2, HV_REG_X0 + 9]
            .iter()
            .enumerate()
        {
            hv_vcpu_get_reg($vcpu, *r, &mut rx[i]);
        }
        println!(
            "REG-HINTS: x0={:#x} x1={:#x} x2={:#x} x9={:#x}  (x0 device: {:?})",
            rx[0],
            rx[1],
            rx[2],
            rx[3],
            machine::device_at(rx[0])
        );
        dump_guest_bytes_if_mapped(&$guest_ram, "REG-HINT[x0]", rx[0], 0x40, 0x100);
        // Legacy late-DXE poll convention: x22 = polled address, x21 = expected value,
        // x20 = last read. These registers are still useful breadcrumbs, but Windows
        // high-VA/SVC stops must be read through the full GPR dump above.
        let mut ry = [0u64; 3];
        for (i, r) in [HV_REG_X0 + 20, HV_REG_X0 + 21, HV_REG_X0 + 22]
            .iter()
            .enumerate()
        {
            hv_vcpu_get_reg($vcpu, *r, &mut ry[i]);
        }
        println!(
            "LEGACY-POLL-HINT: x22(addr)={:#x} (dev {:?})  x21(expect)={:#x}  x20(last)={:#x}",
            ry[2],
            machine::device_at(ry[2]),
            ry[1],
            ry[0]
        );
        dump_guest_bytes_if_mapped(&$guest_ram, "LEGACY-POLL[x22]", ry[2], 0, 0x100);
        if ry[1] != ry[2] {
            dump_guest_bytes_if_mapped(&$guest_ram, "LEGACY-POLL[x21]", ry[1], 0, 0x100);
        }
        if ry[0] != ry[1] && ry[0] != ry[2] {
            dump_guest_bytes_if_mapped(&$guest_ram, "LEGACY-POLL[x20]", ry[0], 0, 0x100);
        }
        let mut cpsr = 0u64;
        hv_vcpu_get_reg($vcpu, HV_REG_CPSR, &mut cpsr);
        println!(
            "CPSR={cpsr:#x}  DAIF: D={} A={} I(irq-masked)={} F={}  EL={}",
            (cpsr >> 9) & 1,
            (cpsr >> 8) & 1,
            (cpsr >> 7) & 1,
            (cpsr >> 6) & 1,
            (cpsr >> 2) & 3
        );
        println!(
        "EL1_EXC: VBAR={vbar_el1:#x} ELR={elr_el1:#x} ESR={esr_el1:#x} ({}) FAR={far_el1:#x} SPSR={spsr_el1:#x}",
        describe_esr(esr_el1)
                );
        print_pe_owner(&$guest_ram, "elr", elr_el1);
        print_translated_pe_owner(&$guest_ram, "elr", elr_ipa);
        dump_guest_bytes_if_mapped(&$guest_ram, "CODE[elr]", elr_el1, 0x20, 0x60);
        dump_translated_guest_bytes(&$guest_ram, "CODE[elr]", elr_ipa, 0x20, 0x60);
        print_translated_instruction_words(
            &$guest_ram,
            "CODE[elr]",
            elr_el1,
            elr_ipa,
            0x20,
            0x60,
        );
        // Timer state: CTL bit0=ENABLE, bit1=IMASK, bit2=ISTATUS(fired).
        let mut tr = [0u64; 4];
        for (i, r) in [0xdf19u16, 0xdf1a, 0xdf11, 0xdf12].iter().enumerate() {
            hv_vcpu_get_sys_reg($vcpu, *r, &mut tr[i]);
        }
        println!(
            "TIMERS: CNTV_CTL={:#x} CNTV_CVAL={:#x} | CNTP_CTL={:#x} CNTP_CVAL={:#x}",
            tr[0], tr[1], tr[2], tr[3]
        );
        let mut voff = 0u64;
        hv_vcpu_get_vtimer_offset($vcpu, &mut voff);
        let mut cntvoff = 0u64;
        hv_vcpu_get_sys_reg($vcpu, 0xe703, &mut cntvoff); // CNTVOFF_EL2
        let hcnt = host_cntvct();
        let guest_cnt = hcnt.wrapping_sub(cntvoff);
        let gap = (tr[1] as i128) - (guest_cnt as i128);
        println!(
        "CNTVCT: host={hcnt:#x} CNTVOFF_EL2={cntvoff:#x} vtimer_off={voff:#x} guest~={guest_cnt:#x}  CVAL={:#x}  gap={gap} ticks (~{} s @24MHz)",
        tr[1],
        gap / 24_000_000
                );

        println!("=== EDK2 boot probe (with Apple hv_gic) ===");
        println!("stop: {}", $stop_reason);
        println!(
            "exits: {} (vtimer {}, psci {}, surplus-canceled {}), last PC: {:#x}", $exits, $vtimer_exits, $psci_calls, $surplus_canceled_exits, $last_pc
        );
        $boot_timer.print_summary($boot_timer_elapsed, $exits, &$secondary_exit_counts);
        $drain_stats.print_summary();
        let last_prerun_pc_ipa = translated_ipa(&$guest_ram, &stage1_ctx, $last_pre_run_pc).ok();
        let last_nonzero_irq_drain_pc_ipa = $drain_stats
            .last_nonzero_pc
            .and_then(|pc| translated_ipa(&$guest_ram, &stage1_ctx, pc).ok());
        WfiWakeSummary {
            $stop_reason: &$stop_reason,
            $stop_reason_code,
            $exits,
            $vtimer_exits,
            final_pc: $last_pc,
            last_prerun_pc: Some($last_pre_run_pc),
            final_pc_observation: wfi_pc_observation(&$guest_ram, pc_ipa),
            last_prerun_pc_observation: wfi_pc_observation(&$guest_ram, last_prerun_pc_ipa),
            last_nonzero_irq_drain_pc_observation: last_nonzero_irq_drain_pc_ipa
                .map(|ipa| wfi_pc_observation(&$guest_ram, Some(ipa))),
        }
        .print(&$drain_stats);
        println!("unmodelled MMIO touched: {:?}", $unimpl);
        print_mmio_traces(&$mmio_traces);
        $recent_pcie_ecam.print();
        $recent_pcie_mmio.print();
        $recent_pcie_pio.print();
        $recent_xhci.print($platform.xhci_event_lifecycle_stats());
        print_hid_semantic_summary($platform);
        if let Some(stats) = $platform.tpm_tis_stats() {
            println!(
                "TPM2 TIS command summary: commands={} success={} errors={} backend_failures={} malformed_commands={} malformed_responses={} last_command={:#010x} clear={} startup={} self_test={} get_capability={} pcr_read={} pcr_extend={} start_auth_session={} create_primary={} read_public={} nv_read_public={} get_random={} other={}",
                stats.commands,
                stats.successful_responses,
                stats.error_responses,
                stats.backend_failures,
                stats.malformed_commands,
                stats.malformed_responses,
                stats.last_command_code.unwrap_or_default(),
                stats.clear_commands,
                stats.startup_commands,
                stats.self_test_commands,
                stats.get_capability_commands,
                stats.pcr_read_commands,
                stats.pcr_extend_commands,
                stats.start_auth_session_commands,
                stats.create_primary_commands,
                stats.read_public_commands,
                stats.nv_read_public_commands,
                stats.get_random_commands,
                stats.other_commands,
            );
        }
        if let Some(stats) = $platform.tpm_ppi_stats() {
            println!(
                "TPM PPI shared-memory summary: reads={} writes={} rejected_accesses={} memory_overwrite_requested={}",
                stats.reads,
                stats.writes,
                stats.rejected_accesses,
                $platform.tpm_memory_overwrite_requested(),
            );
        }
        print_nvme_command_trace($platform);
        println!("UART RX remaining bytes: {}", $platform.uart_input_len());
        for trigger in &$uart_triggers {
            println!(
                "UART RX injection {}: fired={} bytes={}",
                trigger.name(),
                trigger.fired(),
                trigger.bytes_len()
            );
        }
        for trigger in &$xhci_hid_boot_key_triggers {
            trigger.print_summary($platform);
        }
        for trigger in &$xhci_setup_input_triggers {
            trigger.print_summary($platform);
        }
        for trigger in &$xhci_pointer_input_triggers {
            trigger.print_summary($platform);
        }
        if let Some(stats) = $platform.pci_boot_media_stats() {
            print_block_media_stats("PCI boot-media stats", stats);
        }
        if let Some(trace) = $platform.pci_boot_media_request_trace() {
            print_block_request_trace("recent PCI boot-media requests", &trace);
        }
        if let Some(stats) = $platform.virtio_iso_stats() {
            print_block_media_stats("legacy virtio-mmio ISO stats", stats);
        }
        if let Some(stats) = $platform.virtio_net_stats() {
            println!(
                "virtio-net stats: notify={} tx={} rx={} tx_bytes={} rx_bytes={} status={:#x} driver_features={:#x} interrupt_status={:#x} pending_rx={}",
                stats.notify_count,
                stats.tx_count,
                stats.rx_count,
                stats.tx_bytes,
                stats.rx_bytes,
                stats.status,
                stats.driver_features,
                stats.interrupt_status,
                stats.pending_rx_frame,
            );
            for (i, q) in stats.queues.iter().enumerate() {
                println!(
                    "virtio-net queue[{i}]: ready={} size={} desc={:#x} last_avail_idx={} msix_vector={}",
                    q.ready, q.size, q.desc, q.last_avail_idx, q.msix_vector,
                );
            }
        }
        if let Some(stats) = $platform.virtio_net_nat_stats() {
            print_net_nat_stats(stats);
        }
        if let Some(stats) = $platform.virtio_console_stats() {
            const QNAME: [&str; 6] = [
                "port0-rx", "port0-tx", "ctrl-rx", "ctrl-tx", "port1-rx", "port1-tx",
            ];
            println!(
                "virtio-console stats: status={:#x} driver_features={:#x} interrupt_status={:#x} \
                 port1(ready={} guest_open={} host_open={}) agent_confirmed={} \
                 pending_control={} host_to_guest_len={} host_inbound_len={}",
                stats.status,
                stats.driver_features,
                stats.interrupt_status,
                stats.port1_ready,
                stats.port1_guest_open,
                stats.port1_host_open,
                stats.agent_connected_confirmed,
                stats.pending_control,
                stats.host_to_guest_len,
                stats.host_inbound_len,
            );
            for (i, q) in stats.queues.iter().enumerate() {
                // For an RX queue a healthy replenishment loop shows
                // notify/last_avail_seen/used_produced all climbing together;
                // a stall with last_avail_seen > last_avail_idx means the
                // guest posted buffers we failed to consume.
                println!(
                    "virtio-console queue[{i}] {name}: ready={ready} size={size} \
                     notify={notify} avail_seen={seen} last_consumed={consumed} \
                     used_produced={used} rx_no_buffers={nobuf} msix_vector={vec}",
                    name = QNAME[i],
                    ready = q.ready,
                    size = q.size,
                    notify = q.notify_count,
                    seen = q.last_avail_seen,
                    consumed = q.last_avail_idx,
                    used = q.used_produced,
                    nobuf = q.rx_no_buffers,
                    vec = q.msix_vector,
                );
            }
        }
        if let Some(trace) = $platform.virtio_iso_request_trace() {
            print_block_request_trace("recent legacy virtio-mmio ISO requests", &trace);
        }
        let ramfb_config = $platform.ramfb_config();
        let virtio_gpu_scanout = $platform.virtio_gpu_scanout();
        match ramfb_config {
            Some(config) => println!(
                "ramfb config: addr={:#x} fourcc={:#010x} xrgb8888={} {}x{} stride={}",
                config.addr,
                config.fourcc,
                config.is_xrgb8888(),
                config.width,
                config.height,
                config.stride
            ),
            None => println!("ramfb config: inactive"),
        }
        ramfb_dump::print_and_dump_with_virtio_gpu(
            virtio_gpu_scanout,
            ramfb_config,
            &$guest_ram,
        );
        println!("symbol lines: {}", symbols.len());
        for line in symbols.iter().rev().take(8).rev() {
            println!("{line}");
        }
        if $redist_hi != 0 {
            println!(
                "gic-redist IPA range: {:#x}..={:#x} (redist base {:#x}, frame0 ends {:#x})",
                $redist_lo,
                $redist_hi,
                machine::GIC_REDIST.base,
                machine::GIC_REDIST.base + 0x20000
            );
        }
        println!("serial bytes: {}", serial.len());
        println!(
            "--- serial (tail) ---\n{}\n--- end ---",
            String::from_utf8_lossy(&serial)
        );
    }};
}

pub(crate) use persist_and_report_stop;
