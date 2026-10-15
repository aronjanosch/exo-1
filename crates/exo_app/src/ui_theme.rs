//! One look for the HUD: colours, font sizes and the screen margin, so the
//! panels, stamps and lines agree. TODO(initiator): the look itself.
use bevy::prelude::*;

pub const INK: Color = Color::srgb(0.95, 0.95, 0.88);
pub const DIM: Color = Color::srgb(0.62, 0.64, 0.66);
pub const GOLD: Color = Color::srgb(1.0, 0.86, 0.42);
pub const GOOD: Color = Color::srgb(0.45, 1.0, 0.55);
pub const BAD: Color = Color::srgb(1.0, 0.36, 0.3);
pub const INFO: Color = Color::srgb(0.5, 0.88, 1.0);
pub const PANEL: Color = Color::srgba(0.04, 0.05, 0.09, 0.8);
pub const PANEL_ALERT: Color = Color::srgba(0.28, 0.03, 0.03, 0.88);
pub const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.85);

pub const SMALL: f32 = 14.0;
pub const BODY: f32 = 16.0;
pub const LARGE: f32 = 20.0;
pub const STAMP: f32 = 46.0;

/// Space between the screen edge and the HUD (px).
pub const MARGIN: f32 = 16.0;

pub fn faded(c: Color, a: f32) -> Color {
    let mut s = c.to_srgba();
    s.alpha *= a.clamp(0.0, 1.0);
    s.into()
}

/// Ease out with a little overshoot (stamps landing).
pub fn ease_out_back(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0) - 1.0;
    let c = 1.70158;
    1.0 + (c + 1.0) * k * k * k + c * k * k
}

pub fn ease_out(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    1.0 - (1.0 - k) * (1.0 - k)
}
