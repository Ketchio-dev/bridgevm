//! Preserve genuine failure facts while distinguishing expected cancellation I/O.
use bridgevm_hvf_runtime::RuntimeError;
pub(super) fn failure(
    error: &anyhow::Error,
    cancelled: bool,
    helper_started: bool,
) -> Option<&'static str> {
    if let Some(RuntimeError::Io { source, .. }) = error.downcast_ref::<RuntimeError>() {
        if cancelled && source.kind() == std::io::ErrorKind::Interrupted {
            return None;
        }
    }
    match error.downcast_ref::<RuntimeError>() {
        Some(RuntimeError::RestartRefused { .. }) => Some("resetFailed"),
        Some(RuntimeError::Io { context, .. })
            if context.contains("receipt") || context.contains("flush") =>
        {
            Some("resetFailed")
        }
        Some(RuntimeError::Io {
            context: "VM helper failed" | "wait for VM helper",
            ..
        }) => Some("helperFailed"),
        _ if helper_started => Some("helperFailed"),
        _ => Some("startupFailed"),
    }
}
