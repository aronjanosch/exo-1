//! The hull in the engineering corner (#286): hits from the hit simulator (or its volley) land on
//! a hull section of either room through `hull_core`; their effects become device damage and
//! notices; patch kits and hold-F patching; the oxygen generator; breathing; oxygen alarms per
//! room; the pull of a big breach on the walker and loose crates; and `RoomReadout` for other
//! roles' displays.
use super::*;
use crate::cargo::Crate;
use hull_core::{Hit, HitEffect};

/// The hit simulator's choice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitPick {
    pub section: SectionId,
    pub kind: DamageType,
    pub strength: Strength,
}

/// What a room looks like to the ship's status displays (the captain's): air, its change, fire,
/// breach.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomReadout {
    pub name: &'static str,
    pub room: RoomId,
    /// Air 0..1.
    pub oxygen: f64,
    /// Change of `oxygen` per second, smoothed over about a second.
    pub oxygen_delta: f64,
    /// The low-oxygen alarm is on.
    pub oxygen_alarm: bool,
    pub fire: bool,
    /// Sum of the room's fire sizes (m).
    pub fire_size: f64,
    /// Air leaves through a hull breach.
    pub breach: bool,
    /// How far the breach is open, 0..1.
    pub breach_open: f64,
    /// m³/s leaving through it.
    pub leak_flow: f64,
}

/// Low-oxygen alarm: on below, off again above `OXYGEN_OK`. TODO(initiator): the lines.
pub const OXYGEN_LOW: f64 = 0.5;
pub const OXYGEN_OK: f64 = 0.55;
/// Hits in one volley.
pub const VOLLEY_HITS: u32 = 6;
/// A pull of this many m/s² is a full walk input. TODO(initiator): by feel.
const PULL_PER_INPUT: f64 = 4.0;

impl Engineering {
    /// The engineering room and the side room.
    pub fn rooms(&self) -> [RoomId; 2] {
        [self.room, self.side_room]
    }

    pub fn room_name(&self, room: RoomId) -> &'static str {
        if room == self.side_room { "side room" } else { "engineering" }
    }

    /// The hull section of a room.
    pub fn section_of(&self, room: RoomId) -> SectionId {
        if room == self.side_room { self.side_section } else { self.engine_section }
    }

    fn room_index(&self, room: RoomId) -> usize {
        (room == self.side_room) as usize
    }

    pub fn room_readout(&self, room: RoomId) -> RoomReadout {
        let i = self.room_index(room);
        let s = self.section_of(room);
        let fire_size = self.deck.fires_in(room).map(|f| f.size).fold(0.0, |a, b| a + b);
        RoomReadout {
            name: self.room_name(room),
            room,
            oxygen: self.deck.room(room).oxygen_fraction(),
            oxygen_delta: self.oxygen_delta[i],
            oxygen_alarm: self.oxygen_alarm[i],
            fire: fire_size > 0.0,
            fire_size,
            breach: self.hull.breached(s),
            breach_open: self.hull.openness(s),
            leak_flow: self.deck.leak(self.hull.section(s).leak).flow,
        }
    }

    /// Both rooms, engineering first.
    pub fn room_readouts(&self) -> [RoomReadout; 2] {
        [self.room_readout(self.room), self.room_readout(self.side_room)]
    }

    /// Lands a hit on the hull and routes what it did: devices in the room lose condition, each
    /// effect is a notice.
    pub fn land_hit(&mut self, hit: &Hit, gp: &mut Gameplay) {
        let room = self.hull.section(hit.section).room;
        let name = self.room_name(room);
        let effects = self.hull.apply(hit, &mut self.deck);
        self.stats.hits += 1;
        let mut summary = format!("{} {} on {name}:", hit.kind.name(), hit.amount);
        for fx in effects {
            match fx {
                HitEffect::HullDamaged { amount, .. } => {
                    summary.push_str(&format!(" hull -{amount:.0}"));
                    gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.hit").arg("room", Arg::Text(name.into())).arg("kind", Arg::Text(hit.kind.name().into())));
                }
                HitEffect::Breach { open, opened, .. } => {
                    summary.push_str(&format!(", breach {:.0} %", open * 100.0));
                    if opened {
                        self.stats.breaches_opened += 1;
                        gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.breach").arg("room", Arg::Text(name.into())));
                    }
                }
                HitEffect::Fire { .. } => {
                    summary.push_str(", fire");
                    self.stats.fires_started += 1;
                    note(gp, "notice.engineer.fire");
                }
                HitEffect::SystemDamage { room, amount } => {
                    summary.push_str(&format!(", systems -{amount:.0}"));
                    if room == self.room {
                        for d in [Dev::Reactor, Dev::Pump, Dev::Generator] {
                            if let Some(DeviceEvent::Broke) = self.condition_mut(d).damage(amount) {
                                broke(gp, d);
                            }
                        }
                        gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.system_hit").arg("room", Arg::Text(name.into())).arg("amount", Arg::Number(amount.round() as i64)));
                    }
                }
            }
        }
        self.last_hit = summary;
    }
}

/// "hold to patch the hull  [###-------] 30 %" with what is missing.
pub fn patch_verb(e: &Engineering, s: SectionId) -> String {
    let c = &e.hull.section(s).condition;
    let n = (c.fraction() * 10.0).round() as usize;
    let bar = format!("[{}{}]", "#".repeat(n), "-".repeat(10 - n));
    let state = if e.hull.breached(s) {
        if e.kit_in_hand { "  breach!".to_string() } else { "  breach! take a patch kit first".to_string() }
    } else {
        String::new()
    };
    format!("hold to patch the hull  {bar} {:.0} %{state}", c.fraction() * 100.0)
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut e: ResMut<Engineering>, mut gp: ResMut<Gameplay>) {
    for c in clicks.read() {
        if c.panel == HIT_PANEL {
            let a = c.action.as_str();
            if let Some(k) = DamageType::ALL.into_iter().find(|k| a == format!("type_{}", k.name())) {
                e.hit_pick.kind = k;
            } else if let Some(s) = Strength::ALL.into_iter().find(|s| a == format!("str_{}", s.name())) {
                e.hit_pick.strength = s;
            } else if a == "sec_engine" {
                e.hit_pick.section = e.engine_section;
            } else if a == "sec_side" {
                e.hit_pick.section = e.side_section;
            } else if a == "fire" {
                let p = e.hit_pick;
                e.land_hit(&Hit::standard(p.kind, p.strength, p.section), &mut gp);
            } else if a == "volley" {
                e.volleys += 1;
                let seed = 1000 + e.volleys;
                e.volley = Some(Volley::new(seed, vec![e.engine_section, e.side_section], VOLLEY_HITS));
            } else {
                warn!("hit simulator: unknown action {c:?}");
            }
        } else if c.panel == OXYGEN_PANEL {
            match (c.action.as_str(), c.value) {
                ("generator", Some(v)) => e.generator_on = v > 0.5,
                _ => warn!("oxygen panel: unknown action {c:?}"),
            }
        }
    }
}

/// One fixed step of the hull, after the main step: volley, kits, patching, life support,
/// breathing, the oxygen alarms.
#[allow(clippy::too_many_arguments)]
pub fn step(
    time: Res<Time>,
    mut e: ResMut<Engineering>,
    mut gp: ResMut<Gameplay>,
    mut skills: ResMut<DevSkills>,
    actions: Res<Actions>,
    inter: Res<Interaction>,
    players: Query<&Player>,
    (area, planet): (Res<DevArea>, Res<PlanetRes>),
) {
    let dt = time.delta_secs_f64();
    if dt <= 0.0 {
        return;
    }
    let Some(frame) = area.frame else { return };
    let e = &mut *e;

    // The volley.
    if let Some(v) = &mut e.volley {
        let hit = v.step(dt);
        let done = v.left() == 0;
        if let Some(h) = hit {
            e.land_hit(&h, &mut gp);
        }
        if done {
            e.volley = None;
        }
    }

    // The kit box.
    if let Some(Target::Device(id)) = &inter.tapped
        && id == KITS
    {
        if e.kit_in_hand {
            e.kit_in_hand = false;
            e.patch_kits += 1;
        } else if e.patch_kits > 0 {
            e.patch_kits -= 1;
            e.kit_in_hand = true;
        } else {
            note(&mut gp, "notice.engineer.no_kit");
        }
    }

    // Hold F at a section: patch it (a breach needs a kit, which it uses up when it closes).
    let section = match &inter.target {
        Some(Target::Device(id)) => id.strip_prefix(SECTION).and_then(|i| i.parse::<usize>().ok()).map(SectionId),
        _ => None,
    };
    let mut patching_now = None;
    if let Some(s) = section
        && actions.interact_held
        && !e.extinguisher.held
    {
        let breached = e.hull.breached(s);
        if breached && !e.kit_in_hand {
            if matches!(inter.tapped, Some(Target::Device(_))) {
                note(&mut gp, "notice.engineer.no_kit");
            }
        } else {
            patching_now = Some(s);
            if e.patching.is_some_and(|x| x != s) {
                let x = e.patching.unwrap();
                e.hull.stop_patch(x);
            }
            e.patching = Some(s);
            let out = e.hull.patch_step(s, dt, skills.mechanical, Tool::WRENCH, &mut e.deck);
            if breached && !e.hull.breached(s) {
                e.kit_in_hand = false;
                e.stats.patches_done += 1;
                gp.notify(Notice::new(NoticeKind::Completed, "notice.engineer.patched").arg("room", Arg::Text(e.room_name(e.hull.section(s).room).into())));
            }
            if let RepairOutcome::Done { skill_gain } = out {
                skills.mechanical = (skills.mechanical + skill_gain).min(100.0);
                e.stats.skill_gained += skill_gain;
            }
            e.last_patch = Some((s, out));
        }
    }
    if patching_now.is_none()
        && let Some(x) = e.patching.take()
    {
        e.hull.stop_patch(x);
        e.last_patch = None;
    }

    // Life support: the generator wears while it runs and fills both rooms by volume.
    let mult = if e.fast_wear { FAST_WEAR } else { 1.0 };
    let running = e.generator_on && e.grid.factor(Group::LifeSupport) > 0.0;
    if let Some(DeviceEvent::Broke) = e.generator.condition.deteriorate(dt * mult, if running { 1.0 } else { 0.0 }) {
        broke(&mut gp, Dev::Generator);
    }
    let supply = if e.generator_on { e.grid.factor(Group::LifeSupport) } else { 0.0 };
    let rooms = e.rooms();
    e.generator.step(dt, supply, &mut e.deck, &rooms);

    // The player breathes.
    if let Some((eye, _)) = players.single().ok().and_then(crate::displays::eye_on_foot) {
        let feet = area_of(&frame, &planet, eye) - DVec3::Y * EYE_HEIGHT;
        let room = e.room_at(feet);
        e.hull.breathe(&mut e.deck, room, 1.0, dt);
    }

    // Oxygen change and alarms per room.
    let k = (dt / 1.0).min(1.0);
    for (i, room) in rooms.into_iter().enumerate() {
        let o2 = e.deck.room(room).oxygen_fraction();
        let d = (o2 - e.oxygen_prev[i]) / dt;
        e.oxygen_prev[i] = o2;
        e.oxygen_delta[i] += (d - e.oxygen_delta[i]) * k;
        let on = if e.oxygen_alarm[i] { o2 < OXYGEN_OK } else { o2 < OXYGEN_LOW };
        if on && !e.oxygen_alarm[i] {
            gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.oxygen_low").arg("room", Arg::Text(e.room_name(room).into())).arg("percent", Arg::Number((o2 * 100.0).round() as i64)));
        }
        e.oxygen_alarm[i] = on;
    }
}

/// A big breach pulls: the walker gets a walk input towards the hole (not while at a display),
/// loose crates on the ground an acceleration.
pub fn pull(
    e: Res<Engineering>,
    mut actions: ResMut<Actions>,
    active: Res<ActiveDisplay>,
    players: Query<&Player>,
    mut crates: Query<&mut Crate>,
    (area, planet): (Res<DevArea>, Res<PlanetRes>),
) {
    let Some(frame) = area.frame else { return };
    if !e.hull.sections().iter().any(|s| e.deck.leak(s.leak).flow > 0.0) {
        return;
    }
    let world = |a: DVec3| frame.east * a.x + frame.north * a.z;
    if active.using().is_none()
        && let Ok(pl) = players.single()
        && let Some((eye, _)) = crate::displays::eye_on_foot(pl)
    {
        let feet = area_of(&frame, &planet, eye) - DVec3::Y * EYE_HEIGHT;
        let a = e.hull.pull(&e.deck, e.room_at(feet), feet + DVec3::Y);
        if a.length_squared() > 0.0 {
            let up = pl.world_up(walker_core::Frame::IDENTITY);
            let fwd = pl.w.forward;
            let right = fwd.cross(up);
            let w = world(a) / PULL_PER_INPUT;
            let mut m = actions.move_dir;
            m.x += w.dot(right);
            m.z -= w.dot(fwd);
            let flat = DVec2::new(m.x, m.z).clamp_length_max(1.0);
            actions.move_dir = DVec3::new(flat.x, m.y, flat.y);
        }
    }
    for mut c in &mut crates {
        if c.ship.is_some() || c.locked {
            continue;
        }
        let at = area_of(&frame, &planet, c.body.pos);
        let a = e.hull.pull(&e.deck, e.room_at(at), at);
        if a.length_squared() > 0.0 {
            c.push += world(a);
        }
    }
}
