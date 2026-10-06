//! Refuse cross-slot outputs before they can replace another media generation.
use super::*;
#[derive(Clone, Copy, Debug)]
pub enum RuntimeMediaSlot {
    Vars,
    Primary,
    Target,
}
pub(super) struct Policies {
    pub(super) slots: [Option<WritableMedia>; 3],
    originals: [Option<PathBuf>; 3],
}
impl Policies {
    pub(super) fn capture(media: &VirtBootMediaConfig) -> Self {
        Self {
            slots: [None, None, None],
            originals: [
                Some(media.flash_vars.path.clone()),
                media.nvme_disk.as_ref().map(|m| m.path.clone()),
                media.nvme_target.as_ref().map(|m| m.path.clone()),
            ],
        }
    }
    pub(super) fn slot(&self, slot: RuntimeMediaSlot) -> io::Result<WritableMedia> {
        self.validate()?;
        self.slots[slot as usize]
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "runtime media slot absent"))
    }
    pub(super) fn validate(&self) -> io::Result<()> {
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(slot) = slot else { continue };
            for output in slot
                .snapshot_path
                .iter()
                .chain(slot.write_back.then_some(&slot.path))
            {
                for other in 0..self.slots.len() {
                    if other == index {
                        continue;
                    }
                    let input = self.slots[other].as_ref();
                    for path in self.originals[other]
                        .iter()
                        .chain(input.map(|s| &s.path))
                        .chain(input.and_then(|s| s.snapshot_path.as_ref()))
                    {
                        if identity::same(output, path)? {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "runtime media output overlaps another slot",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
#[path = "managed_pair_runtime_output_identity.rs"]
mod identity;
#[cfg(test)]
#[path = "managed_pair_runtime_overlap_tests.rs"]
mod tests;

pub(super) fn capture(
    mut lease: RuntimeLease,
    media: &VirtBootMediaConfig,
) -> io::Result<RuntimeLease> {
    lease.policies.slots = [
        Some(media.flash_vars.clone()),
        media.nvme_disk.clone(),
        media.nvme_target.clone(),
    ];
    if media.nvme_disk.is_some() || media.nvme_target.is_some() {
        for slot in lease.policies.slots.iter().flatten() {
            lease.retained.insert(fs::canonicalize(&slot.path)?);
        }
    }
    if let Some(pair) = &lease._pair {
        lease
            .retained
            .extend([pair.disk.clone(), pair.vars.clone()]);
    }
    lease.policies.validate()?;
    Ok(lease)
}
