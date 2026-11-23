//! What each boot generation reads from the environment: the scripted UART and
//! xHCI input triggers, and the sizes of vCPU0's MMIO trace rings.

use crate::*;

/// This boot's scripted input, each trigger fired by a prompt or serial marker.
pub(super) struct BootTriggers {
    pub(super) uart: Vec<SerialTriggeredUartInput>,
    pub(super) xhci_hid_boot_key: Vec<XhciHidBootKeyTrigger>,
    pub(super) xhci_setup_input: Vec<XhciSetupInputTrigger>,
    pub(super) xhci_pointer_input: Vec<XhciPointerInputTrigger>,
}

impl BootTriggers {
    /// Reads and reports the triggers in the probe's established order.
    pub(super) fn from_env() -> Self {
        let mut uart = Vec::new();
        uart.extend(SerialTriggeredUartInput::from_env(
            "cd-prompt",
            "BRIDGEVM_UART_RX_ON_CD_PROMPT",
            b"Press any key to boot from CD or DVD",
        ));
        uart.extend(SerialTriggeredUartInput::from_env_with_marker_env(
            "serial-marker",
            "BRIDGEVM_UART_RX_ON_SERIAL_MARKER",
            "BRIDGEVM_UART_RX_SERIAL_MARKER",
        ));
        let mut xhci_hid_boot_key = Vec::new();
        xhci_hid_boot_key.extend(XhciHidBootKeyTrigger::from_env(
            "cd-prompt",
            "BRIDGEVM_XHCI_BOOT_KEY_ON_CD_PROMPT",
        ));
        xhci_hid_boot_key.extend(XhciHidBootKeyTrigger::from_env_with_marker_env(
            "serial-marker",
            "BRIDGEVM_XHCI_BOOT_KEY_ON_SERIAL_MARKER",
            "BRIDGEVM_XHCI_BOOT_KEY_SERIAL_MARKER",
        ));
        let mut xhci_setup_input = Vec::new();
        admit(
            &mut xhci_setup_input,
            XhciSetupInputTrigger::from_env(
                "setup-input",
                "BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT_SERIAL_MARKER",
            ),
            |error| print_setup_input_rejection("setup-input", error),
        );
        admit(
            &mut xhci_setup_input,
            XhciSetupInputTrigger::from_env_with_timing_envs(
                "setup-input-2",
                "BRIDGEVM_XHCI_SETUP_INPUT2_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT2_SERIAL_MARKER",
                "BRIDGEVM_XHCI_SETUP_INPUT2_FIRE_DELAY_MS",
                "BRIDGEVM_XHCI_SETUP_INPUT2_RAMFB_DELAY_MS",
            ),
            |error| print_setup_input_rejection("setup-input-2", error),
        );
        admit(
            &mut xhci_setup_input,
            XhciSetupInputTrigger::from_env_with_timing_envs(
                "setup-input-3",
                "BRIDGEVM_XHCI_SETUP_INPUT3_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT3_SERIAL_MARKER",
                "BRIDGEVM_XHCI_SETUP_INPUT3_FIRE_DELAY_MS",
                "BRIDGEVM_XHCI_SETUP_INPUT3_RAMFB_DELAY_MS",
            ),
            |error| print_setup_input_rejection("setup-input-3", error),
        );
        let mut xhci_pointer_input = Vec::new();
        admit(
            &mut xhci_pointer_input,
            XhciPointerInputTrigger::from_env(
                "pointer-input",
                "BRIDGEVM_XHCI_POINTER_INPUT_ACTIONS",
                "BRIDGEVM_XHCI_POINTER_INPUT_SERIAL_MARKER",
            ),
            |error| print_pointer_input_rejection("pointer-input", error),
        );
        Self {
            uart,
            xhci_hid_boot_key,
            xhci_setup_input,
            xhci_pointer_input,
        }
    }

    /// Any trigger at all: the automation pass then runs after every exit.
    pub(super) fn any(&self) -> bool {
        !self.uart.is_empty()
            || !self.xhci_hid_boot_key.is_empty()
            || !self.xhci_setup_input.is_empty()
            || !self.xhci_pointer_input.is_empty()
    }
}

/// A configured trigger joins `triggers`; a rejected one is reported.
fn admit<T, E>(triggers: &mut Vec<T>, configured: Option<Result<T, E>>, reject: impl FnOnce(&E)) {
    match configured {
        Some(Ok(trigger)) => triggers.push(trigger),
        Some(Err(error)) => reject(&error),
        None => {}
    }
}

/// What vCPU0's MMIO exits record for the stop report.
pub(super) struct ExitTraces {
    pub(super) mmio_traces: BTreeMap<&'static str, MmioTrace>,
    pub(super) recent_pcie_mmio: RecentMmio,
    pub(super) recent_pcie_pio: RecentMmio,
    pub(super) recent_pcie_ecam: RecentPcieEcam,
    pub(super) recent_xhci: XhciBringupTrace,
    /// Unmodelled MMIO by device, and the GIC redistributor range it touched.
    pub(super) unimpl: BTreeMap<&'static str, u64>,
    pub(super) redist_lo: u64,
    pub(super) redist_hi: u64,
    pub(super) fwcfg_trace_count: u32,
}

impl ExitTraces {
    pub(super) fn from_env(trace_xhci_bringup: bool) -> Self {
        let recent_pcie_mmio = RecentMmio::new(
            "pcie-mmio-32",
            ring_len("BRIDGEVM_RECENT_PCIE_MMIO", 32),
        );
        let recent_pcie_pio = RecentMmio::new("pcie-pio", ring_len("BRIDGEVM_RECENT_PCIE_PIO", 32));
        let recent_pcie_ecam = RecentPcieEcam::new(ring_len("BRIDGEVM_RECENT_PCIE_ECAM", 128));
        let mut recent_xhci = XhciBringupTrace::new(ring_len("BRIDGEVM_RECENT_XHCI", 160));
        recent_xhci.print_events_immediately(trace_xhci_bringup);
        Self {
            mmio_traces: BTreeMap::new(),
            recent_pcie_mmio,
            recent_pcie_pio,
            recent_pcie_ecam,
            recent_xhci,
            unimpl: BTreeMap::new(),
            redist_lo: u64::MAX,
            redist_hi: 0,
            fwcfg_trace_count: 0,
        }
    }
}

/// A trace ring's length from `env`, else `default`.
fn ring_len(env: &str, default: u16) -> usize {
    usize::try_from(env_u64(env, u64::from(default))).unwrap_or(usize::from(default))
}
