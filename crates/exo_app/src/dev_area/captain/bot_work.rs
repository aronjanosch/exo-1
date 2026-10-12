//! The demo bot's real work in the engineer corner (#288, phase 3): the corner's fires, its broken
//! pump and reactor, and the reactor console after a scram become tasks for `crew_core::choose`,
//! next to the demo's two placeholder tasks. At its task the bot works through the same APIs a
//! player's hands use:
//!
//! - a fire: it sprays at the fire through `Deck::spray` (the player's spray point rule). It
//!   carries its own extinguisher charge (`BOT_SPRAY_S`, refills by itself while not spraying)
//!   instead of taking the one from the wall mount: simpler, and the player's stays on the wall.
//! - the pump or the reactor: `Condition::repair_step` at the bot's skill with a wrench, so the
//!   time and the failure chance follow the skill like a player's.
//! - the reactor console after a scram: once the lockout is over, power on and Auto.
//!
//! A task a player is using right now (repairing that device, spraying, at the reactor panel) is
//! claimed by the player and the bot leaves it alone.
use super::*;
use crate::dev_area::engineer::{self, spray_point, Dev, Engineering};
use crate::gameplay::HOST;
use bevy::math::DVec3;
use crew_core::priorities::TaskKind;
use crew_core::{Claims, DeviceId, StationId, Task, TaskId};
use device_core::{RepairOutcome, Tool};
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use reactor_core::Mode;

pub const PUMP_TASK: TaskId = TaskId(10);
pub const REACTOR_TASK: TaskId = TaskId(11);
pub const CONSOLE_TASK: TaskId = TaskId(12);
/// Fire tasks: this plus the fire's id.
pub const FIRE_TASK_BASE: u64 = 100;
/// Seconds of spray in the bot's own extinguisher, and to refill it. TODO(initiator).
pub const BOT_SPRAY_S: f64 = 30.0;
pub const BOT_REFILL_S: f64 = 15.0;
/// The bot runs at this speed (m/s) to work scored above `RUN_ABOVE` (Barotrauma's run threshold).
pub const RUN_SPEED: f64 = 5.0;
pub const RUN_ABOVE: f64 = 50.0;
/// The bot stops this far from a fire's edge (m), inside the spray's reach.
pub const FIRE_STAND_M: f64 = 1.5;

/// What the bot has done with its hands.
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    /// 0..1 of its extinguisher.
    pub charge: f64,
    pub sprayed_s: f64,
    pub repairs_done: u32,
    pub repair_fails: u32,
    /// Seconds spent repairing.
    pub repair_s: f64,
    pub restarts: u32,
    /// The device under its hands.
    pub repairing: Option<Dev>,
    /// The last thing it did at a task, for the panel.
    pub doing: Option<String>,
}

impl Default for Work {
    fn default() -> Self {
        Work { charge: 1.0, sprayed_s: 0.0, repairs_done: 0, repair_fails: 0, repair_s: 0.0, restarts: 0, repairing: None, doing: None }
    }
}

fn front(local: [f64; 2], depth: f64) -> [f64; 3] {
    let p = crate::dev_area::ENGINEERING.point([local[0], local[1] - depth * 0.5 - 0.7]);
    [p[0], p[1], 0.0]
}

/// The engineer corner's open work.
pub fn real_tasks(e: &Engineering) -> Vec<Task> {
    let mut out: Vec<Task> = e
        .deck
        .fires()
        .iter()
        .map(|f| {
            let room = if f.room == e.side_room { live_status::SIDE_ROOM } else { live_status::ENGINE_ROOM };
            Task { id: TaskId(FIRE_TASK_BASE + f.id.0), kind: TaskKind::ExtinguishFire, target: Target::Room(crew_core::RoomId::new(room)), pos: [f.pos.x, f.pos.z, 0.0], urgency: (f.size / e.deck.tuning().max_size).clamp(0.2, 1.0), need_skill: 0.0 }
        })
        .collect();
    for (d, id, task, at, depth) in [(Dev::Pump, engineer::PUMP, PUMP_TASK, engineer::PUMP_AT, engineer::PUMP_SIZE[2]), (Dev::Reactor, engineer::REACTOR, REACTOR_TASK, [engineer::REACTOR_AT[0] + 1.0, engineer::REACTOR_AT[1]], engineer::REACTOR_SIZE[2])] {
        // Open until full: an order (or the table) may send the bot to top up a worn device too.
        let c = e.condition(d);
        if c.fraction() < 1.0 {
            out.push(Task { id: task, kind: TaskKind::Repair, target: Target::Device(DeviceId::new(id)), pos: front(at, depth), urgency: 1.0 - c.fraction(), need_skill: 0.0 });
        }
    }
    let r = &e.reactor;
    if !r.power_on() && r.condition().functioning() && e.stats.shutdowns > 0 {
        out.push(Task { id: CONSOLE_TASK, kind: TaskKind::OperateEngine, target: Target::Station(StationId::new(live_status::REACTOR_CONSOLE)), pos: front(engineer::REACTOR_CONSOLE_AT, 0.8), urgency: 0.7, need_skill: 0.0 });
    }
    out
}

/// Where the bot stands for a task and how close counts as there (area metres).
pub fn stand(t: &Task, e: Option<&Engineering>) -> ([f64; 2], f64) {
    let at = [t.pos[0], t.pos[1]];
    if t.id.0 >= FIRE_TASK_BASE
        && let Some(f) = e.and_then(|e| e.deck.fire(fire_core::FireId(t.id.0 - FIRE_TASK_BASE)))
    {
        return (at, f.size * 0.5 + FIRE_STAND_M);
    }
    (at, if t.id.0 >= PUMP_TASK.0 { 0.5 } else { bots::ARRIVE_M })
}

/// The tasks a player is using right now are theirs; the others are free again.
pub fn player_claims(claims: &mut Claims, e: &Engineering, tasks: &[Task], reactor_panel_in_use: bool, spraying: bool) {
    for t in tasks.iter().filter(|t| t.id.0 >= PUMP_TASK.0) {
        let used = match t.id {
            PUMP_TASK => e.repairing == Some(Dev::Pump),
            REACTOR_TASK => e.repairing == Some(Dev::Reactor),
            CONSOLE_TASK => reactor_panel_in_use,
            _ => spraying,
        };
        if used {
            claims.player_takes(t.id, HOST);
        } else {
            claims.player_leaves(t.id, HOST);
        }
    }
}

/// The bot works at a real task it has reached, for `dt` seconds.
pub fn act(w: &mut Work, e: &mut Engineering, gp: &mut Gameplay, t: &Task, bot: [f64; 2], dt: f64) {
    let mut repaired = None;
    match t.kind {
        TaskKind::ExtinguishFire => {
            if w.charge > 0.0 {
                let feet = DVec3::new(bot[0], 0.0, bot[1]);
                let flat = DVec3::new(t.pos[0] - bot[0], 0.0, t.pos[1] - bot[1]).normalize_or_zero();
                let at = spray_point(&e.deck, feet, flat);
                e.deck.spray(at, dt);
                w.charge = (w.charge - dt / BOT_SPRAY_S).max(0.0);
                w.sprayed_s += dt;
                w.doing = Some("spraying".into());
            } else {
                w.doing = Some("extinguisher empty".into());
            }
        }
        TaskKind::Repair => {
            let d = if t.id == PUMP_TASK { Dev::Pump } else { Dev::Reactor };
            let c = match d {
                Dev::Pump => &mut e.pump,
                Dev::Reactor => e.reactor.condition_mut(),
            };
            let out = c.repair_step(dt, bots::BOT_SKILL, Tool::WRENCH);
            w.repair_s += dt;
            repaired = Some(d);
            w.doing = Some(format!("repairing the {} ({:.0} %)", d.name(), c.fraction() * 100.0));
            match out {
                RepairOutcome::Done { .. } => {
                    w.repairs_done += 1;
                    gp.log.push(format!("bot: repaired the {}", d.name()));
                    gp.notify(Notice::new(NoticeKind::Completed, "notice.engineer.repaired").arg("device", Arg::Text(d.name().into())));
                }
                RepairOutcome::Failed { lost } => {
                    w.repair_fails += 1;
                    gp.log.push(format!("bot: repair of the {} failed", d.name()));
                    gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.repair_failed").arg("device", Arg::Text(d.name().into())).arg("lost", Arg::Number(lost.round() as i64)));
                }
                _ => {}
            }
        }
        TaskKind::OperateEngine => {
            if e.reactor.lockout_s() > 0.0 {
                w.doing = Some(format!("waiting for the lockout ({:.0} s)", e.reactor.lockout_s().ceil()));
            } else if e.reactor.set_power(true) {
                e.reactor.set_mode(Mode::Auto);
                w.restarts += 1;
                w.doing = Some("restarted the reactor on Auto".into());
                gp.log.push("bot: restarted the reactor".into());
            }
        }
        _ => {}
    }
    if w.repairing != repaired {
        stop_repair(w, e);
        w.repairing = repaired;
    }
}

/// Lets go of the device under the bot's hands (the next step starts a new attempt).
pub fn stop_repair(w: &mut Work, e: &mut Engineering) {
    match w.repairing.take() {
        Some(Dev::Pump) => e.pump.stop_repair(),
        Some(Dev::Reactor) => e.reactor.condition_mut().stop_repair(),
        None => {}
    }
}
