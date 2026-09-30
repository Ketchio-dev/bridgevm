//! Guest-derived text rendered for single-line host report records.

use std::fmt::Write as _;

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
    use super::{escape_guest_text, symbol_report_lines};
    use crate::symbol_lines;

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
