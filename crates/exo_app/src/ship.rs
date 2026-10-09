//! Ship body (Avian rigid body, greybox cabin) driven by flight_core.
use crate::controls::{Actions, Bindings, ShipMouse, Tap};
use crate::env::PlanetRes;
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::{DMat3, DQuat, DVec2, DVec3};
use bevy::prelude::*;
use flight_core::{AxisTuning, BodyState, FlightInput, Lag, ShipController, ShipTuning, VirtualStick};

pub fn plugin(app: &mut App) {
    app.init_resource::<CameraEffects>();
    app.add_systems(FixedUpdate, ship_control.in_set(crate::phases::Fx::Ship));
    app.add_systems(FixedUpdate, camera_fx.in_set(crate::phases::Fx::Effects));
}

/// Seat position in ship space.
pub const SEAT_POS: DVec3 = DVec3::new(0.0, 0.6, -3.0);

#[derive(Component)]
pub struct Ship {
    pub ctl: ShipController,
    pub piloted: bool,
    /// Parked: static until someone first sits down.
    pub parked: bool,
    /// Drive while nobody pilots (scenarios only).
    pub test_input: FlightInput,
    /// Cabin gravity (LAG): off while landed.
    pub lag: Lag,
    /// The mouse as a virtual joystick (bindings `ship_mode: "vjoy"`); centred while nobody pilots.
    pub stick: VirtualStick,
    /// A hull collider touches something (the ground; ships do not touch each other).
    pub grounded: bool,
}

/// A ship owned by another player: kinematic proxy driven from snapshots (net module).
#[derive(Component)]
pub struct RemoteShip {
    pub owner: u32,
    /// Cabin gravity (LAG) level from the owner's snapshots, 0..1 (issue #11).
    pub lag: f64,
}

/// Visual-only part (the view plugin adds meshes for these).
#[derive(Component, Clone)]
pub struct ShipPart {
    pub size: Vec3,
    pub color: Color,
}

/// True if a point in ship space (the walker's feet) is inside the cabin.
pub fn cabin_contains(p: DVec3, margin: f64) -> bool {
    p.x.abs() < 1.95 + margin && p.y > -margin && p.y < 2.9 + margin && p.z.abs() < 4.0 + margin
}

pub fn basis_for_up(up: DVec3) -> DQuat {
    let mut fwd = DVec3::NEG_Z - up * DVec3::NEG_Z.dot(up);
    if fwd.length_squared() < 1e-12 {
        fwd = DVec3::X - up * up.x;
    }
    let fwd = fwd.normalize();
    DQuat::from_mat3(&DMat3::from_cols(fwd.cross(up), up, -fwd))
}

pub fn spawn_ship(commands: &mut Commands, planet: &PlanetRes, tuning: &ShipTuning, axis: &AxisTuning, up: DVec3, offset_x: f64) -> Entity {
    // Parked 15 m ahead of the walker spawn, floor on the highest ground under the hull.
    let dir = (up * planet.radius + DVec3::new(offset_x, 0.0, -15.0)).normalize();
    let rot = basis_for_up(dir);
    let mut ground = f64::MIN;
    for c in [DVec3::ZERO, DVec3::new(2., 0., 4.), DVec3::new(-2., 0., 4.), DVec3::new(2., 0., -4.), DVec3::new(-2., 0., -4.)] {
        let d = (dir * planet.radius + rot * c).normalize();
        ground = ground.max(planet.surface(d) - planet.radius);
    }
    let pos = planet.centre + dir * (planet.radius + ground + 0.05);
    // Mass properties are f32 in Avian even with f64 positions.
    let m = 2000.0f32;
    let (w, h, d) = (4.6f32, 3.2f32, 8.3f32);
    let ship = commands
        .spawn((
            Ship { ctl: ShipController::new(tuning.clone()).with_axis(axis.clone()), piloted: false, parked: true, test_input: FlightInput::default(), lag: Lag::default(), stick: VirtualStick::default(), grounded: false },
            RigidBody::Static,
            Position(pos),
            Rotation(rot),
            Transform::default(),
            // Explicit mass: the ramp and the visuals must not add any.
            Mass(m),
            AngularInertia::new(Vec3::new(m / 12.0 * (h * h + d * d), m / 12.0 * (w * w + d * d), m / 12.0 * (w * w + h * h))),
            CenterOfMass(Vec3::new(0.0, 1.4, -0.3)),
            (NoAutoMass, NoAutoAngularInertia, NoAutoCenterOfMass),
            (LinearDamping(0.0), AngularDamping(0.0), SweptCcd::default(), SleepingDisabled),
            Visibility::default(),
        ))
        .id();
    add_hull(commands, ship, Layer::Ship);
    ship
}

/// Greybox cabin: visual parts, hull colliders, ramp. Shared by the own ship and the remote proxies.
/// `layer` is `Layer::Ship` for the own ship; remote proxies use `Layer::Remote` (no ship contact).
pub fn add_hull(commands: &mut Commands, ship: Entity, layer: Layer) {
    let hull = Color::srgb(0.95, 0.5, 0.15);
    let inner = Color::srgb(0.55, 0.55, 0.6);
    // See-through: the tunnel streaks show through the window.
    let glass = Color::srgba(0.3, 0.8, 0.9, 0.12);
    // [size, position, colour, collides, drawn] in ship space; origin at the floor bottom.
    // The front wall collides as one block; it is drawn as four pieces around the window opening
    // (3.6 x 1.0 m at 1.5 to 2.5 m), which has the glass in it.
    let parts: [(Vec3, Vec3, Color, bool, bool); 12] = [
        (Vec3::new(4.0, 0.3, 8.0), Vec3::new(0.0, 0.15, 0.0), inner, true, true),
        (Vec3::new(0.3, 2.6, 8.0), Vec3::new(-2.15, 1.6, 0.0), hull, true, true),
        (Vec3::new(0.3, 2.6, 8.0), Vec3::new(2.15, 1.6, 0.0), hull, true, true),
        (Vec3::new(4.6, 0.3, 8.0), Vec3::new(0.0, 3.05, 0.0), hull, true, true),
        (Vec3::new(4.6, 2.6, 0.3), Vec3::new(0.0, 1.6, -4.15), hull, true, false),
        (Vec3::new(4.6, 1.2, 0.3), Vec3::new(0.0, 0.9, -4.15), hull, false, true),
        (Vec3::new(4.6, 0.4, 0.3), Vec3::new(0.0, 2.7, -4.15), hull, false, true),
        (Vec3::new(0.5, 1.0, 0.3), Vec3::new(-2.05, 2.0, -4.15), hull, false, true),
        (Vec3::new(0.5, 1.0, 0.3), Vec3::new(2.05, 2.0, -4.15), hull, false, true),
        (Vec3::new(3.6, 1.0, 0.05), Vec3::new(0.0, 2.0, -4.15), glass, false, true),
        (Vec3::new(1.0, 0.5, 0.8), Vec3::new(0.0, 0.55, -3.3), inner, false, true),
        (Vec3::new(9.0, 0.25, 2.0), Vec3::new(0.0, 1.2, 1.0), hull, false, true),
    ];
    commands.entity(ship).with_children(|c| {
        for (i, (size, p, color, collides, drawn)) in parts.into_iter().enumerate() {
            let mut e = c.spawn((Transform::from_translation(p), Visibility::default()));
            if drawn {
                e.insert(ShipPart { size, color });
            }
            if i == 0 {
                e.insert(crate::walker::CabinFloor);
            }
            if collides {
                e.insert((
                    Collider::cuboid(size.x as f64, size.y as f64, size.z as f64),
                    CollisionLayers::new(layer, [Layer::World, Layer::Ship]),
                ));
            }
        }
        // Ramp from the floor edge down to 1.5 m below the hull: walker-only solid wedge.
        let mut pts = Vec::new();
        for x in [-1.5, 1.5] {
            pts.extend([DVec3::new(x, 0.3, 4.0), DVec3::new(x, -0.5, 6.6), DVec3::new(x, -1.5, 6.6), DVec3::new(x, -1.5, 4.0)]);
        }
        c.spawn((
            Transform::default(),
            Collider::convex_hull(pts).expect("ramp hull"),
            CollisionLayers::new(Layer::Ramp, LayerMask::NONE),
        ));
        c.spawn((
            Transform::from_xyz(0.0, -0.148, 5.285).with_rotation(Quat::from_rotation_x(17.1f32.to_radians())),
            ShipPart { size: Vec3::new(3.0, 0.1, 2.72), color: inner },
            Visibility::default(),
        ));
    });
}

pub fn ship_control(
    time: Res<Time>,
    planet: Res<PlanetRes>,
    mut actions: ResMut<Actions>,
    bindings: Res<Bindings>,
    settings: Res<crate::settings::Settings>,
    warp: Res<crate::warp::WarpDrive>,
    mut q: Query<(Entity, &mut Ship, &Position, &Rotation, &mut LinearVelocity, &mut AngularVelocity)>,
    collisions: Collisions,
    colliders: Query<(Entity, &ColliderOf)>,
) {
    let dt = time.delta_secs_f64();
    for (e, mut ship, pos, rot, mut lv, mut av) in &mut q {
        ship.grounded = colliders.iter().any(|(c, of)| of.body == e && collisions.collisions_with(c).next().is_some());
        let clearance = ship.ctl.clearance_at(planet.as_ref(), pos.0);
        ship.lag.step(clearance, lv.0.length(), dt);
        // From the pre-ramp on the drive holds the ship.
        if ship.parked || warp.drive.phase.holds_ship() {
            // The boost meter goes on (released): no boost left running through a quantum flight.
            let ctl = &mut ship.ctl;
            ctl.boost.step(false, &ctl.tuning.boost_capacitor, dt);
            ctl.boost_strength = 0.0;
            continue;
        }
        if !ship.piloted {
            ship.stick = VirtualStick::default();
        }
        let input = if ship.piloted {
            if actions.take_tap(Tap::HoverAssist) {
                ship.ctl.hover_assist = !ship.ctl.hover_assist;
            }
            if actions.take_tap(Tap::HorizonFollow) {
                ship.ctl.horizon_follow = !ship.ctl.horizon_follow;
            }
            if actions.take_tap(Tap::Decoupled) {
                ship.ctl.coupled = !ship.ctl.coupled;
            }
            if actions.take_tap(Tap::BoostMode) {
                ship.ctl.boost_stage = !ship.ctl.boost_stage;
            }
            if actions.take_tap(Tap::LandingMode) {
                ship.ctl.landing_mode = !ship.ctl.landing_mode;
            }
            if actions.take_tap(Tap::FlightModel) {
                let next = ship.ctl.model.next();
                ship.ctl.set_model(next);
            }
            let mb = &bindings.mouse;
            let m = std::mem::take(&mut actions.look);
            let m = DVec2::new(m.x as f64, m.y as f64) * mb.ship_sensitivity * settings.mouse_sensitivity;
            let (mouse, turn) = match mb.ship_mode {
                ShipMouse::Direct => (m, actions.turn),
                ShipMouse::Vjoy => {
                    ship.stick.push(m, mb.vjoy_max_angle);
                    (DVec2::ZERO, actions.turn + ship.stick.deflection(mb.vjoy_deadzone, mb.vjoy_max_angle, Some(&mb.vjoy_curve)))
                }
            };
            FlightInput { thrust: actions.move_dir, roll: actions.roll, boost: actions.boost, brake: actions.brake, mouse, turn: turn.clamp(DVec2::NEG_ONE, DVec2::ONE), piloted: true, grounded: ship.grounded }
        } else {
            FlightInput { piloted: false, grounded: ship.grounded, ..ship.test_input }
        };
        let body = BodyState { pos: pos.0, rot: rot.0, lin_vel: lv.0, ang_vel: av.0 };
        let (v, w) = ship.ctl.step(&body, &input, planet.as_ref(), dt);
        lv.0 = v;
        av.0 = w;
    }
}

/// Camera effects of the own ship (#27), stepped with the simulation so scenarios can check them;
/// the view only applies them.
#[derive(Resource, Default)]
pub struct CameraEffects(pub flight_core::camera::CameraFx);

pub fn camera_fx(
    time: Res<Time>,
    planet: Res<PlanetRes>,
    tuning: Res<crate::tuning::Tuning>,
    mut fx: ResMut<CameraEffects>,
    q: Query<(&Ship, &Position, &Rotation, &LinearVelocity, &AngularVelocity)>,
) {
    let Ok((ship, pos, rot, lv, av)) = q.single() else { return };
    let up = planet.up(pos.0);
    let local = rot.0.inverse() * av.0;
    // Hull corners touch first on a slope: up to 2.5 m above the terrain under the centre.
    let near = ship.ctl.clearance_at(planet.as_ref(), pos.0) < 2.5;
    fx.0.step(&tuning.camera, lv.0.length(), DVec2::new(local.x, local.y), -lv.0.dot(up), near, time.delta_secs_f64());
}
