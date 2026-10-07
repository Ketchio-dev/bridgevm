//! Share-relative path keys and their guest (backslash) wire form.

/// Internal share keys are relative paths with forward slashes. Host joins on
/// macOS accept '/', while Windows guest paths are converted at the wire edge.
pub fn normalize_rel(name: &str) -> String {
    normalize_rel_with_sep(name, '/')
}

#[cfg(test)]
pub fn to_guest_rel(name: &str) -> String {
    normalize_rel_with_sep(name, '\\')
}

pub fn append_guest_rel_into(name: &str, out: &mut String) {
    append_rel_with_sep_into(name, '\\', out);
}

/// Convert a guest listing path to a share key. A `..` component would let a
/// guest-chosen key name a host path outside the share root once joined to it,
/// so such entries are refused. Separators are already collapsed, so leading
/// slashes cannot make the key absolute.
pub fn from_guest_rel(name: &str) -> Option<String> {
    if name.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    Some(normalize_rel(name))
}

fn normalize_rel_with_sep(name: &str, sep: char) -> String {
    let mut out = String::with_capacity(name.len());
    normalize_rel_with_sep_into(name, sep, &mut out);
    out
}

fn normalize_rel_with_sep_into(name: &str, sep: char, out: &mut String) {
    out.clear();
    append_rel_with_sep_into(name, sep, out);
}

fn append_rel_with_sep_into(name: &str, sep: char, out: &mut String) {
    out.reserve(name.len());
    let mut wrote_part = false;
    for part in name.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if wrote_part {
            out.push(sep);
        }
        out.push_str(part);
        wrote_part = true;
    }
}
