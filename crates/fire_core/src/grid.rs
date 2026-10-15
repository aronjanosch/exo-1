//! The cell grid: fire as cells per room, the other model next to the point model of
//! `tuning`). Guesses for the playtest, `TODO(initiator)`.
use crate::{Deck, RoomId, Target, TargetKind};
use gameplay_core::rng::Rng;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// The grid's values. TODO(initiator): starting values. Growth and the oxygen lines follow the
/// point model (which follows Barotrauma); the cell size, the intensities, the oxygen use per
/// cell and the chances are ours.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GridTuning {
    /// Side of a cell, m.
    pub cell_m: f64,
    /// Intensity per second a cell gains with plenty of oxygen; without it, it loses the same.
    pub growth: f64,
    /// Oxygen fraction from which a cell grows at full speed.
    pub grow_full: f64,
    /// Oxygen fraction at which a cell neither grows nor shrinks; below it shrinks.
    pub stall: f64,
    /// m³ of oxygen per second per cell at intensity 1.
    pub oxygen_use: f64,
    /// Below this intensity a cell goes out.
    pub out_below: f64,
    /// Intensity of a cell the glue starts (the reactor, a hit, a dev button).
    pub start_intensity: f64,
    /// A burning cell at least this hot tries to spread.
    pub spread_min: f64,
    /// Intensity of a cell the fire spreads into.
    pub spread_intensity: f64,
    pub spread_interval_s: f64,
    /// Chance per roll and open gap (or neighbour).
    pub spread_chance: f64,
    /// How near (m) a cell must be to a gap to touch it.
    pub gap_reach: f64,
    /// The chance factor through a closed gap (its open fraction is used when it is open).
    pub door_closed: f64,
    /// Heat a person takes per second in a cell of intensity 1.
    pub burn_person: f64,
    /// Condition a device loses per second under a cell of intensity 1.
    pub burn_device: f64,
    /// Intensity per second an extinguisher takes off the cells near its landing.
    pub extinguish: f64,
    /// Cells this close (m) to where a spray lands get the spray.
    pub spray_radius: f64,
    /// The ship's alarm is Danger from this total intensity in the engine room and the corridor;
    /// any fire is Warn.
    pub alarm_danger_total: f64,
}

impl Default for GridTuning {
    fn default() -> GridTuning {
        GridTuning {
            cell_m: 1.0,
            growth: 0.05,
            grow_full: 0.2,
            stall: 0.1,
            oxygen_use: 0.05,
            out_below: 0.05,
            start_intensity: 0.6,
            spread_min: 0.5,
            spread_intensity: 0.3,
            spread_interval_s: 5.0,
            spread_chance: 0.15,
            gap_reach: 1.0,
            door_closed: 0.1,
            burn_person: 10.0,
            burn_device: 2.0,
            extinguish: 0.6,
            spray_radius: 1.0,
            alarm_danger_total: 1.5,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BurningCell {
    pub room: RoomId,
    pub index: usize,
    pub col: usize,
    pub row: usize,
    pub centre: DVec2,
    pub intensity: f64,
}

/// What the grid did this step. Damage as `fire_core::FireEvent::Damage`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GridEvent {
    Damage { target: u64, kind: TargetKind, amount: f64 },
    /// A cell caught from a neighbour, or across a gap.
    Spread { room: RoomId, cell: usize },
    Out { room: RoomId, cell: usize },
}

/// A room's cells: its floor rectangle from `min`, `cols` by `rows` cells of `cell` m.
#[derive(Clone, Debug)]
struct Layout {
    room: RoomId,
    min: DVec2,
    cell: f64,
    cols: usize,
    rows: usize,
    intensity: Vec<f64>,
}

impl Layout {
    fn centre(&self, index: usize) -> DVec2 {
        let (c, r) = (index % self.cols, index / self.cols);
        self.min + DVec2::new((c as f64 + 0.5) * self.cell, (r as f64 + 0.5) * self.cell)
    }

    /// The cell a point is in, the edge cells for a point outside the rectangle.
    fn index_of(&self, p: DVec2) -> usize {
        let c = ((p.x - self.min.x) / self.cell).floor().clamp(0.0, (self.cols - 1) as f64) as usize;
        let r = ((p.y - self.min.y) / self.cell).floor().clamp(0.0, (self.rows - 1) as f64) as usize;
        r * self.cols + c
    }

    fn distance_to(&self, index: usize, p: DVec2) -> f64 {
        rect_distance(self.centre(index), DVec2::splat(self.cell * 0.5), p, DVec2::ZERO)
    }

    fn burning_cell(&self, index: usize) -> BurningCell {
        BurningCell { room: self.room, index, col: index % self.cols, row: index / self.cols, centre: self.centre(index), intensity: self.intensity[index] }
    }
}

/// The distance between two axis-aligned boxes (centre and half size) on the floor; 0 when they
/// overlap or touch.
fn rect_distance(c1: DVec2, h1: DVec2, c2: DVec2, h2: DVec2) -> f64 {
    ((c1 - c2).abs() - h1 - h2).max(DVec2::ZERO).length()
}

#[derive(Clone, Debug)]
pub struct Grid {
    tuning: GridTuning,
    rooms: Vec<Layout>,
    rng: Rng,
    spread_timer: f64,
}

impl Grid {
    pub fn new(tuning: GridTuning, seed: u64) -> Grid {
        Grid { tuning, rooms: Vec::new(), rng: Rng::new(seed), spread_timer: tuning.spread_interval_s }
    }

    pub fn tuning(&self) -> &GridTuning {
        &self.tuning
    }

    /// Gives a room the cells of its floor rectangle: from `min` (m, ship space), `size` m wide
    /// and deep, cut into `cell_m` squares (rounded, at least one each way).
    pub fn add_room(&mut self, room: RoomId, min: DVec2, size: DVec2) {
        let cell = self.tuning.cell_m;
        let cols = (size.x / cell).round().max(1.0) as usize;
        let rows = (size.y / cell).round().max(1.0) as usize;
        self.rooms.push(Layout { room, min, cell, cols, rows, intensity: vec![0.0; cols * rows] });
    }

    fn layout(&self, room: RoomId) -> Option<&Layout> {
        self.rooms.iter().find(|l| l.room == room)
    }

    fn layout_mut(&mut self, room: RoomId) -> Option<&mut Layout> {
        self.rooms.iter_mut().find(|l| l.room == room)
    }

    /// The cells of a room: columns and rows, or None when the room has none.
    pub fn dims(&self, room: RoomId) -> Option<(usize, usize)> {
        self.layout(room).map(|l| (l.cols, l.rows))
    }

    pub fn cell_at(&self, room: RoomId, p: DVec3) -> Option<usize> {
        self.layout(room).map(|l| l.index_of(p.truncate()))
    }

    /// A cell's intensity (0 when it is out or the room has no cells).
    pub fn intensity(&self, room: RoomId, index: usize) -> f64 {
        self.layout(room).and_then(|l| l.intensity.get(index).copied()).unwrap_or(0.0)
    }

    /// Starts the cell at `pos` at `intensity` (or raises it to that). Returns its index, None
    /// when no room with cells holds the point.
    pub fn ignite(&mut self, room: RoomId, pos: DVec3, intensity: f64) -> Option<usize> {
        let v = intensity.clamp(0.0, 1.0);
        let l = self.layout_mut(room)?;
        let i = l.index_of(pos.truncate());
        l.intensity[i] = l.intensity[i].max(v);
        Some(i)
    }

    pub fn burning(&self) -> impl Iterator<Item = BurningCell> + '_ {
        self.rooms.iter().flat_map(|l| (0..l.intensity.len()).filter(|&i| l.intensity[i] > 0.0).map(move |i| l.burning_cell(i)))
    }

    pub fn total(&self, room: RoomId) -> f64 {
        self.layout(room).map_or(0.0, |l| l.intensity.iter().sum())
    }

    /// The extinguisher's spray lands at `at` for `dt` seconds: every burning cell whose square
    /// is within `spray_radius` of it loses `extinguish * dt`. Returns how many cells it hit.
    pub fn spray(&mut self, at: DVec3, dt: f64) -> usize {
        let t = self.tuning;
        let p = at.truncate();
        let mut hit = 0;
        for l in &mut self.rooms {
            for i in 0..l.intensity.len() {
                if l.intensity[i] > 0.0 && l.distance_to(i, p) <= t.spray_radius {
                    l.intensity[i] = (l.intensity[i] - t.extinguish * dt).max(0.0);
                    hit += 1;
                }
            }
        }
        hit
    }

    /// One step: the cells grow or starve and use oxygen (in the deck's rooms), the ones that
    /// went out end, `targets` burn (people by the hottest cell in their 3x3, devices by the
    /// hottest cell their footprint touches), and the spread rolls when their interval is up.
    /// The deck's air moves in `Deck::exchange_air` (the glue calls it; `Deck::step` with no fires
    /// does the same), not here.
    pub fn step(&mut self, deck: &mut Deck, dt: f64, targets: &[Target]) -> Vec<GridEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();

        for l in &mut self.rooms {
            let room = deck.room_mut(l.room);
            let gm = ((room.oxygen_fraction() - t.stall) / (t.grow_full - t.stall)).min(1.0);
            for v in l.intensity.iter_mut().filter(|v| **v > 0.0) {
                let used = (t.oxygen_use * *v * dt).min(room.oxygen.max(0.0));
                room.oxygen -= used;
                *v = (*v + t.growth * gm * dt).clamp(0.0, 1.0);
            }
        }

        for l in &mut self.rooms {
            for i in 0..l.intensity.len() {
                if l.intensity[i] > 0.0 && l.intensity[i] < t.out_below {
                    l.intensity[i] = 0.0;
                    ev.push(GridEvent::Out { room: l.room, cell: i });
                }
            }
        }

        for x in targets {
            let Some(l) = self.rooms.iter().find(|l| l.room == x.room) else { continue };
            let p = x.pos.truncate();
            match x.kind {
                TargetKind::Person => {
                    let c = l.index_of(p);
                    let (col, row) = ((c % l.cols) as isize, (c / l.cols) as isize);
                    let mut hot: f64 = 0.0;
                    for dr in -1..=1 {
                        for dc in -1..=1 {
                            let (nc, nr) = (col + dc, row + dr);
                            if nc < 0 || nr < 0 || nc >= l.cols as isize || nr >= l.rows as isize {
                                continue;
                            }
                            hot = hot.max(l.intensity[nr as usize * l.cols + nc as usize]);
                        }
                    }
                    if hot > 0.0 {
                        ev.push(GridEvent::Damage { target: x.id, kind: x.kind, amount: t.burn_person * hot * dt });
                    }
                }
                TargetKind::Device => {
                    let hot = (0..l.intensity.len()).filter(|&i| l.intensity[i] > 0.0 && rect_distance(l.centre(i), DVec2::splat(l.cell * 0.5), p, x.half) <= 1e-9).map(|i| l.intensity[i]).fold(0.0, f64::max);
                    if hot > 0.0 {
                        ev.push(GridEvent::Damage { target: x.id, kind: x.kind, amount: t.burn_device * hot * dt });
                    }
                }
            }
        }

        self.spread_timer -= dt;
        if self.spread_timer <= 0.0 {
            self.spread_timer = t.spread_interval_s;
            self.spread(deck, &mut ev);
        }
        ev
    }

    /// The spread rolls: every burning cell at `spread_min` or more, for each 4-neighbour (same
    /// room) and for each gap it touches (the other room's cells near the gap, one picked at
    /// random). New cells start at `spread_intensity`; they roll next time, not this one.
    fn spread(&mut self, deck: &Deck, ev: &mut Vec<GridEvent>) {
        let t = self.tuning;
        let mut sparks: Vec<(RoomId, usize)> = Vec::new();
        for li in 0..self.rooms.len() {
            let l = &self.rooms[li];
            for i in 0..l.intensity.len() {
                if l.intensity[i] < t.spread_min {
                    continue;
                }
                let (c, r) = ((i % l.cols) as isize, (i / l.cols) as isize);
                for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nc, nr) = (c + dc, r + dr);
                    if nc < 0 || nr < 0 || nc >= l.cols as isize || nr >= l.rows as isize {
                        continue;
                    }
                    if self.rng.unit() < t.spread_chance {
                        sparks.push((l.room, nr as usize * l.cols + nc as usize));
                    }
                }
                for g in deck.gaps() {
                    let other = if g.a == l.room {
                        g.b
                    } else if g.b == l.room {
                        g.a
                    } else {
                        continue;
                    };
                    if l.distance_to(i, g.pos.truncate()) > t.gap_reach {
                        continue;
                    }
                    let factor = g.open.max(t.door_closed);
                    if self.rng.unit() >= t.spread_chance * factor {
                        continue;
                    }
                    let Some(o) = self.rooms.iter().find(|o| o.room == other) else { continue };
                    let touching: Vec<usize> = (0..o.intensity.len()).filter(|&j| o.distance_to(j, g.pos.truncate()) <= t.gap_reach).collect();
                    if touching.is_empty() {
                        continue;
                    }
                    let pick = ((self.rng.unit() * touching.len() as f64) as usize).min(touching.len() - 1);
                    sparks.push((other, touching[pick]));
                }
            }
        }
        for (room, i) in sparks {
            let l = self.layout_mut(room).expect("a spark is in a room with cells");
            if l.intensity[i] <= 0.0 {
                l.intensity[i] = t.spread_intensity;
                ev.push(GridEvent::Spread { room, cell: i });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FireTuning;

    const DT: f64 = 1.0 / 30.0;

    fn one_room(volume: f64) -> (Deck, RoomId) {
        let mut d = Deck::new(FireTuning::default(), 1);
        let r = d.add_room(volume);
        (d, r)
    }

    fn grid_on(room: RoomId, seed: u64) -> Grid {
        let mut g = Grid::new(GridTuning::default(), seed);
        g.add_room(room, DVec2::ZERO, DVec2::new(4.0, 4.0));
        g
    }

    fn set_oxygen_fraction(d: &mut Deck, r: RoomId, f: f64) {
        let room = d.room_mut(r);
        room.oxygen = room.volume * f;
    }

    #[test]
    fn a_room_gets_one_cell_per_metre_of_its_floor() {
        let (_, r) = one_room(100.0);
        let mut g = Grid::new(GridTuning::default(), 0);
        g.add_room(r, DVec2::new(-19.0, -19.0), DVec2::new(10.0, 8.0));
        assert_eq!(g.dims(r), Some((10, 8)));
        assert_eq!(g.cell_at(r, DVec3::new(-18.5, -18.5, 0.0)), Some(0));
        assert_eq!(g.cell_at(r, DVec3::new(-9.5, -11.5, 0.0)), Some(79));
        assert_eq!(g.dims(RoomId(9)), None, "a room without cells has none");
    }

    #[test]
    fn a_cell_burns_out_without_oxygen() {
        let (mut d, r) = one_room(100.0);
        let mut g = grid_on(r, 0);
        g.ignite(r, DVec3::new(0.5, 0.5, 0.0), 0.5);
        set_oxygen_fraction(&mut d, r, 0.0);
        let mut out = 0;
        for _ in 0..(11.0 / DT) as usize {
            out += g.step(&mut d, DT, &[]).iter().filter(|e| matches!(e, GridEvent::Out { .. })).count();
        }
        assert_eq!(out, 1, "it goes out once");
        assert_eq!(g.burning().count(), 0);
        assert_eq!(g.intensity(r, 0), 0.0);
    }

    #[test]
    fn a_cell_below_the_spread_line_does_not_spread() {
        let (mut d, r) = one_room(1.0e6);
        let mut g = Grid::new(GridTuning::default(), 3);
        g.add_room(r, DVec2::ZERO, DVec2::new(20.0, 20.0));
        g.ignite(r, DVec3::new(10.5, 10.5, 0.0), 0.3);
        // At the stall line the cell neither grows nor shrinks, so it stays under the spread line.
        set_oxygen_fraction(&mut d, r, 0.1);
        let ev = g.step(&mut d, 5.0, &[]);
        assert_eq!(g.intensity(r, 10 * 20 + 10), 0.3);
        assert!(ev.iter().all(|e| !matches!(e, GridEvent::Spread { .. })));
    }

    #[test]
    fn a_person_in_the_cells_round_a_fire_burns_and_one_further_off_does_not() {
        let (mut d, r) = one_room(1000.0);
        let mut g = grid_on(r, 0);
        g.ignite(r, DVec3::new(1.5, 1.5, 0.0), 1.0);
        let next = Target { id: 1, kind: TargetKind::Person, room: r, pos: DVec3::new(2.5, 2.5, 0.0), half: DVec2::ZERO };
        let two_off = Target { id: 2, kind: TargetKind::Person, room: r, pos: DVec3::new(3.5, 1.5, 0.0), half: DVec2::ZERO };
        let ev = g.step(&mut d, 1.0, &[next, two_off]);
        let ids: Vec<u64> = ev.iter().filter_map(|e| if let GridEvent::Damage { target, .. } = e { Some(*target) } else { None }).collect();
        assert_eq!(ids, vec![1], "the cell diagonal to the fire burns, two cells off does not");
    }
}
