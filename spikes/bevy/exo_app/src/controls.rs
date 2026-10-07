//! Input as state the simulation reads, so scripts and the keyboard drive the same path
//! (Godot's SpikeInput pattern). Scripted runs never read the keyboard and never grab the mouse.
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use std::collections::HashSet;

#[derive(Resource, Default)]
pub struct Controls {
    pub held: HashSet<KeyCode>,
    /// One-shot keys (F, H, L, V, O), consumed by the fixed-step systems.
    pub taps: Vec<KeyCode>,
    /// Mouse movement in pixels since the last fixed step.
    pub mouse: Vec2,
    pub scripted: bool,
}

impl Controls {
    pub fn pressed(&self, k: KeyCode) -> bool {
        self.held.contains(&k)
    }
    pub fn axis(&self, pos: KeyCode, neg: KeyCode) -> f64 {
        self.pressed(pos) as i32 as f64 - self.pressed(neg) as i32 as f64
    }
    pub fn take_tap(&mut self, k: KeyCode) -> bool {
        if let Some(i) = self.taps.iter().position(|&t| t == k) {
            self.taps.remove(i);
            true
        } else {
            false
        }
    }
}

const TAPS: [KeyCode; 5] = [KeyCode::KeyF, KeyCode::KeyH, KeyCode::KeyL, KeyCode::KeyV, KeyCode::KeyO];

pub fn read_input(
    mut c: ResMut<Controls>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut cursor: Query<&mut CursorOptions>,
) {
    if c.scripted {
        return;
    }
    c.held = keys.get_pressed().copied().collect();
    for k in TAPS {
        if keys.just_pressed(k) {
            c.taps.push(k);
        }
    }
    let Ok(mut cur) = cursor.single_mut() else { return };
    if mouse_buttons.just_pressed(MouseButton::Left) {
        cur.grab_mode = CursorGrabMode::Locked;
        cur.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cur.grab_mode = CursorGrabMode::None;
        cur.visible = true;
    }
    if cur.grab_mode != CursorGrabMode::None {
        c.mouse += motion.delta;
    }
}
