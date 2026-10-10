//! Real raw-motion/frame/fixed/camera path, including frames with no simulation tick.
use crate::common;
use avian3d::prelude::{Position, Rotation};
use bevy::{input::{keyboard::{Key, KeyboardInput}, mouse::MouseMotion, ButtonState, InputPlugin}, math::{DQuat, DVec3}, prelude::*, time::TimeUpdateStrategy, window::{CursorGrabMode, CursorOptions}};
use exo_app::{controls::{Actions, Controls}, origin::{BodyInterp, WorldPose}, scenario::Script, ship::Ship, view::{MainCamera, PlayerInterp}, walker::Player, Options, TICK};
use walker_core::Frame;

#[derive(Resource, Default)]
struct FixedTicks(u32);

fn app() -> App {
    let o = Options { headless: true, scenario: Some("first-person-settings".into()), out_dir: common::out_dir("raw-mouse-look"), ..default() };
    let mut app = exo_app::build_app(&o);
    // Keep the scenario's renderer-free camera harness, drive live input instead of its script.
    app.world_mut().remove_resource::<Script>();
    app.world_mut().resource_mut::<Controls>().scripted = false;
    app.add_plugins((InputPlugin, exo_app::menu::plugin, exo_app::controls::window_plugin));
    app.world_mut().resource_mut::<exo_app::menu::Menu>().screen = exo_app::menu::Screen::None;
    app.init_resource::<FixedTicks>().add_systems(FixedLast, |mut ticks: ResMut<FixedTicks>| ticks.0 += 1);
    app.world_mut().spawn(CursorOptions { grab_mode: CursorGrabMode::Locked, visible: false, ..default() });
    app.finish();
    app.cleanup();
    app.update();
    app.update();
    app.world_mut().resource_mut::<FixedTicks>().0 = 0;
    app
}

fn player_e(app: &mut App) -> Entity {
    app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap()
}

fn camera_pose(app: &mut App) -> (DVec3, DQuat) {
    let mut q = app.world_mut().query_filtered::<&WorldPose, With<MainCamera>>();
    let pose = q.single(app.world()).unwrap();
    (pose.pos, pose.rot)
}

#[test]
fn raw_motion_is_visible_in_the_same_frame_with_zero_one_or_three_fixed_ticks() {
    for ticks in [0, 1, 3] {
        let mut app = app();
        let e = player_e(&mut app);
        let forward = app.world().get::<Player>(e).unwrap().w.forward;
        // 2 ms is a render frame with no tick. The other cases exercise consumption exactly once.
        let duration = if ticks == 0 { std::time::Duration::from_millis(2) } else { TICK * ticks };
        app.insert_resource(TimeUpdateStrategy::ManualDuration(duration));
        app.world_mut().write_message(MouseMotion { delta: Vec2::new(100.0, 50.0) });
        app.update();
        assert_eq!(app.world().resource::<FixedTicks>().0, ticks);
        let p = app.world().get::<Player>(e).unwrap();
        assert!((forward.angle_between(p.w.forward).to_degrees() - 2.2).abs() < 1e-5);
        assert!((p.pitch.to_degrees() + 1.1).abs() < 1e-9);
        let want = p.world_look(Frame::IDENTITY);
        let pi = app.world().get::<PlayerInterp>(e).unwrap();
        let f = app.world().resource::<Time<Fixed>>().overstep_fraction_f64();
        let want_eye = pi.prev.0.lerp(pi.curr.0, f) + p.view_up * exo_app::walker::EYE_HEIGHT;
        let (eye, rot) = camera_pose(&mut app);
        assert!((rot * DVec3::NEG_Z - want).length() < 1e-10, "camera must show all current counts, even with zero ticks");
        assert!((eye - want_eye).length() < 1e-10, "translation still interpolates");
        assert_eq!(app.world().resource::<Controls>().mouse, Vec2::ZERO);
        assert_eq!(app.world().resource::<Actions>().look, Vec2::ZERO, "fixed step must not turn the mouse again");
        app.update();
        assert!((app.world().get::<Player>(e).unwrap().pitch.to_degrees() + 1.1).abs() < 1e-9, "no tail/smoothing after the mouse stops");
    }
}

#[test]
fn menu_blocks_raw_input_and_seated_input_stays_in_the_vehicle_channel() {
    let mut app = app();
    let e = player_e(&mut app);
    let before = app.world().get::<Player>(e).unwrap().w.forward;
    app.world_mut().write_message(KeyboardInput { key_code: KeyCode::Escape, logical_key: Key::Escape, state: ButtonState::Pressed, text: None, repeat: false, window: Entity::PLACEHOLDER });
    app.world_mut().write_message(MouseMotion { delta: Vec2::new(100.0, 50.0) });
    app.update();
    assert_eq!(app.world().resource::<exo_app::menu::Menu>().screen, exo_app::menu::Screen::Paused, "Escape must open the menu before this frame's gameplay input");
    assert!((app.world().get::<Player>(e).unwrap().w.forward - before).length() < 1e-10);
    assert_eq!(app.world().get::<Player>(e).unwrap().pitch, 0.0);
    app.world_mut().resource_mut::<exo_app::menu::Menu>().screen = exo_app::menu::Screen::None;
    let mut q = app.world_mut().query::<&mut CursorOptions>();
    q.single_mut(app.world_mut()).unwrap().grab_mode = CursorGrabMode::Locked;
    app.world_mut().get_mut::<Player>(e).unwrap().seated = true;
    app.insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(2)));
    app.world_mut().write_message(MouseMotion { delta: Vec2::new(100.0, 50.0) });
    app.update();
    assert_eq!(app.world().get::<Player>(e).unwrap().pitch, 0.0);
    assert_eq!(app.world().resource::<Controls>().mouse, Vec2::new(100.0, 50.0), "vehicle mouse awaits its fixed controller, never first-person look");
}

#[test]
fn the_cabin_transition_does_not_absorb_this_frames_mouse_turn() {
    let mut app = app();
    let e = player_e(&mut app);
    let (_, displayed_before) = camera_pose(&mut app);
    let ship = app.world_mut().query_filtered::<Entity, With<Ship>>().single(app.world()).unwrap();
    let frame = Frame { origin: app.world().get::<Position>(ship).unwrap().0, rot: app.world().get::<Rotation>(ship).unwrap().0 };
    {
        let mut p = app.world_mut().get_mut::<Player>(e).unwrap();
        p.ship = Some(ship);
        p.w.pos = DVec3::new(0.0, 0.32, -2.5);
        p.cabin_up = DVec3::Y;
    }
    let before = {
        let p = app.world().get::<Player>(e).unwrap();
        walker_core::look_rot(p.world_look(frame), frame.rot * p.cabin_up)
    };
    app.insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(2)));
    app.world_mut().write_message(MouseMotion { delta: Vec2::new(100.0, 50.0) });
    app.update();
    assert_eq!(app.world().resource::<FixedTicks>().0, 0);
    let after = {
        let p = app.world().get::<Player>(e).unwrap();
        walker_core::look_rot(p.world_look(frame), frame.rot * p.cabin_up)
    };
    let (_, displayed_after) = camera_pose(&mut app);
    let want = after * before.inverse() * displayed_before;
    assert!((displayed_after * DVec3::NEG_Z - want * DVec3::NEG_Z).length() < 1e-10,
        "even at age zero of the horizon blend, all current mouse counts must be displayed");
}

#[test]
fn current_local_mouse_look_is_carried_by_the_interpolated_cabin() {
    let mut app = app();
    let e = player_e(&mut app);
    let ship = app.world_mut().query_filtered::<Entity, With<Ship>>().single(app.world()).unwrap();
    let pos = app.world().get::<Position>(ship).unwrap().0;
    let rot = app.world().get::<Rotation>(ship).unwrap().0;
    {
        let mut p = app.world_mut().get_mut::<Player>(e).unwrap();
        p.ship = Some(ship);
        p.w.pos = DVec3::new(0.0, 0.32, -2.5);
        p.cabin_up = DVec3::Y;
    }
    // Let the intentional cabin/horizon transition finish before measuring mouse latency.
    for _ in 0..30 { app.update(); }
    // Mimic a smoothly rolling ship between two fixed poses; do not run another fixed tick.
    let prev = rot * DQuat::from_rotation_z(0.2);
    let curr = rot * DQuat::from_rotation_z(0.4);
    app.world_mut().entity_mut(ship).insert(BodyInterp { prev: (pos, prev), curr: (pos, curr) });
    app.insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(2)));
    let pitch_before = app.world().get::<Player>(e).unwrap().pitch;
    app.world_mut().write_message(MouseMotion { delta: Vec2::new(100.0, 50.0) });
    app.update();
    let f = app.world().resource::<Time<Fixed>>().overstep_fraction_f64();
    let frame = Frame { origin: pos, rot: prev.slerp(curr, f) };
    let p = app.world().get::<Player>(e).unwrap();
    assert_eq!(p.ship, Some(ship));
    assert!((p.pitch - pitch_before + 1.1f64.to_radians()).abs() < 1e-9);
    let want = p.world_look(frame);
    let (_, actual) = camera_pose(&mut app);
    assert!((actual * DVec3::NEG_Z - want).length() < 1e-10, "parent rotation interpolates, current local mouse rotation does not");
}
