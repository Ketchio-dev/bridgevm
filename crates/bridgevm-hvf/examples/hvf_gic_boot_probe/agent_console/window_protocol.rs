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
        if line == "WINEND" {
            println!("BVAGENT WINLIST WINEND");
            return Some(ReplyProgress::Complete);
        }
        if line.starts_with("WIN ") {
            if valid_window_record(line) {
                println!("BVAGENT WINLIST {line}");
            } else {
                println!("BVAGENT WINLIST malformed={line}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winlist_stays_in_flight_until_terminator() {
        let title = base64_encode("Untitled - Notepad".as_bytes());
        let record = format!("WIN 42 7 50 60 700 500 {title}");
        assert!(matches!(handle_window_reply(&record, "WINLIST"), Some(ReplyProgress::Incomplete)));
        assert!(matches!(handle_window_reply("WINEND", "WINLIST"), Some(ReplyProgress::Complete)));
        assert!(matches!(handle_window_reply("WIN nope", "WINLIST"), Some(ReplyProgress::Incomplete)));
    }

    #[test]
    fn mutations_require_the_matching_reply() {
        for verb in ["WINBOUNDS", "WINFOCUS", "WINCLOSE"] {
            assert!(matches!(
                handle_window_reply(&format!("OK {verb}"), verb),
                Some(ReplyProgress::Complete)
            ));
        }
        assert!(matches!(
            handle_window_reply("OK WINFOCUS", "WINCLOSE"),
            Some(ReplyProgress::Ignored)
        ));
    }
}
