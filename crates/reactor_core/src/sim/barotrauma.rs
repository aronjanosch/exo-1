//! Test-only oracle: a straight f64 port of Barotrauma's single-reactor update, for side-by-side
//! checks of `reactor::Reactor`. Source: Barotrauma's `Reactor.cs` (`BarotraumaShared/SharedSource/
//! Items/Components/Machines/`). Each ported piece names its line numbers in that file.
//!
//! Left out, on purpose:
//! - the round start (`RoundDuration < 5`: rapid auto-temp), the signal and network parts, the
//!   fuel items and their refuelling, the AI user, and the
//!   item's own condition (fire and meltdown timers run at full condition, `dt` per second);
//! - the fuel burn: Barotrauma takes the rods' condition out of `AvailableFuel`; our rods carry a
//!   fixed heat until they are spent, so `fuel` is a constant input here and the burn is not
//!   ported (it cannot change temperature, fission or turbine in these tests);
//! - the multi-reactor branch of `GetConnectionPowerOut` (one reactor: its output is the load,
//!   clamped to the range).
//! Barotrauma works in f32, this is f64: agreement is to about 1e-6, not bit for bit.
//!
//! Its power-off step (targets to zero) comes after the lerps, as in Barotrauma; `reactor::Reactor`
//! zeroes the targets before the lerps. The difference shows only with power off.

/// Barotrauma's `MathHelper.Lerp`.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// `Math.Sign`: 0 for 0 (Rust's `signum` gives 1).
fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// The reactor as Barotrauma runs it, for one reactor on one grid.
#[derive(Clone, Debug)]
pub struct Baro {
    /// `MaxPowerOutput` (`GetMaxOutput`, line 890).
    max_output: f64,
    /// `FireDelay` (line 113 default 30) and `MeltdownDelay` (line 105 default 120).
    fire_delay: f64,
    meltdown_delay: f64,
    /// `AvailableFuel`, held constant (see the module doc).
    fuel: f64,
    /// `degreeOfSuccess` (line 41), 0..1: the operator's skill.
    degree: f64,
    /// `PowerOn` (line 54).
    pub power_on: bool,
    /// `AutoTemp` (line 27).
    pub auto_temp: bool,
    /// `fissionRate` (line 17), `turbineOutput` (line 20), `temperature` (line 22).
    pub fission: f64,
    pub turbine: f64,
    pub temp: f64,
    /// `temperatureBoost` (Reactor.cs line 14 for the amount, 52 for the allow): set by the test,
    /// as the client's buttons do; faded in `step` (line 327).
    pub boost: f64,
    /// `TargetFissionRate` (line 192), `TargetTurbineOutput` (line 195), `CorrectTurbineOutput`
    /// (line 198).
    pub target_fission: f64,
    pub target_turbine: f64,
    correct_turbine: f64,
    /// `optimalTurbineOutput`, `optimalTemperature`, `optimalFissionRate`, `allowedFissionRate`,
    /// `allowedTemperature` (lines 43-45): (X, Y) pairs, set each step.
    opt_turbine: (f64, f64),
    opt_temp: (f64, f64),
    opt_fission: (f64, f64),
    allowed_fission: (f64, f64),
    allowed_temp: (f64, f64),
    /// `minUpdatePowerOut` and `maxUpdatePowerOut` (lines 35-36), and the output the grid took
    /// (`-currPowerConsumption`, line 601 reads it).
    min_out: f64,
    max_out: f64,
    pub output: f64,
    /// `meltDownTimer`, `fireTimer` (lines 31-32).
    pub melt_timer: f64,
    pub fire_timer: f64,
    /// `MeltDown()` ran.
    pub melted: bool,
    /// Fires started (`new FireSource`, line 577).
    pub fires: u32,
}

impl Baro {
    /// A reactor that is off, cold, with the given values. `degree` is the skill as 0..1.
    pub fn new(
        max_output: f64,
        fire_delay: f64,
        meltdown_delay: f64,
        fuel: f64,
        degree: f64,
    ) -> Baro {
        Baro {
            max_output,
            fire_delay,
            meltdown_delay,
            fuel,
            degree,
            power_on: false,
            auto_temp: false,
            fission: 0.0,
            turbine: 0.0,
            temp: 0.0,
            boost: 0.0,
            target_fission: 0.0,
            target_turbine: 0.0,
            correct_turbine: 0.0,
            opt_turbine: (0.0, 0.0),
            opt_temp: (0.0, 0.0),
            opt_fission: (0.0, 0.0),
            allowed_fission: (0.0, 0.0),
            allowed_temp: (0.0, 0.0),
            min_out: 0.0,
            max_out: 0.0,
            output: 0.0,
            melt_timer: 0.0,
            fire_timer: 0.0,
            melted: false,
            fires: 0,
        }
    }

    /// One `Update(deltaTime)` with the grid asking for `load` kW (`Update`, lines 218-399), then
    /// the power grid's `GetConnectionPowerOut` for this reactor.
    pub fn step(&mut self, dt: f64, load: f64) {
        if self.melted {
            return;
        }
        // Line 298: the fuel of the last step (constant here).
        let prev_fuel = self.fuel;

        // Lines 303-306: the smoothed correct turbine output, 20 per second at most.
        if self.max_output.abs() > 1e-9 {
            self.correct_turbine +=
                ((load / self.max_output * 100.0) - self.correct_turbine).clamp(-20.0, 20.0) * dt;
        }

        // Lines 310-321: the bands, from the skill and the correct turbine output. The fission
        // bands' X is capped at Y - 10 (lines 319-320 and 321-322).
        let d = self.degree;
        let tol = lerp(2.5, 10.0, d);
        self.opt_turbine = (self.correct_turbine - tol, self.correct_turbine + tol);
        self.opt_temp = (lerp(40.0, 30.0, d), lerp(60.0, 70.0, d));
        self.allowed_temp = (lerp(30.0, 10.0, d), lerp(70.0, 90.0, d));
        let f = self.fuel;
        let y = lerp(f - 20.0, f - 10.0, d);
        self.opt_fission = (lerp(30.0, 20.0, d).min(y - 10.0), y);
        let y = f;
        self.allowed_fission = (lerp(20.0, 10.0, d).min(y - 10.0), y);

        // Lines 323-326: heat (`GetGeneratedHeat`, lines 500-503, with the boost) minus turbine,
        // moved towards at 10 degrees per second, the setter clamps to 0..100 (lines 120-128).
        let heat = self.fission * (prev_fuel / 100.0) * 2.0 + self.boost;
        let diff = (heat - self.turbine) - self.temp;
        self.temp =
            (self.temp + (sign(diff) * 10.0 * dt).clamp(-diff.abs(), diff.abs())).clamp(0.0, 100.0);
        // Line 327: the boost fades to 0 at 1 per second, never past 0 (adjustValueWithoutOverShooting,
        // line 293-296).
        self.boost = if self.boost > 0.0 {
            (self.boost - dt).max(0.0)
        } else {
            (self.boost + dt).min(0.0)
        };

        // Lines 332-334: fission and turbine follow their targets (setters clamp, lines 131-150).
        self.fission = lerp(self.fission, self.target_fission.min(prev_fuel), dt).clamp(0.0, 100.0);
        self.turbine = lerp(self.turbine, self.target_turbine, dt).clamp(0.0, 100.0);

        // Lines 338-345: off, the targets are zero; on with auto temp, the controller (speed 2).
        if !self.power_on {
            self.target_fission = 0.0;
            self.target_turbine = 0.0;
        } else if self.auto_temp {
            self.update_auto_temp(2.0, dt, load);
        }

        // Line 395: UpdateFailures (the fire and the meltdown timers).
        self.update_failures(dt);
        if self.melted {
            return;
        }

        // Lines 436-457 (`MinMaxPowerOut`), then 464-495 (`GetConnectionPowerOut`) with one reactor.
        self.min_max();
        let left = load.max(0.0);
        let range = self.max_out - self.min_out;
        let mut ratio = ((left - self.min_out) / range).max(0.0);
        if ratio.is_infinite() {
            ratio = 0.0;
        }
        self.output = (ratio * range + self.min_out).clamp(self.min_out, self.max_out);
    }

    /// `MinMaxPowerOut` (lines 436-457). The tolerance is 3 inside the optimal band (line 444),
    /// else 1 (line 438).
    fn min_max(&mut self) {
        let mut tol = 1.0;
        if self.turbine > self.opt_turbine.0
            && self.turbine < self.opt_turbine.1
            && self.temp > self.opt_temp.0
            && self.temp < self.opt_temp.1
        {
            tol = 3.0;
        }
        let max = self.max_output;
        let tf = (self.temp / 50.0).min(1.0);
        self.min_out = max * ((self.turbine - tol) / 100.0).min(tf).clamp(0.0, 1.0);
        self.max_out = max * ((self.turbine + tol) / 100.0).min(tf);
    }

    /// `UpdateFailures` (lines 545-586). Above the allowed band the meltdown timer runs; above the
    /// optimal band the fire timer runs. The condition factor is taken as full (dt per second).
    fn update_failures(&mut self, dt: f64) {
        if self.temp > self.allowed_temp.1 {
            self.melt_timer += dt;
            if self.melt_timer > self.meltdown_delay {
                // MeltDown (line 556): the reactor is gone for this port.
                self.melted = true;
                return;
            }
        } else {
            self.melt_timer = (self.melt_timer - dt).max(0.0);
        }
        if self.temp > self.opt_temp.1 {
            self.fire_timer += dt;
            if self.fire_timer >= self.fire_delay {
                self.fires += 1;
                self.fire_timer = 0.0;
            }
        } else {
            self.fire_timer = (self.fire_timer - dt).max(0.0);
        }
    }

    /// `UpdateAutoTemp(speed, deltaTime)` (lines 588-614). The grid's last output stands for
    /// `-currPowerConsumption` (line 601).
    fn update_auto_temp(&mut self, speed: f64, dt: f64, load: f64) {
        // Lines 590-592: the turbine target to the middle of the optimal band.
        let desired_turbine = (self.opt_turbine.0 + self.opt_turbine.1) / 2.0;
        self.target_turbine += (desired_turbine - self.target_turbine).clamp(-speed, speed) * dt;
        self.target_turbine = self.target_turbine.clamp(0.0, 100.0);

        // Lines 594-595: the fission target to the middle of the optimal fission band.
        let desired_fission = (self.opt_fission.0 + self.opt_fission.1) / 2.0;
        self.target_fission += (desired_fission - self.target_fission).clamp(-speed, speed) * dt;

        // Lines 597-603: above the middle of the optimal temperature band, fission down; else,
        // with the output short of the load, fission up.
        if self.temp > (self.opt_temp.0 + self.opt_temp.1) / 2.0 {
            self.target_fission =
                (self.target_fission - speed * 2.0 * dt).min(self.allowed_fission.1);
        } else if self.output < load {
            self.target_fission = (self.target_fission + speed * 2.0 * dt).min(100.0);
        }
        // Lines 605-610: 0..100, and at most 5 away from the actual fission.
        self.target_fission = self.target_fission.clamp(0.0, 100.0);
        self.target_fission = self
            .target_fission
            .clamp(self.fission - 5.0, self.fission + 5.0);
    }
}
