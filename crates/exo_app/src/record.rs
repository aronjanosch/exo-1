//! `--record=<file>`: write the local ship and walker of a scripted run as a 60 Hz path
//! (net_core `Trajectory`) for the replay matrix. One state per physics tick, phase name of the
//! scenario step in `flags`.
use crate::env::PlanetRes;
use crate::scenario::Script;
use crate::walker::Player;
use avian3d::prelude::*;
use bevy::prelude::*;
use net_core::replay::{Trajectory, DT};

#[derive(Resource)]
pub struct Recorder {
    pub path: std::path::PathBuf,
    pub traj: Trajectory,
    pub written: bool,
}

impl Recorder {
    pub fn new(path: std::path::PathBuf) -> Recorder {
        Recorder { path, traj: Trajectory { phases: vec!["other".into()], states: Vec::new() }, written: false }
    }
}

pub fn record_tick(
    mut rec: ResMut<Recorder>,
    script: Option<Res<Script>>,
    planet: Res<PlanetRes>,
    ships: Query<(&Position, &Rotation, &LinearVelocity), With<crate::ship::Ship>>,
    players: Query<&Player>,
) {
    let (Ok(ship), Ok(pl)) = (ships.single(), players.single()) else { return };
    let phase_name = script.as_ref().map(|s| s.ctx.phase.as_str()).filter(|p| !p.is_empty()).unwrap_or("other");
    let idx = match rec.traj.phases.iter().position(|p| p == phase_name) {
        Some(i) => i,
        None => {
            rec.traj.phases.push(phase_name.to_string());
            rec.traj.phases.len() - 1
        }
    };
    let t = rec.traj.states.len() as f64 * DT;
    let seq = rec.traj.states.len() as u32;
    let mut s = crate::net::build_snapshot(1, 1, 0, t, seq, &planet, ship, pl);
    s.flags = idx as u32;
    rec.traj.states.push(s);
    if script.is_some_and(|s| s.done) && !rec.written {
        rec.written = true;
        let _ = std::fs::create_dir_all(rec.path.parent().unwrap_or(std::path::Path::new(".")));
        std::fs::write(&rec.path, rec.traj.to_bytes()).expect("write recording");
        println!("recorded {} states ({:.1} s, {} phases) -> {}", rec.traj.states.len(), rec.traj.seconds(), rec.traj.phases.len(), rec.path.display());
    }
}
