//! Escaping and normalization of individual QEMU option values.

/// Escape a value interpolated into a comma-delimited QEMU option string (e.g.
/// `-drive file=...`, `-chardev socket,path=...`). QEMU parses these option
/// strings on commas, so a literal comma in a (manifest-derived) path must be
/// doubled (`,,`) or it would inject additional QEMU options.
pub(crate) fn escape_qemu_opt(value: impl std::fmt::Display) -> String {
    value.to_string().replace(',', ",,")
}

pub(crate) fn memory_arg(value: &str) -> String {
    if value == "auto" {
        "4096".to_string()
    } else if value.ends_with("GiB") {
        value
            .trim_end_matches("GiB")
            .parse::<u64>()
            // checked_mul: a huge GiB value would otherwise panic (debug) or wrap
            // (release) into a garbage -m argument. On overflow, pass through.
            .ok()
            .and_then(|gib| gib.checked_mul(1024))
            .map(|mib| mib.to_string())
            .unwrap_or_else(|| value.to_string())
    } else if value.ends_with("MiB") {
        value.trim_end_matches("MiB").to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn cpu_arg(value: &str) -> String {
    if value == "auto" {
        "2".to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn display_arg(renderer: &str) -> &'static str {
    match renderer {
        "spice" => "default,show-cursor=on",
        "spice-or-vnc" => "vnc=:0",
        "vnc" => "vnc=:0",
        "metal-adapter-preferred" => "cocoa,gl=on",
        _ => "default,show-cursor=on",
    }
}
