//! Keep directory cleanup facts separate from direct-child reap evidence.
use crate::{DirectoryDisposition, RuntimeControl, RuntimeError, RuntimeLifecycleEvent};
use std::path::Path;
pub(super) fn remove_runtime_dir(
    path: &Path,
    control: &RuntimeControl<'_>,
) -> Result<(), RuntimeError> {
    let result = std::fs::remove_dir_all(path);
    let disposition = match &result {
        Ok(()) => DirectoryDisposition::Removed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DirectoryDisposition::Removed,
        Err(_) => DirectoryDisposition::RemoveFailed,
    };
    control.observe(RuntimeLifecycleEvent::SwtpmDirectory(disposition));
    if disposition == DirectoryDisposition::RemoveFailed {
        return Err(RuntimeError::Io {
            context: "remove owned swtpm runtime dir",
            source: result.unwrap_err(),
        });
    }
    Ok(())
}
