#[path = "xhci_hid_input/boot_key.rs"]
mod boot_key;
#[path = "xhci_hid_input/hid_semantic_summary.rs"]
mod hid_semantic_summary;
#[path = "xhci_hid_input/marker.rs"]
mod marker;
#[path = "xhci_hid_input/pointer_input.rs"]
mod pointer_input;
#[path = "xhci_hid_input/report_text.rs"]
mod report_text;
#[path = "xhci_hid_input/setup_input.rs"]
mod setup_input;

#[cfg(test)]
#[path = "xhci_hid_input/boot_key_tests.rs"]
mod boot_key_tests;
#[cfg(test)]
#[path = "xhci_hid_input/marker_tests.rs"]
mod marker_tests;
#[cfg(test)]
#[path = "xhci_hid_input/pointer_input_tests.rs"]
mod pointer_input_tests;
#[cfg(test)]
#[path = "xhci_hid_input/setup_input_action_tests.rs"]
mod setup_input_action_tests;
#[cfg(test)]
#[path = "xhci_hid_input/setup_input_delay_parse_tests.rs"]
mod setup_input_delay_parse_tests;
#[cfg(test)]
#[path = "xhci_hid_input/setup_input_env_tests.rs"]
mod setup_input_env_tests;
#[cfg(test)]
#[path = "xhci_hid_input/setup_input_fire_delay_tests.rs"]
mod setup_input_fire_delay_tests;
#[cfg(test)]
#[path = "xhci_hid_input/setup_input_memory_drain_tests.rs"]
mod setup_input_memory_drain_tests;
#[cfg(test)]
#[path = "xhci_hid_input/setup_input_ramfb_tests.rs"]
mod setup_input_ramfb_tests;
#[cfg(test)]
#[path = "xhci_hid_input/test_support.rs"]
mod test_support;

pub(crate) use boot_key::XhciHidBootKeyTrigger;
pub(crate) use hid_semantic_summary::print_hid_semantic_summary;
pub(crate) use pointer_input::{print_pointer_input_rejection, XhciPointerInputTrigger};
pub(crate) use setup_input::{
    print_setup_input_rejection, SetupInputHostWake, XhciSetupInputTrigger,
};
