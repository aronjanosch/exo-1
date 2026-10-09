//! Player settings (#31): mouse sensitivity, field of view, volume, sound on/off, and the key bindings, in the
//! game's own `settings/` folder (next to where it runs; `--settings-dir` picks another).
//! `settings.json` holds the numbers; rebinding writes `bindings.json` there, which then wins over
//! the shipped `content/tuning/bindings.json`. A file that does not parse falls back to the
//! defaults with a message. Scripted runs never read or write them.
use crate::controls::Bindings;
use bevy::prelude::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Window only.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, apply_volume.in_set(crate::phases::Frame::Input));
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Multiplies the mouse sensitivities of the bindings (ship and walker).
    pub mouse_sensitivity: f64,
    /// Field of view at rest, degrees; the speed curve adds to it.
    pub fov_deg: f64,
    /// 0..1.
    pub volume: f64,
    /// Off silences every sound and keeps the volume for when it comes back on.
    pub sound: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { mouse_sensitivity: 1.0, fov_deg: 75.0, volume: 0.5, sound: true }
    }
}

pub const MOUSE_RANGE: (f64, f64) = (0.1, 5.0);
pub const FOV_RANGE: (f64, f64) = (55.0, 100.0);

impl Settings {
    pub fn from_json(s: &str) -> Result<Settings, String> {
        let v: Value = serde_json::from_str(s).map_err(|e| format!("settings.json: {e}"))?;
        let o = v.as_object().ok_or("settings.json: not an object")?;
        if let Some(k) = o.keys().find(|k| !["mouse_sensitivity", "fov_deg", "volume", "sound"].contains(&k.as_str())) {
            return Err(format!("settings.json: unknown field `{k}`"));
        }
        let num = |k: &str, (lo, hi): (f64, f64)| {
            o.get(k).and_then(|v| v.as_f64()).filter(|v| (lo..=hi).contains(v)).ok_or_else(|| format!("settings.json: `{k}` must be a number in {lo}..{hi}"))
        };
        // Optional: files written before the switch existed keep loading.
        let sound = match o.get("sound") {
            None => true,
            Some(v) => v.as_bool().ok_or("settings.json: `sound` must be true or false")?,
        };
        Ok(Settings { mouse_sensitivity: num("mouse_sensitivity", MOUSE_RANGE)?, fov_deg: num("fov_deg", FOV_RANGE)?, volume: num("volume", (0.0, 1.0))?, sound })
    }

    /// The factor on every sound: the volume, or 0 with the sound off.
    pub fn master(&self) -> f32 {
        if self.sound { self.volume as f32 } else { 0.0 }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&json!({ "mouse_sensitivity": self.mouse_sensitivity, "fov_deg": self.fov_deg, "volume": self.volume, "sound": self.sound })).unwrap()
    }
}

/// Where the settings live.
#[derive(Resource, Clone, Debug)]
pub struct SettingsDir(pub PathBuf);

impl SettingsDir {
    pub fn default_dir() -> PathBuf {
        PathBuf::from("settings")
    }
}

/// Settings and bindings from `dir`; missing files give the defaults, broken ones too (with a
/// message each).
pub fn load(dir: &Path) -> (Settings, Bindings, Vec<String>) {
    let mut notes = Vec::new();
    let settings = match std::fs::read_to_string(dir.join("settings.json")) {
        Err(_) => Settings::default(),
        Ok(s) => Settings::from_json(&s).unwrap_or_else(|e| {
            notes.push(format!("{e}; using the defaults"));
            Settings::default()
        }),
    };
    let bindings = match std::fs::read_to_string(dir.join("bindings.json")) {
        Err(_) => Bindings::default(),
        Ok(s) => Bindings::from_json(&s).unwrap_or_else(|e| {
            notes.push(format!("{e}; using the shipped bindings"));
            Bindings::default()
        }),
    };
    (settings, bindings, notes)
}

pub fn save_settings(dir: &Path, s: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("settings.json"), s.to_json())
}

pub fn save_bindings(dir: &Path, b: &Bindings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("bindings.json"), b.to_json())
}

/// Windowed runs: the global volume follows the setting. Bevy applies it only to sounds that start
/// after the change; the loops, which keep playing, take `master()` in `audio::update`.
pub fn apply_volume(settings: Res<Settings>, mut vol: ResMut<GlobalVolume>) {
    if settings.is_changed() {
        *vol = GlobalVolume::new(bevy::audio::Volume::Linear(settings.master()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::{Button, Slot};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("exo-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_changed_setting_survives_a_restart() {
        let d = dir("restart");
        let s = Settings { mouse_sensitivity: 1.5, fov_deg: 82.0, volume: 0.25, sound: false };
        save_settings(&d, &s).unwrap();
        let mut b = Bindings::default();
        b.rebind(Slot::Button(Button::Brake), KeyCode::KeyB);
        save_bindings(&d, &b).unwrap();
        let (s2, b2, notes) = load(&d);
        assert_eq!((s2, notes.len()), (s, 0));
        assert_eq!(b2.buttons, b.buttons);
    }

    #[test]
    fn sound_starts_on_at_half_volume() {
        let s = Settings::default();
        assert_eq!((s.volume, s.sound, s.master()), (0.5, true, 0.5));
    }

    #[test]
    fn sound_off_silences_whatever_the_volume() {
        let s = Settings { sound: false, volume: 0.7, ..Settings::default() };
        assert_eq!(s.master(), 0.0);
    }

    #[test]
    fn a_settings_file_from_before_the_sound_switch_still_loads() {
        let s = Settings::from_json(r#"{ "mouse_sensitivity": 1.2, "fov_deg": 80, "volume": 0.3 }"#).unwrap();
        assert_eq!((s.volume, s.sound), (0.3, true));
    }

    #[test]
    fn missing_files_give_the_defaults() {
        let (s, b, notes) = load(&dir("missing"));
        assert_eq!((s, b, notes.len()), (Settings::default(), Bindings::default(), 0));
    }

    #[test]
    fn invalid_files_fall_back_to_defaults() {
        let d = dir("invalid");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("settings.json"), r#"{ "mouse_sensitivity": 99, "fov_deg": 75, "volume": 1 }"#).unwrap();
        std::fs::write(d.join("bindings.json"), "{ not json").unwrap();
        let (s, b, notes) = load(&d);
        assert_eq!((s, b), (Settings::default(), Bindings::default()));
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("mouse_sensitivity"));
    }
}
