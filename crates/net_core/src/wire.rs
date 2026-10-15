//! Datagram format between processes (the Bevy crate owns the sockets). First byte is a magic,
//! second the type. Everything but snapshots is tiny; all of it is plain numbers.
use crate::loose::BodyDatagram;
use crate::reliable::Datagram;
use crate::snapshot::{Snapshot, SIZE};

pub const MAGIC: u8 = 0xE1;
/// IPv4 + UDP header bytes per datagram, for the "on the wire" estimate (no Ethernet framing).
pub const UDP_IP_OVERHEAD: usize = 28;

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Hello { slot: u8, planet: u8 },
    Accepted { slot: u8 },
    Ping { sent: f64 },
    Pong { sent: f64, server_time: f64 },
    Snapshot(Snapshot),
    Bye { slot: u8 },
    Reliable(Datagram),
    Bodies(BodyDatagram),
}

const T_HELLO: u8 = 1;
const T_ACCEPTED: u8 = 2;
const T_PING: u8 = 3;
const T_PONG: u8 = 4;
const T_SNAPSHOT: u8 = 5;
const T_BYE: u8 = 6;
const T_RELIABLE: u8 = 7;
const T_BODIES: u8 = 8;

pub fn encode_snapshot(s: &Snapshot) -> Vec<u8> {
    let mut b = Vec::with_capacity(2 + SIZE);
    b.extend_from_slice(&[MAGIC, T_SNAPSHOT]);
    b.extend_from_slice(&s.encode());
    b
}

/// Datagram for an already encoded snapshot (the host relays the bytes unchanged).
pub fn wrap_snapshot_bytes(body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(2 + body.len());
    b.extend_from_slice(&[MAGIC, T_SNAPSHOT]);
    b.extend_from_slice(body);
    b
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Packet::Hello { slot, planet } => vec![MAGIC, T_HELLO, *slot, *planet],
            Packet::Accepted { slot } => vec![MAGIC, T_ACCEPTED, *slot],
            Packet::Ping { sent } => [&[MAGIC, T_PING][..], &sent.to_le_bytes()].concat(),
            Packet::Pong { sent, server_time } => [&[MAGIC, T_PONG][..], &sent.to_le_bytes(), &server_time.to_le_bytes()].concat(),
            Packet::Snapshot(s) => encode_snapshot(s),
            Packet::Bye { slot } => vec![MAGIC, T_BYE, *slot],
            Packet::Reliable(d) => [&[MAGIC, T_RELIABLE][..], &d.encode()].concat(),
            Packet::Bodies(d) => [&[MAGIC, T_BODIES][..], &d.encode()].concat(),
        }
    }

    pub fn decode(b: &[u8]) -> Option<Packet> {
        if b.len() < 3 || b[0] != MAGIC {
            return None;
        }
        let f64_at = |o: usize| -> Option<f64> {
            let v = f64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?);
            v.is_finite().then_some(v)
        };
        match (b[1], b.len()) {
            (T_HELLO, 4) => Some(Packet::Hello { slot: b[2], planet: b[3] }),
            (T_ACCEPTED, 3) => Some(Packet::Accepted { slot: b[2] }),
            (T_PING, 10) => Some(Packet::Ping { sent: f64_at(2)? }),
            (T_PONG, 18) => Some(Packet::Pong { sent: f64_at(2)?, server_time: f64_at(10)? }),
            (T_SNAPSHOT, n) if n == 2 + SIZE => Snapshot::decode(&b[2..]).map(Packet::Snapshot),
            (T_BYE, 3) => Some(Packet::Bye { slot: b[2] }),
            (T_RELIABLE, _) => Datagram::decode(&b[2..]).map(Packet::Reliable),
            (T_BODIES, _) => BodyDatagram::decode(&b[2..]).map(Packet::Bodies),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loose::{BodyFrame, BodyState};
    use crate::reliable::Frame;
    use glam::{DQuat, DVec3};

    #[test]
    fn reliable_and_body_packets_round_trip() {
        let r = Packet::Reliable(Datagram { next: 3, sack: 1, frames: vec![Frame { seq: 3, more: false, data: b"hi".to_vec() }] });
        assert_eq!(Packet::decode(&r.encode()), Some(r));
        let b = Packet::Bodies(BodyDatagram { t: 2.0, seq: 1, bodies: vec![BodyState { id: 4, frame: BodyFrame::Ship(1), pos: DVec3::new(1.0, 2.0, 3.0), vel: DVec3::ZERO, rot: DQuat::IDENTITY }] });
        assert_eq!(Packet::decode(&b.encode()), Some(b));
        assert!(Packet::decode(&[MAGIC, T_RELIABLE, 1, 2, 3]).is_none() && Packet::decode(&[MAGIC, T_BODIES, 0, 0]).is_none());
    }
}
