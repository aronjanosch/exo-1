//! The sounds can not be heard in a test: the window part is untested by ear (`TODO(initiator)`s
//! below; the reactor's drone, beep and whoop are untested by ear: TODO(initiator), levels to tune).
use crate::audio::{loop_volume, Sound, SynthAudio};
use crate::dev_area::engine_room::source_at;
use crate::dev_area::fire::EXTINGUISH;
use crate::dev_area::DevArea;
use crate::env::PlanetRes;
use crate::dev_area::reactor::lamp_blink_on;
use crate::health::{OwnVitals, UseHold};
use crate::items::Items;
use crate::repair::{RepairHurt, RepairPlay};
use crate::settings::Settings;
use crate::walker::Player;
use bevy::audio::{AudioSinkPlayback, PlaybackMode, Volume};
use bevy::math::DVec3;
use bevy::prelude::*;
use device_core::minigame::{Machine, Press};
use item_core::wire::{Lamp, ReactorNews, RepairDevice};
use reactor_core::Source;
use std::collections::BTreeMap;

// ---- Values (TODO(initiator): start values, to be tuned by ear) ----

pub const ALARM_REF: f32 = 0.12;
pub const CRACKLE_VOL: [f32; 3] = [0.04, 0.07, 0.10];
pub const FIRE_LEVEL_M: [f64; 2] = [1.0, 2.5];
pub const FIRE_RANGE_M: f64 = 10.0;
pub const SPRAY_RANGE_M: f64 = 10.0;
pub const PUMP_RANGE_M: f64 = 10.0;
pub const SPRAY_VOL: f32 = 0.06;
pub const STEAM_VOL: f32 = 0.08;
pub const STEAM_EVERY_S: f64 = 0.5;
pub const SHRINK_M: f64 = 0.01;
pub const REPAIR_LOOP_VOL: f32 = 0.05;
pub const BLIP_VOL: f32 = 0.07;
pub const MISS_VOL: f32 = 0.06;
pub const CHIME_VOL: f32 = 0.08;
pub const HISS_VOL: f32 = 0.07;
pub const ZAP_VOL: f32 = 0.09;
pub const ZAP_EVERY_S: f64 = 2.0;
pub const CLUNK_VOL: f32 = 0.09;
/// The GN-01's shot (r40): the thud, louder than the clunk.
pub const GUN_VOL: f32 = 0.14;
/// The enemy's shot (r41), heard from far off: the thud at a low volume (TODO(initiator)).
pub const ENEMY_BOOM_VOL: f32 = 0.04;
/// The dry click of an extinguisher that ran empty: the clunk, quieter.
pub const DRY_VOL: f32 = 0.05;
pub const SWEEP_VOL: f32 = 0.08;
pub const OFF_VOL: f32 = 0.12;
pub const PUMP_VOL: f32 = 0.06;
/// The reactor's drone at fission 100 (Barotrauma's reactor loop: base 0.02, here on our synth's
/// scale), audible within `REACTOR_RANGE_M` of the reactor. Its pitch rises from `REACTOR_PITCH.0`
/// at `REACTOR_PITCH_T.0` degrees to `REACTOR_PITCH.1` at `REACTOR_PITCH_T.1`.
pub const REACTOR_HUM_VOL: f32 = 0.05;
pub const REACTOR_RANGE_M: f64 = 20.0;
pub const REACTOR_PITCH: (f32, f32) = (1.0, 1.15);
pub const REACTOR_PITCH_T: (f64, f64) = (50.0, 100.0);
pub const REACTOR_BEEP_VOL: f32 = 0.08;
pub const BOOST_WHOOP_VOL: f32 = 0.07;
/// The room alarm's klaxon (r40, #165): the alarm beep repeats every `KLAXON_EVERY_S` while the
/// room's alarm sounds, at `KLAXON_VOL` next to the room and falling to 0 at `KLAXON_RANGE_M` (the
/// falloff is the only direction we have: the slice plays no positional sound). TODO(initiator): all three.
pub const KLAXON_VOL: f32 = 0.10;
pub const KLAXON_RANGE_M: f64 = 30.0;
pub const KLAXON_EVERY_S: f64 = 1.2;
/// A boost press moves the boost by the whole amount (25 degrees at the shipped tuning); the fade
/// moves it a degree a second. A move of this many degrees between two snapshots is a press.
pub const BOOST_JUMP_DEG: f64 = 10.0;
pub const PUMP_SPEED: (f32, f32) = (0.8, 1.2);
pub const SLIDE_VOL: f32 = 0.07;
pub const WHOOMP_VOL: f32 = 0.10;
pub const MUFFLED_VOL: f32 = 0.07;
pub const GENTLE_VOL: f32 = 0.07;
pub const CONFIRM_VOL: f32 = 0.06;
pub const CONFIRM_FROM: f64 = 0.9;
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

pub fn crackle_volume(level: usize, dist: f64) -> f32 {
    CRACKLE_VOL[level.min(2)] * falloff(dist, FIRE_RANGE_M)
}

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

pub fn zap_due(overvolt: bool, now: f64, last: Option<f64>) -> bool {
    overvolt && last.is_none_or(|l| now - l >= ZAP_EVERY_S)
}

/// The pump hum's speed for a coolant flow 0..1.
pub fn pump_speed(flow: f64) -> f32 {
    let (lo, hi) = PUMP_SPEED;
    lo + (hi - lo) * flow.clamp(0.0, 1.0) as f32
}

/// The reactor's drone volume: the actual fission (0..100) of a powered reactor, falling off to
/// silence at `REACTOR_RANGE_M`. Silent with the power off.
pub fn reactor_hum_volume(fission: f64, power_on: bool, dist: f64) -> f32 {
    if !power_on {
        return 0.0;
    }
    REACTOR_HUM_VOL * (fission / 100.0).clamp(0.0, 1.0) as f32 * falloff(dist, REACTOR_RANGE_M)
}

/// The drone's level from the news: the actual fission (`ReactorNews::fission`), not the set
/// point, so a set point the reactor has not reached yet is not heard.
pub fn reactor_hum_level(r: &ReactorNews, dist: f64) -> f32 {
    reactor_hum_volume(r.fission, r.power_on, dist)
}

/// The reactor's drone speed for a core temperature in degrees: the pitch rises with the heat.
pub fn reactor_hum_speed(temp: f64) -> f32 {
    let (t0, t1) = REACTOR_PITCH_T;
    let (lo, hi) = REACTOR_PITCH;
    let k = ((temp - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
    lo + (hi - lo) * k
}

/// Whether a bad lamp of the status board is on: the core's critical heat or output, no fuel, or
/// the meltdown. Those blink at 2 Hz (`lamp_blink_on`); the rest of the lamps are steady.
pub fn alarm_lamp_on(r: &ReactorNews) -> bool {
    [Lamp::CritHeat, Lamp::CritOut, Lamp::NoFuel, Lamp::Meltdown].iter().any(|l| r.lamps[l.index()])
}

pub fn alarm_beep_due(bad_lamp: bool, blink_on: bool, prev_blink_on: bool) -> bool {
    bad_lamp && blink_on && !prev_blink_on
}

pub fn boost_jump(prev: f64, now: f64) -> bool {
    (now - prev).abs() >= BOOST_JUMP_DEG
}

/// A device's share of full condition on the reactor snapshot (what the client sees).
pub fn part_share(r: &ReactorNews, d: RepairDevice) -> f64 {
    match d {
        RepairDevice::Reactor => r.reactor.share,
        RepairDevice::CoolantPump => r.pump.share,
        RepairDevice::JunctionBox => r.junction.share,
        RepairDevice::OxygenGenerator => r.oxygen_generator.share,
        RepairDevice::HullBreach => crate::repair::breach_share(&r.breaches),
    }
}

// ---- Resources and components ----

#[derive(Resource)]
pub(crate) struct SliceSounds {
    thud: Handle<SynthAudio>,
    crackle: [Handle<SynthAudio>; 3],
    spray: Handle<SynthAudio>,
    steam: Handle<SynthAudio>,
    repair: Handle<SynthAudio>,
    blip: Handle<SynthAudio>,
    miss: Handle<SynthAudio>,
    chime: Handle<SynthAudio>,
    pub(crate) zap: Handle<SynthAudio>,
    clunk: Handle<SynthAudio>,
    sweep_down: Handle<SynthAudio>,
    sweep_up: Handle<SynthAudio>,
    pump: Handle<SynthAudio>,
    reactor_hum: Handle<SynthAudio>,
    alarm: Handle<SynthAudio>,
    whoop: Handle<SynthAudio>,
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
#[derive(Component)]
struct ReactorLoop;

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
        reactor_hum: add(Sound::ReactorHum),
        alarm: add(Sound::AlarmBeep),
        whoop: add(Sound::Grab),
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
    commands.spawn((AudioPlayer(s.reactor_hum.clone()), looped(), ReactorLoop));
    commands.insert_resource(s);
}

pub(crate) fn one_shot(commands: &mut Commands, h: &Handle<SynthAudio>, volume: f32) {
    commands.spawn((AudioPlayer(h.clone()), PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(volume), ..default() }));
}

/// The gun's shot (r40): the thud when the GN-01 fires (`reactor_screen::shot_fired`), on every machine
/// that gets the news, the host's and the clients'.
fn gun_shot(mut commands: Commands, s: Res<SliceSounds>, items: Res<Items>, mut prev: Local<Option<u8>>, mut jammed: Local<bool>) {
    let Some(news) = items.reactor else {
        *prev = None;
        *jammed = false;
        return;
    };
    if crate::dev_area::reactor_screen::shot_fired(*prev, &news) {
        one_shot(&mut commands, &s.thud, GUN_VOL);
    }
    // The jam (r41): a clunk when the gun jams, once per jam (the news says it until it is cleared).
    if news.weapon_jammed && !*jammed {
        one_shot(&mut commands, &s.clunk, CLUNK_VOL);
    }
    *jammed = news.weapon_jammed;
    *prev = Some(news.weapon_shots);
}

/// The enemy's shot (r41): a distant boom, the thud at `ENEMY_BOOM_VOL`, on every machine that gets the news.
fn enemy_boom(mut commands: Commands, s: Res<SliceSounds>, items: Res<Items>, mut prev: Local<Option<u32>>) {
    let Some(news) = items.reactor else {
        *prev = None;
        return;
    };
    if crate::dev_area::reactor_screen::enemy_shot(*prev, &news) {
        one_shot(&mut commands, &s.thud, ENEMY_BOOM_VOL);
    }
    *prev = news.enemy.map(|e| e.shots);
}

fn walker_pos(players: &Query<&Player>) -> Option<DVec3> {
    players.single().ok().map(|p| p.w.pos)
}

fn fire_world(area: Option<&DevArea>, planet: &PlanetRes, p: [f64; 2]) -> Option<DVec3> {
    area?.ground(planet, p, 0.0).map(|g| g.0)
}

// ---- Systems ----

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
            one_shot(&mut commands, &s.thud, OFF_VOL);
        }
    }
    if zap_due(now_r.overvolt, now, *zap_at) {
        one_shot(&mut commands, &s.zap, ZAP_VOL);
        *zap_at = Some(now);
    }
    *prev = Some(now_r);
}

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

/// The room alarms' klaxons (r40, #165): one repeating beep per room whose alarm sounds, louder the
/// nearer the walker is to the room. A silenced alarm sounds nothing (`ReactorNews::alarms`).
fn room_klaxons(mut commands: Commands, s: Res<SliceSounds>, time: Res<Time>, players: Query<&Player>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut next: Local<[f64; 2]>) {
    let Some(r) = items.reactor else {
        *next = [0.0; 2];
        return;
    };
    let Some(me) = walker_pos(&players) else { return };
    let now = time.elapsed_secs_f64();
    let rooms = [crate::dev_area::engine_room::ROOM.centre, crate::dev_area::engine_room::CORRIDOR.centre];
    for (i, centre) in rooms.into_iter().enumerate() {
        if !r.alarms[i].sounding {
            next[i] = 0.0;
            continue;
        }
        if now < next[i] {
            continue;
        }
        next[i] = now + KLAXON_EVERY_S;
        let Some(at) = fire_world(area.as_deref(), &planet, centre) else { continue };
        let f = falloff((at - me).length(), KLAXON_RANGE_M);
        if f > 0.0 {
            one_shot(&mut commands, &s.alarm, KLAXON_VOL * f);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reactor_alarms(mut commands: Commands, s: Res<SliceSounds>, time: Res<Time>, players: Query<&Player>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut prev_blink: Local<bool>, mut prev_boost: Local<Option<f64>>) {
    let Some(r) = items.reactor else {
        *prev_blink = false;
        *prev_boost = None;
        return;
    };
    let blink = lamp_blink_on(time.elapsed_secs_f64());
    let beep = alarm_beep_due(alarm_lamp_on(&r), blink, *prev_blink);
    *prev_blink = blink;
    let boost_pressed = prev_boost.is_some_and(|p| boost_jump(p, r.boost));
    *prev_boost = Some(r.boost);
    let dist = match (walker_pos(&players), fire_world(area.as_deref(), &planet, source_at(Source::Reactor))) {
        (Some(me), Some(at)) => (at - me).length(),
        _ => return,
    };
    let f = falloff(dist, REACTOR_RANGE_M);
    if beep && f > 0.0 {
        one_shot(&mut commands, &s.alarm, REACTOR_BEEP_VOL * f);
    }
    if boost_pressed && f > 0.0 {
        one_shot(&mut commands, &s.whoop, BOOST_WHOOP_VOL * f);
    }
}

/// The drone follows the actual fission (Barotrauma's `volumeproperty="FissionRate"`). TODO(initiator):
/// the level curve and the pitch are start values, to be tuned by ear.
#[allow(clippy::too_many_arguments)]
fn reactor_hum(players: Query<&Player>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, settings: Res<Settings>, time: Res<Time>, mut loops: Query<&mut AudioSink, With<ReactorLoop>>, mut now: Local<f32>) {
    let mut target = 0.0f32;
    let mut speed = 1.0f32;
    if let Some(r) = items.reactor {
        speed = reactor_hum_speed(r.temp);
        let dist = match (walker_pos(&players), fire_world(area.as_deref(), &planet, source_at(Source::Reactor))) {
            (Some(me), Some(at)) => (at - me).length(),
            _ => f64::MAX,
        };
        target = reactor_hum_level(&r, dist);
    }
    let k = 1.0 - (-time.delta_secs() / 0.3).exp();
    *now += (target - *now) * k;
    for mut sink in &mut loops {
        sink.set_volume(Volume::Linear(loop_volume(*now, &settings)));
        sink.set_speed(speed);
    }
}

fn ecool_hiss(mut commands: Commands, s: Res<SliceSounds>, items: Res<Items>, mut seen: Local<Option<u32>>) {
    let Some(r) = items.reactor else { return };
    if crate::dev_area::reactor::counter_fresh(&mut seen, r.ecools) {
        one_shot(&mut commands, &s.steam, HISS_VOL);
    }
}

/// The repair hurt (a miss this process made): the junction box's zap, the reactor's hiss (the
/// steam), the pump's clank. A new hurt is a new `at`.
fn repair_hurt_sounds(mut commands: Commands, s: Res<SliceSounds>, hurt: Res<RepairHurt>, mut prev: Local<Option<f64>>) {
    if hurt.at.is_some() && hurt.at != *prev {
        match hurt.machine {
            Some(Machine::Electrical) => one_shot(&mut commands, &s.zap, ZAP_VOL),
            Some(Machine::Hot) => one_shot(&mut commands, &s.steam, HISS_VOL),
            Some(Machine::Mechanical) => one_shot(&mut commands, &s.clunk, CLUNK_VOL),
            None => {}
        }
    }
    *prev = hurt.at;
}

/// The held extinguisher's condition went from above 0 to 0 in the same slot (only spraying wears
/// it): one dry click. A slot change or a swap is no click.
fn extinguisher_dry(mut commands: Commands, s: Res<SliceSounds>, items: Res<Items>, mut prev: Local<Option<(usize, f64)>>) {
    let view = &items.client.view;
    let slot = view.selected;
    let held = view
        .slot(slot)
        .filter(|st| items.defs.of(st).and_then(|d| d.use_effect.as_deref()) == Some(EXTINGUISH))
        .and_then(|st| st.item.condition)
        .map(|c| (slot, c));
    if let (Some((ps, pc)), Some((now_slot, c))) = (*prev, held) {
        if ps == now_slot && pc > 0.0 && c <= 0.0 {
            one_shot(&mut commands, &s.clunk, DRY_VOL);
        }
    }
    *prev = held;
}

/// TODO(initiator): the slide is not positional yet: it is the same in the whole ship.
fn door_sound(mut commands: Commands, s: Res<SliceSounds>, items: Res<Items>, mut prev: Local<Option<bool>>) {
    let open = items.room.door_open;
    if prev.is_some_and(|p| p != open) {
        one_shot(&mut commands, &s.slide, SLIDE_VOL);
    }
    *prev = Some(open);
}

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

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup);
    app.add_systems(Update, (fire_loop, spray_loop, steam_puffs, repair_sounds, repair_hurt_sounds, electric_sounds, pump_loop, door_sound, health_sounds, reactor_hum, reactor_alarms, room_klaxons, ecool_hiss, extinguisher_dry, gun_shot, enemy_boom).in_set(crate::phases::Frame::Input));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_is_full_at_the_source_and_silent_past_the_range() {
        assert_eq!(falloff(0.0, 10.0), 1.0);
        assert_eq!(falloff(10.0, 10.0), 0.0);
        assert_eq!(falloff(12.0, 10.0), 0.0);
        assert!(falloff(2.0, 10.0) > falloff(5.0, 10.0));
    }

    #[test]
    fn the_reactor_drone_is_silent_at_no_fission_or_no_power() {
        assert_eq!(reactor_hum_volume(0.0, true, 1.0), 0.0);
        assert_eq!(reactor_hum_volume(100.0, false, 1.0), 0.0);
        assert_eq!(reactor_hum_volume(100.0, true, 20.0), 0.0);
    }

    #[test]
    fn the_drone_follows_the_actual_fission_not_the_set_point() {
        let idle = ReactorNews { power_on: true, set_fission: 100.0, fission: 0.0, ..ReactorNews::default() };
        assert_eq!(reactor_hum_level(&idle, 0.0), 0.0, "the set point alone is silent");
        let full = ReactorNews { power_on: true, set_fission: 0.0, fission: 100.0, ..ReactorNews::default() };
        assert_eq!(reactor_hum_level(&full, 0.0), reactor_hum_volume(100.0, true, 0.0), "the actual 100 is the full level");
        let off = ReactorNews { power_on: false, ..full };
        assert_eq!(reactor_hum_level(&off, 0.0), 0.0, "the power off is silent");
    }

    #[test]
    fn the_alarm_beeps_only_on_the_blink_with_a_bad_lamp() {
        assert!(alarm_beep_due(true, true, false), "the on-phase starts");
        assert!(!alarm_beep_due(true, true, true), "mid on-phase: no second beep");
        assert!(!alarm_beep_due(true, false, true), "the off-phase");
        assert!(!alarm_beep_due(false, true, false), "no bad lamp on");
    }

    #[test]
    fn only_the_bad_lamps_sound_the_alarm() {
        let mut r = ReactorNews::default();
        assert!(!alarm_lamp_on(&r));
        r.lamps[Lamp::Hot.index()] = true;
        r.lamps[Lamp::Scram.index()] = true;
        r.lamps[Lamp::LowOut.index()] = true;
        assert!(!alarm_lamp_on(&r), "warn lamps and the SCRAM lamp are not the alarm");
        for bad in [Lamp::CritHeat, Lamp::CritOut, Lamp::NoFuel, Lamp::Meltdown] {
            let mut one = ReactorNews::default();
            one.lamps[bad.index()] = true;
            assert!(alarm_lamp_on(&one), "{bad:?}");
        }
    }
}
