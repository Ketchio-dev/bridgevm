//! Shared pipe protocol: ownership always spans READY through release or failure.

use super::{READY, RELEASE};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

pub(super) fn hold<Owner>(
    owner: io::Result<Owner>,
    input: &mut impl Read,
    output: &mut impl Write,
) -> io::Result<()> {
    let _owner = owner?;
    output.write_all(READY)?;
    output.flush()?;
    let mut command = [0u8; 8];
    input.read_exact(&mut command)?;
    if command != RELEASE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid lease release command",
        ));
    }
    Ok(())
}

pub(super) fn command<Owner>(
    arguments: &[String],
    acquire: impl FnOnce(&Path, &Path) -> io::Result<Owner>,
) -> ExitCode {
    if arguments.len() != 2 {
        return ExitCode::from(2);
    }
    match hold(
        acquire(Path::new(&arguments[0]), Path::new(&arguments[1])),
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("media lease session: {error}");
            ExitCode::FAILURE
        }
    }
}
