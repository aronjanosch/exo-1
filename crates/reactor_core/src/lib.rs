//! reactor_core: the engine room of the ship (#297, part of #174 and #248), without engine types,
//! in f64. The engineer keeps the reactor in its band by hand or leaves it on Auto; a coolant pump
//! takes heat away, a junction box feeds the consumers, and faults can be triggered for testing.
//!
//! Taken from Barotrauma's reactor: fission and turbine targets, temperature that follows heat
//! minus turbine at a limited rate, fuel rods that burn down, output held between a minimum and a
//! maximum set by turbine and temperature, an optimal and an allowed band widened by the operator's
//! skill, its automatic controller, and two overheat timers (fire, meltdown) that run faster at low
//! condition. Taken from Barotrauma's grid: a minimum supply ratio per consumer. Taken from PULSAR:
//! a limit slider per consumer group. Our own code: the safety shutdown, the restart lockout, the
//! alarm hysteresis, the coolant offset, the junction box overvoltage and sparks, and the faults
//! (`research/barotrauma-mech-power.md`, `pulsar-mech-conduits.md` in the concept repo).
//!
//! - `reactor`: `Reactor` with `Mode::Auto` and `Mode::Manual`, `FuelRod`, `ReactorEvent`
//!   (fire, shutdown, meltdown, alarms) for the glue to route.
//! - `grid`: `Grid` with three consumer groups, a limit, priority and minimum ratio per group.
//! - `engine_room`: `EngineRoom` ties reactor, coolant pump, junction box and grid together:
//!   `step` returns `RoomEvent`s, `trigger` runs a `Fault`, `level` gives the `AlarmLevel`.
//!
//! - `tuning`: `Tuning`, every value above from `content/gameplay/reactor/reactor.json` (strict
//!   loader, like `health_core`).
//!
//! Temperature, fission and turbine are 0..100 as on the panel; power is kW. All values are
//! starting points for the playtest (`TODO(initiator)`).
pub mod engine_room;
pub mod grid;
pub mod reactor;
pub mod tuning;

pub use engine_room::{AlarmLevel, EngineRoom, Fault, Input, JunctionBox, Pump, RoomEvent, RoomTuning, Source};
pub use grid::{Grid, Group, GroupState, Supply};
pub use reactor::{AlarmKind, Band, Bands, FuelRod, Mode, Reactor, ReactorEvent, ReactorTuning};
pub use tuning::{Tuning, REACTOR_PATH};
