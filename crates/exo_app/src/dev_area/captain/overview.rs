//! The captain's overview display: crew roster, alert level, the order wheel, special orders,
//! the target ship and the budget per role (PULSAR's overview tab, in our widgets).
//!
//! Special orders have no text field (the widget set has none): each slot cycles through three
//! canned phrases. TODO(initiator): a text input when the widgets get one.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crew_core::orders::{MAX_CUSTOM, OrderKind};
use crew_core::{Addressee, AlertLevel, DeviceId, OrderEvent, OrderPriority, StationId};
use display_core::{button, column, label, row, space, title, toggle, Panel, Tone};

pub const ID: PanelId = PanelId::new("captain_overview");
pub const NAME: &str = "Captain's overview";

/// Placeholder crew next to the player: name, role, kind, HP. Invented names.
pub const CREW: [(&str, Role, &str, u32); 3] = [("Pim Noodle", Role::Pilot, "bot", 100), ("Ola Sprocket", Role::Engineer, "bot", 87), ("Dax Marmalade", Role::Tactical, "bot", 100)];
/// Stations and devices an order can point at (rooms come from the ship). TODO(initiator): from the ship data.
/// The engineer corner's are real (`live_status`), the rest placeholders; `turret_port` is the bot demo's turret.
pub const STATIONS: [&str; 5] = ["helm", "turret_port", live_status::REACTOR_CONSOLE, live_status::POWER_CONSOLE, "scanner"];
pub const DEVICES: [&str; 6] = [crate::dev_area::engineer::REACTOR, crate::dev_area::engineer::PUMP, crate::dev_area::engineer::EXTINGUISHER, crate::dev_area::engineer::DOOR, "oxygen_generator", "shield_emitter"];
/// Alarm rows shown at most.
pub const ALARM_ROWS: usize = 3;
/// Ships the captain can name as the target. Invented.
pub const SHIPS: [&str; 3] = ["none", "Sticky Wicket (melon freighter)", "Rusty Kettle (scrapper)"];
/// Three canned phrases per special order slot.
pub const PHRASES: [[&str; 3]; MAX_CUSTOM] = [
    ["Everyone to the snack bar", "Nobody touch the big red button", "Look busy, inspection"],
    ["Turrets first, questions later", "Hold fire until they wave", "Aim for the cup holders"],
    ["Seal the hold, the cheese is loose", "Vent nothing, I mean it", "Mop the reactor room"],
    ["Quiet on the bridge", "Sing the docking song", "Pretend we are a cargo of socks"],
];
/// Order addressees in the "to" row.
pub const TO: [Addressee; 5] = [Addressee::Crew, Addressee::Slot(Role::Pilot), Addressee::Slot(Role::Engineer), Addressee::Slot(Role::Tactical), Addressee::Slot(Role::Scientist)];
/// Roles with an allowance.
pub const BUDGET_ROLES: [Role; 4] = [Role::Pilot, Role::Engineer, Role::Tactical, Role::Scientist];

/// What the captain has picked on the order part.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct OverviewUi {
    pub to: Addressee,
    pub kind: OrderKind,
    pub target: usize,
    pub priority: OrderPriority,
    pub phrase: [Option<usize>; MAX_CUSTOM],
    pub ship: usize,
    pub auto_target: bool,
}

impl Default for OverviewUi {
    fn default() -> Self {
        OverviewUi { to: Addressee::Crew, kind: OrderKind::GoTo, target: 0, priority: OrderPriority::Normal, phrase: [None; MAX_CUSTOM], ship: 0, auto_target: true }
    }
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(console(ID, NAME, -21.0, [0.25, 0.32, 0.55]));
    app.init_resource::<OverviewUi>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}

/// The targets an order kind can point at.
pub fn targets(kind: OrderKind, ship: &crew_core::ShipStatus) -> Vec<Target> {
    use crew_core::TargetKind;
    match kind.target_kind() {
        None => Vec::new(),
        Some(TargetKind::Room) => ship.rooms.iter().map(|r| Target::Room(r.id.clone())).collect(),
        Some(TargetKind::Station) => STATIONS.iter().map(|s| Target::Station(StationId::new(*s))).collect(),
        Some(TargetKind::Device) => DEVICES.iter().map(|d| Target::Device(DeviceId::new(*d))).collect(),
    }
}

fn chosen_target(ui: &OverviewUi, ship: &crew_core::ShipStatus) -> Option<Target> {
    let ts = targets(ui.kind, ship);
    (!ts.is_empty()).then(|| ts[ui.target % ts.len()].clone())
}

fn to_name(a: Addressee) -> &'static str {
    match a {
        Addressee::Crew => "CREW",
        Addressee::Slot(r) => match r {
            Role::Captain => "CAPTAIN",
            Role::Pilot => "PILOT",
            Role::Engineer => "ENGINEER",
            Role::Tactical => "TACTICAL",
            Role::Scientist => "SCIENCE",
        },
    }
}

/// The short names of the "to" row (five buttons in half a panel).
fn to_short(a: Addressee) -> &'static str {
    match a {
        Addressee::Crew => "CREW",
        Addressee::Slot(Role::Pilot) => "PIL",
        Addressee::Slot(Role::Engineer) => "ENG",
        Addressee::Slot(Role::Tactical) => "TAC",
        Addressee::Slot(Role::Scientist) => "SCI",
        Addressee::Slot(Role::Captain) => "CPT",
    }
}

fn kind_name(k: OrderKind) -> &'static str {
    match k {
        OrderKind::GoTo => "GO TO",
        OrderKind::Operate => "OPERATE",
        OrderKind::Repair => "REPAIR",
        OrderKind::Extinguish => "FIRE",
        OrderKind::FixBreach => "BREACH",
        OrderKind::FollowMe => "FOLLOW ME",
        OrderKind::Stay => "STAY",
        OrderKind::Report => "REPORT",
        OrderKind::Custom(_) => "SPECIAL",
    }
}

pub fn alert_name(a: AlertLevel) -> &'static str {
    match a {
        AlertLevel::Green => "GREEN",
        AlertLevel::Yellow => "YELLOW",
        AlertLevel::Red => "RED",
    }
}

/// The overview from the state (1280 x 760 panel pixels).
pub fn panel(gp: &Gameplay, crew: &CrewState, ui: &OverviewUi, role: Role, ship: &crew_core::ShipStatus, alarms: &[live_status::AlarmRow]) -> Panel {
    let now = gp.clock();
    let wallet = gp.progress.wallet();
    let line = |cells: Vec<String>| row(8.0, cells.into_iter().enumerate().map(|(i, c)| label(c).flex(if i == 0 { 2.0 } else { 1.0 })).collect()).fixed(22.0);

    // Left: crew, alert, target ship, budget.
    let mut left = vec![title("CAPTAIN  OVERVIEW").fixed(40.0), label("CREW").tone(Tone::Dim).fixed(22.0), line(vec!["You".into(), format!("{role:?}"), "player".into(), "HP 100".into()])];
    for (name, r, kind, hp) in CREW {
        left.push(line(vec![name.into(), format!("{r:?}"), kind.into(), format!("HP {hp}")]));
    }
    left.push(line(vec!["(empty slot)".into(), "Scientist".into(), "-".into(), "-".into()]));
    left.push(button(format!("DEV: PLAY AS {}", to_name(Addressee::Slot(role))), "dev_role").fixed(30.0));
    let level = crew.alert.level();
    let since = crew.alert.log().last().map(|c| format!("{} since {:.0} s ({:?})", alert_name(c.to), now - c.at_s, c.cause)).unwrap_or_else(|| "GREEN since the start".into());
    left.push(label("ALERT").tone(Tone::Dim).fixed(22.0));
    left.push(row(8.0, AlertLevel::ALL.iter().map(|a| button(pick(alert_name(*a), *a == level), format!("alert:{}", a.id()))).collect()).fixed(38.0));
    left.push(label(since).tone(match level { AlertLevel::Green => Tone::Good, AlertLevel::Yellow => Tone::Warn, AlertLevel::Red => Tone::Bad }).fixed(24.0));
    left.push(label(format!("ALARMS  {}", if alarms.is_empty() { "none" } else { "" })).tone(if alarms.is_empty() { Tone::Dim } else { Tone::Bad }).fixed(22.0));
    for (i, a) in alarms.iter().take(ALARM_ROWS).enumerate() {
        let room = a.room.replace('_', " ");
        left.push(row(6.0, vec![label(format!("{}  {room}", a.text)).tone(Tone::Bad).flex(3.0), button("ORDER", format!("alarm:{i}")).enabled(a.order.is_some()).flex(0.9)]).fixed(30.0));
    }
    left.push(row(8.0, vec![label(format!("TARGET  {}", SHIPS[ui.ship])).flex(2.2), button("NEXT", "ship_next").flex(0.8), toggle(ui.auto_target, "AUTO", "auto_target").flex(1.0)]).fixed(34.0));
    let pool = crew.budget.pool(wallet);
    left.push(label(format!("BUDGET  total {}   available {}", pool.total, pool.available)).fixed(26.0));
    for r in BUDGET_ROLES {
        left.push(row(8.0, vec![label(format!("{r:?}  {}", crew.budget.remaining(r))).flex(2.0), button("-100", format!("wd:{}", r.id())).enabled(crew.budget.remaining(r) > 0), button("+100", format!("rel:{}", r.id())).enabled(pool.available >= crew_core::budget::STEP)]).fixed(30.0));
    }
    left.push(toggle(crew.budget.limits(), "PURCHASE LIMITS", "limits").fixed(30.0));
    left.push(space());

    // Right: orders.
    let mut right = vec![title("ORDERS").fixed(40.0)];
    right.push(row(6.0, TO.iter().enumerate().map(|(i, a)| button(pick(to_short(*a), *a == ui.to), format!("to:{i}"))).collect()).fixed(40.0));
    for half in OrderKind::WHEEL.chunks(4) {
        right.push(row(6.0, half.iter().map(|k| button(pick(kind_name(*k), *k == ui.kind), format!("kind:{}", k.id()))).collect()).fixed(40.0));
    }
    let target = chosen_target(ui, ship).map(|t| target_name(&t)).unwrap_or_else(|| "(no target)".into());
    let has_targets = ui.kind.target_kind().is_some();
    right.push(row(6.0, vec![button("<", "tgt_prev").enabled(has_targets).flex(0.5), label(format!("TARGET  {target}")).flex(3.0), button(">", "tgt_next").enabled(has_targets).flex(0.5)]).fixed(40.0));
    right.push(row(6.0, [OrderPriority::Low, OrderPriority::Normal, OrderPriority::Urgent].iter().map(|p| button(pick(&format!("{p:?}").to_uppercase(), *p == ui.priority), format!("pri:{p:?}"))).collect()).fixed(40.0));
    right.push(row(6.0, vec![button(format!("GIVE TO {}", to_name(ui.to)), "give"), button("CLEAR", "clear")]).fixed(44.0));
    right.push(label("IN FORCE").tone(Tone::Dim).fixed(22.0));
    for a in TO {
        let o = match a {
            Addressee::Crew => crew.orders.crew(),
            Addressee::Slot(r) => crew.orders.for_role(r).filter(|o| o.to == a),
        };
        let text = o.map(|o| order_text(gp, &crew.orders, o)).unwrap_or_else(|| "-".into());
        right.push(label(format!("{}: {text}", to_name(a))).tone(if o.is_some() { Tone::Normal } else { Tone::Dim }).fixed(22.0));
    }
    right.push(label("SPECIAL ORDERS").tone(Tone::Dim).fixed(22.0));
    for (i, slot) in crew.orders.custom().iter().enumerate() {
        right.push(row(6.0, vec![label(format!("{}: {}", i + 1, slot.as_deref().unwrap_or("(empty)"))).flex(3.0), button("PHRASE", format!("phrase:{i}")).flex(0.8), button("GIVE", format!("special:{i}")).flex(0.7)]).fixed(34.0));
    }
    right.push(space());

    Panel::new(PANEL_PX[0], PANEL_PX[1], row(28.0, vec![column(4.0, left).flex(1.0), column(6.0, right).flex(1.15)]))
}

fn result_of(gp: &mut Gameplay, crew: &CrewState, r: Result<OrderEvent, Refusal>) {
    match r {
        Ok(OrderEvent::Given { order, .. }) => order_notice(gp, &crew.orders, &order),
        Ok(_) => {}
        Err(e) => refuse(gp, &e),
    }
}

/// The overview's clicks.
#[allow(clippy::too_many_arguments)]
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut ui: ResMut<OverviewUi>, mut crew: ResMut<CrewState>, mut gp: ResMut<Gameplay>, mut role: ResMut<DevRole>, live: Res<live_status::LiveShip>, eng: Option<Res<crate::dev_area::engineer::Engineering>>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        let ship = live.get(&crew);
        let (by, now, wallet) = (role.0, gp.clock(), gp.progress.wallet());
        let crew = &mut *crew;
        let a = c.action.as_str();
        let (head, arg) = a.split_once(':').unwrap_or((a, ""));
        let role_of = |s: &str| Role::ALL.into_iter().find(|r| r.id() == s);
        match head {
            "dev_role" => role.0 = Role::ALL[(Role::ALL.iter().position(|r| *r == role.0).unwrap() + 1) % Role::ALL.len()],
            "alert" => match AlertLevel::ALL.into_iter().find(|l| l.id() == arg).map(|l| crew.alert.set(by, l, now)) {
                Some(Ok(Some(e))) => alert_notice(&mut gp, e),
                Some(Err(r)) => refuse(&mut gp, &r),
                _ => {}
            },
            "alarm" => {
                // Pre-fill the order wheel with the alarm's order and its real target.
                let alarms = eng.as_deref().map(live_status::alarms).unwrap_or_default();
                if let Some((kind, target)) = arg.parse::<usize>().ok().and_then(|i| alarms.get(i)).and_then(|a| a.order.clone()) {
                    ui.kind = kind;
                    ui.target = targets(kind, &ship).iter().position(|t| *t == target).unwrap_or(0);
                }
            }
            "ship_next" => ui.ship = (ui.ship + 1) % SHIPS.len(),
            "auto_target" => ui.auto_target = c.value.is_some_and(|v| v > 0.5),
            "rel" | "wd" => {
                let Some(r) = role_of(arg) else { continue };
                let res = if head == "rel" { crew.budget.release(by, r, wallet) } else { crew.budget.withdraw(by, r) };
                if let Err(e) = res {
                    refuse(&mut gp, &e);
                }
            }
            "limits" => {
                if let Err(e) = crew.budget.set_limits(by, c.value.is_some_and(|v| v > 0.5)) {
                    refuse(&mut gp, &e);
                }
            }
            "to" => ui.to = arg.parse::<usize>().ok().and_then(|i| TO.get(i).copied()).unwrap_or(ui.to),
            "kind" => {
                if let Some(k) = OrderKind::WHEEL.into_iter().find(|k| k.id() == arg) {
                    ui.kind = k;
                    ui.target = 0;
                }
            }
            "tgt_prev" | "tgt_next" => {
                let n = targets(ui.kind, &ship).len().max(1);
                ui.target = if head == "tgt_next" { (ui.target + 1) % n } else { (ui.target + n - 1) % n };
            }
            "pri" => ui.priority = [OrderPriority::Low, OrderPriority::Normal, OrderPriority::Urgent].into_iter().find(|p| format!("{p:?}") == arg).unwrap_or(ui.priority),
            "give" => {
                let t = chosen_target(&ui, &ship);
                let r = crew.orders.give(by, ui.to, ui.kind, t, ui.priority, now, None);
                result_of(&mut gp, crew, r);
            }
            "clear" => match crew.orders.clear(by, ui.to) {
                Ok(Some(_)) => say(&mut gp, Notice::new(NoticeKind::Updated, "notice.captain.order_cleared")),
                Ok(None) => {}
                Err(e) => refuse(&mut gp, &e),
            },
            "phrase" => {
                let Ok(i) = arg.parse::<usize>() else { continue };
                if i >= MAX_CUSTOM {
                    continue;
                }
                let next = ui.phrase[i].map_or(0, |p| (p + 1) % 3);
                match crew.orders.set_custom(by, i, PHRASES[i][next]) {
                    Ok(()) => ui.phrase[i] = Some(next),
                    Err(e) => refuse(&mut gp, &e),
                }
            }
            "special" => {
                let Ok(i) = arg.parse::<u8>() else { continue };
                let r = crew.orders.give(by, ui.to, OrderKind::Custom(i), None, ui.priority, now, None);
                result_of(&mut gp, crew, r);
            }
            _ => warn!("captain overview: unknown action {c:?}"),
        }
    }
}

pub fn write_panel(gp: Res<Gameplay>, crew: Res<CrewState>, ui: Res<OverviewUi>, role: Res<DevRole>, live: Res<live_status::LiveShip>, eng: Option<Res<crate::dev_area::engineer::Engineering>>, mut panels: ResMut<Panels>) {
    let alarms = eng.as_deref().map(live_status::alarms).unwrap_or_default();
    panels.set(ID, panel(&gp, &crew, &ui, role.0, &live.get(&crew), &alarms));
}

