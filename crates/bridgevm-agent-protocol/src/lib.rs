//! Wire format for the host/guest agent channel.
//!
//! Defines the envelope, message and capability types that both sides encode,
//! plus the protocol version they negotiate. It holds no transport or policy so
//! that the host daemon and the in-guest agent can depend on the same shapes.

mod protocol_version;

pub use protocol_version::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;
