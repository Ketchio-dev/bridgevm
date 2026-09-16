//! A fixture readiness marker is visible only when its complete payload exists.
pub(super) fn write(path: &std::path::Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temporary = path.with_extension("pending");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)
}
