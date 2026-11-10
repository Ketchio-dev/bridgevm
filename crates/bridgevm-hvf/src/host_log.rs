//! Lines for the host log: the VMM's stdout and stderr, which the app reads
//! as `<bundle>/logs/hvf/run.log`, a regular file in the same VM bundle as the
//! disk image (bridgevm-hvf-runtime vm_process.rs). `println!` panics when its
//! write fails, so when that volume fills, a line printed because it filled
//! would end the VMM instead of reporting the disk-full pause. These write and
//! drop whatever does not fit.

use std::fmt::Display;
use std::io::{self, Write};

/// `println!` that drops the line when stdout cannot take it.
pub fn out(line: impl Display) {
    write_line(&mut io::stdout().lock(), line);
}

/// `eprintln!` that drops the line when stderr cannot take it.
pub fn err(line: impl Display) {
    write_line(&mut io::stderr().lock(), line);
}

fn write_line(out: &mut impl Write, line: impl Display) {
    let _ = writeln!(out, "{line}").and_then(|()| out.flush());
}

#[cfg(test)]
mod tests {
    use std::io;

    /// A volume with no space left.
    struct Full;

    impl io::Write for Full {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        }
    }

    #[test]
    fn a_line_the_full_volume_cannot_take_is_dropped() {
        super::write_line(&mut Full, "BVSTORAGE full errno=28 free=0 path=/");
        let mut log = Vec::new();
        super::write_line(&mut log, format_args!("BVPAUSE paused"));
        assert_eq!(log, b"BVPAUSE paused\n");
    }
}
