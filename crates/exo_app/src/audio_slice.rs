//! Slice sounds (#300): the fire, the extinguisher, the repair, the electrics, the pump, the door
//! and the health. Synthesized in `audio.rs` (`Sound`), played here from the state every process
//! has: the fire and the reactor snapshots (`Items`), the repair session (`RepairPlay`), the door
//! (`Items::room`) and the local player's vitals (`OwnVitals`). Nothing here reads host-only state.
//!
//! Quiet on purpose: the alarm tone (0.12) is the reference loudness, and no volume here is above
//! it. Distances are the walker's to the source, in metres; a source beyond its range is silent.
//! The sounds can not be heard in a test: the window part is untested by ear (`TODO`s below).
use crate::audio::{loop_volume, Sound, SynthAudio};
use crate::dev_area::engine_room::source_at;
use crate::dev_area::DevArea;
use crate::env::PlanetRes;
use crate::health::{OwnVitals, UseHold};
use crate::items::Items;
use crate::repair::RepairPlay;
use crate::settings::Settings;
use crate::walker::Player;
use bevy::audio::{AudioSinkPlayback, PlaybackMode, Volume};
use bevy::math::DVec3;
use bevy::prelude::*;
use device_core::minigame::Press;
use item_core::wire::{ReactorNews, RepairDevice};
use reactor_core::Source;
use std::collections::BTreeMap;

// ---- Values (TODO(initiator): start values, to be tuned by ear) ----

/// The alarm tone's volume: the reference loudness every slice sound stays at or under.
pub const ALARM_REF: f32 = 0.12;
/// The crackle per fire size level (0 small, 1 middle, 2 big), at the fire.
pub const CRACKLE_VOL: [f32; 3] = [0.04, 0.07, 0.10];
/// A fire size from which the level goes up: below `FIRE_LEVEL_M[0]` level 0, below `[1]` level 1.
pub const FIRE_LEVEL_M: [f64; 2] = [1.0, 2.5];
/// Crackle and spray are silent beyond these ranges (m).
pub const FIRE_RANGE_M: f64 = 10.0;
pub const SPRAY_RANGE_M: f64 = 10.0;
pub const PUMP_RANGE_M: f64 = 10.0;
/// The spray's hiss at the nozzle.
pub const SPRAY_VOL: f32 = 0.06;
/// A steam puff where a spray shrinks a fire: its volume and at least this many seconds apart.
pub const STEAM_VOL: f32 = 0.08;
pub const STEAM_EVERY_S: f64 = 0.5;
/// A fire smaller by more than this (m) since the last snapshot has been sprayed.
pub const SHRINK_M: f64 = 0.01;
/// The repair ratchet at the device while a session runs, and the hit, miss and done sounds.
pub const REPAIR_LOOP_VOL: f32 = 0.05;
pub const BLIP_VOL: f32 = 0.07;
pub const MISS_VOL: f32 = 0.06;
pub const CHIME_VOL: f32 = 0.08;
/// The junction box's zap, at most this often while overvolted (s).
pub const ZAP_VOL: f32 = 0.09;
pub const ZAP_EVERY_S: f64 = 2.0;
/// The breaker's clunk, the power's sweep and the SCRAM's thud.
pub const CLUNK_VOL: f32 = 0.09;
pub const SWEEP_VOL: f32 = 0.08;
pub const SCRAM_VOL: f32 = 0.12;
/// The pump's hum at full flow.
pub const PUMP_VOL: f32 = 0.06;
/// The pump's playback speed at no flow and at full flow.
pub const PUMP_SPEED: (f32, f32) = (0.8, 1.2);
/// The door's slide.
pub const SLIDE_VOL: f32 = 0.07;
/// Health: whoomp on catching fire, muffled on going down, gentle on respawn, a confirmation.
pub const WHOOMP_VOL: f32 = 0.10;
pub const MUFFLED_VOL: f32 = 0.07;
pub const GENTLE_VOL: f32 = 0.07;
pub const CONFIRM_VOL: f32 = 0.06;
/// A treatment counts as done when its hold was at least this far when it ended.
pub const CONFIRM_FROM: f64 = 0.9;
/// The heartbeat: none while the air is at least `HEART_O2_BELOW` (share of good air), then
/// every `HEART_SLOWEST_S` at the edge of it and every `HEART_FASTEST_S` with no air.
pub const HEART_O2_BELOW: f64 = 0.5;
pub const HEART_SLOWEST_S: f64 = 1.4;
pub const HEART_FASTEST_S: f64 = 0.5;
pub const HEART_VOL: f32 = 0.10;

// ---- Decisions (pure, tested) ----

/// The crackle level of a fire of `size` m: 0 small, 1 middle, 2 big.
pub fn fire_level(size: f64) -> usize {
    FIRE_LEVEL_M.iter().filter(|&&m| size >= m).count()
}

/// Linear falloff to silence at `range`, squared so the far tail is soft. 1 at the source.
pub fn falloff(dist: f64, range: f64) -> f32 {
    if range <= 0.0 || dist >= range {
        return 0.0;
    }
    (1.0 - dist / range).powi(2) as f32
}

/// The crackle's volume of a fire at `dist` m.
pub fn crackle_volume(level: usize, dist: f64) -> f32 {
    CRACKLE_VOL[level.min(2)] * falloff(dist, FIRE_RANGE_M)
}

/// Whether a fire shrank since the last snapshot: the spray hit it.
pub fn shrank(prev: f64, now: f64) -> bool {
    now < prev - SHRINK_M
}

/// The heartbeat's period (s) from the air left (`Vitals::oxygen`, 1 is good air); None while
/// the air is good or unknown.
pub fn heartbeat_every(air: Option<f64>) -> Option<f64> {
    let air = air?;
    if air >= HEART_O2_BELOW {
        return None;
    }
    let t = ((HEART_O2_BELOW - air) / HEART_O2_BELOW).clamp(0.0, 1.0);
    Some(HEART_SLOWEST_S + (HEART_FASTEST_S - HEART_SLOWEST_S) * t)
}

/// A zap now: the junction box is overvolted and the last zap is at least `ZAP_EVERY_S` ago.
pub fn zap_due(overvolt: bool, now: f64, last: Option<f64>) -> bool {
    overvolt && last.is_none_or(|l| now - l >= ZAP_EVERY_S)
}

/// The pump hum's speed for a coolant flow 0..1.
pub fn pump_speed(flow: f64) -> f32 {
    let (lo, hi) = PUMP_SPEED;
    lo + (hi - lo) * flow.clamp(0.0, 1.0) as f32
}

/// A device's share of full condition on the reactor snapshot (what the client sees).
pub fn part_share(r: &ReactorNews, d: RepairDevice) -> f64 {
    match d {
        RepairDevice::Reactor => r.reactor.share,
        RepairDevice::CoolantPump => r.pump.share,
        RepairDevice::JunctionBox => r.junction.share,
    }
}

// ---- Resources and components ----

#[derive(Resource)]
struct SliceSounds {
    thud: Handle<SynthAudio>,
    crackle: [Handle<SynthAudio>; 3],
    spray: Handle<SynthAudio>,
    steam: Handle<SynthAudio>,
    repair: Handle<SynthAudio>,
    blip: Handle<SynthAudio>,
    miss: Handle<SynthAudio>,
    chime: Handle<SynthAudio>,
    zap: Handle<SynthAudio>,
    clunk: Handle<SynthAudio>,
    sweep_down: Handle<SynthAudio>,
    sweep_up: Handle<SynthAudio>,
    pump: Handle<SynthAudio>,
    slide: Handle<SynthAudio>,
    whoomp: Handle<SynthAudio>,
    heart: Handle<SynthAudio>,
    confirm: Handle<SynthAudio>,
    muffled: Handle<SynthAudio>,
    gentle: Handle<SynthAudio>,
}

/// A fire's crackle loop, by level.
#[derive(Component)]
struct FireLoop(usize);
#[derive(Component)]
struct SprayLoop;
#[derive(Component)]
struct RepairLoop;
#[derive(Component)]
struct PumpLoop;

fn setup(mut commands: Commands, mut assets: ResMut<Assets<SynthAudio>>) {
    let mut add = |s: Sound| assets.add(SynthAudio(s));
    let s = SliceSounds {
        thud: add(Sound::Thud),
        crackle: [add(Sound::Crackle(0)), add(Sound::Crackle(1)), add(Sound::Crackle(2))],
        spray: add(Sound::Spray),
        steam: add(Sound::Steam),
        repair: add(Sound::RepairLoop),
        blip: add(Sound::Blip),
        miss: add(Sound::Buzz),
        chime: add(Sound::Chime),
        zap: add(Sound::Zap),
        clunk: add(Sound::Clunk),
        sweep_down: add(Sound::SweepDown),
        sweep_up: add(Sound::SweepUp),
        pump: add(Sound::PumpHum),
        slide: add(Sound::Slide),
        whoomp: add(Sound::Whoomp),
        heart: add(Sound::Heart),
        confirm: add(Sound::Confirm),
        muffled: add(Sound::Muffled),
        gentle: add(Sound::Gentle),
    };
    let looped = || PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), ..default() };
    for (level, h) in s.crackle.iter().enumerate() {
        commands.spawn((AudioPlayer(h.clone()), looped(), FireLoop(level)));
    }
    commands.spawn((AudioPlayer(s.spray.clone()), looped(), SprayLoop));
    commands.spawn((AudioPlayer(s.repair.clone()), looped(), RepairLoop));
    commands.spawn((AudioPlayer(s.pump.clone()), looped(), PumpLoop));
    commands.insert_resource(s);
}

/// A one-shot at `volume`, despawned when it ends.
fn one_shot(commands: &mut Commands, h: &Handle<SynthAudio>, volume: f32) {
    commands.spawn((AudioPlayer(h.clone()), PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(volume), ..default() }));
}

/// The player's position, if one walks.
fn walker_pos(players: &Query<&Player>) -> Option<DVec3> {
    players.single().ok().map(|p| p.w.pos)
}

/// A fire's place in the world, on its floor (`DevArea::ground`).
fn fire_world(area: Option<&DevArea>, planet: &PlanetRes, p: [f64; 2]) -> Option<DVec3> {
    area?.ground(planet, p, 0.0).map(|g| g.0)
}

// ---- Systems ----

/// The fire's crackle: the loudest fire in range, its level's loop; the others fade out.
#[allow(clippy::too_many_arguments)]
fn fire_loop(time: Res<Time>, players: Query<&Player>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, settings: Res<Settings>, mut loops: Query<(&FireLoop, &mut AudioSink)>, mut now: Local<[f32; 3]>) {
    let mut target = [0.0f32; 3];
    if let (Some(me), Some(news)) = (walker_pos(&players), items.fire.as_ref()) {
        for f in &news.fires {
            let Some(at) = fire_world(area.as_deref(), &planet, f.pos) else { continue };
            let level = fire_level(f.size);
            let v = crackle_volume(level, (at - me).length());
            if v > target[level] {
                target[level] = v;
            }
        }
    }
    let k = 1.0 - (-time.delta_secs() / 0.3).exp();
    for (v, t) in now.iter_mut().zip(target) {
        *v += (t - *v) * k;
    }
    for (l, mut sink) in &mut loops {
        sink.set_volume(Volume::Linear(loop_volume(now[l.0], &settings)));
    }
}

/// The extinguisher's hiss at the nearest spray, local or remote.
#[allow(clippy::too_many_arguments)]
fn spray_loop(time: Res<Time>, players: Query<&Player>, items: Res<Items>, settings: Res<Settings>, mut loops: Query<&mut AudioSink, With<SprayLoop>>, mut now: Local<f32>) {
    let mut target = 0.0f32;
    if let (Some(me), Some(news)) = (walker_pos(&players), items.fire.as_ref()) {
        for s in &news.sprays {
            target = target.max(SPRAY_VOL * falloff((DVec3::from_array(s.eye) - me).length(), SPRAY_RANGE_M));
        }
    }
    let k = 1.0 - (-time.delta_secs() / 0.15).exp();
    *now += (target - *now) * k;
    for mut sink in &mut loops {
        sink.set_volume(Volume::Linear(loop_volume(*now, &settings)));
    }
}

/// A steam puff where a fire shrinks while a spray is out.
#[allow(clippy::too_many_arguments)]
fn steam_puffs(mut commands: Commands, s: Res<SliceSounds>, time: Res<Time>, players: Query<&Player>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut prev: Local<BTreeMap<u64, f64>>, mut last: Local<Option<f64>>) {
    let Some(news) = items.fire.as_ref() else {
        prev.clear();
        return;
    };
    let now = time.elapsed_secs_f64();
    let mut hit: Option<DVec3> = None;
    for f in &news.fires {
        if let Some(&before) = prev.get(&f.id) {
            if shrank(before, f.size) && !news.sprays.is_empty() {
                hit = fire_world(area.as_deref(), &planet, f.pos).or(hit);
            }
        }
    }
    *prev = news.fires.iter().map(|f| (f.id, f.size)).collect();
    let (Some(at), Some(me)) = (hit, walker_pos(&players)) else { return };
    if last.is_some_and(|l| now - l < STEAM_EVERY_S) {
        return;
    }
    let v = STEAM_VOL * falloff((at - me).length(), FIRE_RANGE_M);
    if v > 0.0 {
        one_shot(&mut commands, &s.steam, v);
        *last = Some(now);
    }
}

/// The repair: the ratchet while a session runs, a blip on a hit, a buzz on a miss, a chime
/// when the machine is full. Local only: the repair state does not reach the clients yet.
/// TODO(initiator): remote repair loops (the co-op lane), once the repair state reaches clients.
#[allow(clippy::too_many_arguments)]
fn repair_sounds(mut commands: Commands, s: Res<SliceSounds>, play: Res<RepairPlay>, items: Res<Items>, settings: Res<Settings>, mut loops: Query<&mut AudioSink, With<RepairLoop>>, mut device: Local<Option<RepairDevice>>, mut age: Local<Option<f64>>) {
    let on = if play.0.is_some() { REPAIR_LOOP_VOL } else { 0.0 };
    for mut sink in &mut loops {
        sink.set_volume(Volume::Linear(loop_volume(on, &settings)));
    }
    match &play.0 {
        Some(p) => {
            *device = Some(p.device);
            // A new press restarts the flash's age, so an age that fell is a new press.
            if let Some((press, a)) = p.flash {
                if age.is_none_or(|prev| a < prev) {
                    match press {
                        Press::Hit => one_shot(&mut commands, &s.blip, BLIP_VOL),
                        Press::Miss => one_shot(&mut commands, &s.miss, MISS_VOL),
                    }
                }
                *age = Some(a);
            } else {
                *age = None;
            }
        }
        None => {
            *age = None;
            if let Some(d) = device.take() {
                if items.reactor.as_ref().is_some_and(|r| part_share(r, d) >= 1.0) {
                    one_shot(&mut commands, &s.chime, CHIME_VOL);
                }
            }
        }
    }
}

/// The reactor's electrics: a zap while overvolted, a clunk per breaker, a sweep when the power
/// goes or comes, a thud on SCRAM (power off with both targets at 0).
#[allow(clippy::too_many_arguments)]
fn electric_sounds(mut commands: Commands, s: Res<SliceSounds>, time: Res<Time>, items: Res<Items>, mut prev: Local<Option<ReactorNews>>, mut zap_at: Local<Option<f64>>) {
    let Some(now_r) = items.reactor else {
        *prev = None;
        return;
    };
    let now = time.elapsed_secs_f64();
    if let Some(before) = *prev {
        for (a, b) in before.breakers.iter().zip(now_r.breakers) {
            if *a != b {
                one_shot(&mut commands, &s.clunk, CLUNK_VOL);
            }
        }
        if before.powered && !now_r.powered {
            one_shot(&mut commands, &s.sweep_down, SWEEP_VOL);
        } else if !before.powered && now_r.powered {
            one_shot(&mut commands, &s.sweep_up, SWEEP_VOL);
        }
        if before.power_on && !now_r.power_on && now_r.set_fission == 0.0 && now_r.set_turbine == 0.0 {
            one_shot(&mut commands, &s.thud, SCRAM_VOL);
        }
    }
    if zap_due(now_r.overvolt, now, *zap_at) {
        one_shot(&mut commands, &s.zap, ZAP_VOL);
        *zap_at = Some(now);
    }
    *prev = Some(now_r);
}

/// The coolant pump's hum: its level and speed follow the flow, positional at the pump.
#[allow(clippy::too_many_arguments)]
fn pump_loop(players: Query<&Player>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, settings: Res<Settings>, mut loops: Query<&mut AudioSink, With<PumpLoop>>) {
    let flow = items.reactor.map_or(0.0, |r| r.flow.clamp(0.0, 1.0));
    let dist = match (walker_pos(&players), fire_world(area.as_deref(), &planet, source_at(Source::Pump))) {
        (Some(me), Some(at)) => (at - me).length(),
        _ => f64::MAX,
    };
    let level = PUMP_VOL * flow as f32 * falloff(dist, PUMP_RANGE_M);
    for mut sink in &mut loops {
        sink.set_volume(Volume::Linear(loop_volume(level, &settings)));
        sink.set_speed(pump_speed(flow));
    }
}

/// The door's slide when its state changes. Every process has the door state (`Items::room`).
/// TODO(initiator): the slide is not positional yet: it is the same in the whole ship.
fn door_sound(mut commands: Commands, s: Res<SliceSounds>, items: Res<Items>, mut prev: Local<Option<bool>>) {
    let open = items.room.door_open;
    if prev.is_some_and(|p| p != open) {
        one_shot(&mut commands, &s.slide, SLIDE_VOL);
    }
    *prev = Some(open);
}

/// The local player's health: a whoomp on catching fire, a muffled tone going down, a gentle one
/// on the way back up, a confirmation when a treatment completes, and the heartbeat by the air.
#[allow(clippy::too_many_arguments)]
fn health_sounds(mut commands: Commands, s: Res<SliceSounds>, time: Res<Time>, own: Res<OwnVitals>, hold: Res<UseHold>, mut prev_hold: Local<Option<f64>>, mut burning: Local<Option<bool>>, mut down: Local<Option<bool>>, mut next_beat: Local<Option<f64>>) {
    let now = time.elapsed_secs_f64();
    if let Some(v) = own.last {
        let on_fire = v.burning;
        if burning.is_some_and(|b| !b) && on_fire {
            one_shot(&mut commands, &s.whoomp, WHOOMP_VOL);
        }
        *burning = Some(on_fire);
        let is_down = v.down.is_some();
        match *down {
            Some(false) if is_down => one_shot(&mut commands, &s.muffled, MUFFLED_VOL),
            Some(true) if !is_down => one_shot(&mut commands, &s.gentle, GENTLE_VOL),
            _ => {}
        }
        *down = Some(is_down);
        // Unconscious: the heartbeat at its slowest, whatever the air.
        let beat = if v.unconscious { Some(HEART_SLOWEST_S) } else { heartbeat_every(v.oxygen) };
        match beat {
            Some(every) => {
                if next_beat.is_none_or(|t| now >= t) {
                    one_shot(&mut commands, &s.heart, HEART_VOL);
                    *next_beat = Some(now + every);
                }
            }
            None => *next_beat = None,
        }
    }
    // The hold ends at its completion or on a release: a release from 90 % on counts as done.
    // TODO(initiator): a hold that ends early at 90 % or more would confirm too.
    let progress = hold.progress();
    if progress.is_none() && prev_hold.is_some_and(|p| p >= CONFIRM_FROM) {
        one_shot(&mut commands, &s.confirm, CONFIRM_VOL);
    }
    *prev_hold = progress;
}

/// Windowed runs: the slice's sounds, from the plugin of `audio`.
pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup);
    app.add_systems(Update, (fire_loop, spray_loop, steam_puffs, repair_sounds, electric_sounds, pump_loop, door_sound, health_sounds).in_set(crate::phases::Frame::Input));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_levels_by_size() {
        assert_eq!(fire_level(0.2), 0);
        assert_eq!(fire_level(1.0), 1);
        assert_eq!(fire_level(2.4), 1);
        assert_eq!(fire_level(2.5), 2);
        assert_eq!(fire_level(5.0), 2);
    }

    #[test]
    fn falloff_is_full_at_the_source_and_silent_past_the_range() {
        assert_eq!(falloff(0.0, 10.0), 1.0);
        assert_eq!(falloff(10.0, 10.0), 0.0);
        assert_eq!(falloff(12.0, 10.0), 0.0);
        assert!(falloff(2.0, 10.0) > falloff(5.0, 10.0));
    }

    #[test]
    fn bigger_fires_crackle_louder_and_farther_is_quieter() {
        assert!(crackle_volume(0, 1.0) < crackle_volume(1, 1.0));
        assert!(crackle_volume(1, 1.0) < crackle_volume(2, 1.0));
        assert!(crackle_volume(2, 1.0) > crackle_volume(2, 6.0));
        assert_eq!(crackle_volume(2, 10.0), 0.0);
    }

    #[test]
    fn a_shrink_is_a_spray_hit() {
        assert!(shrank(2.0, 1.5));
        assert!(!shrank(2.0, 2.0));
        assert!(!shrank(2.0, 2.005));
        assert!(!shrank(1.0, 1.5));
    }

    #[test]
    fn the_heartbeat_speeds_up_as_the_air_falls_below_half() {
        assert_eq!(heartbeat_every(None), None);
        assert_eq!(heartbeat_every(Some(1.0)), None);
        assert_eq!(heartbeat_every(Some(0.5)), None);
        let at_edge = heartbeat_every(Some(0.49)).unwrap();
        assert!((at_edge - HEART_SLOWEST_S).abs() < 0.02, "{at_edge}");
        let mid = heartbeat_every(Some(0.25)).unwrap();
        let none = heartbeat_every(Some(0.0)).unwrap();
        assert!(HEART_SLOWEST_S > mid && mid > none, "{mid} {none}");
        assert_eq!(none, HEART_FASTEST_S);
    }

    #[test]
    fn zaps_come_at_most_every_two_seconds_while_overvolted() {
        assert!(zap_due(true, 0.0, None));
        assert!(!zap_due(true, 1.9, Some(0.0)));
        assert!(zap_due(true, 2.0, Some(0.0)));
        assert!(!zap_due(false, 10.0, Some(0.0)));
        assert!(!zap_due(false, 10.0, None));
    }

    #[test]
    fn the_pump_hum_speeds_with_the_flow() {
        assert_eq!(pump_speed(0.0), PUMP_SPEED.0);
        assert_eq!(pump_speed(1.0), PUMP_SPEED.1);
        assert_eq!(pump_speed(7.0), PUMP_SPEED.1);
    }

    #[test]
    fn the_part_share_follows_the_device() {
        let mut r = ReactorNews::default();
        r.pump.share = 0.3;
        r.junction.share = 1.0;
        assert_eq!(part_share(&r, RepairDevice::CoolantPump), 0.3);
        assert_eq!(part_share(&r, RepairDevice::JunctionBox), 1.0);
        assert_eq!(part_share(&r, RepairDevice::Reactor), 0.0);
    }

    #[test]
    fn every_slice_volume_is_quiet() {
        let vols = [CRACKLE_VOL[0], CRACKLE_VOL[1], CRACKLE_VOL[2], SPRAY_VOL, STEAM_VOL, REPAIR_LOOP_VOL, BLIP_VOL, MISS_VOL, CHIME_VOL, ZAP_VOL, CLUNK_VOL, SWEEP_VOL, SCRAM_VOL, PUMP_VOL, SLIDE_VOL, WHOOMP_VOL, MUFFLED_VOL, GENTLE_VOL, CONFIRM_VOL, HEART_VOL];
        for v in vols {
            assert!(v > 0.0 && v <= ALARM_REF, "{v}");
        }
    }
}
