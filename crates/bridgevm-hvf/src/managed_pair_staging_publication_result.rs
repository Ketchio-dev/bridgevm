//! Keep ownership across a failed publish and retire only confirmed current metadata.

use super::super::StageOwner;
use std::io;

impl StageOwner {
    pub(super) fn finish_publication(&self, result: io::Result<()>) -> io::Result<()> {
        result.inspect_err(|_| self.clear())?;
        if self.record.is_some() {
            super::super::super::recovery::retire_current(self.path.parent().unwrap())?;
        }
        Ok(())
    }
}
