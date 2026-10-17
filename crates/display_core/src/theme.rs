//! Display theme, parsed strictly (every field required, unknown ones refused); colours are
//! `#rrggbb` (sRGB).
use crate::Tone;
use serde::Deserialize;

/// An sRGB colour, 0..1 per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub [f32; 3]);

impl Rgb {
    pub fn from_hex(s: &str) -> Result<Rgb, String> {
        let h = s.strip_prefix('#').filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())).ok_or_else(|| format!("colour {s:?} is not #rrggbb"))?;
        let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map(|v| v as f32 / 255.0).map_err(|e| e.to_string());
        Ok(Rgb([c(0)?, c(2)?, c(4)?]))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Rgb, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Colours {
    /// Surfaces, darkest first: the well, the plate, controls, hovered controls.
    pub bg_0: Rgb,
    pub bg_1: Rgb,
    pub bg_2: Rgb,
    pub bg_3: Rgb,
    pub line_1: Rgb,
    pub line_2: Rgb,
    pub hud: Rgb,
    pub hud_dim: Rgb,
    pub fg_1: Rgb,
    pub fg_2: Rgb,
    pub fg_3: Rgb,
    pub fg_title: Rgb,
    pub fg_on_accent: Rgb,
    pub accent: Rgb,
    pub accent_dim: Rgb,
    pub good: Rgb,
    pub warn: Rgb,
    pub bad: Rgb,
    pub good_dim: Rgb,
    pub warn_dim: Rgb,
    pub bad_dim: Rgb,
    pub screen_idle: Rgb,
    pub bezel: Rgb,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fonts {
    pub display: String,
    pub display_semibold: String,
    pub display_bold: String,
    pub mono: String,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Strokes {
    pub border: f64,
    /// A raised control's dark bottom edge (it collapses when pressed).
    pub edge: f64,
    pub press_drop: f64,
    pub control_cut: f64,
    pub hatch: f64,
    /// Scanlines on the glass: one dark line every this many panel pixels (0 = off).
    pub scanline_every: f64,
    /// Letter spacing of caps text (em).
    pub caps_tracking: f64,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorldScreens {
    /// Only screens nearer than this to the camera draw their panel (m).
    pub range_m: f64,
    /// At most one redraw per screen in this many seconds (the screen in use redraws every frame).
    pub redraw_s: f64,
    /// Texture pixels per panel pixel.
    pub texture_scale: f64,
    /// A screen with moving shapes renders this many frames a second nearer than `anim_near_m`
    /// (or in use), `anim_far_fps` further away: the moving shapes are its only cost per frame.
    pub anim_fps: f64,
    pub anim_far_fps: f64,
    pub anim_near_m: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub source: String,
    pub colours: Colours,
    pub fonts: Fonts,
    pub strokes: Strokes,
    pub world: WorldScreens,
    pub hud: crate::hud::HudLook,
}

impl Theme {
    pub fn from_json(s: &str) -> Result<Theme, String> {
        let t: Theme = content_core::parse_strict("displays.json", s)?;
        let w = t.world;
        if !(w.range_m >= 0.0 && w.redraw_s >= 0.0 && w.texture_scale > 0.0 && w.texture_scale <= 4.0 && w.anim_fps > 0.0 && w.anim_far_fps > 0.0 && w.anim_near_m >= 0.0) {
            return Err("displays.json: world values out of range".into());
        }
        let st = &t.strokes;
        if [st.border, st.edge, st.press_drop, st.control_cut, st.hatch, st.scanline_every, st.caps_tracking].iter().any(|v| !(*v >= 0.0 && v.is_finite())) {
            return Err("displays.json: strokes must be finite and not negative".into());
        }
        if t.fonts.display.is_empty() || t.fonts.mono.is_empty() || t.fonts.display_bold.is_empty() || t.fonts.display_semibold.is_empty() {
            return Err("displays.json: a font path is empty".into());
        }
        t.hud.check()?;
        Ok(t)
    }

    pub fn tone(&self, t: Tone) -> Rgb {
        let c = &self.colours;
        match t {
            Tone::Normal => c.fg_1,
            Tone::Dim => c.fg_2,
            Tone::Micro => c.hud_dim,
            Tone::Title => c.fg_title,
            Tone::Good => c.good,
            Tone::Warn => c.warn,
            Tone::Bad => c.bad,
        }
    }

    pub fn tone_dim(&self, t: Tone) -> Rgb {
        let c = &self.colours;
        match t {
            Tone::Good => c.good_dim,
            Tone::Warn => c.warn_dim,
            Tone::Bad => c.bad_dim,
            _ => c.bg_3,
        }
    }
}
