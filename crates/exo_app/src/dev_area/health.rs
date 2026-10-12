//! Health in the dev area (#287, #254): the player's health (`health_core`), hurt by the
//! engineer corner's fire (burn) and by bad air in its rooms (oxygen low), a fracture from a hard
//! fall; a medbay in the LAB zone with a bed patient who starts burnt, where syringes and first aid
//! from the hands treat the player or the patient; a HUD readout; death shows a banner and the
//! player comes back at the pads after a few seconds with a little less skill.
use super::engineer::{self, area_of, Engineering};
use super::science::{first_aid_item, hands_line, item_label, syringe_item, Hands, LabItem, ScienceLab, GAIN_BASE};
use super::skills::DevSkills;
use super::*;
use crate::controls::Actions;
use crate::displays::{DisplayAction, Panels};
use crate::gameplay::Gameplay;
use crate::walker::Player;
use display_core::{bar, button, column, label, row, space, title, Panel, Tone};
use gameplay_core::notice::{Arg, Notice, NoticeKind, Weight};
use gameplay_core::CommodityId;
use health_core::{Env, Health, HealthEvent, HealthTuning, Life, Part, Patient, Treater};
use production_core::skill;

pub const MEDBAY: PanelId = PanelId::new("lab_medbay");
pub const MEDBAY_NAME: &str = "Medbay";
/// Where the medbay console and the bed stand in the lab zone.
pub const MEDBAY_AT: [f64; 2] = [19.0, -15.0];
pub const BED_AT: [f64; 2] = [23.0, -15.0];
pub const PANEL_W: f64 = 560.0;
pub const PANEL_H: f64 = 600.0;
/// Burn strength per unit of fire damage. TODO(initiator).
pub const BURN_PER_DAMAGE: f64 = 1.0;
/// Seconds dead before the respawn. TODO(initiator).
pub const RESPAWN_S: f64 = 5.0;
/// Where the dead come back: in the pad strip, beside the pad (m from its centre).
pub const RESPAWN_AT: [f64; 2] = [10.0, 0.0];
/// Landing faster than this (m/s down) cracks the legs, by this much per m/s over. TODO(initiator).
pub const FALL_SAFE: f64 = 9.0;
pub const FRACTURE_PER_MS: f64 = 12.0;
/// The bed patient's burn at the start (on the arms).
pub const PATIENT_BURN: f64 = 60.0;

/// The text keys this module uses.
pub const TEXT_KEYS: &[&str] = &["notice.health.unconscious", "notice.health.died", "notice.health.respawn", "notice.health.fracture"];

/// Who the medbay treats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Me,
    Bed,
}

/// Counters for checks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub burned: f64,
    pub fractures: u32,
    pub deaths: u32,
    pub respawns: u32,
    pub last_cause: String,
}

/// The player's and the bed patient's health.
#[derive(Resource, Clone, Debug)]
pub struct Medical {
    pub tuning: HealthTuning,
    pub me: Health,
    pub bed: Health,
    pub who: Who,
    /// Seconds since death, while dead.
    pub dead_for: Option<f64>,
    pub line: (String, Tone),
    pub stats: Stats,
    /// The air at the player (0..1), for the readout.
    pub air: f64,
    grounded: bool,
    down_speed: f64,
}

impl Medical {
    pub fn new(tuning: HealthTuning) -> Medical {
        let me = Health::new(&tuning, 0x4EA1);
        let mut bed = Health::new(&tuning, 0xBED);
        bed.add(&tuning, "burn", Some(Part::Arms), PATIENT_BURN);
        Medical { tuning, me, bed, who: Who::Me, dead_for: None, line: Default::default(), stats: Stats::default(), air: 1.0, grounded: true, down_speed: 0.0 }
    }

    pub fn target(&self) -> &Health {
        match self.who {
            Who::Me => &self.me,
            Who::Bed => &self.bed,
        }
    }
}

pub fn plugin(app: &mut App) {
    {
        let mut d = app.world_mut().resource_mut::<DevAreaDevices>();
        let h = PANEL_H / PANEL_W;
        d.add(PlaceholderBox {
            id: "lab_medbay".into(),
            zone: LAB,
            local_pos: MEDBAY_AT,
            size: [1.4, 1.0, 0.8],
            turn_deg: 0.0,
            colour: [0.85, 0.92, 0.95],
            panel: Some(PanelSpec { id: MEDBAY, name: MEDBAY_NAME.into(), size_m: [1.0, h], at: [0.0, 1.0 + 0.5 * h, 0.2], tilt_deg: 15.0 }),
        });
        d.add(PlaceholderBox { id: "lab_bed".into(), zone: LAB, local_pos: BED_AT, size: [1.0, 0.6, 2.1], turn_deg: 0.0, colour: [0.92, 0.92, 0.88], panel: None });
        d.add(PlaceholderBox { id: "lab_bed_patient".into(), zone: LAB, local_pos: [BED_AT[0], BED_AT[1] + 0.1], size: [0.5, 0.85, 1.6], turn_deg: 0.0, colour: [0.95, 0.6, 0.45], panel: None });
    }
    let files = crate::content_files("gameplay");
    let tuning = HealthTuning::load(&files).unwrap_or_else(|e| panic!("content/gameplay (health):\n  {}", e.join("\n  ")));
    app.insert_resource(Medical::new(tuning));
    app.add_systems(FixedUpdate, limit_moves.after(crate::controls::resolve_actions).before(crate::walker::walker_step).in_set(crate::phases::Fx::Walker));
    app.add_systems(FixedUpdate, (step_health.after(engineer::step), read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_hud);
    app.add_systems(Update, draw_hud.in_set(crate::phases::Frame::Hud));
}

/// The body limits the walker: slower with a fracture, no sprint, nothing while down or dead.
pub fn limit_moves(med: Res<Medical>, mut actions: ResMut<Actions>) {
    let m = med.me.mobility(&med.tuning);
    if !m.conscious || med.me.state() == Life::Dead {
        actions.move_dir = bevy::math::DVec3::ZERO;
        actions.run = false;
        actions.jump = false;
        actions.look = Vec2::ZERO;
        return;
    }
    actions.move_dir.x *= m.walk;
    actions.move_dir.z *= m.walk;
    actions.run &= m.sprint;
}

fn note(gp: &mut Gameplay, kind: NoticeKind, key: &str) -> Notice {
    let n = Notice::new(kind, key);
    gp.notify(n.clone());
    n
}

/// One step: fire burns, the room's air, a hard landing; afflictions run; death and respawn.
#[allow(clippy::too_many_arguments)]
pub fn step_health(
    time: Res<Time>,
    mut med: ResMut<Medical>,
    mut eng: Option<ResMut<Engineering>>,
    mut skills: ResMut<DevSkills>,
    mut gp: ResMut<Gameplay>,
    mut players: Query<&mut Player>,
    (area, planet): (Res<DevArea>, Res<PlanetRes>),
) {
    let dt = time.delta_secs_f64();
    if dt <= 0.0 {
        return;
    }
    let med = &mut *med;
    let t = med.tuning.clone();
    let Ok(mut pl) = players.single_mut() else { return };
    let on_foot = pl.ship.is_none() && !pl.seated;

    // Fire.
    if let Some(e) = eng.as_deref_mut() {
        let burn = std::mem::take(&mut e.player_burn) * BURN_PER_DAMAGE;
        if burn > 0.0 && med.me.state() != Life::Dead {
            med.me.add(&t, "burn", Some(Part::Torso), burn);
            med.stats.burned += burn;
        }
    }
    // Air: the engineer corner's rooms have their own; everywhere else is open air.
    med.air = 1.0;
    if on_foot && let (Some(frame), Some(e)) = (area.frame, eng.as_deref()) {
        let p = area_of(&frame, &planet, pl.w.pos);
        if ENGINEERING.contains([p.x, p.z]) {
            med.air = e.deck.room(e.room_at(p)).oxygen_fraction();
        }
    }
    // A hard landing.
    if on_foot {
        let up = planet.up(pl.w.pos);
        let down = -pl.w.vel.dot(up);
        if pl.w.grounded && !med.grounded && med.down_speed > FALL_SAFE {
            let s = (med.down_speed - FALL_SAFE) * FRACTURE_PER_MS;
            med.me.add(&t, "fracture", Some(Part::Legs), s);
            med.stats.fractures += 1;
            note(&mut gp, NoticeKind::Warning, "notice.health.fracture");
        }
        med.grounded = pl.w.grounded;
        med.down_speed = down;
    }

    for ev in med.me.step(&t, dt, Env { oxygen: med.air }) {
        match ev {
            HealthEvent::Unconscious => {
                note(&mut gp, NoticeKind::Warning, "notice.health.unconscious");
            }
            HealthEvent::Died { cause } => {
                med.stats.deaths += 1;
                med.stats.last_cause = cause.clone();
                med.dead_for = Some(0.0);
                gp.notify(Notice::new(NoticeKind::Failed, "notice.health.died").arg("cause", Arg::Text(cause.replace('_', " "))).weight(Weight::Banner));
            }
            _ => {}
        }
    }
    med.bed.step(&t, dt, Env { oxygen: 1.0 });

    // Respawn at the pads.
    if let Some(s) = med.dead_for.as_mut() {
        *s += dt;
        if *s >= RESPAWN_S {
            med.dead_for = None;
            if let HealthEvent::Respawn { skills_lost } = med.me.respawn(&t) {
                let sk = &mut *skills;
                for k in [&mut sk.piloting, &mut sk.weapons, &mut sk.mechanical, &mut sk.electrical, &mut sk.medical] {
                    *k *= 1.0 - skills_lost;
                }
                med.stats.respawns += 1;
                gp.notify(Notice::new(NoticeKind::Updated, "notice.health.respawn").arg("lost", Arg::Number((skills_lost * 100.0).round() as i64)));
                if let Some((pos, _)) = area.ground(&planet, PADS.point(RESPAWN_AT), 0.1) {
                    pl.ship = None;
                    pl.seated = false;
                    pl.w.pos = pos;
                    pl.w.halt();
                    pl.w.grounded = false;
                }
                med.grounded = false;
                med.down_speed = 0.0;
            }
        }
    }
}

/// The medbay panel's clicks: who to treat, and use an item from the hands on them.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut med: ResMut<Medical>, mut hands: ResMut<Hands>, mut skills: ResMut<DevSkills>, mut lab: ResMut<ScienceLab>) {
    for c in clicks.read().filter(|c| c.panel == MEDBAY) {
        let med = &mut *med;
        if c.action == "who" {
            med.who = if med.who == Who::Me { Who::Bed } else { Who::Me };
            continue;
        }
        let Some(slot) = c.action.strip_prefix("use:").and_then(|s| s.parse::<usize>().ok()) else { continue };
        let skill_now = skills.medical;
        let t = med.tuning.clone();
        let target = match med.who {
            Who::Me => &mut med.me,
            Who::Bed => &mut med.bed,
        };
        let who = if med.who == Who::Me { "you" } else { "the patient" };
        let Some(item) = hands.take_one(slot) else { continue };
        let (line, ok) = match item {
            LabItem::Syringe(mut y) => {
                let lab = &mut *lab;
                let mut p = Patient::new(target, &t, skill_now);
                let r = y.inject(&mut p, skill_now, &lab.tuning, &mut lab.dice);
                let (last, statuses) = (p.last.clone(), p.statuses.clone());
                if y.doses > 0 {
                    hands.give(syringe_item(y));
                }
                match (r, last) {
                    (Ok(d), Some(tr)) => (format!("{who}: -{:.0} ({:.0} % dose){}", tr.reduced, d.strength * 100.0, if statuses.is_empty() { String::new() } else { format!(", {}", statuses.join(", ")) }), tr.reduced > 0.0),
                    (Ok(_), None) => (format!("{who}: nothing to treat, {}", if statuses.is_empty() { "no effect".to_string() } else { statuses.join(", ") }), false),
                    (Err(_), _) => ("the syringe is empty".into(), false),
                }
            }
            LabItem::FirstAid(kind) => match target.treat(&t, &kind, Treater { skill: skill_now, potency: 1.0, success: None }) {
                Some(r) => (format!("{who}: {kind} -{:.0}{}{}", r.reduced, if r.success { "" } else { " (fumbled)" }, if r.overdose.is_some() { ", poisoned!" } else { "" }), r.reduced > 0.0),
                None => {
                    hands.give(first_aid_item(&kind));
                    ("that does not treat anything".into(), false)
                }
            },
            other => {
                // Not a medicine: back into the hands.
                let item = match other {
                    LabItem::Ingredient(id) => super::science::ingredient_item(&id),
                    LabItem::Batch(b) => super::science::batch_item(b),
                    LabItem::Key => super::science::key_item(),
                    _ => unreachable!(),
                };
                hands.give(item);
                ("that is not a medicine".into(), false)
            }
        };
        if ok {
            skills.medical = (skills.medical + skill::gain(skills.medical, GAIN_BASE)).min(100.0);
        }
        info!("medbay: {line}");
        med.line = (line, if ok { Tone::Good } else { Tone::Warn });
    }
}

/// The affliction list of a body: "burn (torso) 42".
pub fn affliction_lines(h: &Health) -> Vec<String> {
    let mut v: Vec<_> = h.afflictions().iter().collect();
    v.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    v.iter().map(|a| format!("{}{} {:.0}", a.id.replace('_', " "), a.part.map(|p| format!(" ({})", p.name())).unwrap_or_default(), a.strength)).collect()
}

fn state_line(h: &Health, dead_for: Option<f64>) -> (String, Tone) {
    match h.state() {
        Life::Conscious if h.vitality() < h.max_vitality() * 0.5 => ("hurt".into(), Tone::Warn),
        Life::Conscious => ("conscious".into(), Tone::Good),
        Life::Unconscious => ("UNCONSCIOUS".into(), Tone::Bad),
        Life::Dead => (format!("DEAD{}", dead_for.map(|s| format!(": back in {:.0} s", (RESPAWN_S - s).max(0.0))).unwrap_or_default()), Tone::Bad),
    }
}

/// The medbay panel.
pub fn medbay_panel(med: &Medical, hands: &Hands, names: &dyn Fn(&CommodityId) -> String) -> Panel {
    let h = med.target();
    let max = h.max_vitality();
    let (state, tone) = state_line(h, if med.who == Who::Me { med.dead_for } else { None });
    let mut items = vec![
        title("MEDBAY").fixed(36.0),
        row(8.0, vec![label(if med.who == Who::Me { "PATIENT: you" } else { "PATIENT: on the bed" }), button("switch", "who").fixed(120.0)]).fixed(32.0),
        bar((h.vitality() / max).clamp(0.0, 1.0), format!("VITALITY {:.0} / {max:.0}", h.vitality())).thresholds(Some(0.5), Some(0.25)).fixed(30.0),
        label(state).tone(tone).fixed(22.0),
        label("AFFLICTIONS").tone(Tone::Dim).fixed(20.0),
    ];
    let lines = affliction_lines(h);
    if lines.is_empty() {
        items.push(label("none").tone(Tone::Dim).fixed(24.0));
    }
    for l in lines.into_iter().take(5) {
        items.push(label(l).tone(Tone::Warn).fixed(24.0));
    }
    items.push(label("FROM YOUR HANDS").tone(Tone::Dim).fixed(20.0));
    let meds: Vec<usize> = (0..hands.0.len()).filter(|&i| matches!(hands.0.slot(i).and_then(|s| s.item.payload.as_ref()), Some(LabItem::Syringe(_) | LabItem::FirstAid(_)))).take(4).collect();
    if meds.is_empty() {
        items.push(label("no medicine: syringes, bandages, medkits").tone(Tone::Dim).fixed(28.0));
    }
    for i in meds {
        let s = hands.0.slot(i).unwrap();
        items.push(row(8.0, vec![label(format!("{} x{}", item_label(&s.item, names), s.count)), button("use", format!("use:{i}")).enabled(h.state() != Life::Dead).fixed(90.0)]).fixed(28.0));
    }
    items.push(label(med.line.0.clone()).tone(med.line.1).fixed(22.0));
    items.push(label(hands_line(hands)).tone(Tone::Dim).fixed(20.0));
    items.push(space());
    Panel::new(PANEL_W, PANEL_H, column(6.0, items))
}

pub fn write_panel(med: Res<Medical>, hands: Res<Hands>, gp: Res<Gameplay>, mut panels: ResMut<Panels>) {
    let names = |id: &CommodityId| super::science::commodity_name(&gp, id);
    panels.set(MEDBAY, medbay_panel(&med, &hands, &names));
}

// ---------- window: the HUD readout ----------

#[derive(Component)]
struct HudVitality;
#[derive(Component)]
struct HudFill;
#[derive(Component)]
struct HudList;

/// Bottom left: vitality as text and a bar, then the afflictions.
fn spawn_hud(mut commands: Commands) {
    use crate::ui_theme as th;
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: px(th::MARGIN), bottom: px(th::MARGIN), flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::all(px(8)), border_radius: BorderRadius::all(px(4)), ..default() },
            BackgroundColor(th::PANEL),
        ))
        .with_children(|c| {
            c.spawn((HudVitality, Text::new(""), TextFont { font_size: FontSize::Px(th::BODY), ..default() }, TextColor(th::INK)));
            c.spawn((Node { width: px(200), height: px(8), border: UiRect::all(px(1)), ..default() }, BorderColor::all(th::DIM)))
                .with_children(|b| {
                    b.spawn((HudFill, Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(th::GOOD)));
                });
            c.spawn((HudList, Text::new(""), TextFont { font_size: FontSize::Px(th::SMALL), ..default() }, TextColor(th::GOLD)));
        });
}

#[allow(clippy::type_complexity)]
fn draw_hud(
    med: Res<Medical>,
    mut texts: ParamSet<(Query<(&mut Text, &mut TextColor), With<HudVitality>>, Query<&mut Text, With<HudList>>)>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<HudFill>>,
) {
    use crate::ui_theme as th;
    let h = &med.me;
    let share = (h.vitality() / h.max_vitality()).clamp(0.0, 1.0);
    let (state, tone) = state_line(h, med.dead_for);
    let colour = match tone {
        Tone::Bad => th::BAD,
        Tone::Warn => th::GOLD,
        _ => th::INK,
    };
    if let Ok((mut t, mut c)) = texts.p0().single_mut() {
        t.0 = format!("VITALITY {:.0}   {state}{}", h.vitality(), if med.air < 0.3 { "   NO AIR" } else { "" });
        c.0 = colour;
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        t.0 = affliction_lines(h).join("\n");
    }
    if let Ok((mut n, mut bg)) = fill.single_mut() {
        n.width = percent((share * 100.0) as f32);
        bg.0 = if share > 0.5 { th::GOOD } else if share > 0.25 { th::GOLD } else { th::BAD };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_key_is_in_the_table() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../../../content/gameplay/text/en.json")).unwrap();
        let missing: Vec<_> = TEXT_KEYS.iter().filter(|k| v.get(**k).is_none()).collect();
        assert!(missing.is_empty(), "en.json lacks {missing:?}");
    }

    #[test]
    fn the_medbay_panel_fits_with_many_afflictions_and_medicine() {
        let files = crate::content_files("gameplay");
        let t = HealthTuning::load(&files).unwrap();
        let mut med = Medical::new(t.clone());
        for (id, part) in [("burn", Some(Part::Torso)), ("burn", Some(Part::Arms)), ("bleeding", Some(Part::Legs)), ("fracture", Some(Part::Legs)), ("poison", None), ("giggly", None)] {
            med.me.add(&t, id, part, 20.0);
        }
        let mut hands = Hands::default();
        for k in ["bandage", "medkit"] {
            hands.give(first_aid_item(k));
        }
        let names = |id: &CommodityId| id.to_string();
        let p = medbay_panel(&med, &hands, &names);
        for w in p.layout() {
            assert!(w.rect.max().y <= p.size.y - p.padding + 1e-9, "{:?} spills out", w.widget);
        }
        assert!(p.find("use:0").is_some() && p.find("who").is_some());
        assert_eq!(med.bed.strength("burn"), PATIENT_BURN, "the bed patient starts burnt");
    }
}
