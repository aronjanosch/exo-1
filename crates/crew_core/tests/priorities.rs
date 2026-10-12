//! Bot priorities (#288, #253): the table, the choice with orders and hysteresis, station claims,
//! JSON save and load.
use crew_core::priorities::{self, Override, When};
use crew_core::*;
use gameplay_core::ClientId;
use gameplay_core::rng::Rng;

const BOT: BotId = BotId(1);
const OTHER: BotId = BotId(2);

fn turret(id: u64, pos: [f64; 3]) -> Task {
    Task { id: TaskId(id), kind: TaskKind::OperateTurret, target: Target::Station(StationId::new("turret_port")), pos, urgency: 0.5, need_skill: 0.0 }
}

fn fire(id: u64, pos: [f64; 3], urgency: f64) -> Task {
    Task { id: TaskId(id), kind: TaskKind::ExtinguishFire, target: Target::Room(RoomId::new(format!("room_{id}"))), pos, urgency, need_skill: 0.0 }
}

fn bot(class: Role) -> BotView {
    BotView { id: BOT, class, pos: [0.0; 3], skill: 30.0, current: None }
}

fn calm(now_s: f64) -> Situation<'static> {
    Situation { alert: AlertLevel::Green, order: None, flags: &[], now_s }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < priorities::JITTER + 1e-9
}

/// A Tactical page with fire at Max and the turret at High.
fn fire_first() -> PriorityTable {
    let mut t = PriorityTable::default();
    t.set_level(Role::Captain, Role::Tactical, TaskKind::ExtinguishFire, Level::Max).unwrap();
    t.set_level(Role::Captain, Role::Tactical, TaskKind::OperateTurret, Level::High).unwrap();
    t
}

#[test]
fn the_table_decides_fire_at_max_over_turret_at_high() {
    let table = fire_first();
    let tasks = [turret(1, [0.0; 3]), fire(2, [20.0, 0.0, 0.0], 0.5)];
    let c = choose(&bot(Role::Tactical), &tasks, &Claims::default(), &table, &calm(0.0), &mut Rng::new(7)).unwrap();
    assert_eq!(c.pick, Pick::Task(TaskId(2)));
    assert_eq!(c.level, Some(Level::Max));
    // 80 (Max) + 10 × 0.5 urgency + 5 × distance factor (20 m of 40: 0.55).
    assert!(near(c.score, 87.75), "{}", c.score);
    // Without the fire, the turret: 60 (High) + 5 + 5 × 1.0.
    let c = choose(&bot(Role::Tactical), &tasks[..1], &Claims::default(), &table, &calm(0.0), &mut Rng::new(7)).unwrap();
    assert_eq!((c.pick, c.level), (Pick::Task(TaskId(1)), Some(Level::High)));
    assert!(near(c.score, 70.0), "{}", c.score);
}

#[test]
fn defaults_put_turrets_first_and_give_the_scientist_no_bot() {
    let table = PriorityTable::default();
    assert_eq!(table.page(Role::Tactical).unwrap().base(TaskKind::OperateTurret), Level::Max);
    assert!(table.page(Role::Scientist).is_none());
    assert!(table.page(Role::Captain).is_none());
    for class in [Role::Pilot, Role::Engineer, Role::Tactical] {
        assert_eq!(table.page(class).unwrap().base(TaskKind::Scan), Level::Ignore, "{class:?}");
    }
    let tasks = [turret(1, [10.0, 0.0, 0.0]), fire(2, [1.0, 0.0, 0.0], 1.0)];
    let c = choose(&bot(Role::Tactical), &tasks, &Claims::default(), &table, &calm(0.0), &mut Rng::new(1)).unwrap();
    assert_eq!(c.pick, Pick::Task(TaskId(1)));
    let scan = Task { id: TaskId(3), kind: TaskKind::Scan, target: Target::Station(StationId::new("scanner")), pos: [0.0; 3], urgency: 1.0, need_skill: 0.0 };
    assert_eq!(choose(&bot(Role::Scientist), &[scan], &Claims::default(), &table, &calm(0.0), &mut Rng::new(1)), None);
}

#[test]
fn a_captain_order_lifts_its_target_over_the_table() {
    let table = fire_first();
    let tasks = [turret(1, [30.0, 0.0, 0.0]), fire(2, [0.0; 3], 1.0)];
    let mut orders = Orders::default();
    orders.give(Role::Captain, Addressee::Slot(Role::Tactical), OrderKind::Operate, Some(tasks[0].target.clone()), OrderPriority::Normal, 0.0, None).unwrap();
    let sit = Situation { alert: AlertLevel::Red, order: orders.for_role(Role::Tactical), flags: &[], now_s: 0.0 };
    let c = choose(&bot(Role::Tactical), &tasks, &Claims::default(), &table, &sit, &mut Rng::new(3)).unwrap();
    assert_eq!(c.pick, Pick::Task(TaskId(1)));
    assert_eq!(c.level, None);
    // 100 (order) + 2 (normal) + 5 (urgency) + 5 × 0.325 (30 m).
    assert!(near(c.score, 108.625), "{}", c.score);

    // An order without a task: the bot does what it says.
    orders.give(Role::Captain, Addressee::Crew, OrderKind::FollowMe, None, OrderPriority::Urgent, 1.0, None).unwrap();
    let sit = Situation { alert: AlertLevel::Green, order: orders.for_role(Role::Engineer), flags: &[], now_s: 1.0 };
    let c = choose(&bot(Role::Engineer), &tasks, &Claims::default(), &table, &sit, &mut Rng::new(3)).unwrap();
    assert_eq!(c.pick, Pick::Order { kind: OrderKind::FollowMe, target: None });
    assert!(near(c.score, 104.0), "{}", c.score);
}

#[test]
fn overrides_follow_alert_orders_and_flags() {
    let mut table = PriorityTable::default();
    let page = table.page_mut(Role::Engineer).unwrap();
    page.overrides.push(Override { task: TaskKind::OperateTurret, when: When::Alert(AlertLevel::Red), inverted: false, level: Level::Max });
    page.overrides.push(Override { task: TaskKind::Repair, when: When::Order(OrderKind::Custom(0)), inverted: false, level: Level::Ignore });
    page.overrides.push(Override { task: TaskKind::FetchCargo, when: When::Custom("docked".into()), inverted: true, level: Level::Ignore });
    let page = table.page(Role::Engineer).unwrap();
    let base = page.base(TaskKind::OperateTurret);
    assert_eq!(page.level(TaskKind::OperateTurret, &calm(0.0)), base);
    let red = Situation { alert: AlertLevel::Red, ..calm(0.0) };
    assert_eq!(page.level(TaskKind::OperateTurret, &red), Level::Max);

    let mut orders = Orders::default();
    orders.set_custom(Role::Captain, 0, "Hands off the toolbox").unwrap();
    orders.give(Role::Captain, Addressee::Crew, OrderKind::Custom(0), None, OrderPriority::Normal, 0.0, None).unwrap();
    let ordered = Situation { order: orders.crew(), ..calm(0.0) };
    assert_eq!(page.level(TaskKind::Repair, &ordered), Level::Ignore);
    // Inverted: cargo is ignored unless the ship is docked.
    assert_eq!(page.level(TaskKind::FetchCargo, &calm(0.0)), Level::Ignore);
    let docked = ["docked".to_string()];
    assert_eq!(page.level(TaskKind::FetchCargo, &Situation { flags: &docked, ..calm(0.0) }), page.base(TaskKind::FetchCargo));
}

#[test]
fn a_bot_does_not_flip_between_tasks() {
    let mut table = PriorityTable::default();
    table.set_level(Role::Captain, Role::Engineer, TaskKind::ExtinguishFire, Level::Medium).unwrap();
    table.set_level(Role::Captain, Role::Engineer, TaskKind::OperateTurret, Level::High).unwrap();
    let claims = Claims::default();
    let mut b = bot(Role::Engineer);
    let mut rng = Rng::new(11);
    // Two fires whose urgency swaps every tick: the bot stays with the first one it picked.
    let mut first = None;
    for i in 0..20 {
        let now = i as f64 * 0.5;
        let (u1, u2) = if i % 2 == 0 { (0.8, 0.2) } else { (0.2, 0.8) };
        let tasks = [fire(1, [5.0, 0.0, 0.0], u1), fire(2, [5.0, 0.0, 0.0], u2)];
        let c = choose(&b, &tasks, &claims, &table, &calm(now), &mut rng).unwrap();
        let pick = first.get_or_insert(c.pick.clone()).clone();
        assert_eq!(c.pick, pick, "tick {i}");
        assert_eq!(c.since_s, 0.0);
        b.current = Some(c);
    }
    // A turret at High (one level more) waits for the hold window, then wins.
    let tasks = [fire(1, [5.0, 0.0, 0.0], 0.5), fire(2, [5.0, 0.0, 0.0], 0.5), turret(3, [5.0, 0.0, 0.0])];
    b.current.as_mut().unwrap().since_s = 10.0;
    let c = choose(&b, &tasks, &claims, &table, &calm(10.0 + priorities::HOLD_S - 0.1), &mut rng).unwrap();
    assert_ne!(c.pick, Pick::Task(TaskId(3)));
    let c = choose(&b, &tasks, &claims, &table, &calm(10.0 + priorities::HOLD_S), &mut rng).unwrap();
    assert_eq!(c.pick, Pick::Task(TaskId(3)));
    assert_eq!(c.since_s, 10.0 + priorities::HOLD_S);
}

#[test]
fn a_player_sitting_down_takes_the_station_from_the_bot() {
    let table = PriorityTable::default();
    let mut claims = Claims::default();
    let tasks = [turret(1, [0.0; 3]), fire(2, [3.0, 0.0, 0.0], 0.5)];
    let mut b = bot(Role::Tactical);
    let c = choose(&b, &tasks, &claims, &table, &calm(0.0), &mut Rng::new(5)).unwrap();
    assert_eq!(c.pick, Pick::Task(TaskId(1)));
    claims.commit(BOT, &c);
    assert_eq!(claims.taker(TaskId(1)), Some(Taker::Bot(BOT)));
    b.current = Some(c);

    // Another bot does not take a claimed task.
    let mut other = bot(Role::Tactical);
    other.id = OTHER;
    let c2 = choose(&other, &tasks, &claims, &table, &calm(0.0), &mut Rng::new(5)).unwrap();
    assert_eq!(c2.pick, Pick::Task(TaskId(2)));

    // A player sits down: the bot's claim goes, and the bot leaves at once (no hold).
    let evicted = claims.player_takes(TaskId(1), ClientId(9));
    assert_eq!(evicted, Some(BOT));
    assert_eq!(claims.taker(TaskId(1)), Some(Taker::Player(ClientId(9))));
    let c = choose(&b, &tasks, &claims, &table, &calm(0.5), &mut Rng::new(5)).unwrap();
    assert_eq!(c.pick, Pick::Task(TaskId(2)));
    assert!(!claims.claim(TaskId(1), BOT));

    // The player stands up: the station is free again.
    claims.player_leaves(TaskId(1), ClientId(9));
    assert_eq!(claims.taker(TaskId(1)), None);
}

#[test]
fn low_skill_bots_leave_hard_work_alone() {
    let table = PriorityTable::default();
    let mut hard = fire(1, [0.0; 3], 1.0);
    hard.kind = TaskKind::Repair;
    hard.target = Target::Device(DeviceId::new("reactor"));
    hard.need_skill = 60.0;
    // Skill 30 is more than 20 below 60: no.
    assert_eq!(choose(&bot(Role::Engineer), &[hard.clone()], &Claims::default(), &table, &calm(0.0), &mut Rng::new(1)), None);
    hard.need_skill = 50.0;
    assert!(choose(&bot(Role::Engineer), &[hard], &Claims::default(), &table, &calm(0.0), &mut Rng::new(1)).is_some());
}

#[test]
fn only_the_captain_edits_and_tables_round_trip_as_json() {
    let mut table = PriorityTable::default();
    assert_eq!(table.set_level(Role::Engineer, Role::Engineer, TaskKind::Heal, Level::Max), Err(Refusal::NotCaptain));
    assert_eq!(table.set_level(Role::Captain, Role::Captain, TaskKind::Heal, Level::Max), Err(Refusal::NoSuchSlot));
    table.set_level(Role::Captain, Role::Pilot, TaskKind::Heal, Level::Max).unwrap();
    table.page_mut(Role::Pilot).unwrap().overrides.push(Override { task: TaskKind::Heal, when: When::Order(OrderKind::Custom(2)), inverted: true, level: Level::Low });

    let json = table.to_json();
    let back = PriorityTable::from_json(&json).unwrap();
    assert_eq!(back, table);

    let bad = json.replacen("\"pages\"", "\"bogus\": 1, \"pages\"", 1);
    assert!(PriorityTable::from_json(&bad).is_err());
    let old = json.replacen(&format!("\"version\": {}", priorities::TABLE_VERSION), "\"version\": 999", 1);
    assert!(PriorityTable::from_json(&old).unwrap_err().contains("version"));
}
