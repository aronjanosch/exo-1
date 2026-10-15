//! net_core: snapshot format, interpolation buffer, clock sync, fault injection and the replay
//! metrics, without Bevy types, in f64.
//! No sockets here: the replay matrix runs in `cargo test` (see `replay`).
//! All numbers are test values (assumptions), not designed.
pub mod buffer;
pub mod clock;
pub mod link;
pub mod loose;
pub mod metrics;
pub mod reliable;
pub mod replay;
pub mod snapshot;
pub mod wire;

pub const MAX_OWNER: u32 = 8;
