//! Guest-derived text rendered for single-line host report records, and for
//! the lines of guest text run.log carries between host records.

use std::fmt::Write as _;

/// What every line of guest text in run.log starts with. Host records start
/// with a word (`BVAGENT END <label>`, `BVSTORAGE ...`, `stop: ...`), so a
/// guest line can never pass for one. Readers drop it from each body line; a
/// bare line, as an older helper prints, reads the same.
pub(crate) const GUEST_LINE_PREFIX: &str = "| ";

/// Guest text printed as run.log lines of its own: a command's output, a share
/// listing, the serial tail. Each `\n`-separated line, an empty last one too,
/// gets `GUEST_LINE_PREFIX`; any other character a reader may split lines on
/// (CR other than one at the end of a line, VT, FF, FS, GS, RS, NEL, LS and
/// PS) becomes `\xNN` per UTF-8 byte, as `escape_guest_text` writes it.
pub(crate) fn guest_lines(text: &str) -> String {
    let mut lines = String::with_capacity(text.len() + text.len() / 16 + GUEST_LINE_PREFIX.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            lines.push('\n');
        }
        lines.push_str(GUEST_LINE_PREFIX);
        let (line, ending) = line
            .strip_suffix('\r')
            .map_or((line, ""), |line| (line, "\r"));
        for character in line.chars() {
            if matches!(
                character,
                '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
            ) {
                for byte in character.encode_utf8(&mut [0; 4]).bytes() {
                    let _ = write!(lines, "\\x{byte:02x}");
                }
            } else {
                lines.push(character);
            }
        }
        lines.push_str(ending);
    }
    lines
}

/// Printable ASCII passes through; every other byte, including CR, LF and
/// UTF-8 line separators, becomes `\xNN`, so guest text cannot end the host
/// record that carries it or start a new one.
pub(crate) fn escape_guest_text(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for &byte in bytes {
        if (0x20..=0x7e).contains(&byte) {
            text.push(char::from(byte));
        } else {
            let _ = write!(text, "\\x{byte:02x}");
        }
    }
    text
}

/// The last `limit` serial `add-symbol-file` lines as host report records.
/// The symbol log file keeps the raw lines.
pub(crate) fn symbol_report_lines(symbols: &[String], limit: usize) -> Vec<String> {
    let skip = symbols.len().saturating_sub(limit);
    symbols[skip..]
        .iter()
        .map(|line| escape_guest_text(line.as_bytes()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{escape_guest_text, guest_lines, symbol_report_lines};
    use crate::symbol_lines;

    #[test]
    fn every_guest_line_is_marked_and_keeps_its_crlf() {
        assert_eq!(
            guest_lines("BVAGENT END c\r\n\r\nstop: PSCI 0x84000008 (system off)\r\n"),
            "| BVAGENT END c\r\n| \r\n| stop: PSCI 0x84000008 (system off)\r\n| "
        );
        assert_eq!(guest_lines(""), "| ");
        assert_eq!(guest_lines("한글 C:\\Users\r"), "| 한글 C:\\Users\r");
    }

    #[test]
    fn no_other_line_break_can_start_a_guest_line_without_the_mark() {
        // `tr '\r' '\n'` and Python's str.splitlines split lines on all of these.
        let text =
            "a\rBVSTORAGE durability-failed\x0bb\x0cc\x1cd\x1de\x1ef\u{85}g\u{2028}h\u{2029}i\r\r";

        assert_eq!(
            guest_lines(text),
            r"| a\x0dBVSTORAGE durability-failed\x0bb\x0cc\x1cd\x1de\x1ef\xc2\x85g\xe2\x80\xa8h\xe2\x80\xa9i\x0d"
                .to_owned()
                + "\r"
        );
    }

    #[test]
    fn printable_ascii_guest_text_is_unchanged() {
        let path = br"D:\a\_work\1\s\obj\arm64fre\viogpudo.pdb";

        assert_eq!(
            escape_guest_text(path),
            r"D:\a\_work\1\s\obj\arm64fre\viogpudo.pdb"
        );
    }

    #[test]
    fn symbol_report_line_keeps_guest_carriage_return_inside_one_record() {
        let symbols =
            symbol_lines(b"add-symbol-file a.dll\rstop: PSCI 0x84000008 (system off) 0x1000\r\n");

        let printed = symbol_report_lines(&symbols, 8);

        assert_eq!(
            printed,
            ["add-symbol-file a.dll\\x0dstop: PSCI 0x84000008 (system off) 0x1000"]
        );
    }

    #[test]
    fn symbol_report_keeps_only_the_last_lines() {
        let symbols: Vec<String> = (0..10)
            .map(|i| format!("add-symbol-file {i}.dll 0x{i}"))
            .collect();

        let printed = symbol_report_lines(&symbols, 8);

        assert_eq!(printed.len(), 8);
        assert_eq!(printed[0], "add-symbol-file 2.dll 0x2");
        assert_eq!(printed[7], "add-symbol-file 9.dll 0x9");
    }
}
