//! Native query and owned-runtime command selection.

use crate::*;
#[path = "app_query_command.rs"]
mod query;
pub(crate) use query::AppQueryCommand;

#[path = "app_command_variants.rs"]
mod variants;
pub(crate) use variants::{AppCommand, AppCreateWindowsArgs};
