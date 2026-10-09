//! Hot reload of the tuning files mid-run.
use super::*;

/// Copies the shipped tuning files to `<out>/tuning`, watches that copy, edits it mid-run.
pub(super) fn reload_steps(s: &mut Vec<Step>, out_dir: &std::path::Path) {
    let dir = out_dir.join("tuning");
    let ship_file = dir.join("ship.json");
    let ship_text = crate::tuning::SHIP.to_string();
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, _| {
            std::fs::create_dir_all(&dir).expect("tuning copy dir");
            for (f, t) in [("ship.json", crate::tuning::SHIP), ("walker.json", crate::tuning::WALKER), ("suit.json", crate::tuning::SUIT), ("camera.json", crate::tuning::CAMERA), ("bindings.json", crate::controls::BINDINGS)] {
                std::fs::write(dir.join(f), t).expect("tuning copy");
            }
            w.resource_mut::<crate::hot_reload::HotReload>().dir = dir.clone();
            true
        }));
    }
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let n = w.resource::<crate::hot_reload::HotReload>().reloads;
        check(c, n == 0, format!("reload: unchanged files change nothing ({n} reloads)"));
        true
    }));
    let edit = |name: &'static str, file: std::path::PathBuf, text: String, then: fn(&mut World) -> Option<String>| -> Step {
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                std::fs::write(&file, &text).expect("edit tuning");
            }
            if let Some(note) = then(w) {
                end(w, c, format!("after {:.2} s simulated", c.t));
                check(c, true, format!("{name}: {note}"));
                return true;
            }
            if c.t > 30.0 {
                end(w, c, "timed out".into());
                check(c, false, format!("{name}: no effect within 30 s"));
                return true;
            }
            false
        })
    };
    let slower = ship_text.replacen("\"turn_rate\": 2.5", "\"turn_rate\": 1.25", 1);
    assert_ne!(slower, ship_text, "fixture: turn_rate in ship.json");
    s.push(edit("reload: ship.json turn_rate 2.5 -> 1.25", ship_file.clone(), slower, |w| {
        let (ship, res) = (with_ship(w, |s| s.ctl.tuning.turn_rate), w.resource::<crate::tuning::Tuning>().ship.turn_rate);
        (ship == 1.25 && res == 1.25).then(|| format!("the ship turns at {ship} rad/s now"))
    }));
    let broken = ship_text.replacen("\"drag_k\"", "\"drag_kk\": 1, \"drag_k\"", 1);
    s.push(edit("reload: a broken ship.json is refused", ship_file.clone(), broken, |w| {
        let hr = w.resource::<crate::hot_reload::HotReload>();
        let err = hr.last_error.clone()?;
        let rate = with_ship(w, |s| s.ctl.tuning.turn_rate);
        (rate == 1.25 && err.contains("drag_kk")).then(|| format!("old value {rate} stays, error: {err}"))
    }));
    s.push(edit("reload: ship.json restored", ship_file, ship_text, |w| {
        let rate = with_ship(w, |s| s.ctl.tuning.turn_rate);
        (rate == 2.5 && w.resource::<crate::hot_reload::HotReload>().last_error.is_none()).then(|| format!("turn rate {rate} again"))
    }));
}
