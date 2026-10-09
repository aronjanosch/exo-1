//! net_core: snapshot format, interpolation buffer, clock sync, fault injection and the replay
//! metrics of spike 4, without Bevy types, in f64.
//! No sockets here: the replay matrix runs in `cargo test` (see `replay`).
//! All numbers are spike test values (assumptions), not designed.
pub mod buffer;
pub mod clock;
pub mod link;
pub mod metrics;
pub mod replay;
pub mod snapshot;
pub mod wire;

pub const MAX_OWNER: u32 = 8;
