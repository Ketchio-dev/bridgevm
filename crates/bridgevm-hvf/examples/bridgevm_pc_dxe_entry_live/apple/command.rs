use std::path::PathBuf;

const USAGE: &str =
    "usage: bridgevm_pc_dxe_entry_live FIRMWARE_FD [--vars-file PATH --expect written|restored]";

pub(super) struct Arguments {
    pub(super) firmware: PathBuf,
    pub(super) mode: RunMode,
}

pub(super) enum RunMode {
    InMemory,
    VarsFile {
        path: PathBuf,
        expectation: Expectation,
    },
}

#[derive(Clone, Copy)]
pub(super) enum Expectation {
    Written,
    Restored,
}

impl Expectation {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Written => "written",
            Self::Restored => "restored",
        }
    }
}

pub(super) fn parse() -> Result<Arguments, String> {
    let mut args = std::env::args_os().skip(1);
    let firmware = PathBuf::from(args.next().ok_or_else(|| USAGE.to_string())?);
    let Some(flag) = args.next() else {
        return Ok(Arguments {
            firmware,
            mode: RunMode::InMemory,
        });
    };
    if flag != "--vars-file" {
        return Err(USAGE.to_string());
    }
    let path = PathBuf::from(args.next().ok_or_else(|| USAGE.to_string())?);
    if args.next().as_deref() != Some("--expect".as_ref()) {
        return Err(USAGE.to_string());
    }
    let expectation = match args.next().as_deref() {
        Some(value) if value == "written" => Expectation::Written,
        Some(value) if value == "restored" => Expectation::Restored,
        _ => return Err(USAGE.to_string()),
    };
    if args.next().is_some() {
        return Err(USAGE.to_string());
    }
    Ok(Arguments {
        firmware,
        mode: RunMode::VarsFile { path, expectation },
    })
}
