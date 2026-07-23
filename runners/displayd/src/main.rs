mod input_events;
mod max_runtime_policy_bytes;

pub(crate) use input_events::*;
pub(crate) use max_runtime_policy_bytes::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() {
    main_entry()
}
