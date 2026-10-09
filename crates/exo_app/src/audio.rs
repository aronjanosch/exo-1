//! Audio minimum (#28): sounds synthesized in code, no files. Thrust hum by the ship's
//! thrust, wind by airspeed in the atmosphere, a thud on touchdown, a click on UI toggles.
//! Cargo (night extra E4): a fwip on grabbing a crate, a whoosh on a throw, a ka-chunk when a
//! crate locks onto the plates.
//! Windowed runs only; headless runs have no audio. Volume controls come with the settings menu.
use crate::controls::{Actions, Tap};
use crate::ship::{CameraEffects, Ship};
use crate::walker::Player;
use bevy::audio::{AddAudioSource, AudioSinkPlayback, ChannelCount, PlaybackMode, SampleRate, Source, Volume};
use bevy::prelude::*;
use std::time::Duration;

const RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Loop: a low rumble, noise through two low-pass stages with a faint 38 Hz under it. (The
    /// first try, a 55 Hz tone with overtones and a wobble, sounded like a toy duck.)
    Hum,
    /// Loop: noise through a one-pole low-pass, slowly breathing.
    Wind,
    /// 0.35 s: a falling low sine with a noise burst.
    Thud,
    /// 30 ms: a short high blip.
    Click,
    /// 0.12 s: a rising chirp with a breath of noise ("fwip"), taking hold of a crate.
    Grab,
    /// 0.35 s: noise swelling and falling through a sweeping low-pass, a throw.
    Throw,
    /// 0.2 s: a low clunk, then a bright ping 60 ms later ("ka-chunk"), a crate locks.
    Lock,
}

impl Sound {
    /// Length in seconds; loops have none.
    pub fn length(self) -> Option<f32> {
        match self {
            Sound::Hum | Sound::Wind => None,
            Sound::Thud => Some(0.35),
            Sound::Click => Some(0.03),
            Sound::Grab => Some(0.12),
            Sound::Throw => Some(0.35),
            Sound::Lock => Some(0.2),
        }
    }
}

/// Sample generator: plain math, testable without a device.
#[derive(Clone, Debug)]
pub struct Synth {
    sound: Sound,
    n: u64,
    noise: u32,
    lp: f32,
    lp2: f32,
}

impl Synth {
    pub fn new(sound: Sound) -> Synth {
        Synth { sound, n: 0, noise: 0x1234_5678, lp: 0.0, lp2: 0.0 }
    }

    fn white(&mut self) -> f32 {
        // xorshift32
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

impl Iterator for Synth {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let t = self.n as f32 / RATE as f32;
        if self.sound.length().is_some_and(|l| t >= l) {
            return None;
        }
        self.n += 1;
        let tau = std::f32::consts::TAU;
        let s = match self.sound {
            Sound::Hum => {
                // About 90 Hz cut-off twice: no pitch to hear, only a body.
                let w = self.white();
                self.lp += (w - self.lp) * 0.013;
                self.lp2 += (self.lp - self.lp2) * 0.013;
                self.lp2 * 9.0 + 0.08 * (tau * 38.0 * t).sin()
            }
            Sound::Wind => {
                let w = self.white();
                self.lp += (w - self.lp) * 0.04;
                self.lp * 3.0 * (0.7 + 0.3 * (tau * 0.23 * t).sin())
            }
            Sound::Thud => {
                let env = (-t * 14.0).exp();
                let f = 70.0 - 30.0 * t / 0.35;
                env * (0.8 * (tau * f * t).sin() + 0.3 * self.white() * (-t * 40.0).exp())
            }
            Sound::Click => (-t * 180.0).exp() * (tau * 1400.0 * t).sin(),
            Sound::Grab => {
                // 300 Hz to 900 Hz over the sound: phase of a linear sweep.
                let phase = tau * (300.0 * t + 0.5 * (600.0 / 0.12) * t * t);
                let env = (t / 0.01).min(1.0) * (-t * 25.0).exp();
                env * (0.7 * phase.sin() + 0.15 * self.white())
            }
            Sound::Throw => {
                let x = t / 0.35;
                let env = (x * std::f32::consts::PI).sin().powi(2);
                // The low-pass opens then closes again: the whoosh passes by.
                let k = 0.02 + 0.25 * env;
                let w = self.white();
                self.lp += (w - self.lp) * k;
                env * self.lp * 2.5
            }
            Sound::Lock => {
                let clunk = (-t * 30.0).exp() * (tau * 110.0 * t).sin() * 0.9;
                let t2 = t - 0.06;
                let ping = if t2 > 0.0 { (-t2 * 45.0).exp() * (tau * 1900.0 * t2).sin() * 0.45 } else { 0.0 };
                clunk + ping
            }
        };
        Some(s.clamp(-1.0, 1.0))
    }
}

impl Source for Synth {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.sound.length().map(Duration::from_secs_f32)
    }
}

#[derive(Asset, TypePath)]
pub struct SynthAudio(pub Sound);

impl bevy::audio::Decodable for SynthAudio {
    type Decoder = Synth;
    fn decoder(&self) -> Synth {
        Synth::new(self.0)
    }
}

#[derive(Resource)]
struct Sounds {
    thud: Handle<SynthAudio>,
    click: Handle<SynthAudio>,
    grab: Handle<SynthAudio>,
    throw: Handle<SynthAudio>,
    lock: Handle<SynthAudio>,
}

#[derive(Component)]
struct Loop(Sound);

/// Clicks heard this frame (counted in the fixed step, where taps live).
#[derive(Resource, Default)]
struct Clicks(u32);

#[derive(Resource, Default)]
struct Heard {
    bumps: u32,
    throws: usize,
    locks: u32,
    /// The crate held last frame: a new one is a grab.
    held: Option<Entity>,
}

fn setup(mut commands: Commands, mut assets: ResMut<Assets<SynthAudio>>) {
    for s in [Sound::Hum, Sound::Wind] {
        let h = assets.add(SynthAudio(s));
        commands.spawn((AudioPlayer(h), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), ..default() }, Loop(s)));
    }
    commands.insert_resource(Sounds {
        thud: assets.add(SynthAudio(Sound::Thud)),
        click: assets.add(SynthAudio(Sound::Click)),
        grab: assets.add(SynthAudio(Sound::Grab)),
        throw: assets.add(SynthAudio(Sound::Throw)),
        lock: assets.add(SynthAudio(Sound::Lock)),
    });
}

/// Fixed step, right after the actions are resolved: UI toggles click.
fn count_clicks(actions: Res<Actions>, mut clicks: ResMut<Clicks>) {
    let ui = [Tap::HoverAssist, Tap::HorizonFollow, Tap::Decoupled, Tap::DebugHud, Tap::WarpTarget, Tap::OrbitCamera, Tap::Lag, Tap::BoostMode];
    clicks.0 += actions.taps().iter().filter(|t| ui.contains(t)).count() as u32;
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    sounds: Res<Sounds>,
    mut clicks: ResMut<Clicks>,
    mut heard: ResMut<Heard>,
    fx: Res<CameraEffects>,
    planet: Res<crate::env::PlanetRes>,
    players: Query<&Player>,
    ships: Query<(&Ship, &avian3d::prelude::Position, &avian3d::prelude::LinearVelocity)>,
    mut loops: Query<(&Loop, &mut AudioSink)>,
    time: Res<Time>,
    grab: Res<crate::grab::Grab>,
    cargo: Res<crate::cargo::CargoStats>,
    mut level: Local<[f32; 2]>,
) {
    let (Ok(pl), Ok((ship, pos, lv))) = (players.single(), ships.single()) else { return };
    let near_ship = pl.seated || pl.ship.is_some();
    let o = ship.ctl.ramp.out;
    let thrust = if ship.parked { 0.0 } else { (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt().min(1.0) as f32 };
    let idle = if ship.parked { 0.0 } else { 0.08 };
    let density = flight_core::PlanetEnv::density_at(planet.as_ref(), pos.0) as f32;
    let airspeed = if pl.seated { lv.0.length() as f32 } else { 0.0 };
    // Volumes glide (0.25 s), so thrust taps do not click the loop on and off.
    let k = 1.0 - (-time.delta_secs() / 0.25).exp();
    for (l, mut sink) in &mut loops {
        let (i, want) = match l.0 {
            Sound::Hum => (0, if near_ship { idle + 0.25 * thrust } else { 0.0 }),
            _ => (1, density * (airspeed / 200.0).min(1.0) * 0.5),
        };
        level[i] += (want - level[i]) * k;
        sink.set_volume(Volume::Linear(level[i]));
    }
    if fx.0.bumps > heard.bumps {
        heard.bumps = fx.0.bumps;
        if near_ship {
            commands.spawn((AudioPlayer(sounds.thud.clone()), PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(0.8), ..default() }));
        }
    }
    // Cargo, E4. TODO(initiator): volumes are start values.
    let one_shot = |commands: &mut Commands, h: &Handle<SynthAudio>, v: f32| {
        commands.spawn((AudioPlayer(h.clone()), PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(v), ..default() }));
    };
    let held = grab.held.map(|h| h.crate_e);
    if held.is_some() && held != heard.held {
        one_shot(&mut commands, &sounds.grab, 0.5);
    }
    heard.held = held;
    if grab.throws.len() > heard.throws {
        heard.throws = grab.throws.len();
        one_shot(&mut commands, &sounds.throw, 0.6);
    }
    if cargo.locks > heard.locks {
        heard.locks = cargo.locks;
        one_shot(&mut commands, &sounds.lock, 0.6);
    }
    for _ in 0..std::mem::take(&mut clicks.0).min(3) {
        commands.spawn((AudioPlayer(sounds.click.clone()), PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(0.3), ..default() }));
    }
}

/// Windowed runs: the synthesized sources and the systems that play them.
pub fn plugin(app: &mut App) {
    app.add_audio_source::<SynthAudio>().init_resource::<Clicks>().init_resource::<Heard>();
    app.add_systems(Startup, setup);
    // Before anyone consumes the taps (the warp and the view toggles come first).
    app.add_systems(FixedUpdate, count_clicks.after(crate::controls::resolve_actions).before(crate::warp::warp_input).before(crate::view::orbit_toggle).before(crate::view::debug_hud_toggle));
    app.add_systems(Update, update);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shots_end_and_every_sample_is_in_range() {
        for s in [Sound::Thud, Sound::Click, Sound::Grab, Sound::Throw, Sound::Lock] {
            let samples: Vec<f32> = Synth::new(s).collect();
            assert_eq!(samples.len(), (s.length().unwrap() * RATE as f32).ceil() as usize, "{s:?}");
            assert!(samples.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            assert!(samples.iter().any(|x| x.abs() > 0.1), "{s:?} is audible");
        }
    }

    #[test]
    fn loops_keep_going_and_stay_in_range() {
        for s in [Sound::Hum, Sound::Wind] {
            let samples: Vec<f32> = Synth::new(s).take(RATE as usize * 3).collect();
            assert_eq!(samples.len(), RATE as usize * 3);
            assert!(samples.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(rms > 0.05, "{s:?} rms {rms}");
        }
    }
}
