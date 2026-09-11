//! Reply framing for the guest-window control verbs.

use super::*;

fn window_verb(command: &str) -> Option<&str> {
    match command.split_whitespace().next()? {
        verb @ ("WINLIST" | "WINBOUNDS" | "WINFOCUS" | "WINCLOSE") => Some(verb),
        _ => None,
    }
}

fn valid_window_record(line: &str) -> bool {
    let fields: Vec<_> = line.split_whitespace().collect();
    fields.len() == 8
        && fields[0] == "WIN"
        && fields[1].parse::<u64>().is_ok()
        && fields[2].parse::<u32>().is_ok()
        && fields[3].parse::<i32>().is_ok()
        && fields[4].parse::<i32>().is_ok()
        && fields[5].parse::<u32>().is_ok()
        && fields[6].parse::<u32>().is_ok()
        && base64_decode(fields[7]).is_ok()
}

pub(super) fn handle_window_reply(line: &str, command: &str) -> Option<ReplyProgress> {
    let verb = window_verb(command)?;
    if verb == "WINLIST" {
        let label = window_list_label(command);
        if line == "WINEND" {
            println!("BVAGENT {label} WINEND");
            return Some(ReplyProgress::Complete);
        }
        if line.starts_with("WIN ") {
            if valid_window_record(line) {
                println!("BVAGENT {label} {line}");
            } else {
                println!("BVAGENT {label} malformed={line}");
            }
            return Some(ReplyProgress::Incomplete);
        }
        if line == "ERR WINLIST" || line.starts_with("ERR WINLIST ") {
            println!("BVAGENT {command} -> {line}");
            return Some(ReplyProgress::Complete);
        }
        return Some(ReplyProgress::Ignored);
    }
    if line == format!("OK {verb}")
        || line == format!("ERR {verb}")
        || line.starts_with(&format!("ERR {verb} "))
    {
        println!("BVAGENT {command} -> {line}");
        Some(ReplyProgress::Complete)
    } else {
        Some(ReplyProgress::Ignored)
    }
}

pub(super) fn window_request_id(command: &str) -> Option<&str> {
    let id = command.strip_prefix("WINLIST ")?;
    if id.len() != 36 {
        return None;
    }
    id.bytes().enumerate().all(|(index, byte)| {
        if [8, 13, 18, 23].contains(&index) { byte == b'-' } else { byte.is_ascii_hexdigit() }
    }).then_some(id)
}

fn window_list_label(command: &str) -> String {
    window_request_id(command).map(|id| format!("WINLIST {id}"))
        .unwrap_or_else(|| "WINLIST".to_string())
}

#[cfg(test)]
#[path = "window_protocol_tests.rs"]
mod tests;
