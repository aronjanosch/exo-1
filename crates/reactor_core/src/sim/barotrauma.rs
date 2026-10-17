//! Test-only oracle: a straight f64 port of Barotrauma's single-reactor update, for side-by-side
//! checks of `reactor::Reactor`. Source: Barotrauma's `Reactor.cs` (`BarotraumaShared/SharedSource/
//! Items/Components/Machines/`). Each ported piece names its line numbers in that file.
//!
//! Left out, on purpose:
//! - the round start (`RoundDuration < 5`: rapid auto-heat), the signal and network parts, the
//!   fill items and their refuelling, the AI user, and the
//!   item's own condition (fire and implosion timers run at full condition, `dt` per second);
//! - the fill burn: Barotrauma takes the canisters' condition out of `AvailableFuel`; our canisters carry a
//!   fixed heat until they are spent, so `fill` is a constant input here and the burn is not
//!   ported (it cannot change heat, injection or converter in these tests);
//! - the multi-reactor branch of `GetConnectionPowerOut` (one reactor: its output is the load,
//!   clamped to the range).
//! Barotrauma works in f32, this is f64: agreement is to about 1e-6, not bit for bit.
//!
//! Its power-off step (targets to zero) comes after the lerps, as in Barotrauma; `reactor::Reactor`
//! zeroes the targets before the lerps. The difference shows only with power off.

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

#[derive(Clone, Debug)]
pub struct Baro {
    /// `MaxPowerOutput` (`GetMaxOutput`, line 890).
    max_output: f64,
    /// `FireDelay` (line 113 default 30) and `MeltdownDelay` (line 105 default 120).
    fire_delay: f64,
    implosion_delay: f64,
    /// `AvailableFuel`, held constant.
    fill: f64,
    /// `degreeOfSuccess` (line 41), 0..1: the operator's skill.
    degree: f64,
    /// `PowerOn` (line 54).
    pub power_on: bool,
    /// `AutoTemp` (line 27).
    pub auto_heat: bool,
    /// `fissionRate` (line 17), `turbineOutput` (line 20), `temperature` (line 22).
    pub injection: f64,
    pub converter: f64,
    pub heat: f64,
    /// `temperatureBoost` (Reactor.cs line 14 for the amount, 52 for the allow): set by the test,
    /// as the client's buttons do; faded in `step` (line 327).
    pub boost: f64,
    /// `TargetFissionRate` (line 192), `TargetTurbineOutput` (line 195), `CorrectTurbineOutput`
    /// (line 198).
    pub target_injection: f64,
    pub target_converter: f64,
    correct_converter: f64,
    /// `optimalTurbineOutput`, `optimalTemperature`, `optimalFissionRate`, `allowedFissionRate`,
    /// `allowedTemperature` (lines 43-45): (X, Y) pairs, set each step.
    opt_converter: (f64, f64),
    opt_heat: (f64, f64),
    opt_injection: (f64, f64),
    allowed_injection: (f64, f64),
    allowed_heat: (f64, f64),
    /// `minUpdatePowerOut` and `maxUpdatePowerOut` (lines 35-36), and the output the grid took
    /// (`-currPowerConsumption`, line 601 reads it).
    min_out: f64,
    max_out: f64,
    pub output: f64,
    /// `meltDownTimer`, `fireTimer` (lines 31-32).
    pub implode_timer: f64,
    pub fire_timer: f64,
    /// `ImplodeDown()` ran.
    pub imploded: bool,
    /// Fires started (`new FireSource`, line 577).
    pub fires: u32,
}

impl Baro {
    pub fn new(
        max_output: f64,
        fire_delay: f64,
        implosion_delay: f64,
        fill: f64,
        degree: f64,
    ) -> Baro {
        Baro {
            max_output,
            fire_delay,
            implosion_delay,
            fill,
            degree,
            power_on: false,
            auto_heat: false,
            injection: 0.0,
            converter: 0.0,
            heat: 0.0,
            boost: 0.0,
            target_injection: 0.0,
            target_converter: 0.0,
            correct_converter: 0.0,
            opt_converter: (0.0, 0.0),
            opt_heat: (0.0, 0.0),
            opt_injection: (0.0, 0.0),
            allowed_injection: (0.0, 0.0),
            allowed_heat: (0.0, 0.0),
            min_out: 0.0,
            max_out: 0.0,
            output: 0.0,
            implode_timer: 0.0,
            fire_timer: 0.0,
            imploded: false,
            fires: 0,
        }
    }

    /// One `Update(deltaTime)` with the grid asking for `load` kW (`Update`, lines 218-399), then
    /// the power grid's `GetConnectionPowerOut` for this reactor.
    pub fn step(&mut self, dt: f64, load: f64) {
        if self.imploded {
            return;
        }
        // Line 298: the fill of the last step (constant here).
        let prev_fill = self.fill;

        // Lines 303-306: the smoothed correct converter output, 20 per second at most.
        if self.max_output.abs() > 1e-9 {
            self.correct_converter +=
                ((load / self.max_output * 100.0) - self.correct_converter).clamp(-20.0, 20.0) * dt;
        }

        // Lines 310-321: the bands, from the skill and the correct converter output. The injection
        // bands' X is capped at Y - 10 (lines 319-320 and 321-322).
        let d = self.degree;
        let tol = lerp(2.5, 10.0, d);
        self.opt_converter = (self.correct_converter - tol, self.correct_converter + tol);
        self.opt_heat = (lerp(40.0, 30.0, d), lerp(60.0, 70.0, d));
        self.allowed_heat = (lerp(30.0, 10.0, d), lerp(70.0, 90.0, d));
        let f = self.fill;
        let y = lerp(f - 20.0, f - 10.0, d);
        self.opt_injection = (lerp(30.0, 20.0, d).min(y - 10.0), y);
        let y = f;
        self.allowed_injection = (lerp(20.0, 10.0, d).min(y - 10.0), y);

        // Lines 323-326: heat (`GetGeneratedHeat`, lines 500-503, with the boost) minus converter,
        // moved towards at 10 degrees per second, the setter clamps to 0..100 (lines 120-128).
        let heat = self.injection * (prev_fill / 100.0) * 2.0 + self.boost;
        let diff = (heat - self.converter) - self.heat;
        self.heat =
            (self.heat + (sign(diff) * 10.0 * dt).clamp(-diff.abs(), diff.abs())).clamp(0.0, 100.0);
        // Line 327: the boost fades to 0 at 1 per second, never past 0 (adjustValueWithoutOverShooting,
        // line 293-296).
        self.boost = if self.boost > 0.0 {
            (self.boost - dt).max(0.0)
        } else {
            (self.boost + dt).min(0.0)
        };

        // Lines 332-334: injection and converter follow their targets (setters clamp, lines 131-150).
        self.injection = lerp(self.injection, self.target_injection.min(prev_fill), dt).clamp(0.0, 100.0);
        self.converter = lerp(self.converter, self.target_converter, dt).clamp(0.0, 100.0);

        // Lines 338-345: off, the targets are zero; on with auto heat, the controller (speed 2).
        if !self.power_on {
            self.target_injection = 0.0;
            self.target_converter = 0.0;
        } else if self.auto_heat {
            self.update_auto_heat(2.0, dt, load);
        }

        // Line 395: UpdateFailures (the fire and the implosion timers).
        self.update_failures(dt);
        if self.imploded {
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
        if self.converter > self.opt_converter.0
            && self.converter < self.opt_converter.1
            && self.heat > self.opt_heat.0
            && self.heat < self.opt_heat.1
        {
            tol = 3.0;
        }
        let max = self.max_output;
        let tf = (self.heat / 50.0).min(1.0);
        self.min_out = max * ((self.converter - tol) / 100.0).min(tf).clamp(0.0, 1.0);
        self.max_out = max * ((self.converter + tol) / 100.0).min(tf);
    }

    /// `UpdateFailures` (lines 545-586). Above the allowed band the implosion timer runs; above the
    /// optimal band the fire timer runs. The condition factor is taken as full (dt per second).
    fn update_failures(&mut self, dt: f64) {
        if self.heat > self.allowed_heat.1 {
            self.implode_timer += dt;
            if self.implode_timer > self.implosion_delay {
                // ImplodeDown (line 556): the reactor is gone for this port.
                self.imploded = true;
                return;
            }
        } else {
            self.implode_timer = (self.implode_timer - dt).max(0.0);
        }
        if self.heat > self.opt_heat.1 {
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
    fn update_auto_heat(&mut self, speed: f64, dt: f64, load: f64) {
        // Lines 590-592: the converter target to the middle of the optimal band.
        let desired_converter = (self.opt_converter.0 + self.opt_converter.1) / 2.0;
        self.target_converter += (desired_converter - self.target_converter).clamp(-speed, speed) * dt;
        self.target_converter = self.target_converter.clamp(0.0, 100.0);

        // Lines 594-595: the injection target to the middle of the optimal injection band.
        let desired_injection = (self.opt_injection.0 + self.opt_injection.1) / 2.0;
        self.target_injection += (desired_injection - self.target_injection).clamp(-speed, speed) * dt;

        // Lines 597-603: above the middle of the optimal heat band, injection down; else,
        // with the output short of the load, injection up.
        if self.heat > (self.opt_heat.0 + self.opt_heat.1) / 2.0 {
            self.target_injection =
                (self.target_injection - speed * 2.0 * dt).min(self.allowed_injection.1);
        } else if self.output < load {
            self.target_injection = (self.target_injection + speed * 2.0 * dt).min(100.0);
        }
        // Lines 605-610: 0..100, and at most 5 away from the actual injection.
        self.target_injection = self.target_injection.clamp(0.0, 100.0);
        self.target_injection = self
            .target_injection
            .clamp(self.injection - 5.0, self.injection + 5.0);
    }
}
