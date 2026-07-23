//! nvme, split for the 1000-line rule.
mod part;
mod sq_entry_size;

pub(crate) use part::*;
pub use sq_entry_size::*;

#[cfg(test)]
mod tests;

mod sq_entry_size_impl_2;
mod sq_entry_size_impl_3;
mod sq_entry_size_impl_4;
