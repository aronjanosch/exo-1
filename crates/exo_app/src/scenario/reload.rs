//! Hot reload of the tuning files mid-run.
use super::*;

/// Copies tuning and ship files to `<out>/content`, watches that copy, edits it mid-run.
pub(super) fn reload_steps(s: &mut Vec<Step>, out_dir: &std::path::Path) {
    let dir = out_dir.join("content");
    let ground_file = dir.join("tuning/ground.json");
    let ground_text = crate::tuning::GROUND.to_string();
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, _| {
            std::fs::create_dir_all(dir.join("tuning")).expect("tuning copy dir");
            std::fs::create_dir_all(dir.join("ship/kackfass_1")).expect("ship type copy dir");
            for (f, t) in [("ground.json", crate::tuning::GROUND), ("walker.json", crate::tuning::WALKER), ("suit.json", crate::tuning::SUIT), ("camera.json", crate::tuning::CAMERA), ("bindings.json", crate::controls::BINDINGS)] {
                std::fs::write(dir.join("tuning").join(f), t).expect("tuning copy");
            }
            for (file, text) in flight_core::sc::tuning::FILES.into_iter().zip(crate::tuning::SC) {
                std::fs::write(dir.join("ship/kackfass_1").join(file), text).expect("ship flight copy");
            }
            std::fs::write(dir.join("ship/kackfass_1/type.json"), crate::tuning::SHIP_MODEL).expect("ship model copy");
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
    let slower = ground_text.replacen("\"angular_decay\": 12.0", "\"angular_decay\": 6.0", 1);
    assert_ne!(slower, ground_text, "fixture: angular_decay in ground.json");
    s.push(edit("reload: ground.json angular_decay 12 -> 6", ground_file.clone(), slower, |w| {
        let (ship, res) = (with_ship(w, |s| s.ground.tuning.angular_decay), w.resource::<crate::tuning::Tuning>().ground.angular_decay);
        (ship == 6.0 && res == 6.0).then(|| format!("the ship's turns settle at {ship} 1/s now"))
    }));
    let broken = ground_text.replacen("\"linear_decay\"", "\"linear_decay_x\": 1, \"linear_decay\"", 1);
    s.push(edit("reload: a broken ground.json is refused", ground_file.clone(), broken, |w| {
        let hr = w.resource::<crate::hot_reload::HotReload>();
        let err = hr.last_error.clone()?;
        let rate = with_ship(w, |s| s.ground.tuning.angular_decay);
        (rate == 6.0 && err.contains("linear_decay_x")).then(|| format!("old value {rate} stays, error: {err}"))
    }));
    s.push(edit("reload: ground.json restored", ground_file, ground_text, |w| {
        let rate = with_ship(w, |s| s.ground.tuning.angular_decay);
        (rate == 12.0 && w.resource::<crate::hot_reload::HotReload>().last_error.is_none()).then(|| format!("angular decay {rate} again"))
    }));
    let model_file = dir.join("ship/kackfass_1/type.json");
    let renamed = crate::tuning::SHIP_MODEL.replace("Kackfass-1", "Kackfass-1 Test");
    s.push(edit("reload: ship model name", model_file.clone(), renamed, |w| {
        let name = with_ship(w, |s| s.model.name.clone());
        (name == "Kackfass-1 Test").then(|| format!("ship name: {name}"))
    }));
    let body_file = dir.join("ship/kackfass_1/sc_ship.json");
    let heavier = crate::tuning::SC[0].replace("\"mass\": 2000.0", "\"mass\": 3500.0");
    assert_ne!(heavier, crate::tuning::SC[0]);
    s.push(edit("reload: ship type mass", body_file.clone(), heavier, |w| {
        let model_mass = with_ship(w, |s| s.sc.mass());
        let e = ship_e(w);
        let physics_mass = w.get::<avian3d::prelude::Mass>(e).unwrap().0 as f64;
        (model_mass == 3500.0 && physics_mass == 3500.0).then(|| format!("flight and contact mass: {model_mass} kg"))
    }));
    s.push(edit("reload: ship mass restored", body_file, crate::tuning::SC[0].to_string(), |w| {
        let mass = with_ship(w, |s| s.sc.mass());
        (mass == 2000.0).then(|| format!("restored mass {mass} kg"))
    }));
    s.push(edit("reload: ship name restored", model_file, crate::tuning::SHIP_MODEL.to_string(), |w| {
        let name = with_ship(w, |s| s.model.name.clone());
        (name == "Kackfass-1").then(|| name)
    }));
}
