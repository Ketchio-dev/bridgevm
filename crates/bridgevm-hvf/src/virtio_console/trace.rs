//! The console_trace! macro and its env-gated enable check.

macro_rules! console_trace {
    ($($arg:tt)*) => {
        if console_trace_enabled() {
            eprintln!("[vcon] {}", format_args!($($arg)*));
        }
    };
}

/// Whether the env-gated control-plane trace is on. Read once; when off the
/// per-event trace sites collapse to a single cached bool check.
pub(crate) fn console_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_VIRTIO_CONSOLE_TRACE")
                .as_deref()
                .map(str::trim),
            Ok("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
        )
    })
}
