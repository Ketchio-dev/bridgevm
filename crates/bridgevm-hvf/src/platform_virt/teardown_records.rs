//! Host records the platform prints once, as it is dropped.
//!
//! The probe drops the platform after the final report's footer, which is
//! where the HDA controller prints its diagnostic records
//! (hda/diagnostics_teardown.rs). A host whose own records must also follow
//! the footer, such as the ramfb display exporter the probe joins before the
//! report, installs them here. They print from the platform's last field, so
//! after every device, the HDA controller's records and its CoreAudio sink's
//! teardown records included: one post-footer host tail, which
//! scripts/live-gates/hvf_host_tail.py admits. Without a host nothing prints.
//! Resets keep the records. Printing happens after the guest stopped running
//! and touches no device state.

use super::VirtPlatform;

/// Records a host prints as the platform drops; read once, at drop.
pub type HostTeardownRecords = Box<dyn Fn() -> Vec<String> + Send>;

/// The platform's last field, so that it drops, and prints, after every device.
#[derive(Default)]
pub(crate) struct TeardownRecords(Option<HostTeardownRecords>);

impl std::fmt::Debug for TeardownRecords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TeardownRecords")
            .field(&self.0.is_some())
            .finish()
    }
}

impl Drop for TeardownRecords {
    fn drop(&mut self) {
        for record in self.0.as_ref().map(|records| records()).unwrap_or_default() {
            println!("{record}");
        }
    }
}

// The probe shares the platform across vCPU threads, host records included.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<crate::platform_virt::VirtPlatform>();
};

impl VirtPlatform {
    /// Install the host's records, printed once as the platform drops.
    pub fn set_teardown_host_records(&mut self, records: HostTeardownRecords) {
        self.teardown_records.0 = Some(records);
    }

    /// What the platform would print as it drops: the host's records, if any.
    pub fn teardown_host_records(&self) -> Vec<String> {
        self.teardown_records
            .0
            .as_ref()
            .map(|records| records())
            .unwrap_or_default()
    }
}
