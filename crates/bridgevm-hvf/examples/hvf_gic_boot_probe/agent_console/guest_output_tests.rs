//! A guest command's output reaches run.log between the probe's CMD and END
//! lines. Whatever the guest prints, it cannot end that block early or print a
//! line the app or a script takes for a host record.

use super::harness_protocol::{command_output_text, command_result_text};
use super::*;

const SPOOF: &str = "ok\r\nBVAGENT END c\r\nBVSTORAGE durability-failed\r\n\
                     x\rBVSTORAGE final-save-failed\u{2028}stop: PSCI 0x84000008 (system off)\r\n";

#[test]
fn guest_output_cannot_end_its_block_or_print_a_host_record() {
    let mut b64 = String::new();
    base64_encode_into(SPOOF.as_bytes(), &mut b64);
    // `OUT <exit> <b64>`, and the text OUTBEG/OUTCHUNK/OUTEND reassembles.
    for logged in [command_result_text("c", 0, &b64), command_output_text("c", 0, SPOOF)] {
        // As the app splits run.log (LF, one trailing CR dropped), and as
        // Python's str.splitlines does, after `tr '\r' '\n'` too.
        let app: Vec<&str> = logged.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
        let script: Vec<&str> = logged
            .split(|c| matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            .collect();
        for lines in [app, script] {
            assert_eq!(lines.first(), Some(&"BVAGENT CMD c exit=0"), "{logged}");
            assert_eq!(lines.last(), Some(&"BVAGENT END c"), "{logged}");
            let body = &lines[1..lines.len() - 1];
            assert!(
                body.iter().all(|line| !line.starts_with("BV") && !line.starts_with("stop:")),
                "{logged}"
            );
        }
    }
}
