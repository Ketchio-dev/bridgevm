//! Host-side policy and bookkeeping for the guest agent channel.
//!
//! Accepts the guest hello, authorizes requests against an [`AgentPolicy`], and
//! tracks in-flight commands. Message shapes come from `bridgevm-agent-protocol`;
//! this crate decides what the host is willing to do with them.

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

mod authorization;
mod command_tracker;
mod errors;
mod handshake;
mod line_codec;
mod policy;
mod session;

pub use authorization::*;
pub use command_tracker::*;
pub use errors::*;
pub use handshake::*;
pub use line_codec::*;
pub use policy::*;
pub use session::*;
