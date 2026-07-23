use anyhow::Result;

mod connect_supervisor_qmp;
mod optional_u16_env;
mod qmp_supervisor_drain_limit;

pub(crate) use connect_supervisor_qmp::*;
pub(crate) use optional_u16_env::*;
pub(crate) use qmp_supervisor_drain_limit::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}

mod qmp_supervisor_drain_limit_impl_2;
mod qmp_supervisor_drain_limit_impl_3;

mod qmp_supervisor_drain_limit_impl_4;

mod qmp_supervisor_drain_limit_impl_5;
