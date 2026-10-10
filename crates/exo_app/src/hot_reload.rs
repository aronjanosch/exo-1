//! Hot reload of `content/tuning/*.json` in dev builds (#21): every 250 ms of wall time the files
//! are read and compared with what is loaded; when one changed and all parse, the new values take
//! effect at once (ship and walker get them, the suit, camera, grab, HUD and bindings are read every step).
//! A file that does not parse is reported and the old values stay. Polling instead of Bevy's
//! `file_watcher` keeps the feature set as it is.
use crate::controls::Bindings;
use crate::ship::Ship;
use crate::tuning::Tuning;
use crate::walker::Player;
use bevy::prelude::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Dev builds only: watches `dir` and applies changed tuning files.
pub fn plugin(dir: PathBuf) -> impl Plugin {
    move |app: &mut App| {
        if cfg!(debug_assertions) {
            app.insert_resource(HotReload::new(dir.clone()));
            app.add_systems(Update, poll.before(crate::controls::read_input).in_set(crate::phases::Frame::Input));
        }
    }
}

const FILES: [&str; 13] = [
    "ship.json",
    "walker.json",
    "suit.json",
    "camera.json",
    "bindings.json",
    "grab.json",
    "hud.json",
    "sc_ship.json",
    "sc_modes.json",
    "sc_linear.json",
    "sc_angular.json",
    "sc_drive.json",
    "sc_air.json",
];
const POLL: Duration = Duration::from_millis(250);

#[derive(Resource)]
pub struct HotReload {
    pub dir: PathBuf,
    loaded: Vec<String>,
    next: Instant,
    pub reloads: u32,
    pub last_error: Option<String>,
}

impl HotReload {
    /// Starts from the embedded files: shipped files on disk that equal them change nothing.
    pub fn new(dir: PathBuf) -> HotReload {
        let mut loaded = [crate::tuning::SHIP, crate::tuning::WALKER, crate::tuning::SUIT, crate::tuning::CAMERA, crate::controls::BINDINGS, crate::tuning::GRAB, crate::tuning::HUD].map(String::from).to_vec();
        loaded.extend(crate::tuning::SC.map(String::from));
        HotReload { dir, loaded, next: Instant::now(), reloads: 0, last_error: None }
    }

    /// The repo's own `content/tuning` (dev builds run from the source tree).
    pub fn source_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/tuning"))
    }
}

fn parse(texts: &[String]) -> Result<(Tuning, Bindings), String> {
    Ok((
        Tuning {
            ground: flight_core::GroundTuning::from_json(&texts[0])?,
            walker: walker_core::WalkerConfig::from_json(&texts[1])?,
            suit: walker_core::SuitConfig::from_json(&texts[2])?,
            camera: flight_core::camera::CameraTuning::from_json(&texts[3])?,
            grab: grab_core::GrabConfig::from_json(&texts[5])?,
            hud: crate::hud::HudTuning::from_json(&texts[6])?,
            sc: flight_core::sc::ScTuning::from_json([&texts[7], &texts[8], &texts[9], &texts[10], &texts[11], &texts[12]])?,
        },
        Bindings::from_json(&texts[4])?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn poll(
    mut hr: ResMut<HotReload>,
    mut tuning: ResMut<Tuning>,
    mut bindings: ResMut<Bindings>,
    controls: Res<crate::controls::Controls>,
    mut ships: Query<&mut Ship>,
    mut players: Query<&mut Player>,
) {
    let now = Instant::now();
    if now < hr.next {
        return;
    }
    hr.next = now + POLL;
    // A file missing or unreadable for a moment (an editor saving) keeps the loaded text.
    let texts: Vec<String> = FILES.iter().zip(&hr.loaded).map(|(f, old)| std::fs::read_to_string(hr.dir.join(f)).unwrap_or_else(|_| old.clone())).collect();
    if texts == hr.loaded {
        return;
    }
    hr.loaded = texts.clone();
    match parse(&texts) {
        Ok((t, mut b)) => {
            // Scripted runs pin their mouse mode; a reload keeps it.
            if controls.scripted {
                b.mouse.ship_mode = bindings.mouse.ship_mode;
            }
            for mut s in &mut ships {
                s.ground.tuning = t.ground.clone();
                s.sc.tuning = t.sc.clone();
            }
            for mut p in &mut players {
                p.w.cfg = t.walker;
            }
            *tuning = t;
            *bindings = b;
            hr.reloads += 1;
            hr.last_error = None;
            println!("tuning: reloaded from {}", hr.dir.display());
        }
        Err(e) => {
            println!("tuning: reload refused, old values stay: {e}");
            hr.last_error = Some(e);
        }
    }
}
