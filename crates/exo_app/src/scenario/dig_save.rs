//! Veins and fizz pockets in the save (#336): change both, save, reset, load, and the changed state is back,
//! heat, timers and the pockets' generator included. A save without the sections loads fresh veins and pockets.
//! The load is one synchronous call, so the whole state is compared, no waiting on the clock.
use super::*;
use crate::dev_area::dig::{DigCfg, DigState};
use crate::dev_area::sniffer::{PocketCfg, PocketHost};
use crate::dev_area::DevArea;
use crate::savefile;
use gameplay_core::save::Envelope;

fn dig_state(w: &World) -> String {
    format!("{:?} {:?}", w.resource::<DigState>().veins, w.resource::<PocketHost>().field.saved())
}

/// Leaves a vein part cut and hot, one empty and growing back, a vent open with a jar taken and another tapped.
fn change_dig(w: &mut World) {
    let cfg = w.resource::<DigCfg>().0.clone();
    let mut d = w.resource_mut::<DigState>();
    d.veins[0].chunks_left = cfg.chunks.saturating_sub(2).max(1);
    d.veins[0].heat = 0.4;
    if let Some(v) = d.veins.get_mut(1) {
        v.chunks_left = 0;
        v.regrow = cfg.regrow_s * 0.25;
    }
    let mut p = w.resource_mut::<PocketHost>();
    let at = p.field.pockets()[0].at;
    let tap_s = p.field.config().tap_s;
    p.field.step(tap_s + 0.1, &[at]);
    p.field.fill(0);
    let at1 = p.field.pockets()[1].at;
    p.field.step(0.5 * tap_s, &[at1]);
}

fn fresh(w: &mut World) {
    let (dcfg, pcfg) = (w.resource::<DigCfg>().0.clone(), w.resource::<PocketCfg>().0.clone());
    crate::dev_area::dig::apply_section(&mut w.resource_mut::<DigState>(), &dcfg, None);
    crate::dev_area::sniffer::apply_section(&mut w.resource_mut::<PocketHost>(), &pcfg, None);
}

pub(super) fn dig_save_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        if w.resource::<DevArea>().frame.is_none() {
            if c.t > 5.0 {
                check(c, false, "dig_save: the area is there".into());
                return true;
            }
            return false;
        }
        begin(w, c, "dig_save: veins and pockets survive a save and a load (#336)");
        fresh(w);
        let start = dig_state(w);
        change_dig(w);
        let changed = dig_state(w);
        check(c, changed != start, "dig_save: the veins and pockets are changed before the save".into());
        let r = savefile::save_now(w);
        check(c, r.is_ok(), format!("dig_save: saved to the file ({r:?})"));
        fresh(w);
        check(c, dig_state(w) == start, "dig_save: the reset puts them back to the start".into());
        let dir = w.resource::<savefile::SaveDir>().0.clone();
        let text = savefile::read_slot(&dir, savefile::SLOT).ok().flatten().unwrap_or_default();
        match Envelope::from_json(&text) {
            Ok(env) => {
                let r = savefile::load(w, &env);
                check(c, r.is_ok(), format!("dig_save: the save loads ({r:?})"));
            }
            Err(e) => check(c, false, format!("dig_save: the file reads back ({e})")),
        }
        check(c, dig_state(w) == changed, format!("dig_save: veins and pockets are as saved\n  saved {changed}\n  after {}", dig_state(w)));
        // A save from before the sections: the load starts them fresh.
        change_dig(w);
        let mut old: serde_json::Value = serde_json::from_str(&text).unwrap();
        for name in [crate::dev_area::dig::SECTION, crate::dev_area::sniffer::SECTION] {
            let gone = old["sections"].as_object_mut().and_then(|m| m.remove(name));
            check(c, gone.is_some(), format!("dig_save: the save had a '{name}' section"));
        }
        let r = Envelope::from_json(&old.to_string()).map_err(|e| e.to_string()).and_then(|env| savefile::load(w, &env).map_err(|e| e.to_string()));
        check(c, r.is_ok() && dig_state(w) == start, format!("dig_save: a save without the sections starts them fresh ({r:?})"));
        end(w, c, "done".into());
        true
    }));
}
