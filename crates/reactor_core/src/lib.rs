//! reactor_core: the ship's reactor and the power grid behind it, without engine types, in f64
//! (#286, part of #174 and #248). The engineer keeps it in its band by hand or leaves it on Auto.
//!
//! Taken from Barotrauma's reactor: fission and turbine targets, temperature that follows heat
//! minus turbine at a limited rate, fuel rods that burn down, output held between a minimum and a
//! maximum set by turbine and temperature, an optimal and an allowed band widened by the operator's
//! skill, its automatic controller, and two overheat timers (fire, meltdown) that run faster at low
//! condition. Taken from Barotrauma's grid: a minimum supply ratio per consumer. Taken from PULSAR:
//! a limit slider per consumer group. Our own code; the safety shutdown, the restart lockout, the
//! alarm hysteresis and the priority shedding are ours (`research/barotrauma-mech-power.md`,
//! `pulsar-mech-conduits.md` in the concept repo).
//!
//! - `reactor`: `Reactor` with `Mode::Auto` and `Mode::Manual`, `FuelRod`, `ReactorEvent`
//!   (fire, shutdown, meltdown, alarms) for the glue to route.
//! - `grid`: `Grid` with five consumer groups, a limit, priority and minimum ratio per group, and
//!   `run`, which steps reactor and grid together.
//!
//! Temperature, fission and turbine are 0..100 as on the panel; power is kW. All values are
//! starting points for the playtest (`TODO(initiator)`).
pub mod grid;
pub mod reactor;

pub use grid::{Grid, Group, GroupState, Supply};
pub use reactor::{AlarmKind, Band, Bands, FuelRod, Mode, Reactor, ReactorEvent, ReactorTuning};
