//! Captured runtime owner and once-per-raw-slot source lifetime pins.
use super::*;

pub struct RuntimeLease {
    pub(super) _pair: Option<LockedPair>,
    pub(super) _logical: MediaLease,
    pub(super) policies: Policies,
    pub(super) retained: BTreeSet<PathBuf>,
    pub(super) raw_source_pinned: [bool; 2],
}

impl RuntimeLease {
    pub(super) fn new(media: &VirtBootMediaConfig) -> io::Result<Self> {
        Ok(Self {
            _pair: None,
            _logical: MediaLease::acquire([])?,
            policies: Policies::capture(media),
            retained: BTreeSet::new(),
            raw_source_pinned: [false; 2],
        })
    }
}
