//! The reactor panel and the power board, built from `Engineering` each tick.
use super::*;
use display_core::{bar, button, column, label, row, slider, space, title, toggle, Item, Panel, Tone};
use reactor_core::AlarmKind;

pub const REACTOR_W: f64 = 560.0;
pub const REACTOR_H: f64 = 760.0;
pub const POWER_W: f64 = 560.0;
pub const POWER_H: f64 = 800.0;
pub const HIT_W: f64 = 560.0;
pub const HIT_H: f64 = 620.0;
pub const OXYGEN_W: f64 = 560.0;
pub const OXYGEN_H: f64 = 560.0;

pub fn group_key(g: Group) -> &'static str {
    match g {
        Group::Engines => "engines",
        Group::Shields => "shields",
        Group::Weapons => "weapons",
        Group::LifeSupport => "life",
        Group::Aux => "aux",
    }
}

fn group_name(g: Group) -> &'static str {
    match g {
        Group::Engines => "ENGINES",
        Group::Shields => "SHIELDS",
        Group::Weapons => "WEAPONS",
        Group::LifeSupport => "LIFE",
        Group::Aux => "LIGHTS",
    }
}

fn alarm_name(k: AlarmKind) -> &'static str {
    match k {
        AlarmKind::TempHigh => "HOT",
        AlarmKind::TempCritical => "CRITICAL",
        AlarmKind::FuelLow => "FUEL",
        AlarmKind::Shutdown => "SCRAM",
    }
}

fn small(text: impl Into<String>, tone: Tone) -> Item {
    label(text).tone(tone)
}

/// The reactor's panel. `skill`: the operator's electrical skill (shown; it counts while the panel
/// is in use).
pub fn reactor(e: &Engineering, skill: f64) -> Panel {
    let r = &e.reactor;
    let b = r.bands();
    let (fis, turb) = r.targets();
    let (fire_t, over_t) = r.timers();
    let status = if !r.condition().functioning() {
        ("BROKEN".to_string(), Tone::Bad)
    } else if r.lockout_s() > 0.0 {
        (format!("LOCKED {:.0} s", r.lockout_s().ceil()), Tone::Bad)
    } else if r.power_on() {
        ("ON".to_string(), Tone::Good)
    } else {
        ("OFF".to_string(), Tone::Dim)
    };
    let manual = r.mode() == Mode::Manual;
    let life: f64 = r.rods().iter().map(|x| x.life).sum();
    let slots = r.tuning().slots as f64;
    let spent = r.rods().iter().filter(|x| x.spent()).count();
    let alarms: Vec<Item> = AlarmKind::ALL.iter().map(|&k| small(alarm_name(k), if r.alarm(k) { Tone::Bad } else { Tone::Dim })).collect();
    let cond = r.condition();
    Panel::new(
        REACTOR_W,
        REACTOR_H,
        column(
            8.0,
            vec![
                row(10.0, vec![title("REACTOR"), small(status.0, status.1).fixed(170.0)]).fixed(40.0),
                row(10.0, vec![button(if r.power_on() { "POWER OFF" } else { "POWER ON" }, "power"), toggle(!manual, "AUTO", "auto")]).fixed(50.0),
                slider(fis / 100.0, format!("FISSION target {fis:.0}  (now {:.0})", r.fission()), "fission").fixed(34.0),
                slider(turb / 100.0, format!("TURBINE target {turb:.0}  (now {:.0})", r.turbine()), "turbine").fixed(34.0),
                bar(r.temperature() / 100.0, format!("TEMP {:.1}", r.temperature())).thresholds(Some(b.temp_optimal.hi / 100.0), Some(b.temp_allowed.hi / 100.0)).fixed(30.0),
                small(format!("green {:.0}-{:.0}  fire >{:.0}  scram >{:.0}", b.temp_optimal.lo, b.temp_optimal.hi, b.temp_optimal.hi, b.temp_allowed.hi), Tone::Dim).fixed(22.0),
                bar(r.output_kw() / r.tuning().max_output_kw, format!("OUTPUT {:.0} kW  LOAD {:.0} kW", r.output_kw(), e.grid.request_kw())).fixed(30.0),
                bar(fire_t, "FIRE TIMER").thresholds(Some(0.5), Some(0.8)).fixed(26.0),
                bar(over_t, if r.safety() { "SCRAM TIMER" } else { "MELTDOWN TIMER" }).thresholds(Some(0.5), Some(0.8)).fixed(26.0),
                row(8.0, alarms).fixed(28.0),
                bar(life / (slots * 100.0), format!("FUEL {} rods, {spent} spent, crate {}", r.rods().len(), e.rods_in_crate)).thresholds(Some(0.2), Some(0.1)).fixed(30.0),
                row(10.0, vec![
                    button("INSERT ROD", "rod_in").enabled(e.rods_in_crate > 0 && r.rods().len() < r.tuning().slots),
                    button("EJECT SPENT", "rod_out").enabled(spent > 0),
                ])
                .fixed(46.0),
                bar(cond.fraction(), format!("CONDITION {:.0}", cond.value())).thresholds(Some(0.5), Some(0.2)).fixed(30.0),
                toggle(r.safety(), "SAFETY (off: no scram, meltdown)", "safety").fixed(40.0),
                small(format!("operator skill {skill:.0} (electrical)  {}", if manual { "MANUAL" } else { "AUTO" }), Tone::Dim).fixed(22.0),
                space(),
            ],
        ),
    )
}

/// The power board: a limit and a priority per group, what each gets, the rooms' oxygen.
pub fn power(e: &Engineering) -> Panel {
    let g = &e.grid;
    let mut items = vec![
        title("POWER BOARD").fixed(38.0),
        small(format!("reactor {:.0} kW   asked {:.0} kW", e.reactor.output_kw(), g.request_kw()), Tone::Dim).fixed(22.0),
    ];
    for grp in Group::ALL {
        let s = g.supply(grp);
        let st = g.group(grp);
        let key = group_key(grp);
        let state = if s.request_kw <= 0.0 {
            "off (limit 0)".to_string()
        } else if !s.on {
            "SHED".to_string()
        } else if s.ratio < 0.999 {
            format!("brownout {:.0} %", s.ratio * 100.0)
        } else {
            "full".to_string()
        };
        items.push(
            row(8.0, vec![
                small(group_name(grp), Tone::Normal).fixed(110.0),
                slider(st.limit, format!("limit {:.0} %", st.limit * 100.0), format!("limit_{key}")),
                button(format!("P{}", st.priority), format!("prio_{key}")).fixed(56.0),
            ])
            .fixed(34.0),
        );
        items.push(
            bar(if s.request_kw > 0.0 { s.ratio } else { 0.0 }, format!("{:.0}/{:.0} kW  works {:.0} %  {state}", s.delivered_kw, st.demand_kw, s.factor * 100.0))
                .thresholds(Some(0.99), Some(st.min_ratio))
                .fixed(26.0),
        );
    }
    let o2 = |r: RoomId| e.deck.room(r).oxygen_fraction();
    items.push(bar(o2(e.room), format!("OXYGEN engineering {:.0} %", o2(e.room) * 100.0)).thresholds(Some(0.5), Some(0.2)).fixed(26.0));
    items.push(bar(o2(e.side_room), format!("OXYGEN side room {:.0} %  door {}", o2(e.side_room) * 100.0, if e.door_open() { "open" } else { "closed" })).thresholds(Some(0.5), Some(0.2)).fixed(26.0));
    let fires = e.deck.fires().len();
    items.push(small(format!("fires {fires}   pump {:.0} %   extinguisher {:.0} %", e.pump.value(), e.extinguisher.charge * 100.0), if fires > 0 { Tone::Bad } else { Tone::Dim }).fixed(22.0));
    items.push(toggle(e.fast_wear, "FAST WEAR (dev)", "fast_wear").fixed(40.0));
    items.push(space());
    Panel::new(POWER_W, POWER_H, column(6.0, items))
}

/// "[X]" marks the picked one.
fn pick(on: bool, text: &str) -> String {
    if on { format!("[{text}]") } else { text.to_string() }
}

/// The hit simulator: where, what, how hard; fire one hit or a volley; the sections' state.
pub fn hit_simulator(e: &Engineering) -> Panel {
    use hull_core::{DamageType, Strength};
    let p = e.hit_pick;
    let mut items = vec![
        title("HIT SIMULATOR").fixed(38.0),
        small("the dev area's enemy: hits on the hull", Tone::Dim).fixed(22.0),
        row(8.0, vec![
            button(pick(p.section == e.engine_section, "ENGINE ROOM"), "sec_engine"),
            button(pick(p.section == e.side_section, "SIDE ROOM"), "sec_side"),
        ])
        .fixed(44.0),
        row(8.0, DamageType::ALL.iter().map(|k| button(pick(p.kind == *k, &k.name().to_uppercase()), format!("type_{}", k.name()))).collect()).fixed(44.0),
        row(8.0, Strength::ALL.iter().map(|x| button(pick(p.strength == *x, &x.name().to_uppercase()), format!("str_{}", x.name()))).collect()).fixed(44.0),
        row(8.0, vec![
            button("FIRE", "fire"),
            button(match &e.volley { Some(v) => format!("VOLLEY ({} left)", v.left()), None => "RANDOM VOLLEY".into() }, "volley").enabled(e.volley.is_none()),
        ])
        .fixed(50.0),
        small(if e.last_hit.is_empty() { "no hit yet".to_string() } else { e.last_hit.clone() }, Tone::Warn).fixed(24.0),
    ];
    for room in e.rooms() {
        let s = e.section_of(room);
        let sec = e.hull.section(s);
        let r = e.room_readout(room);
        items.push(
            bar(sec.condition.fraction(), format!("HULL {}  {:.0} %  {}", r.name, sec.condition.value(), if r.breach { format!("BREACH {:.0} % open", r.breach_open * 100.0) } else { "tight".into() }))
                .thresholds(Some(0.9), Some(0.3))
                .fixed(30.0),
        );
    }
    items.push(small(format!("hits {}  breaches {}  patched {}  kits {}{}", e.stats.hits, e.stats.breaches_opened, e.stats.patches_done, e.patch_kits, if e.kit_in_hand { " (+1 in hand)" } else { "" }), Tone::Dim).fixed(22.0));
    items.push(space());
    Panel::new(HIT_W, HIT_H, column(8.0, items))
}

/// The oxygen generator: switch, supply, output, condition, and air per room.
pub fn oxygen(e: &Engineering) -> Panel {
    let g = &e.generator;
    let supply = e.grid.factor(Group::LifeSupport);
    let mut items = vec![
        title("OXYGEN").fixed(38.0),
        toggle(e.generator_on, "GENERATOR", "generator").fixed(44.0),
        bar(supply, format!("POWER (life support) {:.0} %", supply * 100.0)).thresholds(Some(0.99), Some(0.5)).fixed(30.0),
        bar(g.output / GENERATOR_RATE, format!("OUTPUT {:.2} m³/s", g.output)).fixed(30.0),
        bar(g.condition.fraction(), format!("CONDITION {:.0}", g.condition.value())).thresholds(Some(0.5), Some(0.2)).fixed(30.0),
    ];
    for r in e.room_readouts() {
        let mut flags = Vec::new();
        if r.breach {
            flags.push(format!("BREACH {:.1} m³/s", r.leak_flow));
        }
        if r.fire {
            flags.push("FIRE".to_string());
        }
        items.push(
            bar(r.oxygen, format!("{}  {:.0} %  {:+.1} %/s", r.name.to_uppercase(), r.oxygen * 100.0, r.oxygen_delta * 100.0))
                .thresholds(Some(hull::OXYGEN_OK), Some(0.25))
                .fixed(32.0),
        );
        items.push(small(if flags.is_empty() { "ok".to_string() } else { flags.join("  ") }, if r.oxygen_alarm || r.breach || r.fire { Tone::Bad } else { Tone::Dim }).fixed(22.0));
    }
    items.push(small(format!("door {}", if e.door_open() { "open" } else { "closed: rooms apart" }), Tone::Dim).fixed(22.0));
    items.push(space());
    Panel::new(OXYGEN_W, OXYGEN_H, column(8.0, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fits(p: &Panel) -> bool {
        p.layout().iter().all(|w| w.rect.max().y <= p.size.y - p.padding + 1e-9 && w.rect.size.y >= 0.0)
    }

    #[test]
    fn panels_fit_and_name_their_actions() {
        let e = Engineering::default();
        let r = reactor(&e, 50.0);
        let p = power(&e);
        assert!(fits(&r), "reactor panel overflows");
        assert!(fits(&p), "power panel overflows");
        for a in ["power", "auto", "fission", "turbine", "rod_in", "safety"] {
            assert!(r.find(a).is_some(), "reactor panel lacks {a}");
        }
        for grp in Group::ALL {
            assert!(p.find(&format!("limit_{}", group_key(grp))).is_some());
            assert!(p.find(&format!("prio_{}", group_key(grp))).is_some());
        }
        assert!(p.find("fast_wear").is_some());
        let h = hit_simulator(&e);
        let o = oxygen(&e);
        assert!(fits(&h), "hit simulator overflows");
        assert!(fits(&o), "oxygen panel overflows");
        for a in ["sec_engine", "sec_side", "type_energy", "type_physical", "type_collision", "str_light", "str_medium", "str_heavy", "fire", "volley"] {
            assert!(h.find(a).is_some(), "hit simulator lacks {a}");
        }
        assert!(o.find("generator").is_some());
    }
}
