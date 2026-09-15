//! Forward native inventory without a shell or a second process lifecycle.

use crate::*;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;

pub(crate) fn run(args: AppArgs) -> Result<()> {
    let executable = app_cli_resolver::resolve()?;
    let error = ProcessCommand::new(&executable)
        .args(native_arguments(args))
        .exec();
    Err(error)
        .with_context(|| format!("could not execute native app CLI: {}", executable.display()))
}

fn native_arguments(args: AppArgs) -> Vec<OsString> {
    let mut arguments = vec![OsString::from("--cli")];
    match args.command {
        AppCommand::List => arguments.push("list".into()),
        AppCommand::Inspect { id } => {
            arguments.push("inspect".into());
            arguments.push(id.into());
        }
        AppCommand::Readiness { id } => {
            arguments.push("readiness".into());
            arguments.push(id.into());
        }
    }
    if let Some(library) = args.library {
        arguments.push("--library".into());
        arguments.push(library.into_os_string());
    }
    if args.json {
        arguments.push("--json".into());
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_unicode_and_spaces_as_single_arguments() {
        let args = AppArgs {
            command: AppCommand::Inspect {
                id: "개발-vm".into(),
            },
            library: Some(PathBuf::from("/owned library/with 'quotes'")),
            json: true,
        };
        assert_eq!(
            native_arguments(args),
            [
                "--cli",
                "inspect",
                "개발-vm",
                "--library",
                "/owned library/with 'quotes'",
                "--json"
            ]
            .map(OsString::from)
        );
        let list = AppArgs {
            command: AppCommand::List,
            library: None,
            json: false,
        };
        assert_eq!(
            native_arguments(list),
            ["--cli", "list"].map(OsString::from)
        );
        let readiness = AppArgs {
            command: AppCommand::Readiness {
                id: "개발-vm".into(),
            },
            library: None,
            json: true,
        };
        assert_eq!(
            native_arguments(readiness),
            ["--cli", "readiness", "개발-vm", "--json"].map(OsString::from)
        );
    }
}
