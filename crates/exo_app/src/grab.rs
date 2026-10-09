//! Grab (#82, #83): the crate the walker holds, with the hands (to 2 m) or the grab tool (full
//! force to 6 m, none at 10 m; it reels the crate in to 3 m). Each step `grab_step` pulls the
//! crate towards a hold point in front of the eye through `grab_core::hold_force`; heavy crates
//! lag, one holder cannot lift the large one. The crate keeps its heading relative to the
//! walker's; Q/E turn it. R throws. Weightless, the reaction pulls the walker. The walker side
//! (carry costs, slower view turn) reads `Grab::carry` in `walker_step`.
use crate::cargo::Crate;
use crate::controls::{Actions, Tap};
use crate::ship::Ship;
use crate::walker::{cabin_frame, CabinFloor, Player, EYE_HEIGHT};
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use grab_core::{BreakTimer, CrateSize, GrabConfig, Holder, Reach};
use walker_core::Frame;

#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub crate_e: Entity,
    pub reach: Reach,
    /// Distance from the eye to the hold point (m); the tool reels it in.
    pub dist: f64,
    pub breaker: BreakTimer,
    /// Hands the crate takes and its mass (carry costs on the walker).
    pub hands: u8,
    pub mass: f64,
    /// Heading of the crate relative to the walker's, radians about up (Q/E change it).
    pub yaw: f64,
}

#[derive(Resource, Default, Debug)]
pub struct Grab {
    pub held: Option<Held>,
    /// Reaction on the walker from the last step, world space (N).
    pub reaction: DVec3,
    /// Holds that broke (error too long, standing on the crate).
    pub breaks: u32,
    /// Throws: (crate, world velocity right after the throw).
    pub throws: Vec<(Entity, DVec3)>,
}

impl Grab {
    /// Takes hold of crate `c` (entity `e`); `eye` and `look` are world space, `ship` the own cabin's frame.
    pub fn start(&mut self, cfg: &GrabConfig, e: Entity, c: &Crate, size: &CrateSize, reach: Reach, eye: DVec3, look_fwd: DVec3, ship: &Frame) {
        let frame = if c.ship.is_some() { *ship } else { Frame::IDENTITY };
        let centre = frame.to_world(c.body.pos);
        let dist = match reach {
            Reach::Hands => cfg.hold_gap + c.body.half.z,
            Reach::Tool => centre.distance(eye),
        };
        // Keep the crate's heading relative to the walker's.
        let up = frame.rot * c.body.up;
        let yaw = signed_angle(look_fwd, frame.rot * c.body.forward, up);
        self.held = Some(Held { crate_e: e, reach, dist, breaker: BreakTimer::default(), hands: size.hands, mass: size.mass, yaw });
    }

    pub fn release(&mut self) {
        self.held = None;
        self.reaction = DVec3::ZERO;
    }

    /// What carrying costs the walker now.
    pub fn carry(&self, cfg: &GrabConfig) -> grab_core::Carry {
        grab_core::carry(cfg, self.held.map_or(0, |h| h.hands))
    }
}

/// Angle (radians) that turns `from` to `to` about `up`, both projected onto the plane of `up`.
pub fn signed_angle(from: DVec3, to: DVec3, up: DVec3) -> f64 {
    let a = from - up * from.dot(up);
    let b = to - up * to.dot(up);
    if a.length_squared() < 1e-12 || b.length_squared() < 1e-12 {
        return 0.0;
    }
    up.dot(a.cross(b)).atan2(a.dot(b))
}

/// How fast the crate's heading follows the wanted one (1/s); `grab_core::turn_rate` caps it.
const HEADING_GAIN: f64 = 8.0;

#[allow(clippy::too_many_arguments)]
pub fn grab_step(
    time: Res<Time>,
    tuning: Res<crate::tuning::Tuning>,
    mut actions: ResMut<Actions>,
    mut grab: ResMut<Grab>,
    mut players: Query<&mut Player>,
    ships: Query<(Entity, &Position, &Rotation, &LinearVelocity), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut crates: Query<&mut Crate>,
) {
    let dt = time.delta_secs_f64();
    let cfg = &tuning.grab;
    let throw = actions.take_tap(Tap::Throw);
    let Some(mut held) = grab.held else { return };
    let Ok(mut pl) = players.single_mut() else { return };
    let Some((ship_e, sp, sr, sv)) = ships.iter().next() else { return };
    let ship = cabin_frame(ship_e, (sp, sr), &floors);
    let Ok(mut c) = crates.get_mut(held.crate_e) else {
        grab.release();
        return;
    };
    // Seated, in another player's cabin or in debug fly: let go.
    if pl.seated || pl.fly || pl.ship.is_some_and(|e| e != ship_e) {
        grab.release();
        return;
    }
    let wf = if pl.ship.is_some() { ship } else { Frame::IDENTITY };
    let cf = if c.ship.is_some() { ship } else { Frame::IDENTITY };
    let cf_vel = if c.ship.is_some() { sv.0 } else { DVec3::ZERO };
    let up_w = pl.world_up(wf);
    let eye = pl.world_pos(wf) + up_w * EYE_HEIGHT;
    let look = pl.world_look(wf);
    let walker_vel = if pl.ship.is_some() { sv.0 + ship.rot * pl.w.vel } else { pl.w.vel };
    let to_local = cf.rot.inverse();

    if held.reach == Reach::Tool {
        held.dist = (held.dist - cfg.tool_reel_speed * dt).max(cfg.tool_hold_distance);
    }
    // In the hands the crate sits a little low, its top about at eye height.
    let drop = if held.reach == Reach::Hands { c.body.half.y } else { 0.0 };
    let target = cf.to_local(eye + look * held.dist - up_w * drop);
    let holder = Holder { target, target_vel: to_local * (walker_vel - cf_vel), source: cf.to_local(eye), reach: held.reach };
    // Grabbing unlocks a crate from the plates (#84).
    if c.locked {
        c.locked = false;
    }
    c.touched = Some(time.elapsed_secs_f64());
    let mass = c.body.mass;
    let g = -c.body.up * c.g;
    let out = grab_core::hold_force(cfg, mass, c.body.pos, c.body.vel, g, &[holder]);
    c.push += out.force / mass;
    c.body.wake();

    // Heading: follow the walker's, offset by `yaw`; Q/E turn the offset (not while the suit rolls).
    if pl.body.is_none() {
        held.yaw += actions.roll * cfg.max_turn_rate * dt;
    }
    let fwd_local = to_local * (wf.rot * pl.w.forward);
    let want = DQuat::from_axis_angle(c.body.up, held.yaw) * fwd_local;
    let err = signed_angle(c.body.forward, want, c.body.up);
    c.turn = grab_core::turn_rate(cfg, mass, err * HEADING_GAIN, c.body.contact);

    grab.reaction = cf.rot * out.reactions[0];

    // Standing on it drops it: the feet on its top face, inside its footprint (approximate box).
    let feet = to_local * (pl.world_pos(wf) - cf.origin);
    let rel = feet - c.body.pos;
    let along = rel.dot(c.body.up);
    let flat = rel - c.body.up * along;
    let standing = pl.w.grounded && (along - c.body.half.y).abs() < 0.15 && flat.length() < c.body.half.x.min(c.body.half.z);
    if held.breaker.step(cfg, c.body.pos.distance(target), standing, dt) {
        grab.breaks += 1;
        grab.release();
        return;
    }

    if throw {
        let dir = to_local * look;
        c.body.vel = grab_core::throw_velocity(cfg, mass, c.body.vel, dir);
        let world_vel = cf_vel + cf.rot * c.body.vel;
        grab.throws.push((held.crate_e, world_vel));
        // Weightless, the throw pushes the thrower back.
        if pl.body.is_some() {
            pl.w.vel += grab_core::throw_kick(cfg, mass, look);
        }
        grab.release();
        return;
    }
    grab.held = Some(held);
}

/// The grab tool's beam (night extra E3): a thin glowing rod from the tool, low right in front
/// of the eye, to the crate's centre, only while the tool holds. It turns from cyan to hot
/// orange as the hold strains towards breaking. Render only; headless runs never spawn it.
#[derive(Component)]
pub struct GrabBeam(Handle<StandardMaterial>);

/// Muzzle of the tool in the view frame (right, up, forward; m). Start value, TODO(initiator): tune by feel.
const BEAM_MUZZLE: [f64; 3] = [0.22, -0.25, 0.6];
/// Beam radius (m) and how much it breathes (share of the radius, 6 Hz).
const BEAM_RADIUS: f32 = 0.012;
const BEAM_WOBBLE: f32 = 0.3;

/// Pose of a unit cylinder (along Y, centred) stretched from `from` to `to`: centre, rotation,
/// length. None when the two points coincide.
pub fn beam_pose(from: DVec3, to: DVec3) -> Option<(DVec3, DQuat, f64)> {
    let d = to - from;
    let len = d.length();
    (len > 1e-6).then(|| ((from + to) * 0.5, DQuat::from_rotation_arc(DVec3::Y, d / len), len))
}

pub fn setup_beam(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mat = materials.add(StandardMaterial { base_color: Color::srgba(0.5, 1.0, 1.0, 0.8), emissive: LinearRgba::new(0.4, 3.0, 4.0, 1.0), unlit: true, alpha_mode: AlphaMode::Add, ..default() });
    commands.spawn((GrabBeam(mat.clone()), Mesh3d(meshes.add(Cylinder::new(1.0, 1.0))), MeshMaterial3d(mat), Transform::default(), crate::origin::WorldPose::default(), Visibility::Hidden));
}

/// Places the beam each frame, after the crate's and the walker's interpolated poses.
#[allow(clippy::too_many_arguments)]
pub fn update_beam(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    tuning: Res<crate::tuning::Tuning>,
    grab: Res<Grab>,
    view: Res<crate::view::ViewState>,
    players: Query<(&Player, &crate::view::PlayerInterp)>,
    crates: Query<&crate::origin::WorldPose, (With<Crate>, Without<GrabBeam>)>,
    mut beam: Query<(&GrabBeam, &mut crate::origin::WorldPose, &mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((b, mut pose, mut t, mut vis)) = beam.single_mut() else { return };
    let held = grab.held.filter(|h| h.reach == Reach::Tool);
    let ends = held.and_then(|h| {
        let (pl, pi) = players.single().ok()?;
        // Seen from orbit or a fixed viewpoint the muzzle would float; seated, nothing is held.
        if view.orbit || view.look.is_some() || pl.seated {
            return None;
        }
        let f = fixed.overstep_fraction_f64();
        let feet = pi.prev.0.lerp(pi.curr.0, f);
        let up = pi.prev.1.lerp(pi.curr.1, f).normalize();
        let look = pi.prev.2.lerp(pi.curr.2, f).normalize();
        let rot = walker_core::look_rot(look, up);
        let [x, y, z] = BEAM_MUZZLE;
        let muzzle = feet + up * EYE_HEIGHT + rot * DVec3::new(x, y, -z);
        Some((h, muzzle, crates.get(h.crate_e).ok()?.pos))
    });
    let Some((h, (centre, rot, len))) = ends.and_then(|(h, a, b)| Some((h, beam_pose(a, b)?))) else {
        *vis = Visibility::Hidden;
        return;
    };
    let strain = (h.breaker.t / tuning.grab.break_time.max(1e-6)).clamp(0.0, 1.0) as f32;
    let s = time.elapsed_secs();
    let r = BEAM_RADIUS * (1.0 + BEAM_WOBBLE * (s * 6.0 * std::f32::consts::TAU).sin() * (0.3 + strain));
    (pose.pos, pose.rot) = (centre, rot);
    t.scale = Vec3::new(r, len as f32, r);
    if let Some(mut m) = materials.get_mut(&b.0) {
        m.emissive = LinearRgba::new(0.4 + 3.6 * strain, 3.0 - 1.8 * strain, 4.0 * (1.0 - strain), 1.0);
    }
    *vis = Visibility::Inherited;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_spans_both_ends() {
        let (a, b) = (DVec3::new(1.0, 2.0, 3.0), DVec3::new(4.0, -2.0, 3.0));
        let (c, r, len) = beam_pose(a, b).unwrap();
        assert!((len - 5.0).abs() < 1e-9);
        // The unit cylinder's ends (y = ±0.5) land on the two points.
        assert!((c + r * DVec3::Y * (0.5 * len) - b).length() < 1e-9);
        assert!((c - r * DVec3::Y * (0.5 * len) - a).length() < 1e-9);
        assert!(beam_pose(a, a).is_none());
    }
}
