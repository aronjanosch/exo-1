//! A reliable, ordered message channel next to the unreliable snapshots, over any datagram
//! transport (C2 of the interaction model: item requests and events must not be lost, doubled or
//! reordered, while snapshots may be). No sockets here: the glue sends what `poll` returns and
//! feeds what arrives into `on_datagram`; time is passed in, so tests run under `link`.
//!
//! One `Reliable` is one end of one connection (the host keeps one per client).
//!
//! - A message is cut into frames of at most `MAX_FRAME` bytes (the last without the `more` flag),
//!   every frame has a running sequence number. The receiver hands a message out once all its
//!   frames have arrived in order, so big messages (a late joiner's state) fit the datagram size.
//! - Every datagram carries the receiver's state: `next` (every frame below it has arrived) and a
//!   bit mask of the frames after it that have arrived too. So acks ride along on whatever is
//!   sent anyway; a datagram without frames is only sent when an ack is owed.
//! - The sender keeps a frame until it is acked and sends it again after the retransmit timeout
//!   (smoothed round trip plus four times its deviation, doubling per retry), or at once when a
//!   later frame was acked and the hole is older than a round trip (fast retransmit).
//! - At most `WINDOW` frames are in flight; the rest waits in a queue.
//! - Frames below `next` and frames already held are duplicates and dropped; frames beyond the
//!   window are ignored (the sender sends them again).
//!
//! Sequence numbers are `u32` and do not wrap (4 billion frames); a session never gets there.
use std::collections::{BTreeMap, VecDeque};

/// Frames in flight at once (the ack mask covers 64 frames after `next`).
pub const WINDOW: u32 = 64;
/// Bytes of message in one frame.
pub const MAX_FRAME: usize = 900;
/// Bytes of one datagram body, the ack header included (below a typical MTU).
pub const MAX_DATAGRAM: usize = 1200;
/// Biggest message accepted or sent (bytes).
pub const MAX_MESSAGE: usize = 256 * 1024;
/// Frames waiting behind the window before the channel counts as broken.
pub const MAX_QUEUE: usize = 4096;
/// An ack waits this long for something to ride on (s).
const ACK_DELAY: f64 = 0.02;
/// Retransmit timeout before the first round trip is measured, and its bounds (s).
const RTO_INITIAL: f64 = 0.5;
const RTO_MIN: f64 = 0.1;
const RTO_MAX: f64 = 2.0;

const HEADER: usize = 4 + 8 + 1;
const FRAME_HEADER: usize = 4 + 1 + 2;

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub seq: u32,
    /// Another frame of the same message follows.
    pub more: bool,
    pub data: Vec<u8>,
}

/// What goes into one datagram body.
#[derive(Clone, Debug, PartialEq)]
pub struct Datagram {
    /// Every frame below this one has arrived at the sender of this datagram.
    pub next: u32,
    /// Bit `i`: the frame `next + 1 + i` has arrived too.
    pub sack: u64,
    pub frames: Vec<Frame>,
}

impl Datagram {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER + self.frames.iter().map(|f| FRAME_HEADER + f.data.len()).sum::<usize>());
        b.extend_from_slice(&self.next.to_le_bytes());
        b.extend_from_slice(&self.sack.to_le_bytes());
        b.push(self.frames.len() as u8);
        for f in &self.frames {
            b.extend_from_slice(&f.seq.to_le_bytes());
            b.push(f.more as u8);
            b.extend_from_slice(&(f.data.len() as u16).to_le_bytes());
            b.extend_from_slice(&f.data);
        }
        b
    }

    /// None for anything malformed: short, trailing bytes, a frame over `MAX_FRAME`.
    pub fn decode(b: &[u8]) -> Option<Datagram> {
        if b.len() < HEADER || b.len() > MAX_DATAGRAM {
            return None;
        }
        let next = u32::from_le_bytes(b[0..4].try_into().ok()?);
        let sack = u64::from_le_bytes(b[4..12].try_into().ok()?);
        let n = b[12] as usize;
        let mut at = HEADER;
        let mut frames = Vec::with_capacity(n.min(16));
        for _ in 0..n {
            let h = b.get(at..at + FRAME_HEADER)?;
            let (seq, more, len) = (u32::from_le_bytes(h[0..4].try_into().ok()?), h[4], u16::from_le_bytes(h[5..7].try_into().ok()?) as usize);
            if more > 1 || len > MAX_FRAME {
                return None;
            }
            let data = b.get(at + FRAME_HEADER..at + FRAME_HEADER + len)?.to_vec();
            at += FRAME_HEADER + len;
            frames.push(Frame { seq, more: more == 1, data });
        }
        (at == b.len()).then_some(Datagram { next, sack, frames })
    }
}

/// Counters, for tests and the HUD line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub frames_sent: u64,
    pub resent: u64,
    pub fast_resent: u64,
    pub duplicates: u64,
    pub out_of_order: u64,
    pub messages_in: u64,
    pub messages_out: u64,
}

struct InFlight {
    frame: Frame,
    last_sent: Option<f64>,
    tries: u32,
    /// Send again at the next poll, whatever the timeout says.
    resend: bool,
}

pub struct Reliable {
    next_seq: u32,
    queue: VecDeque<Frame>,
    flight: BTreeMap<u32, InFlight>,
    expect: u32,
    held: BTreeMap<u32, Frame>,
    partial: Vec<u8>,
    inbox: VecDeque<Vec<u8>>,
    ack_due: Option<f64>,
    srtt: Option<f64>,
    rttvar: f64,
    /// Since when the oldest unacked frame has seen no progress.
    stuck_since: Option<f64>,
    broken: bool,
    pub stats: Stats,
}

impl Default for Reliable {
    fn default() -> Self {
        Reliable::new()
    }
}

impl Reliable {
    pub fn new() -> Reliable {
        Reliable {
            next_seq: 0,
            queue: VecDeque::new(),
            flight: BTreeMap::new(),
            expect: 0,
            held: BTreeMap::new(),
            partial: Vec::new(),
            inbox: VecDeque::new(),
            ack_due: None,
            srtt: None,
            rttvar: 0.0,
            stuck_since: None,
            broken: false,
            stats: Stats::default(),
        }
    }

    /// Queues a message for sending. False, and nothing is queued, for an empty or too big message
    /// or a channel with too many frames waiting (then it is `broken`).
    pub fn send(&mut self, msg: &[u8]) -> bool {
        if msg.is_empty() || msg.len() > MAX_MESSAGE || self.broken {
            return false;
        }
        let parts = msg.chunks(MAX_FRAME).count();
        if self.queue.len() + parts > MAX_QUEUE {
            self.broken = true;
            return false;
        }
        let mut chunks = msg.chunks(MAX_FRAME).peekable();
        while let Some(c) = chunks.next() {
            self.queue.push_back(Frame { seq: self.next_seq, more: chunks.peek().is_some(), data: c.to_vec() });
            self.next_seq += 1;
        }
        self.stats.messages_out += 1;
        true
    }

    /// The next message that has arrived complete and in order.
    pub fn recv(&mut self) -> Option<Vec<u8>> {
        self.inbox.pop_front()
    }

    /// Frames not yet acked, in flight or waiting.
    pub fn unacked(&self) -> usize {
        self.flight.len() + self.queue.len()
    }

    /// Too many frames piled up, or the peer sent something that cannot be (the owner drops the
    /// connection).
    pub fn broken(&self) -> bool {
        self.broken
    }

    /// No ack has made progress for `timeout` seconds while frames wait for one.
    pub fn stalled(&self, now: f64, timeout: f64) -> bool {
        self.stuck_since.is_some_and(|t| now - t > timeout)
    }

    /// Smoothed round trip (s), once measured.
    pub fn rtt(&self) -> Option<f64> {
        self.srtt
    }

    fn rto(&self, tries: u32) -> f64 {
        let base = self.srtt.map_or(RTO_INITIAL, |s| (s + 4.0 * self.rttvar).clamp(RTO_MIN, RTO_MAX));
        (base * 2f64.powi(tries.saturating_sub(1).min(4) as i32)).min(RTO_MAX)
    }

    /// A datagram from the peer.
    pub fn on_datagram(&mut self, now: f64, d: &Datagram) {
        self.on_ack(now, d.next, d.sack);
        for f in &d.frames {
            if f.seq < self.expect || self.held.contains_key(&f.seq) {
                self.stats.duplicates += 1;
                self.ack_due = Some(now); // the peer is resending: it missed our ack
            } else if f.seq >= self.expect + WINDOW {
                // Beyond what we accept; the sender sends it again once the window moves.
            } else {
                if f.seq != self.expect {
                    self.stats.out_of_order += 1;
                    self.ack_due = Some(now); // a hole: tell the sender at once
                }
                self.held.insert(f.seq, f.clone());
            }
        }
        while let Some(f) = self.held.remove(&self.expect) {
            self.expect += 1;
            self.partial.extend_from_slice(&f.data);
            if self.partial.len() > MAX_MESSAGE {
                self.partial.clear();
                self.broken = true;
            } else if !f.more {
                self.inbox.push_back(std::mem::take(&mut self.partial));
                self.stats.messages_in += 1;
            }
        }
        if !d.frames.is_empty() {
            self.ack_due.get_or_insert(now + ACK_DELAY);
        }
    }

    fn on_ack(&mut self, now: f64, next: u32, sack: u64) {
        // An ack for frames never sent is nonsense: ignore it.
        let sent_to = self.queue.front().map_or(self.next_seq, |f| f.seq);
        if next > sent_to {
            return;
        }
        let mut sample: Option<(u32, f64)> = None;
        let mut progress = false;
        let mut take = |flight: &mut BTreeMap<u32, InFlight>, seq: u32| {
            if let Some(f) = flight.remove(&seq) {
                progress = true;
                // Only a frame sent once gives a clean round trip (a resend's ack is ambiguous).
                if let (1, Some(t)) = (f.tries, f.last_sent) && sample.is_none_or(|(s, _)| seq > s) {
                    sample = Some((seq, now - t));
                }
            }
        };
        let below: Vec<u32> = self.flight.range(..next).map(|(s, _)| *s).collect();
        for s in below {
            take(&mut self.flight, s);
        }
        let mut highest_sacked = None;
        for i in 0..64u32 {
            if sack >> i & 1 == 1 {
                let seq = next + 1 + i;
                take(&mut self.flight, seq);
                highest_sacked = Some(seq);
            }
        }
        if let Some((_, rtt)) = sample {
            match self.srtt {
                None => {
                    self.srtt = Some(rtt);
                    self.rttvar = rtt / 2.0;
                }
                Some(s) => {
                    self.rttvar = 0.75 * self.rttvar + 0.25 * (s - rtt).abs();
                    self.srtt = Some(0.875 * s + 0.125 * rtt);
                }
            }
        }
        // Fast retransmit: a later frame arrived, this one is older than a round trip.
        if let Some(high) = highest_sacked {
            let age = self.srtt.map_or(RTO_INITIAL * 0.5, |s| s * 1.1);
            for (_, f) in self.flight.range_mut(..high) {
                if !f.resend && f.last_sent.is_some_and(|t| now - t > age) {
                    f.resend = true;
                    self.stats.fast_resent += 1;
                }
            }
        }
        if progress || self.flight.is_empty() {
            self.stuck_since = (!self.flight.is_empty()).then_some(now);
        }
    }

    /// What to send now, as datagram bodies: new frames, frames whose timeout ran out, or only an
    /// ack when one is owed. Call every tick.
    pub fn poll(&mut self, now: f64) -> Vec<Datagram> {
        let base = self.flight.keys().next().copied().unwrap_or_else(|| self.queue.front().map_or(self.next_seq, |f| f.seq));
        while let Some(f) = self.queue.front() {
            if f.seq >= base + WINDOW {
                break;
            }
            let f = self.queue.pop_front().unwrap();
            self.flight.insert(f.seq, InFlight { frame: f, last_sent: None, tries: 0, resend: false });
        }
        let due: Vec<u32> = self
            .flight
            .iter()
            .filter(|(_, f)| match f.last_sent {
                None => true,
                Some(t) => f.resend || now - t >= self.rto(f.tries),
            })
            .map(|(s, _)| *s)
            .collect();
        if !due.is_empty() && self.stuck_since.is_none() {
            self.stuck_since = Some(now);
        }
        let (next, sack) = self.ack_state();
        let mut out: Vec<Datagram> = Vec::new();
        let mut used = HEADER;
        for seq in due {
            let f = self.flight.get_mut(&seq).unwrap();
            if f.tries > 0 {
                self.stats.resent += 1;
            }
            f.tries += 1;
            f.last_sent = Some(now);
            f.resend = false;
            self.stats.frames_sent += 1;
            let size = FRAME_HEADER + f.frame.data.len();
            if out.is_empty() || used + size > MAX_DATAGRAM || out.last().unwrap().frames.len() >= 32 {
                out.push(Datagram { next, sack, frames: Vec::new() });
                used = HEADER;
            }
            out.last_mut().unwrap().frames.push(f.frame.clone());
            used += size;
        }
        if out.is_empty() && self.ack_due.is_some_and(|t| t <= now) {
            out.push(Datagram { next, sack, frames: Vec::new() });
        }
        if !out.is_empty() {
            self.ack_due = None;
        }
        out
    }

    fn ack_state(&self) -> (u32, u64) {
        let mut sack = 0u64;
        for s in self.held.keys() {
            let i = s - self.expect - 1; // held frames are above `expect`
            if i < 64 {
                sack |= 1 << i;
            }
        }
        (self.expect, sack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Link, Rng};

    const DT: f64 = 1.0 / 60.0;

    /// Two ends joined by a link each way, stepped at 60 Hz.
    struct Pair {
        a: Reliable,
        b: Reliable,
        ab: Link<Vec<u8>>,
        ba: Link<Vec<u8>>,
        now: f64,
        /// Deliver every datagram twice.
        double: bool,
        got_a: Vec<Vec<u8>>,
        got_b: Vec<Vec<u8>>,
        max_flight: usize,
    }

    impl Pair {
        fn new(delay_ms: f64, jitter_ms: f64, loss: f64, seed: u64) -> Pair {
            Pair {
                a: Reliable::new(),
                b: Reliable::new(),
                ab: Link::new(delay_ms, jitter_ms, loss, seed),
                ba: Link::new(delay_ms, jitter_ms, loss, seed ^ 0xABCD),
                now: 0.0,
                double: false,
                got_a: Vec::new(),
                got_b: Vec::new(),
                max_flight: 0,
            }
        }

        fn step(&mut self) {
            self.now += DT;
            let now = self.now;
            for bytes in self.ab.ready(now) {
                self.b.on_datagram(now, &Datagram::decode(&bytes).expect("valid"));
            }
            for bytes in self.ba.ready(now) {
                self.a.on_datagram(now, &Datagram::decode(&bytes).expect("valid"));
            }
            while let Some(m) = self.a.recv() {
                self.got_a.push(m);
            }
            while let Some(m) = self.b.recv() {
                self.got_b.push(m);
            }
            for d in self.a.poll(now) {
                let bytes = d.encode();
                assert!(bytes.len() <= MAX_DATAGRAM);
                self.ab.enqueue(now, bytes.clone());
                if self.double {
                    self.ab.enqueue(now, bytes);
                }
            }
            for d in self.b.poll(now) {
                let bytes = d.encode();
                self.ba.enqueue(now, bytes.clone());
                if self.double {
                    self.ba.enqueue(now, bytes);
                }
            }
            self.max_flight = self.max_flight.max(self.a.flight.len()).max(self.b.flight.len());
        }

        fn run(&mut self, seconds: f64) {
            for _ in 0..(seconds / DT) as usize {
                self.step();
            }
        }
    }

    /// Message `i` of a stream: its number, then a pattern, with sizes from a few bytes to 20 kB.
    fn message(i: u32, rng: &mut Rng) -> Vec<u8> {
        let len = match rng.next_u64() % 10 {
            0 => 5_000 + (rng.next_u64() % 15_000) as usize,
            1 | 2 => 900 + (rng.next_u64() % 400) as usize,
            _ => 8 + (rng.next_u64() % 120) as usize,
        };
        let mut m = i.to_le_bytes().to_vec();
        m.extend((0..len).map(|k| (k as u32 ^ i) as u8));
        m
    }

    fn check_stream(sent: &[Vec<u8>], got: &[Vec<u8>], what: &str) {
        assert_eq!(got.len(), sent.len(), "{what}: every message arrived once");
        for (i, (s, g)) in sent.iter().zip(got).enumerate() {
            assert_eq!(s, g, "{what}: message {i} intact and in order");
        }
    }

    fn two_way_run(delay: f64, jitter: f64, loss: f64, seed: u64, double: bool, n: u32) -> Pair {
        let mut p = Pair::new(delay, jitter, loss, seed);
        p.double = double;
        let mut rng = Rng::new(seed);
        let (mut sent_a, mut sent_b) = (Vec::new(), Vec::new());
        // Bursts, as the game sends them: a few messages at a time over four seconds.
        for i in 0..n {
            for (end, sent) in [(0, &mut sent_a), (1, &mut sent_b)] {
                let m = message(i, &mut rng);
                assert!(if end == 0 { p.a.send(&m) } else { p.b.send(&m) });
                sent.push(m);
            }
            if i % 5 == 4 {
                p.run(0.1);
            }
        }
        p.run(60.0);
        check_stream(&sent_a, &p.got_b, "a to b");
        check_stream(&sent_b, &p.got_a, "b to a");
        assert_eq!((p.a.unacked(), p.b.unacked()), (0, 0), "everything acked in the end");
        p
    }

    #[test]
    fn lossless_link_needs_no_resend_and_delivers_in_one_way_time() {
        let mut p = Pair::new(150.0, 0.0, 0.0, 1);
        assert!(p.a.send(b"hello"));
        let mut at = None;
        for _ in 0..120 {
            p.step();
            if at.is_none() && !p.got_b.is_empty() {
                at = Some(p.now);
            }
        }
        // One way is 150 ms; the tick rate adds at most two ticks.
        let at = at.expect("delivered");
        assert!((0.15..0.15 + 3.0 * DT).contains(&at), "arrived after {at:.3} s");
        assert_eq!(p.got_b, vec![b"hello".to_vec()]);
        assert_eq!(p.a.stats.resent, 0, "no resend without loss");
        assert_eq!(p.a.unacked(), 0);
    }

    #[test]
    fn internet_150_ms_with_jitter_and_2_percent_loss() {
        let p = two_way_run(150.0, 20.0, 2.0, 7, false, 300);
        assert!(p.a.stats.messages_out == 300 && p.b.stats.messages_in == 300);
        let rtt = p.a.rtt().expect("measured");
        assert!((0.28..0.45).contains(&rtt), "round trip learned: {rtt:.3} s");
    }

    #[test]
    fn heavy_loss_and_reordering_still_deliver_everything_in_order() {
        let p = two_way_run(120.0, 80.0, 25.0, 99, false, 200);
        assert!(p.a.stats.resent > 0 && p.b.stats.resent > 0, "loss makes it resend");
        assert!(p.b.stats.out_of_order > 0, "jitter reorders");
    }

    #[test]
    fn doubled_datagrams_do_not_double_messages() {
        let p = two_way_run(60.0, 10.0, 3.0, 5, true, 100);
        assert!(p.a.stats.duplicates > 0 && p.b.stats.duplicates > 0, "the doubles were seen and dropped");
    }

    #[test]
    fn window_caps_the_frames_in_flight() {
        let mut p = Pair::new(100.0, 0.0, 0.0, 3);
        let msgs: Vec<Vec<u8>> = (0..1000u32).map(|i| i.to_le_bytes().to_vec()).collect();
        for m in &msgs {
            assert!(p.a.send(m));
        }
        p.run(60.0);
        assert!(p.max_flight <= WINDOW as usize, "{} in flight", p.max_flight);
        check_stream(&msgs, &p.got_b, "burst");
    }

    #[test]
    fn a_silent_peer_stalls_the_channel() {
        let mut a = Reliable::new();
        assert!(a.send(b"anyone there?"));
        let mut now = 0.0;
        while now < 4.0 {
            now += DT;
            let _ = a.poll(now);
        }
        assert!(a.stalled(now, 3.0) && !a.stalled(now, 10.0));
        assert!(a.stats.resent >= 2, "it keeps trying with backoff");
        assert!(a.stats.resent <= 8, "but not every tick: {}", a.stats.resent);
    }

    #[test]
    fn an_ack_for_frames_never_sent_is_ignored() {
        let mut a = Reliable::new();
        assert!(a.send(b"x"));
        let _ = a.poll(0.0);
        a.on_datagram(0.1, &Datagram { next: 500, sack: u64::MAX, frames: vec![] });
        assert_eq!(a.unacked(), 1);
        a.on_datagram(0.2, &Datagram { next: 1, sack: 0, frames: vec![] });
        assert_eq!(a.unacked(), 0);
    }

    #[test]
    fn frames_beyond_the_window_are_ignored_until_they_come_again() {
        let mut b = Reliable::new();
        let far = Datagram { next: 0, sack: 0, frames: vec![Frame { seq: WINDOW + 5, more: false, data: vec![1] }] };
        b.on_datagram(0.0, &far);
        assert!(b.recv().is_none() && b.held.is_empty());
    }

    #[test]
    fn datagram_format_round_trips_and_rejects_garbage() {
        let d = Datagram { next: 7, sack: 0b101, frames: vec![Frame { seq: 7, more: true, data: vec![1; MAX_FRAME] }, Frame { seq: 8, more: false, data: vec![] }] };
        let bytes = d.encode();
        assert_eq!(Datagram::decode(&bytes), Some(d));
        assert!(Datagram::decode(&bytes[..bytes.len() - 1]).is_none(), "truncated");
        assert!(Datagram::decode(&[bytes.clone(), vec![0]].concat()).is_none(), "trailing byte");
        assert!(Datagram::decode(&[]).is_none() && Datagram::decode(&[0; 5]).is_none());
        let mut big = Datagram { next: 0, sack: 0, frames: vec![Frame { seq: 0, more: false, data: vec![0; MAX_FRAME + 1] }] }.encode();
        assert!(Datagram::decode(&big).is_none(), "frame over the limit");
        big.truncate(0);
        let mut rng = Rng::new(11);
        for _ in 0..2000 {
            let n = (rng.next_u64() % 60) as usize;
            let junk: Vec<u8> = (0..n).map(|_| rng.next_u64() as u8).collect();
            let _ = Datagram::decode(&junk); // never panics
        }
    }

    #[test]
    fn a_message_over_the_limit_is_refused() {
        let mut a = Reliable::new();
        assert!(!a.send(&[]) && !a.send(&vec![0; MAX_MESSAGE + 1]));
        assert!(a.send(&vec![0; MAX_MESSAGE]));
        assert_eq!(a.unacked(), MAX_MESSAGE.div_ceil(MAX_FRAME));
    }

    #[test]
    fn a_piled_up_queue_breaks_the_channel() {
        let mut a = Reliable::new();
        let mut n = 0;
        while a.send(&[0; 10]) {
            n += 1;
            assert!(n <= MAX_QUEUE);
        }
        assert!(a.broken());
    }
}
