//! Forwarded VM ID and NAME operands must not look like native options.
//!
//! Native `BridgeVMControl --cli` has no `--` operand separator, so a value that
//! clap accepts after `--` (for example `--help`) would be re-read there as an
//! option. Canonical VM IDs never start with '-', so such values are refused here.

use clap::{Arg, Command};

pub(super) fn reject_option_like_operands(command: Command) -> Command {
    command.mut_args(guard_operand)
}

fn guard_operand(arg: Arg) -> Arg {
    if arg.is_positional() && ["id", "name"].contains(&arg.get_id().as_str()) {
        arg.value_parser(vm_operand)
    } else {
        arg
    }
}

fn vm_operand(value: &str) -> Result<String, String> {
    if value.starts_with('-') {
        return Err("VM IDs and names starting with '-' are not accepted by the CLI".into());
    }
    Ok(value.to_owned())
}
