//! Schedule phases (#97): every domain plugin puts its systems into one of these sets, and the
//! order between phases lives here, once. Order inside a phase is the plugin's own business.
//!
//! Avian runs in `FixedPostUpdate`; all `Fx` phases stay in `FixedUpdate`, so `ship_control`
//! reads the previous tick's `Collisions`. `FixedLast` (interpolation records, `net_post`,
//! `record_tick`, perf `step_end`) and `PostUpdate` (render origin) are not phased.
use bevy::prelude::*;

/// One physics tick, in `FixedUpdate`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fx {
    /// Scripts, network input and the resolved `Actions` plus the tap consumers that must see them first.
    Input,
    /// The warp state machine: input, drive, planet swap, telemetry, swap audit.
    Drive,
    Ship,
    Walker,
    /// Grab, crates, object budget.
    Cargo,
    /// Camera effects, day clock, readout: reads the finished tick.
    Effects,
}

/// One rendered frame, in `Update`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Frame {
    /// Read the keyboard and mouse, menus, hot reload, audio.
    Input,
    /// Bring entities and scene state up to date: ring, visuals, lights.
    Sync,
    /// Camera and the render-origin view (everything using `RenderOrigin.view` comes after).
    Camera,
    /// Terrain and things placed in the world.
    World,
    Hud,
}

pub fn plugin(app: &mut App) {
    app.configure_sets(FixedUpdate, (Fx::Input, Fx::Drive, Fx::Ship, Fx::Walker, Fx::Cargo, Fx::Effects).chain());
    app.configure_sets(Update, (Frame::Input, Frame::Sync, Frame::Camera, Frame::World, Frame::Hud).chain());
}
