//! Line framing, base64, and command wire encoding.

#[derive(Default)]
pub(super) struct LineFramer {
    pub(super) pending: Vec<u8>,
    pub(super) discarding_oversized_line: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Base64DecodeError {
    Byte,
    Length,
    Padding,
}

#[cfg(test)]
pub(super) fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    base64_encode_into(bytes, &mut out);
    out
}

pub(super) fn base64_encode_into(bytes: &[u8], out: &mut String) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() >= 2 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() == 3 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
}

pub(super) fn base64_decode(text: &str) -> Result<Vec<u8>, Base64DecodeError> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Base64DecodeError::Length);
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut saw_padding = false;
    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        let mut padding = 0usize;
        for (i, byte) in chunk.iter().copied().enumerate() {
            match byte {
                b'A'..=b'Z' if !saw_padding => vals[i] = byte - b'A',
                b'a'..=b'z' if !saw_padding => vals[i] = byte - b'a' + 26,
                b'0'..=b'9' if !saw_padding => vals[i] = byte - b'0' + 52,
                b'+' if !saw_padding => vals[i] = 62,
                b'/' if !saw_padding => vals[i] = 63,
                b'=' => {
                    saw_padding = true;
                    padding += 1;
                    if i < 2 {
                        return Err(Base64DecodeError::Padding);
                    }
                }
                _ => return Err(Base64DecodeError::Byte),
            }
        }
        if padding > 2 {
            return Err(Base64DecodeError::Padding);
        }
        if padding > 0 && chunk[3] != b'=' {
            return Err(Base64DecodeError::Padding);
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if padding < 2 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if padding == 0 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Ok(out)
}

/// Verbs the guest agent handles directly (not shell commands). These are sent
/// to the agent verbatim; everything else is wrapped as `RUN <base64>`.
pub(super) fn is_raw_verb(token: &str) -> bool {
    matches!(
        token,
        "CLIPGET"
            | "CLIPSET"
            | "LS"
            | "LSR"
            | "GET"
            | "PUT"
            | "PUTBEG"
            | "PUTCHUNK"
            | "PUTEND"
            | "DEL"
            | "PING"
    )
}

/// Build the wire line for a command string using the scripted-command rule: a
/// protocol verb (CLIPGET/CLIPSET/LS/GET/PUT/PING) is sent verbatim; anything
/// else is a shell line wrapped as `RUN <base64(cmd)>`. This lets both
/// BRIDGEVM_VIRTIO_CONSOLE_CMDS and the control file drive clipboard/file verbs
/// directly, e.g. "CLIPSET <b64>" or "CLIPGET".
pub(super) fn command_wire_line(command: &str) -> String {
    let mut line = String::new();
    write_command_wire_line_into(command, &mut line);
    line
}

pub(super) fn write_command_wire_line_into(command: &str, out: &mut String) {
    let first = command.split_whitespace().next().unwrap_or("");
    if is_raw_verb(first) {
        out.push_str(command);
        out.push('\n');
    } else {
        out.push_str("RUN ");
        base64_encode_into(command.as_bytes(), out);
        out.push('\n');
    }
}

pub(super) fn parse_out_line(line: &str) -> Option<(i32, &str)> {
    let rest = line.strip_prefix("OUT ")?;
    let (exit_code, output) = rest.split_once(' ')?;
    Some((exit_code.parse().ok()?, output))
}
