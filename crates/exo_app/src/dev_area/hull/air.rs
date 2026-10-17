//! The ship's air (`ShipAir`): one `fire_core::Deck` with a room per space of the inner ship (the
//! engine room, the corridor, the airlock, the bridge) and the outside, the doors as gaps between
//! them, a leak per hull section and the airlock's cycle. The outside's air is the world's
//! (`PlanetRes::ambient_air`), never a config value.
use crate::dev_area::*;
use crate::dev_area::layout::*;
use fire_core::{split_by_volume, Deck, FireTuning, GapId, LeakId, RoomId};
use item_core::wire::HULL_SECTIONS;
use reactor_core::engine_room::AirSave;
use reactor_core::hull::SECTION_CENTRES;
use reactor_core::{Airlock, AirlockTuning, Dir, Door, Side};

#[derive(Resource)]
pub struct ShipAir {
    pub deck: Deck,
    pub engine: RoomId,
    pub corridor: RoomId,
    /// The equipment room (#309): its own air behind its door to the corridor; the airlock's inner door opens into it.
    pub equipment: RoomId,
    pub airlock: RoomId,
    pub outside: RoomId,
    /// The bridge (r44): its volume is its size, and its door is a gap from the corridor.
    pub bridge: RoomId,
    pub door: GapId,
    pub bridge_door: GapId,
    /// The equipment room's door: corridor to equipment room.
    pub equip_door: GapId,
    /// One hole per wall section (r37's breach, by section in r40): opened and closed each step
    /// from the hull's openness.
    pub breaches: [LeakId; HULL_SECTIONS],
    /// The airlock's inner door: equipment room to airlock.
    pub inner: GapId,
    /// The outer door onto the breathable outside (airlock to outside). Open only while the outside is not vacuum.
    pub outer: GapId,
    /// The outer door onto vacuum (airlock to space, a leak). Open only while the outside is vacuum.
    pub outer_leak: LeakId,
    /// The airlock's cycle and doors (`reactor_core::Airlock`).
    pub lock: Airlock,
    /// The outside is vacuum: the world's air at the ship (`set_ambient`).
    pub vacuum: bool,
    /// The air outside the hull at the area (0..1), from the planet's atmosphere and the height.
    ambient: f64,
    /// m³ the running cycle moves at its full share: the air at its start (see `request`).
    cycle_air: f64,
    /// Each airlock door's openness, 0..1 (the slab's progress), as the deck has it.
    door_open: [f64; 2],
}

/// m³: the engine room 10 x 8 x 3.2 m, the airlock 2 x 2 x 3.2 m; the outside has no end.
const ROOM_VOLUME: f64 = 10.0 * 8.0 * ROOM_HEIGHT;
/// m³: the equipment room 3.5 x 3 x 3.2 m.
const EQUIPMENT_VOLUME: f64 = EQUIPMENT_AREA * ROOM_HEIGHT;
/// The corridor's air fills the rooms that open onto it without a door (`layout::Place`).
const CORRIDOR_VOLUME: f64 = CORRIDOR_AREA * ROOM_HEIGHT;
const AIRLOCK_VOLUME: f64 = 2.0 * 2.0 * ROOM_HEIGHT;
const OUTSIDE_VOLUME: f64 = 1.0e6;
/// m³: the bridge 8 x 6 x 3.2 m.
const BRIDGE_VOLUME: f64 = 8.0 * 6.0 * ROOM_HEIGHT;

impl ShipAir {
    pub fn new() -> ShipAir {
        ShipAir::with_tuning(&AirlockTuning::default())
    }

    /// The air at the start: the rooms full, the doors shut, the outside breathable until the world's
    /// air says otherwise (`set_ambient`); in vacuum nothing flows out but through the outer door's leak.
    pub fn with_tuning(t: &AirlockTuning) -> ShipAir {
        let mut deck = Deck::new(FireTuning::default(), 0x296);
        let engine = deck.add_room(ROOM_VOLUME);
        let corridor = deck.add_room(CORRIDOR_VOLUME);
        let airlock = deck.add_room(AIRLOCK_VOLUME);
        let outside = deck.add_room(OUTSIDE_VOLUME);
        let bridge = deck.add_room(BRIDGE_VOLUME);
        let equipment = deck.add_room(EQUIPMENT_VOLUME);
        let door = deck.add_gap(engine, corridor, DVec3::new(DOOR_AT[0], DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let bridge_door = deck.add_gap(corridor, bridge, DVec3::new(BRIDGE_DOOR_AT[0], BRIDGE_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let equip_door = deck.add_gap(corridor, equipment, DVec3::new(EQUIP_DOOR_AT[0], EQUIP_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let inner = deck.add_gap(equipment, airlock, DVec3::new(INNER_DOOR_AT[0], INNER_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let outer = deck.add_gap(airlock, outside, DVec3::new(OUTER_DOOR_AT[0], OUTER_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let outer_leak = deck.add_leak(airlock, DVec3::new(OUTER_DOOR_AT[0], OUTER_DOOR_AT[1], 0.0), 0.0, 0.0);
        // Shut at the start: the area and the openness come from the reactor each step.
        let breaches = SECTION_CENTRES.map(|c| deck.add_leak(engine, DVec3::new(c[0], c[1], 0.0), 0.0, 0.0));
        ShipAir { deck, engine, corridor, equipment, airlock, outside, bridge, door, bridge_door, equip_door, breaches, inner, outer, outer_leak, lock: Airlock::new(t), vacuum: false, ambient: 1.0, cycle_air: 0.0, door_open: [0.0; 2] }
    }

    /// The ship as the game starts it: as `with_tuning`, but the outer airlock door open, so a crew
    /// outside walks straight in (the initiator, 2026-10-15).
    pub fn at_start(t: &AirlockTuning) -> ShipAir {
        ShipAir { lock: Airlock::outer_open(t), ..ShipAir::with_tuning(t) }
    }

    pub fn room_of(&self, place: Place) -> RoomId {
        match place {
            Place::Engine => self.engine,
            Place::Corridor => self.corridor,
            Place::Equipment => self.equipment,
            Place::Airlock => self.airlock,
            Place::Bridge => self.bridge,
            Place::Outside => self.outside,
        }
    }

    /// The captain's SEAL (r41): shuts the inner door when it is open and no cycle runs (`Airlock::close`).
    pub fn seal_inner(&mut self) -> bool {
        self.lock.close(Door::Inner)
    }

    /// F on the inner door from the ship (#309): opens or shuts it while the airlock rests inside.
    pub fn toggle_inner(&mut self) -> bool {
        self.lock.toggle_inner()
    }

    /// A panel's PUSH (#309): `Airlock::press` starts the cycle to the other side. A cycle that starts
    /// takes its air reference now: an outward cycle pumps the airlock's air, an inward one fills the
    /// airlock to the corridor's fraction (the air it takes from the ship). False while a cycle runs.
    pub fn push(&mut self) -> bool {
        if !self.lock.press() {
            return false;
        }
        self.cycle_air = match self.lock.direction() {
            Some(Dir::Out) => self.deck.room(self.airlock).oxygen,
            Some(Dir::In) => self.deck.room(self.airlock).volume * self.deck.room(self.equipment).oxygen_fraction(),
            None => 0.0,
        };
        true
    }

    /// One step of the airlock's cycle: the pump moves its share of the reference air (what fits;
    /// the rest stays where it was), then the cycle's timer runs.
    pub fn airlock_step(&mut self, dt: f64) {
        let share = self.lock.pump_share(dt);
        if share > 0.0 {
            let want = share * self.cycle_air;
            match self.lock.direction() {
                Some(Dir::Out) => self.move_air(self.airlock, self.equipment, want),
                Some(Dir::In) => self.move_air(self.equipment, self.airlock, want),
                None => {}
            }
        }
        self.lock.step(dt);
    }

    /// Moves up to `want` m³ from one room to another: what the source has and the target has room for.
    /// Takes and adds through the deck's `breathe` and `add_oxygen`, so no air is made or lost.
    fn move_air(&mut self, from: RoomId, to: RoomId, want: f64) {
        let (f, t) = (*self.deck.room(from), *self.deck.room(to));
        let free = (t.volume - t.oxygen).max(0.0);
        let m = want.min(f.oxygen).min(free).max(0.0);
        self.deck.breathe(from, m);
        self.deck.add_oxygen(to, m);
    }

    /// An airlock door's openness, 0..1 (the slab's progress). The outer door opens onto the
    /// outside's air, or onto vacuum (a leak) while the outside is vacuum.
    pub fn set_door(&mut self, d: Door, open: f64) {
        match d {
            Door::Inner => {
                self.door_open[0] = open;
                self.deck.set_open(self.inner, open);
            }
            Door::Outer => {
                self.door_open[1] = open;
                self.apply_outer();
            }
        }
    }

    fn apply_outer(&mut self) {
        let open = self.door_open[1];
        let (gap, leak) = if self.vacuum { (0.0, open) } else { (open, 0.0) };
        self.deck.set_open(self.outer, gap);
        self.deck.set_leak(self.outer_leak, DOOR_AREA, leak);
    }

    /// The world's air outside the hull (0..1): the outside room is a source of it, not a finite
    /// volume, so every step sets it again. The doors follow at once.
    pub fn set_ambient(&mut self, density: f64) {
        self.ambient = density;
        let d = density;
        self.vacuum = health_core::vacuum::in_vacuum(d);
        let outside = self.outside;
        let volume = self.deck.room(outside).volume;
        self.deck.room_mut(outside).oxygen = volume * d;
        self.apply_outer();
    }

    /// The airlock as the panels show it (r40).
    pub fn airlock_news(&self) -> item_core::wire::AirlockNews {
        use item_core::wire::AirlockCycle;
        let cycling = match self.lock.cycling() {
            None => AirlockCycle::Idle,
            Some(Door::Inner) => AirlockCycle::Inner,
            Some(Door::Outer) => AirlockCycle::Outer,
        };
        item_core::wire::AirlockNews { inner_open: self.lock.is_open(Door::Inner), outer_open: self.lock.is_open(Door::Outer), inside: self.lock.side() == Side::Inside, cycling, progress: self.lock.progress() }
    }

    /// The generator's air (m³) through the three vents (r38, r44): `open[0]` the engine room's, `open[1]`
    /// the corridor's, `open[2]` the bridge's. Each open vent gets its room's volume share
    /// (`fire_core::split_by_volume`, Barotrauma `OxygenGenerator.UpdateVents`); with all shut the air goes nowhere.
    pub fn add_oxygen_through_vents(&mut self, m3: f64, open: [bool; 3]) {
        let (e, c, b) = (self.engine, self.corridor, self.bridge);
        let shares = split_by_volume(m3, &[(self.deck.room(e).volume, open[0]), (self.deck.room(c).volume, open[1]), (self.deck.room(b).volume, open[2])]);
        self.deck.add_oxygen(e, shares[0]);
        self.deck.add_oxygen(c, shares[1]);
        self.deck.add_oxygen(b, shares[2]);
    }

    /// The air as a save holds it (r42): each room's oxygen in m³ and the airlock.
    pub fn to_save(&self) -> AirSave {
        let rooms = [self.engine, self.corridor, self.airlock, self.outside, self.bridge];
        AirSave {
            oxygen: rooms.map(|r| self.deck.room(r).oxygen),
            vacuum: self.vacuum,
            lock: self.lock.to_save(),
            cycle_air: self.cycle_air,
            door_open: self.door_open,
            equipment: Some(self.deck.room(self.equipment).oxygen),
        }
    }

    /// The air of a save, the rooms and doors as they were. Breaches and the outer leak follow the
    /// hull and the vacuum at the next step, as they do in a new room.
    pub fn from_save(t: &AirlockTuning, s: &AirSave) -> ShipAir {
        let mut a = ShipAir::with_tuning(t);
        for (r, ox) in [a.engine, a.corridor, a.airlock, a.outside, a.bridge].into_iter().zip(s.oxygen) {
            a.deck.room_mut(r).oxygen = ox;
        }
        let eq = s.equipment.unwrap_or_else(|| a.deck.room(a.equipment).volume * a.deck.room(a.corridor).oxygen_fraction());
        a.deck.room_mut(a.equipment).oxygen = eq;
        a.lock = Airlock::from_save(t, &s.lock);
        a.cycle_air = s.cycle_air;
        a.door_open = s.door_open;
        a.deck.set_open(a.inner, s.door_open[0]);
        a.apply_outer();
        a
    }

    /// The air fraction (0..1) of the room at an area point: what a body breathes there.
    pub fn oxygen_at(&self, p: [f64; 2]) -> f64 {
        self.deck.room(self.room_of(place_of(p))).oxygen_fraction()
    }

    /// What a body breathes: its room's air when it stands in one of the area's rooms, else the
    /// world's `ambient` (0..1).
    pub fn air_at(&self, area_point: Option<[f64; 2]>, ambient: f64) -> f64 {
        match area_point.map(place_of) {
            Some(place) if place != Place::Outside => self.deck.room(self.room_of(place)).oxygen_fraction(),
            _ => ambient,
        }
    }
}

impl Default for ShipAir {
    fn default() -> ShipAir {
        ShipAir::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reactor_core::{AirlockTuning, Door};

    /// r37: the breach's hole loses air only while it is open; a mended wall keeps what it has.
    #[test]
    fn a_breach_loses_air_only_while_it_is_open() {
        let mut a = ShipAir::new();
        let full = a.deck.room(a.engine).oxygen;
        for _ in 0..60 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.engine).oxygen, full, "a shut breach keeps the air");
        a.deck.set_leak(a.breaches[0], 0.5, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        let leaking = a.deck.room(a.engine).oxygen;
        assert!(leaking < full * 0.99, "an open breach lets the air out");
        assert!((a.deck.lost_to_space() - (full - leaking)).abs() < 1e-9, "the lost air is the air that left");
        a.deck.set_leak(a.breaches[0], 0.5, 0.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.engine).oxygen, leaking, "a mended breach stops the loss");
    }

    /// r38: the generator's air goes through the vents by volume; a shut vent sends it all to the other room.
    #[test]
    fn a_shut_engine_vent_sends_the_generators_air_to_the_corridor() {
        let mut a = ShipAir::new();
        a.deck.breathe(a.engine, 5.0);
        a.deck.breathe(a.corridor, 5.0);
        let (e0, c0) = (a.deck.room(a.engine).oxygen, a.deck.room(a.corridor).oxygen);
        a.add_oxygen_through_vents(0.5, [false, true, false]);
        assert_eq!(a.deck.room(a.engine).oxygen, e0, "the shut engine vent takes nothing");
        assert!((a.deck.room(a.corridor).oxygen - c0 - 0.5).abs() < 1e-9, "the corridor gets all of it");
        a.add_oxygen_through_vents(0.5, [true, true, true]);
        let shared = a.deck.room(a.engine).oxygen - e0;
        let expected = 0.5 * ROOM_VOLUME / (ROOM_VOLUME + CORRIDOR_VOLUME + BRIDGE_VOLUME);
        assert!((shared - expected).abs() < 1e-9, "all open: the engine room takes its volume share ({shared})");
    }

    /// r44: the bridge is a fourth room of the deck with its own volume; its door is a gap from the
    /// corridor that opens with the slab, and the air still adds up while it flows.
    #[test]
    fn the_bridge_shares_air_through_its_door_and_the_air_adds_up() {
        let mut a = ShipAir::new();
        assert!((a.deck.room(a.bridge).volume - BRIDGE_VOLUME).abs() < 1e-12, "the bridge's volume is its size");
        assert_eq!(a.room_of(place_of(BRIDGE.centre)), a.bridge, "the bridge's zone is the bridge's room");
        let thin = a.deck.room(a.corridor).volume * 0.3;
        a.deck.breathe(a.corridor, thin);
        let total = a.deck.total_oxygen();
        let full = a.deck.room(a.bridge).oxygen;
        a.deck.set_open(a.bridge_door, 0.0);
        for _ in 0..60 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.bridge).oxygen, full, "a shut bridge door keeps the bridge's air");
        a.deck.set_open(a.bridge_door, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert!(a.deck.room(a.bridge).oxygen < full * 0.99, "an open bridge door lets the bridge's air flow to the thinner corridor");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air is conserved");
    }

    /// Runs the cycle and the air for `secs` at 30 steps a second, with the door's openness fixed.
    fn cycle(a: &mut ShipAir, secs: f64) {
        for _ in 0..(secs * 30.0).round() as usize {
            a.airlock_step(1.0 / 30.0);
            a.deck.exchange_air(1.0 / 30.0);
        }
    }

    /// r40: an outward cycle pumps 95 % of the airlock's air back into the equipment room; the rest stays.
    #[test]
    fn the_airlock_pumps_its_air_back_into_the_equipment_room() {
        let mut a = ShipAir::new();
        a.deck.breathe(a.equipment, 19.2);
        let total = a.deck.total_oxygen();
        let (lock, equipment) = (a.deck.room(a.airlock).oxygen, a.deck.room(a.equipment).oxygen);
        assert!(a.push(), "from inside the push starts the cycle out");
        cycle(&mut a, 4.5);
        assert!(a.lock.is_open(Door::Outer), "the outer door opens at the cycle's end");
        let moved = a.deck.room(a.equipment).oxygen - equipment;
        assert!((moved - 0.95 * lock).abs() < 1e-6, "the equipment room gets 95 % of the airlock's air ({moved})");
        assert!((a.deck.room(a.airlock).oxygen - 0.05 * lock).abs() < 1e-6, "5 % stays in the airlock");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air is conserved through the cycle");
    }

    /// r40: the inward cycle fills the airlock from the equipment room, and the air still adds up.
    #[test]
    fn the_inward_cycle_fills_the_airlock_from_the_equipment_room() {
        let mut a = ShipAir::at_start(&AirlockTuning::default());
        a.deck.breathe(a.airlock, 12.0);
        let total = a.deck.total_oxygen();
        let before = a.deck.room(a.airlock).oxygen;
        assert!(a.push(), "from outside the push starts the cycle in");
        cycle(&mut a, 4.5);
        assert!(a.lock.is_open(Door::Inner));
        assert!(a.deck.room(a.airlock).oxygen > before + 10.0, "the airlock is filled to the equipment room's pressure");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air came out of the equipment room");
    }

    /// r40: with the outside vacuum, the outer door opens onto space: the outside stays empty, the air
    /// that leaves the airlock is lost to space, and the total adds up.
    #[test]
    fn the_outer_door_opens_onto_vacuum() {
        let mut a = ShipAir::new();
        a.set_ambient(0.0);
        assert!(a.vacuum);
        assert_eq!(a.deck.room(a.outside).oxygen, 0.0, "the outside is empty");
        a.deck.breathe(a.equipment, 19.2);
        assert!(a.push());
        cycle(&mut a, 4.5);
        let total = a.deck.total_oxygen() + a.deck.lost_to_space();
        for _ in 0..90 {
            a.set_door(Door::Outer, 1.0);
            a.airlock_step(1.0 / 30.0);
            a.deck.exchange_air(1.0 / 30.0);
            assert_eq!(a.deck.room(a.outside).oxygen, 0.0, "the outside stays vacuum");
        }
        assert!(a.deck.room(a.airlock).oxygen < 0.01 * a.deck.room(a.airlock).volume, "the airlock empties into space");
        assert!((a.deck.total_oxygen() + a.deck.lost_to_space() - total).abs() < 1e-9, "what left went to space");
        assert_eq!(a.oxygen_at(AIRLOCK.centre), a.deck.room(a.airlock).oxygen_fraction());
    }

    /// r40: with the outside breathable, the outer door is a
    /// gap to a full outside: the airlock takes air from it.
    #[test]
    fn a_breathable_outside_gives_air_through_the_outer_door() {
        let mut a = ShipAir::with_tuning(&AirlockTuning::default());
        a.deck.breathe(a.airlock, 12.0);
        assert!((a.deck.room(a.outside).oxygen_fraction() - 1.0).abs() < 1e-12);
        a.set_door(Door::Outer, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert!(a.deck.room(a.airlock).oxygen_fraction() > 0.9, "the airlock takes the outside's air");
    }

    /// #309: a press while the cycle runs is ignored; the cycle goes on and finishes.
    #[test]
    fn a_press_during_a_cycle_is_ignored() {
        let mut a = ShipAir::new();
        assert!(a.push());
        assert!(!a.push(), "no second press during the cycle");
        assert!(!a.toggle_inner(), "the inner door is locked during the cycle");
        assert!(a.lock.cycling().is_some());
        cycle(&mut a, 4.5);
        assert_eq!(a.lock.cycling(), None, "the cycle finished");
        assert!(a.lock.is_open(Door::Outer) && !a.lock.is_open(Door::Inner));
    }

    /// #309: the air is kept across a cycle in and out of the ship: nothing leaves while the doors are
    /// shut (the 5 % that stay in the chamber leave with the outer door once it is open, see the vacuum test).
    #[test]
    fn the_air_is_kept_across_the_cycles() {
        let mut a = ShipAir::at_start(&AirlockTuning::default());
        a.deck.breathe(a.airlock, 12.0);
        let total = a.deck.total_oxygen();
        assert!(a.push());
        cycle(&mut a, 4.5);
        assert_eq!(a.lock.side(), Side::Inside);
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "a cycle in keeps the ship's air");
        let before_out = a.deck.total_oxygen();
        assert!(a.push());
        cycle(&mut a, 4.5);
        assert_eq!(a.lock.side(), Side::Outside);
        assert!((a.deck.total_oxygen() - before_out).abs() < 1e-9, "a cycle out keeps it too: the pump moves the air into the ship");
    }

    /// #309: the host's news carries the side and the cycle for the panels and the light.
    #[test]
    fn the_news_carries_the_side_and_the_cycle() {
        use item_core::wire::AirlockCycle;
        let mut a = ShipAir::at_start(&AirlockTuning::default());
        let n = a.airlock_news();
        assert!(!n.inside && n.outer_open && n.cycling == AirlockCycle::Idle);
        assert!(a.push());
        let n = a.airlock_news();
        assert!(!n.outer_open && n.cycling == AirlockCycle::Inner, "the cycle in: the outer door has shut");
        cycle(&mut a, 4.5);
        let n = a.airlock_news();
        assert!(n.inside && n.inner_open && n.cycling == AirlockCycle::Idle);
    }

    /// #309: the equipment room has its own air behind its door; opening the door shares it with the
    /// corridor and the air adds up.
    #[test]
    fn the_equipment_room_keeps_its_own_air_until_its_door_opens() {
        let mut a = ShipAir::new();
        assert_eq!(a.room_of(place_of(EQUIPMENT.centre)), a.equipment);
        assert_ne!(a.equipment, a.corridor, "not the corridor's cell");
        let thin = a.deck.room(a.equipment).volume * 0.4;
        a.deck.breathe(a.equipment, thin);
        let (total, eq) = (a.deck.total_oxygen(), a.deck.room(a.equipment).oxygen);
        a.deck.set_open(a.equip_door, 0.0);
        for _ in 0..60 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.equipment).oxygen, eq, "a shut door keeps the equipment room's air");
        a.deck.set_open(a.equip_door, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert!(a.deck.room(a.equipment).oxygen > eq * 1.05, "an open door lets the corridor's air in");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air is conserved");
    }

    #[test]
    fn the_deck_has_the_room_full_of_air_and_the_door_shut() {
        let mut a = ShipAir::new();
        assert!((a.oxygen_at(ROOM.centre) - 1.0).abs() < 1e-9);
        assert_eq!(a.air_at(Some([0.0, -12.0]), 1.0), 1.0, "outside the rooms the air is the world's");
        assert_eq!(a.air_at(None, 0.0), 0.0, "no air above the atmosphere");
        assert_eq!(a.air_at(Some(ROOM.centre), 0.0), 1.0, "a room keeps its own air in space");
        a.deck.room_mut(a.engine).oxygen *= 0.25;
        assert!((a.oxygen_at(ROOM.centre) - 0.25).abs() < 1e-9, "the room's air is what the walker breathes");
        assert!((a.oxygen_at(CORRIDOR.centre) - 1.0).abs() < 1e-9, "the corridor has its own air");
        let before = a.deck.total_oxygen();
        for _ in 0..60 {
            let _ = a.deck.step(1.0 / 30.0, &[]);
        }
        assert!((a.deck.total_oxygen() - before).abs() < 1e-6 * before, "air is conserved");
    }
}
