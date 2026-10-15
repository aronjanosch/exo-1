//! The medbay in the LAB zone (#287, #254): a console and a bed with a patient. Syringes and first
//! aid from the hands are used on the local player (their body in `HealthState`, the same one the
//! fire, the air and the death work on) or on the bed patient. A syringe's dose goes through
//! `Health::dose` (its treatments and status effects, an overdose as poison), first aid through
//! `Health::treat`; both are `health_core`.
//!
//! Not ported from the parallel line (#287): its fracture from a hard fall and the body parts it
//! hangs on (`health_core` here has no parts), its bleeding and medkit (no bleeding source yet), and
//! its own player health and HUD (ours are `crate::health` and `crate::health_view`).
use super::science::{batch_item, first_aid_item, gain, hands_line, ingredient_item, item_label, key_item, medical_of, syringe_item, Hands, LabItem, ScienceLab};
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::gameplay::Gameplay;
use crate::health::HealthState;
use crate::items::LOCAL;
use display_core::{bar, button, column, label, row, space, title, Panel, Tone};
use gameplay_core::CommodityId;
use health_core::{DoseResult, Env, Health, HealthTuning, Life};
use production_core::devices::{Dose, Target};

pub const MEDBAY: PanelId = PanelId::new("lab_medbay");
/// Where the medbay console and the bed stand in the LAB zone (m from its centre).
pub const MEDBAY_AT: [f64; 2] = [-4.0, 3.0];
pub const BED_AT: [f64; 2] = [3.5, 3.0];
pub const PANEL_W: f64 = 560.0;
pub const PANEL_H: f64 = 600.0;
/// The bed patient's burn at the start. TODO(initiator).
pub const PATIENT_BURN: f64 = 60.0;

/// Who the medbay treats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Me,
    Bed,
}

/// The bed patient (a body that is not a crew member) and the console's state.
#[derive(Resource)]
pub struct Medbay {
    pub bed: Health,
    pub who: Who,
    /// The last line of the console.
    pub line: (String, Tone),
}

impl Medbay {
    pub fn new(t: &HealthTuning) -> Medbay {
        let mut bed = Health::new(t, 0xBED);
        bed.add(t, "burn", PATIENT_BURN);
        Medbay { bed, who: Who::Me, line: Default::default() }
    }
}

/// A body a syringe's dose lands on: the dose's effects go through `Health::dose`.
pub struct Dosed<'a> {
    pub health: &'a mut Health,
    pub tuning: &'a HealthTuning,
    pub skill: f64,
    /// What the last dose did.
    pub last: Option<DoseResult>,
}

impl Target for Dosed<'_> {
    fn receive(&mut self, dose: &Dose) {
        let fx: Vec<&str> = dose.effects.iter().map(|e| e.as_str()).collect();
        self.last = Some(self.health.dose(self.tuning, &fx, dose.strength, dose.success, self.skill));
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
            panel: Some(PanelSpec { id: MEDBAY, name: "Medbay".into(), size_m: [1.0, h], at: [0.0, 1.0 + 0.5 * h, 0.2], tilt_deg: 15.0 }),
        });
        d.add(PlaceholderBox { id: "lab_bed".into(), zone: LAB, local_pos: BED_AT, size: [1.0, 0.6, 2.1], turn_deg: 0.0, colour: [0.92, 0.92, 0.88], panel: None });
        d.add(PlaceholderBox { id: "lab_bed_patient".into(), zone: LAB, local_pos: [BED_AT[0], BED_AT[1] + 0.1], size: [0.5, 0.85, 1.6], turn_deg: 0.0, colour: [0.95, 0.6, 0.45], panel: None });
    }
    let files = crate::content_files("gameplay");
    let tuning = HealthTuning::load(&files).unwrap_or_else(|e| panic!("content/gameplay (health):\n  {}", e.join("\n  ")));
    app.insert_resource(Medbay::new(&tuning));
    app.add_systems(FixedUpdate, (step_bed, read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}

/// The bed patient's own time: its afflictions run in good air (it is not on the crew's air).
fn step_bed(time: Res<Time>, mut med: ResMut<Medbay>, st: Res<HealthState>) {
    let dt = time.delta_secs_f64();
    if dt <= 0.0 {
        return;
    }
    med.bed.step(&st.tuning, dt, Env { oxygen: 1.0, hot: false, glow: false, fire: 0.0, rod_per_s: 0.0, vacuum: false, suited: false });
}

/// The console's clicks: the patient switch, and a use of an item from the hands on the patient.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut med: ResMut<Medbay>, mut hands: ResMut<Hands>, mut st: ResMut<HealthState>, mut lab: ResMut<ScienceLab>) {
    for c in clicks.read().filter(|c| c.panel == MEDBAY) {
        if c.action == "who" {
            med.who = if med.who == Who::Me { Who::Bed } else { Who::Me };
            continue;
        }
        let Some(slot) = c.action.strip_prefix("use:").and_then(|s| s.parse::<usize>().ok()) else { continue };
        let Some(item) = hands.take_one(slot) else { continue };
        let skill_now = medical_of(&st);
        let t = st.tuning.clone();
        let who = if med.who == Who::Me { "you" } else { "the patient" };
        let (line, ok) = {
            let target = match med.who {
                Who::Me => &mut st.body(LOCAL).health,
                Who::Bed => &mut med.bed,
            };
            use_item(item, target, &t, skill_now, &mut lab, &mut hands, who)
        };
        if ok {
            gain(&mut st);
        }
        info!("medbay: {line}");
        med.line = (line, if ok { Tone::Good } else { Tone::Warn });
    }
}

/// One item on one body: a syringe's dose, first aid, or back into the hands for anything else.
/// The line for the console and whether the use helped (the medical skill grows then).
fn use_item(item: LabItem, h: &mut Health, t: &HealthTuning, skill_now: f64, lab: &mut ScienceLab, hands: &mut Hands, who: &str) -> (String, bool) {
    match item {
        LabItem::Syringe(mut y) => {
            let mut target = Dosed { health: h, tuning: t, skill: skill_now, last: None };
            let r = y.inject(&mut target, skill_now, &lab.tuning, &mut lab.dice);
            let last = target.last.take();
            if y.doses > 0 {
                hands.give(syringe_item(y));
            }
            match (r, last) {
                (Ok(d), Some(res)) => {
                    let reduced: f64 = res.treated.iter().map(|x| x.reduced).sum();
                    let poisoned = res.treated.iter().any(|x| x.overdose.is_some());
                    let statuses = res.statuses.join(", ");
                    let line = format!(
                        "{who}: -{reduced:.0} ({:.0} % dose){}{}",
                        d.strength * 100.0,
                        if statuses.is_empty() { String::new() } else { format!(", {statuses}") },
                        if poisoned { ", poisoned!" } else { "" }
                    );
                    (line, reduced > 0.0)
                }
                _ => ("the syringe is empty".into(), false),
            }
        }
        LabItem::FirstAid(kind) => match h.treat(t, &kind, skill_now) {
            Some(r) => (format!("{who}: {kind} -{:.0}{}", r.reduced, if r.success { "" } else { " (fumbled)" }), r.reduced > 0.0),
            None => {
                hands.give(first_aid_item(&kind));
                ("that does not treat anything".into(), false)
            }
        },
        other => {
            // Not a medicine: back into the hands.
            let back = match other {
                LabItem::Ingredient(id) => ingredient_item(&id),
                LabItem::Batch(b) => batch_item(b),
                LabItem::Key => key_item(),
                _ => unreachable!("syringes and first aid are matched above"),
            };
            hands.give(back);
            ("that is not a medicine".into(), false)
        }
    }
}

/// The affliction list of a body, the strongest first: "burn 42".
pub fn affliction_lines(h: &Health) -> Vec<String> {
    let mut v: Vec<(&String, &f64)> = h.afflictions().iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(a.1));
    v.iter().map(|(id, s)| format!("{} {:.0}", id.replace('_', " "), s)).collect()
}

fn state_line(h: &Health, respawn_in: Option<f64>) -> (String, Tone) {
    match h.state() {
        Life::Conscious if h.vitality() < h.max_vitality() * 0.5 => ("hurt".into(), Tone::Warn),
        Life::Conscious => ("conscious".into(), Tone::Good),
        Life::Unconscious => ("UNCONSCIOUS".into(), Tone::Bad),
        Life::Dead => (format!("DEAD{}", respawn_in.map(|s| format!(": back in {s:.0} s")).unwrap_or_default()), Tone::Bad),
    }
}

/// The medbay panel: who is treated (the player or the bed), their vitality and afflictions, and
/// the medicines in the hands with a use button each.
pub fn medbay_panel(med: &Medbay, me: &Health, me_respawn_in: Option<f64>, hands: &Hands, names: &dyn Fn(&CommodityId) -> String) -> Panel {
    let (h, respawn_in) = match med.who {
        Who::Me => (me, me_respawn_in),
        Who::Bed => (&med.bed, None),
    };
    let max = h.max_vitality();
    let (state, tone) = state_line(h, respawn_in);
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
        items.push(label("no medicine: syringes and bandages").tone(Tone::Dim).fixed(28.0));
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

pub fn write_panel(med: Res<Medbay>, mut st: ResMut<HealthState>, hands: Res<Hands>, gp: Res<Gameplay>, mut panels: ResMut<Panels>) {
    let names = |id: &CommodityId| super::science::commodity_name(&gp, id);
    let respawn_in = st.body(LOCAL).respawn_in;
    let me = &st.body(LOCAL).health;
    panels.set(MEDBAY, medbay_panel(&med, me, respawn_in, &hands, &names));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gameplay_core::EffectId;

    fn tuning() -> HealthTuning {
        HealthTuning::load(&crate::content_files("gameplay")).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn the_bed_patient_starts_burnt_and_the_console_fits() {
        let t = tuning();
        let mut med = Medbay::new(&t);
        assert_eq!(med.bed.strength("burn"), PATIENT_BURN);
        let mut me = Health::new(&t, 1);
        for (id, s) in [("burn", 20.0), ("poison", 10.0), ("giggly", 10.0)] {
            me.add(&t, id, s);
        }
        let mut hands = Hands::default();
        for k in ["bandage", "bandage"] {
            hands.give(first_aid_item(k));
        }
        let names = |id: &CommodityId| id.to_string();
        let p = medbay_panel(&med, &me, None, &hands, &names);
        assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
        assert!(p.find("use:0").is_some() && p.find("who").is_some());
        med.who = Who::Bed;
        assert!(medbay_panel(&med, &me, None, &hands, &names).overflowing().is_empty());
    }

    #[test]
    fn a_dose_of_cooling_on_the_bed_cuts_its_burn() {
        let t = tuning();
        let mut med = Medbay::new(&t);
        let dose = Dose { effects: [EffectId::new("cooling")].into_iter().collect(), strength: 1.0, success: true };
        let mut target = Dosed { health: &mut med.bed, tuning: &t, skill: 50.0, last: None };
        target.receive(&dose);
        assert!(target.last.as_ref().is_some_and(|r| r.treated[0].reduced > 0.0));
        assert!(med.bed.strength("burn") < PATIENT_BURN, "cooling takes burn off the patient");
    }
}
