//! Clipboard normalization and synchronization decisions.

/// Fold CRLF to LF (the host/macOS convention). `last_synced` always stores this
/// form so the same content in either line-ending compares equal and doesn't
/// trigger a redundant re-sync.
pub(super) fn normalize_clip(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Convert to the guest/Windows CRLF convention WITHOUT doubling existing CRLFs:
/// normalize to LF first, then expand every LF. (A naive \n -> \r\n over text
/// that already had \r\n would yield \r\r\n.)
#[cfg(test)]
pub(super) fn to_guest_crlf(s: &str) -> String {
    let mut out = String::with_capacity(to_guest_crlf_len(s));
    to_guest_crlf_into(s, &mut out);
    out
}

pub(super) fn to_guest_crlf_into(s: &str, out: &mut String) {
    out.clear();
    out.reserve(to_guest_crlf_len(s));
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' && chars.peek() == Some(&'\n') {
            out.push_str("\r\n");
            let _ = chars.next();
        } else if ch == '\n' {
            out.push_str("\r\n");
        } else {
            out.push(ch);
        }
    }
}

pub(super) fn to_guest_crlf_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut len = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            len += 2;
            index += 2;
        } else if bytes[index] == b'\n' {
            len += 2;
            index += 1;
        } else {
            len += 1;
            index += 1;
        }
    }
    len
}

/// Decide whether a guest clipboard snapshot should be adopted host-side.
/// Returns the normalized (LF) text to store/apply, or None when it is empty or
/// already equal (normalized) to what we last synced.
pub(super) fn guest_clip_decision(last_synced: &Option<String>, text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let normalized = normalize_clip(text);
    if last_synced.as_deref() == Some(normalized.as_str()) {
        return None;
    }
    Some(normalized)
}
