//! Port of `spikes/planet/flight_test.gd`: controller checks on a cheap
//! spherical fixture at 60 Hz. Initial placement is scripted; measured motion
//! uses input and physics steps. `cargo test -p flight_core -- --nocapture`.
use flight_core::*;
use glam::{DQuat, DVec2, DVec3};

const DT: f64 = 1.0 / 60.0;

struct TestPlanet {
    radius: f64,
    centre: DVec3,
    terrain_height: f64,
    atmosphere: bool,
    field: Field,
}

impl PlanetEnv for TestPlanet {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world - self.centre
    }
    fn radius(&self) -> f64 {
        self.radius
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        self.terrain_height
    }
    fn field(&self) -> &Field {
        &self.field
    }
    fn density_at(&self, world: DVec3) -> f64 {
        if self.atmosphere {
            1.0 - smoothstep(0.0, 1200.0, self.to_planet(world).length() - self.radius)
        } else {
            0.0
        }
    }
}

/// main.gd's planet model (centre, radius 5000, default field).
struct MainModel {
    centre: DVec3,
    field: Field,
}

impl PlanetEnv for MainModel {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world - self.centre
    }
    fn radius(&self) -> f64 {
        5000.0
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.field
    }
}

/// Godot's SpikeInput.held as flags.
#[derive(Default, Clone, Copy)]
struct Keys {
    w: bool,
    s: bool,
    a: bool,
    d: bool,
    space: bool,
    ctrl: bool,
    q: bool,
    e: bool,
    shift: bool,
    x: bool,
}

fn axis(p: bool, n: bool) -> f64 {
    p as i32 as f64 - n as i32 as f64
}

struct Sim {
    planet: TestPlanet,
    body: BodyState,
    ship: ShipController,
    keys: Keys,
    /// Godot's `ship._mouse`, consumed by the next step.
    mouse: DVec2,
    failures: u32,
    checks: u32,
}

impl Sim {
    fn new() -> Sim {
        Sim {
            planet: TestPlanet { radius: 5000.0, centre: DVec3::ZERO, terrain_height: 0.0, atmosphere: false, field: Field::default() },
            body: BodyState::default(),
            ship: ShipController::default(),
            keys: Keys::default(),
            mouse: DVec2::ZERO,
            failures: 0,
            checks: 0,
        }
    }

    fn spawn(&mut self, height: f64, atmosphere: bool) {
        self.keys = Keys::default();
        self.planet = TestPlanet { radius: 5000.0, centre: DVec3::ZERO, terrain_height: 0.0, atmosphere, field: Field::default() };
        self.body = BodyState { pos: DVec3::new(0.0, 5000.0 + height, 0.0), ..Default::default() };
        self.ship = ShipController::default();
        self.ticks(3);
    }

    fn tick(&mut self) {
        let k = self.keys;
        let input = FlightInput {
            thrust: DVec3::new(axis(k.d, k.a), axis(k.space, k.ctrl), -axis(k.w, k.s)),
            roll: axis(k.q, k.e),
            boost: k.shift,
            brake: k.x,
            mouse: self.mouse,
            piloted: true,
        };
        self.mouse = DVec2::ZERO;
        let (v, w) = self.ship.step(&self.body, &input, &self.planet, DT);
        self.body.lin_vel = v;
        self.body.ang_vel = w;
        self.body.integrate(DT);
    }

    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn check(&mut self, ok: bool, note: String) {
        println!("{} {}", if ok { "PASS" } else { "FAIL" }, note);
        self.checks += 1;
        if !ok {
            self.failures += 1;
        }
    }

    fn speed(&self) -> f64 {
        self.body.lin_vel.length()
    }
    fn pos(&self) -> DVec3 {
        self.body.pos
    }
    fn clearance(&self) -> f64 {
        self.ship.clearance_at(&self.planet, self.body.pos)
    }
    fn forward(&self) -> DVec3 {
        self.body.rot * DVec3::NEG_Z
    }
    fn view(&self) -> DVec3 {
        self.body.rot * (DQuat::from_rotation_x(CHASE_CAMERA_PITCH_DEG.to_radians()) * DVec3::NEG_Z)
    }
    fn local_v(&self) -> DVec3 {
        self.body.rot.inverse() * self.body.lin_vel
    }
    /// Elevation of a direction above the local horizontal plane at the ship, degrees.
    fn elevation(&self, dir: DVec3) -> f64 {
        let up = self.planet.to_planet(self.body.pos).normalize();
        dir.dot(up).clamp(-1.0, 1.0).asin().to_degrees()
    }
}

fn fmt_v(v: DVec3) -> String {
    format!("({:.2}, {:.2}, {:.2})", v.x, v.y, v.z)
}

fn horizon_comparison(s: &mut Sim) {
    for (name, pitch_deg, follow, secs) in [("level nose", 0.0, true, 60), ("level view", 10.0, true, 30), ("follow off", 0.0, false, 30)] {
        s.spawn(1200.0, false); // full-follow boundary; above it the field fades
        // Initial orientation only; the controller supplies all later motion.
        s.body.rot = DQuat::from_rotation_x(f64::to_radians(pitch_deg));
        s.ship.horizon_follow = follow;
        s.ticks(2);
        let starting_view = s.elevation(s.view());
        let start_altitude = s.clearance();
        let mut max_change: f64 = 0.0;
        s.keys.w = true;
        for _ in 0..secs * 60 {
            s.tick();
            max_change = max_change.max((s.clearance() - start_altitude).abs());
        }
        let altitude_change = s.clearance() - start_altitude;
        let up = s.planet.to_planet(s.pos()).normalize();
        let nose = s.elevation(s.forward());
        let view = s.elevation(s.view());
        println!(
            "HORIZON {name}: {secs} s, altitude change {altitude_change:+.2} m, max {max_change:.2} m, vertical {:+.2} m/s, nose {nose:+.2}°, view {view:+.2}° (initial view {starting_view:+.2}°)",
            s.body.lin_vel.dot(up)
        );
        match name {
            "level nose" => s.check(max_change < 20.0 && nose.abs() < 0.2, "level nose/follow retains reference altitude over 60 s".into()),
            "level view" => {
                s.check(starting_view.abs() < 0.1, "camera-level setup is initially horizontal".into());
                s.check(
                    altitude_change > 1000.0 && nose >= 9.8,
                    "camera-level aim climbs; fading follow permits further upward pitch".into(),
                );
            }
            _ => s.check(altitude_change > 1000.0, "assist without planet follow leaves the sphere along a straight path".into()),
        }
    }
}

fn field_checks(s: &mut Sim) {
    let mut model = MainModel { centre: DVec3::ZERO, field: Field::default() };
    for altitude in [-10.0, 0.0, 600.0, 1200.0] {
        let g = model.gravity_at(DVec3::Y * (5000.0 + altitude));
        s.check(g.distance(DVec3::NEG_Y * 9.81) < 0.0001, format!("full 9.81 m/s² gravity at {altitude:.0} m"));
    }
    s.check((model.field_strength_at(DVec3::Y * 8600.0) - 0.5).abs() < 0.0001, "half planetary influence at 3600 m".into());
    for boundary in [1200.0, 6000.0] {
        let below = model.field_strength_at(DVec3::Y * (5000.0 + boundary - 1.0));
        let above = model.field_strength_at(DVec3::Y * (5000.0 + boundary + 1.0));
        s.check((below - above).abs() < 0.00001, format!("smooth field boundary at {boundary:.0} m"));
    }
    for altitude in [6000.0, 7000.0, 20000.0] {
        let g = model.gravity_at(DVec3::Y * (5000.0 + altitude));
        s.check(g.abs().max_element() < 1e-5, format!("zero gravity at {altitude:.0} m"));
    }
    let sample = DVec3::Y * 8600.0;
    let expected = model.gravity_at(sample);
    model.centre = DVec3::new(-10000.0, 2000.0, 10000.0);
    s.check(
        model.gravity_at(sample + model.centre).distance(expected) < 0.0001,
        "field and gravity are invariant under origin shifts".into(),
    );

    s.spawn(7000.0, false);
    s.ship.hover_assist = false;
    s.ship.horizon_follow = false;
    let start = s.pos();
    s.ticks(300);
    s.check(
        s.pos().distance(start) < 0.01 && s.speed() < 0.001,
        "unassisted stationary ship outside field does not fall".into(),
    );

    s.spawn(7000.0, false);
    let start = s.pos();
    s.keys.w = true;
    s.ticks(1200);
    let ok = (s.pos().y - start.y).abs() < 0.01
        && s.body.lin_vel.y.abs() < 0.001
        && s.elevation(s.forward()).abs() > 20.0
        && s.ship.planet_follow_strength == 0.0;
    s.check(ok, "enabled planet follow outside field flies straight without rotating the ship".into());
    s.check((s.speed() - 350.0).abs() < 1.0, "assisted forward cruise still works outside field".into());
    let initial_forward = s.forward();
    for _ in 0..30 {
        s.mouse = DVec2::new(0.008, 0.0);
        s.tick();
    }
    s.check(initial_forward.angle_between(s.forward()) > 0.1, "manual mouse steering remains available outside field".into());
}

fn run(s: &mut Sim) {
    s.spawn(15.0, true);
    s.keys.w = true;
    s.ticks(2);
    let sp = s.speed();
    s.check(sp > 0.0 && sp < 0.2, format!("takeoff builds thrust instead of applying full acceleration ({sp:.3} m/s after two ticks)"));
    s.ticks(238);
    let sp = s.speed();
    s.check((sp - 45.0).abs() < 1.0, format!("low flight {sp:.2} m/s (target 45)"));
    s.keys = Keys::default();
    s.ticks(30);
    let sp = s.speed();
    s.check(sp > 20.0, format!("ground stop retains movement after 0.5 s ({sp:.2} m/s)"));
    s.ticks(90);
    let sp = s.speed();
    s.check(sp > 12.0, format!("gentle ground release still moving after 2 s ({sp:.2} m/s)"));
    let mut ground_ticks = 120;
    while s.speed() > 0.5 && ground_ticks < 300 {
        s.tick();
        ground_ticks += 1;
    }
    s.check(s.speed() < 0.5, format!("neutral stops in atmosphere in {:.2} s", ground_ticks as f64 / 60.0));
    let c = s.clearance();
    s.check((c - 15.0).abs() < 2.0, format!("hover/curvature clearance {c:.2} m"));

    s.keys.w = true;
    s.ticks(240);
    s.keys.shift = true;
    s.keys.x = true;
    s.ticks(120);
    s.check(
        s.ship.brake_active && s.ship.commanded_speed == 0.0 && s.speed() < 0.5,
        "firm brake overrides forward/boost and stops ground flight in 2 s".into(),
    );
    s.keys.x = false;
    s.ticks(240);
    s.check(!s.ship.brake_active && (s.speed() - 45.0).abs() < 1.0, "releasing brake restores held movement input".into());

    s.spawn(150.0, false);
    s.keys.w = true;
    s.ticks(300);
    let sp = s.speed();
    s.check((sp - 60.0).abs() < 2.0, format!("150 m flight {sp:.2} m/s (target 60)"));
    let mut distance = 0.0;
    let mut previous = s.pos();
    s.keys = Keys::default();
    s.keys.x = true;
    for _ in 0..120 {
        s.tick();
        distance += previous.distance(s.pos());
        previous = s.pos();
    }
    s.check(s.speed() < 0.5 && distance < 60.0, format!("firm stop {distance:.2} m over 2 s, speed {:.3}", s.speed()));

    s.spawn(150.0, false);
    s.keys.w = true;
    s.keys.shift = true;
    s.ticks(240);
    let sp = s.speed();
    s.check(sp > 140.0 && sp < 155.0, format!("boost at 150 m {sp:.2} m/s"));
    s.keys.shift = false;
    s.ticks(240);
    let sp = s.speed();
    s.check((sp - 60.0).abs() < 3.0, format!("boost release returns to cruise {sp:.2} m/s"));

    s.spawn(150.0, false);
    s.keys.w = true;
    s.keys.d = true;
    s.ticks(240);
    let sp = s.speed();
    s.check(sp < 61.0, format!("diagonal speed {sp:.2} m/s"));
    let mut acceleration_max: f64 = 0.0;
    let mut last_v = s.body.lin_vel;
    for _ in 0..120 {
        s.mouse = DVec2::new(0.01, 0.0); // same mouse accumulator used by input events
        s.tick();
        acceleration_max = acceleration_max.max((s.body.lin_vel - last_v).length() * 60.0);
        last_v = s.body.lin_vel;
    }
    s.ticks(120);
    let lv = s.local_v();
    s.check(lv.z < -30.0 && lv.y.abs() < 2.0, format!("turn redirects movement; local velocity {}", fmt_v(lv)));
    s.check(acceleration_max < 40.5, format!("turn total acceleration {acceleration_max:.2} m/s²"));

    s.spawn(1200.0, false); // full-follow high-speed regression before the fade starts
    s.keys.w = true;
    s.ticks(1500);
    let sp = s.speed();
    s.check((sp - 350.0).abs() < 3.0, format!("high flight {sp:.2} m/s (target 350)"));
    let c = s.clearance();
    s.check((c - 1200.0).abs() < 20.0, format!("curved high flight clearance {c:.2} m"));
    let mut high_distance = 0.0;
    previous = s.pos();
    s.keys = Keys::default();
    let mut stop_ticks = 0;
    while s.speed() > 0.5 && stop_ticks < 540 {
        s.tick();
        high_distance += previous.distance(s.pos());
        previous = s.pos();
        stop_ticks += 1;
    }
    s.check(
        stop_ticks > 240 && stop_ticks < 540 && s.speed() < 0.5,
        format!("350 m/s gentle stop {:.2} s / {high_distance:.1} m / residual {:.3} m/s", stop_ticks as f64 / 60.0, s.speed()),
    );
    s.check((s.clearance() - 1200.0).abs() < 20.0, "gentle high-speed stop retains curved flight".into());

    s.keys.w = true;
    s.ticks(480);
    s.keys = Keys::default();
    s.keys.x = true;
    let before_brake = s.body.lin_vel;
    s.ticks(2);
    s.check(
        s.speed() > 340.0 && (s.body.lin_vel - before_brake).length() < 6.0,
        "pressing firm brake preserves momentum and bounds initial correction".into(),
    );
    let mut firm_ticks = 2;
    while s.speed() > 0.5 && firm_ticks < 240 {
        s.tick();
        firm_ticks += 1;
    }
    s.check(
        firm_ticks < stop_ticks / 2 && firm_ticks <= 210 && s.speed() < 0.5,
        format!("350 m/s firm stop {:.2} s (gentle {:.2} s)", firm_ticks as f64 / 60.0, stop_ticks as f64 / 60.0),
    );
    s.ticks(240 - firm_ticks);
    s.check(s.speed() < 0.5, "held brake settles at rest within 4 s".into());
    s.keys.x = false;

    // A heading change must redirect trajectory, not just the model.
    s.keys.w = true;
    s.ticks(480);
    for _ in 0..30 {
        s.mouse = DVec2::new(0.008, 0.0);
        s.tick();
    }
    s.ticks(150);
    let lv = s.local_v();
    s.check(lv.z < -330.0 && lv.x.abs() < 5.0, format!("high-speed turn catches heading within 2.5 s; local velocity {}", fmt_v(lv)));

    s.spawn(2000.0, false);
    s.keys.w = true;
    s.ticks(600);
    s.keys = Keys::default();
    let v0 = s.body.lin_vel;
    s.ship.hover_assist = false;
    s.ticks(2);
    s.check((s.body.lin_vel - v0).length() < 1.0, "assist toggle preserves momentum".into());
    s.ticks(120);
    let sp = s.speed();
    s.check(sp > 340.0, format!("unassisted vacuum coasts {sp:.2} m/s"));
    s.ship.hover_assist = true;
    s.ticks(540);
    let sp = s.speed();
    s.check(sp < 0.5, format!("assist arrests high-speed drift in 9 s ({sp:.3} m/s)"));

    s.spawn(2000.0, false);
    s.keys.w = true;
    s.ticks(600);
    s.ship.hover_assist = false;
    s.keys.x = true;
    s.ticks(240);
    s.check(
        s.ship.brake_active && s.ship.commanded_speed == 0.0 && s.speed() < 0.5,
        format!(
            "firm brake works with assist off and overrides held W; active {}, goal {:.3}, residual {:.3}",
            s.ship.brake_active,
            s.ship.commanded_speed,
            s.speed()
        ),
    );
    // Issue #6: held longer, the brake stops exactly, so nothing drifts on after release.
    s.ticks(120);
    let sp = s.speed();
    s.check(sp < 0.01, format!("firm brake with assist off comes to rest within 6 s ({sp:.5} m/s)"));
    s.keys.x = false;
    s.ticks(30);
    s.check(!s.ship.brake_active && s.speed() > 5.0, "brake release restores manual thrust".into());
    s.keys = Keys::default();
    s.ticks(120);
    s.check(s.speed() > 5.0, "manual flight still coasts after braking".into());

    s.spawn(700.0, false);
    s.keys.w = true;
    s.keys.ctrl = true;
    s.ticks(1800);
    s.check(
        s.ship.forward_speed_limit < 120.0 && s.speed() < 90.0,
        format!("descending lowers limit {:.2}, speed {:.2}, clearance {:.2}", s.ship.forward_speed_limit, s.speed(), s.clearance()),
    );

    s.spawn(600.0, false);
    s.planet.terrain_height = 570.0;
    s.keys.w = true;
    s.ticks(240);
    let sp = s.speed();
    s.check((sp - 45.0).abs() < 2.0, format!("mountain clearance governs speed {sp:.2}"));
    let before = s.body.lin_vel;
    let offset = DVec3::new(10000.0, 0.0, 0.0);
    s.planet.centre -= offset;
    s.body.pos -= offset;
    s.ticks(3);
    s.check(
        (s.clearance() - 30.0).abs() < 2.0 && (s.body.lin_vel - before).length() < 1.0,
        "origin shift preserves flight frame".into(),
    );

    horizon_comparison(s);
    field_checks(s);
    s.keys = Keys::default();
}

#[test]
fn flight_checks() {
    let mut s = Sim::new();
    run(&mut s);
    println!("FLIGHT TEST: {} checks, {} failures", s.checks, s.failures);
    assert_eq!(s.failures, 0);
}
