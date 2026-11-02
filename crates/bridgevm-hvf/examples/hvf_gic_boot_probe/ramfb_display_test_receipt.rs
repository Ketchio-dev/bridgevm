//! Test access to the existing completion instant recorded after publication.

impl super::super::RamfbDisplayTiming {
    pub(in super::super::super) fn publication_receipt(
        &self,
    ) -> (u64, u64, Option<std::time::Instant>) {
        (self.ticks, self.published, self.last_published)
    }
}
