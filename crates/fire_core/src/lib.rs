//! fire_core: fire in the ship's rooms, without engine types, in f64. The seed of the rooms as air
//! All values are starting points for the playtest (`TODO(initiator)`).
use gameplay_core::rng::Rng;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

pub mod grid;
pub mod oxygen;
pub mod tuning;

pub use oxygen::{generator_flow, split_by_volume};

#[cfg(test)]
mod tests;

/// Fire values. TODO(initiator): starting values. Growth, oxygen lines, spread interval and
/// chance, the burn curve and the spray rate follow the original (converted at 100 px = 1 m);
/// oxygen use, the size cap, the device burn and the air flow are guesses for our room sizes.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FireTuning {
    /// Metres of size per second with plenty of oxygen; the same rate shrinks it without any.
    pub growth: f64,
    /// Oxygen fraction from which a fire grows at full speed.
    pub grow_full: f64,
    /// Oxygen fraction at which it neither grows nor shrinks; below it shrinks.
    pub stall: f64,
    /// m³ of oxygen per metre of fire per second.
    pub oxygen_use: f64,
    /// Largest size, metres.
    pub max_size: f64,
    pub out_below: f64,
    /// Size of a new fire that spread through a gap.
    pub spread_size: f64,
    pub spread_interval_s: f64,
    /// Chance per try and open gap in reach.
    pub spread_chance: f64,
    /// Burn range beyond the fire's edge at size 1 m; it grows with the square root of the size.
    pub range: f64,
    pub max_range: f64,
    /// Burn per second to a person at size 1 m; grows with the square root of the size.
    pub burn_person: f64,
    /// Sizes above this burn no harder.
    pub burn_cap_size: f64,
    /// Condition per second a device in range loses at size 1 m; it grows with the square root of
    /// the size, like a person's burn, up to `burn_cap_size`.
    pub burn_device: f64,
    /// Metres of size per second an extinguisher takes off.
    pub extinguish: f64,
    /// A spray hits fires whose edge is this close to where it lands.
    pub spray_radius: f64,
    /// m³ per second through 1 m² of open gap per unit of oxygen fraction difference.
    pub air_flow: f64,
    /// m³ per second through 1 m² of open leak to space per unit of the room's air fraction
    /// (space has none). TODO(initiator): a guess.
    pub leak_flow: f64,
}

impl Default for FireTuning {
    fn default() -> FireTuning {
        FireTuning {
            growth: 0.2,
            grow_full: 0.2,
            stall: 0.1,
            oxygen_use: 0.5,
            max_size: 5.0,
            out_below: 0.01,
            spread_size: 0.1,
            spread_interval_s: 5.0,
            spread_chance: 0.15,
            range: 1.0,
            max_range: 2.5,
            burn_person: 10.0,
            burn_cap_size: 5.0,
            burn_device: 2.0,
            extinguish: 0.6,
            spray_radius: 1.0,
            air_flow: 2.0,
            leak_flow: 4.0,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoomId(pub usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GapId(pub usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FireId(pub u64);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LeakId(pub usize);

/// A hole from a room to space (a hull breach): air leaves through it, nothing comes in.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Leak {
    pub room: RoomId,
    pub pos: DVec3,
    /// m² when fully open.
    pub area: f64,
    /// 0 closed .. 1 open.
    pub open: f64,
    /// m³ per second that left in the last exchange.
    pub flow: f64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Room {
    /// m³.
    pub volume: f64,
    /// m³ of breathable air; `volume` is full.
    pub oxygen: f64,
}

impl Room {
    pub fn oxygen_fraction(&self) -> f64 {
        if self.volume > 0.0 { self.oxygen / self.volume } else { 0.0 }
    }

    /// The room's pressure against space, 0..1: the same as its air fraction here (one gas).
    pub fn pressure(&self) -> f64 {
        self.oxygen_fraction()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Gap {
    pub a: RoomId,
    pub b: RoomId,
    pub pos: DVec3,
    /// m² when fully open.
    pub area: f64,
    pub open: f64,
}

impl Gap {
    fn other(&self, r: RoomId) -> Option<RoomId> {
        if self.a == r {
            Some(self.b)
        } else if self.b == r {
            Some(self.a)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Fire {
    pub id: FireId,
    pub room: RoomId,
    pub pos: DVec3,
    /// Diameter, metres.
    pub size: f64,
    spread_timer: f64,
}

impl Fire {
    pub fn range(&self, t: &FireTuning) -> f64 {
        (self.size.max(0.0).sqrt() * t.range).min(t.max_range)
    }

    fn reaches(&self, p: DVec3, extra: f64) -> bool {
        self.pos.distance(p) <= self.size * 0.5 + extra
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Person,
    Device,
}

/// Something fire can hurt. The glue lists them each step.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Target {
    pub id: u64,
    pub kind: TargetKind,
    pub room: RoomId,
    /// Centre, ship space (the floor's x and y; z is not used).
    pub pos: DVec3,
    /// Half the footprint on the floor, m (x and y): a device's box. Zero for a person, who is a
    /// point.
    pub half: DVec2,
}

impl Target {
    /// From `p` to the nearest point of the footprint on the floor, m (0 inside it); to the
    /// centre for a point.
    pub fn distance_to(&self, p: DVec3) -> f64 {
        let gap = ((p.truncate() - self.pos.truncate()).abs() - self.half).max(DVec2::ZERO);
        gap.length()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FireEvent {
    /// Burn damage this step: health for a person, condition for a device.
    Damage { target: u64, kind: TargetKind, amount: f64 },
    /// A fire spread through an open gap and started (or fed) `to` in `room`.
    Spread { from: FireId, to: FireId, room: RoomId },
    Out { fire: FireId, room: RoomId },
}

#[derive(Clone, Debug)]
pub struct Deck {
    tuning: FireTuning,
    rooms: Vec<Room>,
    gaps: Vec<Gap>,
    fires: Vec<Fire>,
    next_fire: u64,
    rng: Rng,
    leaks: Vec<Leak>,
    lost: f64,
}

impl Deck {
    /// An empty deck; `seed` makes spread rolls repeatable.
    pub fn new(tuning: FireTuning, seed: u64) -> Deck {
        Deck { tuning, rooms: Vec::new(), gaps: Vec::new(), fires: Vec::new(), next_fire: 0, rng: Rng::new(seed), leaks: Vec::new(), lost: 0.0 }
    }

    pub fn tuning(&self) -> &FireTuning {
        &self.tuning
    }

    pub fn add_room(&mut self, volume: f64) -> RoomId {
        self.rooms.push(Room { volume, oxygen: volume });
        RoomId(self.rooms.len() - 1)
    }

    pub fn add_gap(&mut self, a: RoomId, b: RoomId, pos: DVec3, area: f64, open: f64) -> GapId {
        self.gaps.push(Gap { a, b, pos, area, open: open.clamp(0.0, 1.0) });
        GapId(self.gaps.len() - 1)
    }

    pub fn set_open(&mut self, gap: GapId, open: f64) {
        self.gaps[gap.0].open = open.clamp(0.0, 1.0);
    }

    pub fn room(&self, r: RoomId) -> &Room {
        &self.rooms[r.0]
    }

    pub fn room_mut(&mut self, r: RoomId) -> &mut Room {
        &mut self.rooms[r.0]
    }

    pub fn gaps(&self) -> &[Gap] {
        &self.gaps
    }

    pub fn gap(&self, g: GapId) -> &Gap {
        &self.gaps[g.0]
    }

    /// A hole to space of `area` m² in a room at `pos`, `open` 0..1 (a breach; `hull_core` sets
    /// its size from the hull's damage).
    pub fn add_leak(&mut self, room: RoomId, pos: DVec3, area: f64, open: f64) -> LeakId {
        self.leaks.push(Leak { room, pos, area: area.max(0.0), open: open.clamp(0.0, 1.0), flow: 0.0 });
        LeakId(self.leaks.len() - 1)
    }

    /// Resizes a leak (a patch closes it with `open` 0).
    pub fn set_leak(&mut self, leak: LeakId, area: f64, open: f64) {
        let l = &mut self.leaks[leak.0];
        l.area = area.max(0.0);
        l.open = open.clamp(0.0, 1.0);
        if l.open <= 0.0 || l.area <= 0.0 {
            l.flow = 0.0;
        }
    }

    pub fn leak(&self, leak: LeakId) -> &Leak {
        &self.leaks[leak.0]
    }

    pub fn leaks(&self) -> &[Leak] {
        &self.leaks
    }

    pub fn lost_to_space(&self) -> f64 {
        self.lost
    }

    pub fn fires(&self) -> &[Fire] {
        &self.fires
    }

    pub fn fire(&self, id: FireId) -> Option<&Fire> {
        self.fires.iter().find(|f| f.id == id)
    }

    pub fn fires_in(&self, r: RoomId) -> impl Iterator<Item = &Fire> {
        self.fires.iter().filter(move |f| f.room == r)
    }

    /// Puts `amount` m³ of air into room `r` (a generator's flow times dt), never past full.
    pub fn add_oxygen(&mut self, r: RoomId, amount: f64) {
        let room = &mut self.rooms[r.0];
        room.oxygen = (room.oxygen + amount.max(0.0)).min(room.volume);
    }

    /// Takes `amount` m³ of air out of room `r` for the breathers (each one's rate times dt), never
    /// below empty. Barotrauma `Character.cs:3835-3848` drains the hull the same way.
    pub fn breathe(&mut self, r: RoomId, amount: f64) {
        let room = &mut self.rooms[r.0];
        room.oxygen = (room.oxygen - amount.max(0.0)).max(0.0);
    }

    pub fn total_oxygen(&self) -> f64 {
        self.rooms.iter().map(|r| r.oxygen).sum()
    }

    /// Starts a fire of `size` metres (the reactor overheating, a hit, a short). A fire it
    /// touches in the same room takes it in instead; that one's id comes back.
    pub fn ignite(&mut self, room: RoomId, pos: DVec3, size: f64) -> FireId {
        let size = size.clamp(0.0, self.tuning.max_size);
        if let Some(f) = self.fires.iter_mut().find(|f| f.room == room && f.reaches(pos, size * 0.5)) {
            merge(f, pos, size, self.tuning.max_size);
            return f.id;
        }
        let id = FireId(self.next_fire);
        self.next_fire += 1;
        self.fires.push(Fire { id, room, pos, size, spread_timer: self.tuning.spread_interval_s });
        id
    }

    /// The extinguisher's spray lands at `at` for `dt` seconds (the glue finds the point along
    /// the nozzle's aim). Returns how many fires it hit.
    pub fn spray(&mut self, at: DVec3, dt: f64) -> usize {
        let t = self.tuning;
        let mut hit = 0;
        for f in self.fires.iter_mut().filter(|f| f.reaches(at, t.spray_radius)) {
            f.size -= t.extinguish * dt;
            hit += 1;
        }
        hit
    }

    /// Moves air through the open gaps for `dt` seconds. Each gap moves oxygen from the richer
    /// room to the poorer one, never past even, so the total stays the same. Open leaks then let
    /// air out to space (`lost_to_space`).
    pub fn exchange_air(&mut self, dt: f64) {
        for g in &self.gaps {
            if g.open <= 0.0 || g.a == g.b {
                continue;
            }
            let (ra, rb) = (self.rooms[g.a.0], self.rooms[g.b.0]);
            if ra.volume <= 0.0 || rb.volume <= 0.0 {
                continue;
            }
            let diff = ra.oxygen_fraction() - rb.oxygen_fraction();
            let even = (ra.oxygen * rb.volume - rb.oxygen * ra.volume) / (ra.volume + rb.volume);
            let flow = self.tuning.air_flow * g.area * g.open * diff * dt;
            let moved = if even >= 0.0 { flow.clamp(0.0, even) } else { flow.clamp(even, 0.0) };
            self.rooms[g.a.0].oxygen -= moved;
            self.rooms[g.b.0].oxygen += moved;
        }
        // Leaks to space: out with the room's pressure (space has none), never below empty.
        for l in &mut self.leaks {
            let room = &mut self.rooms[l.room.0];
            if l.open <= 0.0 || l.area <= 0.0 || room.volume <= 0.0 {
                l.flow = 0.0;
                continue;
            }
            let out = (self.tuning.leak_flow * l.area * l.open * room.pressure() * dt).clamp(0.0, room.oxygen);
            room.oxygen -= out;
            self.lost += out;
            l.flow = if dt > 0.0 { out / dt } else { 0.0 };
        }
    }

    /// One step: air through gaps, fires grow or starve and use oxygen, burn `targets`, try to
    /// spread, go out, touching fires merge.
    pub fn step(&mut self, dt: f64, targets: &[Target]) -> Vec<FireEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();
        self.exchange_air(dt);

        for f in &mut self.fires {
            let room = &mut self.rooms[f.room.0];
            let gm = ((room.oxygen_fraction() - t.stall) / (t.grow_full - t.stall)).min(1.0);
            room.oxygen = (room.oxygen - f.size.max(0.0) * t.oxygen_use * dt).max(0.0);
            f.size = (f.size + t.growth * gm * dt).min(t.max_size);
            f.spread_timer -= dt;
        }

        for f in self.fires.iter().filter(|f| f.size >= t.out_below) {
            let range = f.range(&t);
            for x in targets.iter().filter(|x| x.room == f.room && x.distance_to(f.pos) <= f.size * 0.5 + range) {
                let rate = match x.kind {
                    TargetKind::Person => t.burn_person * f.size.min(t.burn_cap_size).sqrt(),
                    TargetKind::Device => t.burn_device * f.size.min(t.burn_cap_size).sqrt(),
                };
                ev.push(FireEvent::Damage { target: x.id, kind: x.kind, amount: rate * dt });
            }
        }

        self.spread(&mut ev);

        let mut i = 0;
        while i < self.fires.len() {
            if self.fires[i].size < t.out_below {
                let f = self.fires.remove(i);
                ev.push(FireEvent::Out { fire: f.id, room: f.room });
            } else {
                i += 1;
            }
        }
        self.merge_touching();
        ev
    }

    fn spread(&mut self, ev: &mut Vec<FireEvent>) {
        let t = self.tuning;
        let mut new = Vec::new();
        for f in &mut self.fires {
            if f.spread_timer > 0.0 || f.size < t.out_below {
                continue;
            }
            f.spread_timer = t.spread_interval_s;
            let reach = f.range(&t);
            for g in &self.gaps {
                let Some(other) = g.other(f.room) else { continue };
                if g.open <= 0.0 || !f.reaches(g.pos, reach) {
                    continue;
                }
                if self.rng.unit() < t.spread_chance {
                    new.push((f.id, other, g.pos));
                }
            }
        }
        for (from, room, pos) in new {
            let to = self.ignite(room, pos, t.spread_size);
            ev.push(FireEvent::Spread { from, to, room });
        }
    }

    /// Fires in one room that touch become one (the older keeps its id).
    fn merge_touching(&mut self) {
        let max = self.tuning.max_size;
        let mut i = 0;
        while i < self.fires.len() {
            let mut j = i + 1;
            while j < self.fires.len() {
                let (a, b) = (self.fires[i], self.fires[j]);
                if a.room == b.room && a.reaches(b.pos, b.size * 0.5) {
                    merge(&mut self.fires[i], b.pos, b.size, max);
                    self.fires.remove(j);
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
}

/// Where an extinguisher's spray from `eye` along `dir` (a unit vector) lands on a fire whose
/// centre is `centre` and whose edge is `radius` from it: the distance along the aim to the point
/// nearest the fire, at most `reach`; None when the fire is out of the cone (`reach` metres,
/// `half_angle` radians) or behind the nozzle.
pub fn cone_landing(eye: DVec3, dir: DVec3, centre: DVec3, radius: f64, reach: f64, half_angle: f64) -> Option<f64> {
    let v = centre - eye;
    let along = v.dot(dir);
    if along <= 0.0 || along - radius > reach {
        return None;
    }
    let off = (v - dir * along).length();
    if off > along * half_angle.tan() + radius {
        return None;
    }
    Some(along.min(reach))
}

/// Takes a fire of `size` at `pos` into `f`: the span both cover on the line between their
/// centres, centred on that span.
fn merge(f: &mut Fire, pos: DVec3, size: f64, max: f64) {
    let d = f.pos.distance(pos);
    let lo = (-f.size * 0.5).min(d - size * 0.5);
    let hi = (f.size * 0.5).max(d + size * 0.5);
    if d > 1e-9 {
        f.pos += (pos - f.pos) / d * ((lo + hi) * 0.5);
    }
    f.size = (hi - lo).min(max);
}
