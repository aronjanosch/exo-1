//! Ship body (Avian rigid body, greybox cabin from ship.gd) driven by flight_core.
use crate::controls::Controls;
use crate::env::PlanetRes;
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::{DMat3, DQuat, DVec2, DVec3};
use bevy::prelude::*;
use flight_core::{BodyState, FlightInput, ShipController};

/// Seat position in ship space (ship.gd).
pub const SEAT_POS: DVec3 = DVec3::new(0.0, 0.6, -3.0);
/// Mouse: radians per pixel (ship.gd).
const MOUSE_SENSITIVITY: f64 = 0.002;

#[derive(Component)]
#[cfg_attr(feature = "remote", derive(Reflect), reflect(Component, from_reflect = false))]
pub struct Ship {
    #[cfg_attr(feature = "remote", reflect(ignore))]
    pub ctl: ShipController,
    pub piloted: bool,
    /// Parked: static until someone first sits down (ship.gd `freeze`).
    pub parked: bool,
    /// Drive while nobody pilots (scripts only), like ship.gd test_input/test_roll/test_boost.
    #[cfg_attr(feature = "remote", reflect(ignore))]
    pub test_input: FlightInput,
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

pub fn spawn_ship(commands: &mut Commands, planet: &PlanetRes, up: DVec3) -> Entity {
    // Parked 15 m ahead of the walker spawn, floor on the highest ground under the hull (main.gd).
    let dir = (up * planet.radius + DVec3::new(0.0, 0.0, -15.0)).normalize();
    let rot = basis_for_up(dir);
    let mut ground = f64::MIN;
    for c in [DVec3::ZERO, DVec3::new(2., 0., 4.), DVec3::new(-2., 0., 4.), DVec3::new(2., 0., -4.), DVec3::new(-2., 0., -4.)] {
        let d = (dir * planet.radius + rot * c).normalize();
        ground = ground.max(planet.surface(d) - planet.radius);
    }
    let pos = planet.centre + dir * (planet.radius + ground + 0.05);
    let hull = Color::srgb(0.95, 0.5, 0.15);
    let inner = Color::srgb(0.55, 0.55, 0.6);
    let glass = Color::srgb(0.3, 0.8, 0.9);
    // [size, position, colour, collides] in ship space; origin at the floor bottom.
    let parts: [(Vec3, Vec3, Color, bool); 8] = [
        (Vec3::new(4.0, 0.3, 8.0), Vec3::new(0.0, 0.15, 0.0), inner, true),
        (Vec3::new(0.3, 2.6, 8.0), Vec3::new(-2.15, 1.6, 0.0), hull, true),
        (Vec3::new(0.3, 2.6, 8.0), Vec3::new(2.15, 1.6, 0.0), hull, true),
        (Vec3::new(4.6, 0.3, 8.0), Vec3::new(0.0, 3.05, 0.0), hull, true),
        (Vec3::new(4.6, 2.6, 0.3), Vec3::new(0.0, 1.6, -4.15), hull, true),
        (Vec3::new(3.6, 1.0, 0.05), Vec3::new(0.0, 2.0, -3.98), glass, false),
        (Vec3::new(1.0, 0.5, 0.8), Vec3::new(0.0, 0.55, -3.3), inner, false),
        (Vec3::new(9.0, 0.25, 2.0), Vec3::new(0.0, 1.2, 1.0), hull, false),
    ];
    // Mass properties are f32 in Avian even with f64 positions.
    let m = 2000.0f32;
    let (w, h, d) = (4.6f32, 3.2f32, 8.3f32);
    let ship = commands
        .spawn((
            Ship { ctl: ShipController::default(), piloted: false, parked: true, test_input: FlightInput::default() },
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
    commands.entity(ship).with_children(|c| {
        for (i, (size, p, color, collides)) in parts.into_iter().enumerate() {
            let mut e = c.spawn((Transform::from_translation(p), ShipPart { size, color }, Visibility::default()));
            if i == 0 {
                e.insert(crate::walker::CabinFloor);
            }
            if collides {
                e.insert((
                    Collider::cuboid(size.x as f64, size.y as f64, size.z as f64),
                    CollisionLayers::new(Layer::Ship, [Layer::World, Layer::Ship]),
                ));
            }
        }
        // Ramp from the floor edge down to 1.5 m below the hull: walker-only solid wedge (ship.gd).
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
    ship
}

pub fn ship_control(
    time: Res<Time>,
    planet: Res<PlanetRes>,
    mut controls: ResMut<Controls>,
    mut q: Query<(&mut Ship, &Position, &Rotation, &mut LinearVelocity, &mut AngularVelocity)>,
) {
    let dt = time.delta_secs_f64();
    for (mut ship, pos, rot, mut lv, mut av) in &mut q {
        if ship.parked {
            continue;
        }
        let input = if ship.piloted {
            if controls.take_tap(KeyCode::KeyH) {
                ship.ctl.hover_assist = !ship.ctl.hover_assist;
            }
            if controls.take_tap(KeyCode::KeyL) {
                ship.ctl.horizon_follow = !ship.ctl.horizon_follow;
            }
            let m = std::mem::take(&mut controls.mouse);
            FlightInput {
                thrust: DVec3::new(
                    controls.axis(KeyCode::KeyD, KeyCode::KeyA),
                    controls.axis(KeyCode::Space, KeyCode::ControlLeft),
                    -controls.axis(KeyCode::KeyW, KeyCode::KeyS),
                ),
                roll: controls.axis(KeyCode::KeyQ, KeyCode::KeyE),
                boost: controls.pressed(KeyCode::ShiftLeft),
                brake: controls.pressed(KeyCode::KeyX),
                mouse: DVec2::new(m.x as f64, m.y as f64) * MOUSE_SENSITIVITY,
                piloted: true,
            }
        } else {
            FlightInput { piloted: false, ..ship.test_input }
        };
        let body = BodyState { pos: pos.0, rot: rot.0, lin_vel: lv.0, ang_vel: av.0 };
        let (v, w) = ship.ctl.step(&body, &input, planet.as_ref(), dt);
        lv.0 = v;
        av.0 = w;
    }
}
