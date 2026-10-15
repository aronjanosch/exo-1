//! The scientist's lab in the dev area (#287): the mixer (the kettle), a heater, a centrifuge, a
//! syringe filler and a test dummy, each a box with a panel in the LAB zone (the medbay is
//! `dev_area/medbay.rs`). A batch goes from device to device through the player's `Hands` ("take"
//! and "put" on each panel). Skill is the local player's medical skill (`health_core::skills`, kept
//! in `HealthState`): it sets the device times and the bonus quality, and grows with each finished
//! step. The rules are `production_core` (lab, devices, skill); this is the glue.
//!
//! Systems as in `test_console`: `read_clicks` for the panels' actions, `tick` for time, and
//! `write_panels`; the panels are pure functions of the state (`science/panels.rs`).
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::gameplay::Gameplay;
use crate::health::HealthState;
use crate::items::LOCAL;
use container_core::{Access, Container, Item, SlotRule, Stack, Tag};
use display_core::Tone;
use health_core::skills::Skill;
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::{CommodityId, TextKey};
use production_core::devices::{fill_syringe, Batch, Centrifuge, DeviceTuning, Dose, Heater, Spin, Syringe, Target};
use production_core::skill::{self, Dice};
use production_core::{Kettle, Lab, LabContent, LabOutcome};
use std::collections::HashMap;

mod panels;
pub use panels::*;

pub const MIXER: PanelId = PanelId::new("lab_mixer");
pub const HEATER: PanelId = PanelId::new("lab_heater");
pub const CENTRIFUGE: PanelId = PanelId::new("lab_centrifuge");
pub const FILLER: PanelId = PanelId::new("lab_filler");
pub const DUMMY: PanelId = PanelId::new("lab_dummy");

/// The kettle's station id in `Lab`.
pub const STATION: &str = "dev_mixer";
/// Slots in the player's hands (any item, stacks of 64). TODO(initiator): a real inventory.
pub const HAND_SLOTS: usize = 8;
/// The longest run on the centrifuge's time slider (s).
pub const MAX_SPIN_S: f64 = 20.0;
/// Skill gained per finished step before the 1.5 power (`skill::gain`). TODO(initiator).
pub const GAIN_BASE: f64 = 40.0;

/// What the player and the cabinets hold: the payload of `container_core` items here.
#[derive(Clone, Debug, PartialEq)]
pub enum LabItem {
    /// A base or an ingredient of the lab (a commodity).
    Ingredient(CommodityId),
    Batch(Batch),
    Syringe(Syringe),
    /// Opens a secure locker.
    Key,
    /// A first aid item: its kind is a treatment id (`bandage`, `medkit`).
    FirstAid(String),
}

pub fn ingredient_item(id: &CommodityId) -> Item<LabItem> {
    Item::new(id.as_str(), &["small", "ingredient"], 64).with_payload(LabItem::Ingredient(id.clone()))
}
pub fn batch_item(b: Batch) -> Item<LabItem> {
    Item::new("batch", &["medium", "batch"], 1).with_payload(LabItem::Batch(b))
}
pub fn syringe_item(s: Syringe) -> Item<LabItem> {
    Item::new("syringe", &["small", "medical", "syringe"], 1).with_payload(LabItem::Syringe(s))
}
/// A bandage or a medkit (a treatment of `health/health.json`).
pub fn first_aid_item(kind: &str) -> Item<LabItem> {
    Item::new(kind, &["small", "medical", "first_aid"], 16).with_payload(LabItem::FirstAid(kind.to_string()))
}
pub fn key_item() -> Item<LabItem> {
    Item::new(container_core::presets::ID_CARD, &["small", container_core::presets::ID_CARD], 1).with_payload(LabItem::Key)
}

/// What the player holds: a small bag of slots.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Hands(pub Container<LabItem>);

impl Default for Hands {
    fn default() -> Self {
        Hands(Container::new(vec![SlotRule::any(64); HAND_SLOTS], Access::Open))
    }
}

impl Hands {
    /// The tags of everything held (a key opens what needs its tag).
    pub fn carried(&self) -> Vec<Tag> {
        let mut out: Vec<Tag> = (0..self.0.len()).filter_map(|i| self.0.slot(i)).flat_map(|s| s.item.tags.iter().cloned()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The first slot whose payload matches.
    pub fn find(&self, f: impl Fn(&LabItem) -> bool) -> Option<usize> {
        (0..self.0.len()).find(|&i| self.0.slot(i).and_then(|s| s.item.payload.as_ref()).is_some_and(&f))
    }

    /// Takes one item out of a slot and gives its payload.
    pub fn take_one(&mut self, slot: usize) -> Option<LabItem> {
        self.0.take(&[], slot, 1).ok().and_then(|s| s.item.payload)
    }

    /// Puts one item in; false (and nothing changed) when it does not fit.
    pub fn give(&mut self, item: Item<LabItem>) -> bool {
        matches!(self.0.try_insert(&[], Stack::new(item, 1)), Ok(None))
    }

    /// Room for one more single item (a batch or a syringe needs a free slot).
    pub fn has_free_slot(&self) -> bool {
        self.0.capacity().free_slots() > 0
    }
}

/// The lab's devices and what they last said.
#[derive(Resource)]
pub struct ScienceLab {
    pub lc: LabContent,
    pub tuning: DeviceTuning,
    pub lab: Lab,
    /// Seconds since the kettle's brew was done: the needle's time.
    pub needle_t: f64,
    pub heater: Heater,
    pub centrifuge: Centrifuge,
    /// The centrifuge's settings for the next start.
    pub spin_rpm: f64,
    pub spin_s: f64,
    pub filler: Option<Batch>,
    pub dice: Dice,
    /// The last message per panel.
    pub lines: HashMap<PanelId, (String, Tone)>,
    pub last_dose: Option<Dose>,
}

impl ScienceLab {
    pub fn new(lc: LabContent, tuning: DeviceTuning) -> ScienceLab {
        let heater = Heater::new(&tuning);
        let spin_rpm = (tuning.centrifuge.sweet_low_rpm + tuning.centrifuge.sweet_high_rpm) * 0.5;
        let spin_s = tuning.centrifuge.separate_s;
        ScienceLab {
            lc,
            tuning,
            lab: Lab::new(),
            needle_t: 0.0,
            heater,
            centrifuge: Centrifuge::new(),
            spin_rpm,
            spin_s,
            filler: None,
            // TODO(initiator): seeded per run for repeatable scenarios; a real game seeds from the save.
            dice: Dice::new(0x5C1E_7CE),
            lines: HashMap::new(),
            last_dose: None,
        }
    }

    pub fn kettle(&self) -> Kettle {
        self.lab.kettle(STATION).cloned().unwrap_or(Kettle::Empty)
    }

    fn say(&mut self, panel: PanelId, text: impl Into<String>, tone: Tone) {
        self.lines.insert(panel, (text.into(), tone));
    }
}

/// The dummy: records the doses it got.
#[derive(Default)]
struct Dummy(Vec<Dose>);

impl Target for Dummy {
    fn receive(&mut self, dose: &Dose) {
        self.0.push(dose.clone());
    }
}

/// A device box: (id, panel, name, x in the lab zone, colour, panel height in panel pixels).
const DEVICES: [(&str, PanelId, &str, f64, [f32; 3], f64); 5] = [
    ("lab_mixer", MIXER, "Mixer", -6.0, [0.30, 0.62, 0.52], 560.0),
    ("lab_heater", HEATER, "Heater", -3.0, [0.78, 0.38, 0.22], 520.0),
    ("lab_centrifuge", CENTRIFUGE, "Centrifuge", 0.0, [0.42, 0.45, 0.70], 560.0),
    ("lab_filler", FILLER, "Syringe filler", 3.0, [0.80, 0.80, 0.85], 420.0),
    ("lab_dummy", DUMMY, "Test dummy", 6.0, [0.85, 0.70, 0.45], 360.0),
];
/// The row of devices in the lab zone: metres north of its centre, 3 m apart, between the test
/// console (y -4.5) and the medbay (y 3). The LAB zone is 14 m wide: x -7..7.
pub const ROW_Y: f64 = -1.0;
/// Panel width in panel pixels; every panel is 1 m wide.
pub const PANEL_W: f64 = 560.0;

pub fn plugin(app: &mut App) {
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        for (id, panel, name, x, colour, h) in DEVICES {
            let dummy = panel == DUMMY;
            devices.add(PlaceholderBox {
                id: id.into(),
                zone: LAB,
                local_pos: [x, ROW_Y],
                size: if dummy { [0.6, 1.0, 0.5] } else { [1.4, 1.0, 0.8] },
                turn_deg: 0.0,
                colour,
                panel: Some(PanelSpec { id: panel, name: name.into(), size_m: [1.0, h / PANEL_W], at: [0.0, 1.0 + 0.5 * h / PANEL_W, 0.2], tilt_deg: 15.0 }),
            });
        }
    }
    let gp = app.world().resource::<Gameplay>();
    let files = crate::content_files("gameplay");
    let lc = LabContent::load(&files, &gp.kernel).unwrap_or_else(|e| panic!("content/gameplay (lab):\n  {}", e.join("\n  ")));
    let tuning = DeviceTuning::load(&files, &gp.kernel).unwrap_or_else(|e| panic!("content/gameplay (lab devices):\n  {}", e.join("\n  ")));
    app.insert_resource(ScienceLab::new(lc, tuning));
    app.init_resource::<Hands>();
    app.add_systems(FixedUpdate, (read_clicks, tick, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

/// A commodity's name from the text table.
pub fn commodity_name(gp: &Gameplay, id: &CommodityId) -> String {
    gp.text(&TextKey::new(format!("commodity.{id}.name")))
}

fn effect_names(gp: &Gameplay, fx: &std::collections::BTreeSet<gameplay_core::EffectId>) -> String {
    if fx.is_empty() {
        return "nothing".into();
    }
    fx.iter().map(|e| gp.text(&TextKey::new(format!("effect.{e}.name")))).collect::<Vec<_>>().join(", ")
}

/// The local player's medical skill: what the devices and the dose roll use.
pub fn medical_of(st: &HealthState) -> f64 {
    st.skills.of(LOCAL).medical
}

/// One finished step: the local player's medical skill grows by `skill::gain`.
pub fn gain(st: &mut HealthState) {
    let now = medical_of(st);
    st.skills.gain(LOCAL, Skill::Medical, skill::gain(now, GAIN_BASE));
}

fn refusal_text(r: &production_core::LabRefusal) -> &'static str {
    use production_core::LabRefusal::*;
    match r {
        UnknownGoods => "that does not go in a mixer",
        SecondBase => "one base per batch",
        IngredientBeforeBase => "the base goes in first",
        TooManyIngredients => "the mixer is full",
        NoBase => "no base in the mixer",
        Busy => "the mixer is busy",
        NotDone => "the brew is not done",
    }
}

/// The panels' clicks.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut s: ResMut<ScienceLab>, mut hands: ResMut<Hands>, mut st: ResMut<HealthState>, mut gp: ResMut<Gameplay>) {
    for c in clicks.read() {
        let p = c.panel.clone();
        let (verb, arg) = c.action.split_once(':').unwrap_or((c.action.as_str(), ""));
        let medical = medical_of(&st);
        let speed = skill::speed(medical, s.tuning.skill.required, &s.tuning.skill);
        match (p.as_str(), verb) {
            ("lab_mixer", "add") => {
                let Ok(slot) = arg.parse::<usize>() else { continue };
                let Some(LabItem::Ingredient(id)) = hands.0.slot(slot).and_then(|x| x.item.payload.clone()) else { continue };
                let s = &mut *s;
                match s.lab.insert(&s.lc, STATION, &id) {
                    Ok(()) => {
                        hands.take_one(slot);
                        let n = commodity_name(&gp, &id);
                        s.say(MIXER, format!("{n} goes in"), Tone::Normal);
                    }
                    Err(r) => s.say(MIXER, format!("refused: {}", refusal_text(&r)), Tone::Warn),
                }
            }
            ("lab_mixer", "start") => {
                let s = &mut *s;
                match s.lab.start_at_speed(&s.lc, STATION, speed) {
                    Ok(()) => s.say(MIXER, format!("brewing ({:.1} s)", s.lc.tuning.brew_s / speed), Tone::Normal),
                    Err(r) => s.say(MIXER, format!("refused: {}", refusal_text(&r)), Tone::Warn),
                }
            }
            ("lab_mixer", "release") => {
                let s = &mut *s;
                let t = s.needle_t;
                match s.lab.release(&s.lc, STATION, t) {
                    Ok(out) => {
                        for o in out {
                            if let LabOutcome::Notice(n) = o {
                                gp.notify(n);
                            }
                        }
                        let q = match s.kettle() {
                            Kettle::Ready { product, .. } => product.quality,
                            _ => 0.0,
                        };
                        gain(&mut st);
                        let needle = production_core::needle(t, s.lc.tuning.needle_period_s);
                        info!("lab: released at needle {needle:.3}, quality {q:.3}");
                        s.say(MIXER, format!("released at {needle:.2}: quality {q:.2}"), if q > 0.8 { Tone::Good } else { Tone::Warn });
                    }
                    Err(r) => s.say(MIXER, format!("refused: {}", refusal_text(&r)), Tone::Warn),
                }
            }
            ("lab_mixer", "take") => {
                if !hands.has_free_slot() {
                    s.say(MIXER, "your hands are full", Tone::Warn);
                    continue;
                }
                let s = &mut *s;
                if let Some(b) = s.lab.take_batch(STATION, &s.tuning) {
                    s.say(MIXER, format!("batch taken: {:.1} doses", b.volume), Tone::Normal);
                    hands.give(batch_item(b));
                }
            }
            ("lab_heater", "put") => {
                if s.heater.batch().is_some() {
                    continue;
                }
                let Some(slot) = hands.find(|x| matches!(x, LabItem::Batch(_))) else { continue };
                if let Some(LabItem::Batch(b)) = hands.take_one(slot) {
                    let _ = s.heater.load(b);
                    s.say(HEATER, "batch in", Tone::Normal);
                }
            }
            ("lab_heater", "take") => {
                if !hands.has_free_slot() {
                    s.say(HEATER, "your hands are full", Tone::Warn);
                } else if let Some(b) = s.heater.unload() {
                    hands.give(batch_item(b));
                    s.say(HEATER, "batch taken", Tone::Normal);
                }
            }
            ("lab_heater", "setpoint") => {
                let v = c.value.unwrap_or(0.0) as f64;
                let s = &mut *s;
                let (lo, hi) = (s.tuning.ambient_c, s.tuning.heater.max_setpoint_c);
                s.heater.set_setpoint(lo + v * (hi - lo), &s.tuning);
            }
            ("lab_centrifuge", "put") => {
                if s.centrifuge.batch().is_some() {
                    continue;
                }
                let Some(slot) = hands.find(|x| matches!(x, LabItem::Batch(_))) else { continue };
                if let Some(LabItem::Batch(b)) = hands.take_one(slot) {
                    let _ = s.centrifuge.load(b);
                    s.say(CENTRIFUGE, "batch in", Tone::Normal);
                }
            }
            ("lab_centrifuge", "take") => {
                if !hands.has_free_slot() {
                    s.say(CENTRIFUGE, "your hands are full", Tone::Warn);
                } else if let Some(b) = s.centrifuge.unload() {
                    hands.give(batch_item(b));
                    s.say(CENTRIFUGE, "batch taken", Tone::Normal);
                }
            }
            ("lab_centrifuge", "rpm") => {
                let s = &mut *s;
                s.spin_rpm = c.value.unwrap_or(0.0) as f64 * s.tuning.centrifuge.max_rpm;
                s.centrifuge.set_rpm(s.spin_rpm, &s.tuning);
            }
            ("lab_centrifuge", "time") => s.spin_s = c.value.unwrap_or(0.0) as f64 * MAX_SPIN_S,
            ("lab_centrifuge", "start") => {
                let s = &mut *s;
                match s.centrifuge.start(s.spin_rpm, s.spin_s, &s.tuning) {
                    Ok(()) => s.say(CENTRIFUGE, format!("spinning at {:.0} rpm for {:.1} s", s.spin_rpm, s.spin_s), Tone::Normal),
                    Err(_) => s.say(CENTRIFUGE, "nothing to spin", Tone::Warn),
                }
            }
            ("lab_centrifuge", "stop") => {
                s.centrifuge.stop();
                s.say(CENTRIFUGE, "stopped", Tone::Normal);
            }
            ("lab_filler", "put") => {
                if s.filler.is_some() {
                    continue;
                }
                let Some(slot) = hands.find(|x| matches!(x, LabItem::Batch(_))) else { continue };
                if let Some(LabItem::Batch(b)) = hands.take_one(slot) {
                    s.filler = Some(b);
                    s.say(FILLER, "batch in", Tone::Normal);
                }
            }
            ("lab_filler", "take") => {
                if !hands.has_free_slot() {
                    s.say(FILLER, "your hands are full", Tone::Warn);
                } else if let Some(b) = s.filler.take() {
                    hands.give(batch_item(b));
                    s.say(FILLER, "batch taken", Tone::Normal);
                }
            }
            ("lab_filler", "discard") => {
                if s.filler.take().is_some() {
                    s.say(FILLER, "the rest is poured out", Tone::Normal);
                }
            }
            ("lab_filler", "fill") => {
                if !hands.has_free_slot() {
                    s.say(FILLER, "your hands are full", Tone::Warn);
                    continue;
                }
                let s = &mut *s;
                let Some(b) = s.filler.as_mut() else { continue };
                let first = b.bonus_tiers.is_none();
                match fill_syringe(b, s.tuning.syringe.capacity, medical, &s.tuning, &mut s.dice) {
                    Ok(y) => {
                        let tiers = b.bonus_tiers.unwrap_or(0);
                        info!("lab: filled {} doses at quality {:.3} (+{tiers} tiers)", y.doses, y.product.quality);
                        let line = format!("{} doses at quality {:.2} (+{tiers} skill tiers)", y.doses, y.product.quality);
                        hands.give(syringe_item(y));
                        if first {
                            gain(&mut st);
                        }
                        s.say(FILLER, line, Tone::Good);
                    }
                    Err(_) => s.say(FILLER, "not enough for one dose", Tone::Warn),
                }
            }
            ("lab_dummy", "inject") => {
                let Some(slot) = hands.find(|x| matches!(x, LabItem::Syringe(y) if y.doses > 0)) else { continue };
                let Some(LabItem::Syringe(mut y)) = hands.take_one(slot) else { continue };
                let s = &mut *s;
                let mut dummy = Dummy::default();
                if let Ok(d) = y.inject(&mut dummy, medical, &s.tuning, &mut s.dice) {
                    let fx = effect_names(&gp, &d.effects);
                    let key = if d.success { "lab.injected" } else { "lab.injected_weak" };
                    gp.notify(Notice::new(NoticeKind::Updated, key).arg("effects", Arg::Text(fx.clone())).arg("strength", Arg::Number((d.strength * 100.0).round() as i64)));
                    info!("lab: injected {fx} at strength {:.2} (success {})", d.strength, d.success);
                    s.say(DUMMY, format!("{fx} at {:.0} %{}", d.strength * 100.0, if d.success { "" } else { " (shaky hand)" }), if d.success { Tone::Good } else { Tone::Warn });
                    s.last_dose = Some(d);
                    gain(&mut st);
                }
                if y.doses > 0 {
                    hands.give(syringe_item(y));
                }
            }
            _ => {}
        }
    }
}

/// Time: the brew, the needle, the heater and the centrifuge.
pub fn tick(time: Res<Time>, mut s: ResMut<ScienceLab>, mut st: ResMut<HealthState>) {
    let dt = time.delta_secs_f64();
    let medical = medical_of(&st);
    let s = &mut *s;
    let was_done = matches!(s.kettle(), Kettle::Done { .. });
    s.lab.step(dt);
    match s.kettle() {
        Kettle::Done { .. } if was_done => s.needle_t += dt,
        Kettle::Done { .. } => {
            s.needle_t = 0.0;
            s.say(MIXER, "done: release at the sweet spot", Tone::Good);
        }
        _ => s.needle_t = 0.0,
    }
    let before = s.heater.batch().map(|b| (b.heat, b.burnt));
    s.heater.step(dt, medical, &s.tuning);
    if let (Some((heat0, burnt0)), Some(b)) = (before, s.heater.batch()) {
        if !burnt0 && b.burnt {
            s.say(HEATER, "BURNT: the quality halves", Tone::Bad);
        } else if heat0 < 1.0 && b.heat >= 1.0 {
            gain(&mut st);
            s.say(HEATER, "heated through: quality up", Tone::Good);
        }
    }
    let (state0, sep0) = (s.centrifuge.state(), s.centrifuge.batch().map(|b| b.separation).unwrap_or(0.0));
    s.centrifuge.step(dt, medical, &s.tuning);
    let sep1 = s.centrifuge.batch().map(|b| b.separation).unwrap_or(0.0);
    if sep0 < 1.0 && sep1 >= 1.0 {
        gain(&mut st);
    }
    if state0 == Spin::Spinning {
        match s.centrifuge.state() {
            Spin::Tripped => s.say(CENTRIFUGE, format!("TRIPPED: it wobbled and spilled {:.0} %", s.tuning.centrifuge.trip_loss * 100.0), Tone::Bad),
            Spin::Done => s.say(CENTRIFUGE, format!("done: separated {:.0} %", sep1 * 100.0), if sep1 >= 1.0 { Tone::Good } else { Tone::Warn }),
            _ => {}
        }
    }
}

/// The five panels from the state.
pub fn write_panels(s: Res<ScienceLab>, hands: Res<Hands>, st: Res<HealthState>, gp: Res<Gameplay>, mut panels: ResMut<Panels>) {
    let names = |id: &CommodityId| commodity_name(&gp, id);
    let fx = |b: &Batch| effect_names(&gp, &b.effects);
    let v = View { s: &s, hands: &hands, medical: medical_of(&st), names: &names, effects: &fx };
    panels.set(MIXER, mixer_panel(&v));
    panels.set(HEATER, heater_panel(&v));
    panels.set(CENTRIFUGE, centrifuge_panel(&v));
    panels.set(FILLER, filler_panel(&v));
    panels.set(DUMMY, dummy_panel(&v));
}
