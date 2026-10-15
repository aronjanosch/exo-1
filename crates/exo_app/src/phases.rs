//! Avian runs in `FixedPostUpdate`; all `Fx` phases stay in `FixedUpdate`, so `ship_control`
//! reads the previous tick's `Collisions`. `FixedLast` (interpolation records, `net_post`,
//! `record_tick`, perf `step_end`) and `PostUpdate` (render origin) are not phased.
use bevy::prelude::*;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fx {
    /// Scripts, network input and the resolved `Actions` plus the tap consumers that must see them first.
    Input,
    Drive,
    Ship,
    Walker,
    Cargo,
    /// Camera effects, day clock, readout: reads the finished tick.
    Effects,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Frame {
    Input,
    Sync,
    /// Camera and the render-origin view (everything using `RenderOrigin.view` comes after).
    Camera,
    World,
    Hud,
}

pub fn plugin(app: &mut App) {
    app.configure_sets(FixedUpdate, (Fx::Input, Fx::Drive, Fx::Ship, Fx::Walker, Fx::Cargo, Fx::Effects).chain());
    app.configure_sets(Update, (Frame::Input, Frame::Sync, Frame::Camera, Frame::World, Frame::Hud).chain());
}
