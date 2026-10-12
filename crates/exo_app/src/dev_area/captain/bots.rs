//! The bot priorities display: `crew_core`'s priority table as rows with five levels per task,
//! a page per bot class, save, load and reset; and a demo bot in the captain zone that walks to
//! what `crew_core::choose` picks among two placeholder tasks (a fire marker and a turret
//! station), so the captain sees a table change, an order or the alert change what the bot does.
//!
//! The demo bot moves in a straight line at `BOT_SPEED` without collision and has no animation;
//! the window draws it as an orange box. Saving keeps the table as JSON in memory (TODO: a file
//! in the save folder once bots are real).
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::origin::WorldPose;
use crew_core::priorities::{Level, TaskKind};
use crew_core::{choose, BotId, BotView, Choice, Claims, Pick, Situation, StationId, Task, TaskId};
use display_core::{button, column, label, row, space, title, Panel, Tone};
use gameplay_core::rng::Rng;

pub const ID: PanelId = PanelId::new("captain_bots");
pub const NAME: &str = "Bot priorities";

/// The demo bot's class: the tactical slot (turrets first).
pub const BOT_CLASS: Role = Role::Tactical;
/// Where the demo bot starts and where its two tasks stand, relative to the captain zone's centre.
pub const BOT_HOME: [f64; 2] = [14.0, -4.0];
pub const FIRE_AT: [f64; 2] = [8.0, 3.0];
pub const TURRET_AT: [f64; 2] = [20.0, 3.0];
/// The demo bot's speed (m/s) and how close it stops at a task (m).
pub const BOT_SPEED: f64 = 2.5;
pub const ARRIVE_M: f64 = 1.2;
/// The demo bot's skill (lower than a player's 50; `skills-and-talents.md`, "Bot skill").
pub const BOT_SKILL: f64 = 30.0;
/// The room the demo fire burns in, and the demo turret's station (an Operate order on
/// `turret_port` or an Extinguish order on `weapons_bay` points at them).
pub const FIRE_ROOM: &str = "weapons_bay";
pub const TURRET_STATION: &str = "turret_port";

/// The pages the display shows.
pub const PAGES: [Role; 4] = [Role::Pilot, Role::Engineer, Role::Tactical, Role::Scientist];

#[derive(Resource, Debug, Clone)]
pub struct BotsUi {
    pub page: Role,
    /// The table saved with SAVE, as JSON.
    pub saved: Option<String>,
}

impl Default for BotsUi {
    fn default() -> Self {
        BotsUi { page: BOT_CLASS, saved: None }
    }
}

/// The demo bot.
#[derive(Resource, Debug, Clone)]
pub struct BotDemo {
    /// Area metres (x east, y north).
    pub pos: [f64; 2],
    pub current: Option<Choice>,
    pub claims: Claims,
    rng: Rng,
}

impl Default for BotDemo {
    fn default() -> Self {
        BotDemo { pos: CAPTAIN.point(BOT_HOME), current: None, claims: Claims::default(), rng: Rng::new(0xB07) }
    }
}

impl BotDemo {
    /// What the bot does, for the panel and scenarios: "turret", "fire", an order, or "idle".
    pub fn doing(&self) -> String {
        match self.current.as_ref().map(|c| &c.pick) {
            Some(Pick::Task(TaskId(1))) => "fire".into(),
            Some(Pick::Task(TaskId(2))) => "turret".into(),
            Some(Pick::Task(t)) => format!("task {}", t.0),
            Some(Pick::Order { kind, .. }) => format!("order {}", kind.id()),
            None => "idle".into(),
        }
    }
}

pub fn plugin(app: &mut App) {
    {
        let mut d = app.world_mut().resource_mut::<DevAreaDevices>();
        d.add(console(ID, NAME, 14.0, [0.26, 0.34, 0.4]));
        d.add(PlaceholderBox { id: "demo_fire".into(), zone: CAPTAIN, local_pos: FIRE_AT, size: [1.0, 0.4, 1.0], turn_deg: 0.0, colour: [0.95, 0.3, 0.1], panel: None });
        d.add(PlaceholderBox { id: "demo_turret".into(), zone: CAPTAIN, local_pos: TURRET_AT, size: [1.2, 1.4, 1.2], turn_deg: 0.0, colour: [0.45, 0.5, 0.42], panel: None });
    }
    app.init_resource::<BotsUi>().init_resource::<BotDemo>();
    app.add_systems(FixedUpdate, (read_clicks, think_and_walk, write_panel).chain().in_set(crate::phases::Fx::Effects).after(super::overview::read_clicks));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, draw_bot.in_set(crate::phases::Frame::World));
}

/// The demo's two open tasks (positions in area metres).
pub fn demo_tasks() -> Vec<Task> {
    let at = |p: [f64; 2]| {
        let a = CAPTAIN.point(p);
        [a[0], a[1], 0.0]
    };
    vec![
        Task { id: TaskId(1), kind: TaskKind::ExtinguishFire, target: Target::Room(crew_core::RoomId::new(FIRE_ROOM)), pos: at(FIRE_AT), urgency: 0.6, need_skill: 0.0 },
        Task { id: TaskId(2), kind: TaskKind::OperateTurret, target: Target::Station(StationId::new(TURRET_STATION)), pos: at(TURRET_AT), urgency: 0.5, need_skill: 0.0 },
    ]
}

fn level_name(l: Level) -> &'static str {
    match l {
        Level::Ignore => "IGNORE",
        Level::Low => "LOW",
        Level::Medium => "MED",
        Level::High => "HIGH",
        Level::Max => "MAX",
    }
}

pub fn panel(crew: &CrewState, ui: &BotsUi, demo: &BotDemo) -> Panel {
    let mut items = vec![title("BOT PRIORITIES").fixed(40.0)];
    items.push(row(6.0, PAGES.iter().map(|r| button(pick(&format!("{r:?}").to_uppercase(), *r == ui.page), format!("page:{}", r.id()))).collect()).fixed(40.0));
    match crew.table.page(ui.page) {
        None => {
            items.push(label(format!("No bot page for {:?}: this slot needs no bot.", ui.page)).tone(Tone::Dim).fixed(30.0));
            items.push(button("ADD A PAGE (all low)", "add_page").fixed(40.0));
        }
        Some(page) => {
            for t in TaskKind::ALL {
                let base = page.base(t);
                let mut cells = vec![label(t.id().replace('_', " ")).flex(2.2)];
                cells.extend(Level::ALL.iter().map(|l| button(pick(level_name(*l), *l == base), format!("lvl:{}:{}", t.id(), level_name(*l))).flex(1.0)));
                items.push(row(5.0, cells).fixed(40.0));
            }
        }
    }
    items.push(row(8.0, vec![button("SAVE", "save"), button("LOAD", "load").enabled(ui.saved.is_some()), button("RESET", "reset")]).fixed(42.0));
    let score = demo.current.as_ref().map(|c| format!("score {:.1}{}", c.score, c.level.map(|l| format!(", {}", level_name(l))).unwrap_or_else(|| ", order".into()))).unwrap_or_default();
    items.push(label(format!("DEMO BOT ({BOT_CLASS:?}): {}   {score}", demo.doing())).tone(Tone::Good).fixed(28.0));
    items.push(space());
    Panel::new(PANEL_PX[0], PANEL_PX[1], column(5.0, items))
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut ui: ResMut<BotsUi>, mut crew: ResMut<CrewState>, mut gp: ResMut<Gameplay>, role: Res<DevRole>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        let parts: Vec<&str> = c.action.split(':').collect();
        let by = role.0;
        let res = match parts.as_slice() {
            ["page", r] => {
                ui.page = Role::ALL.into_iter().find(|x| x.id() == *r).unwrap_or(ui.page);
                Ok(())
            }
            ["lvl", t, l] => match (TaskKind::ALL.into_iter().find(|k| k.id() == *t), Level::ALL.into_iter().find(|x| level_name(*x) == *l)) {
                (Some(t), Some(l)) => crew.table.set_level(by, ui.page, t, l),
                _ => continue,
            },
            ["add_page"] => TaskKind::ALL.into_iter().try_for_each(|t| crew.table.set_level(by, ui.page, t, Level::Low)),
            ["save"] => {
                ui.saved = Some(crew.table.to_json());
                Ok(())
            }
            ["load"] => match ui.saved.as_deref().map(crew_core::PriorityTable::from_json) {
                Some(Ok(t)) if by == Role::Captain => {
                    crew.table = t;
                    Ok(())
                }
                Some(Err(e)) => {
                    warn!("bot table: {e}");
                    Ok(())
                }
                Some(Ok(_)) => Err(Refusal::NotCaptain),
                None => Ok(()),
            },
            ["reset"] if by == Role::Captain => {
                crew.table = crew_core::PriorityTable::default();
                Ok(())
            }
            ["reset"] => Err(Refusal::NotCaptain),
            _ => {
                warn!("bot priorities: unknown action {c:?}");
                continue;
            }
        };
        if let Err(r) = res {
            refuse(&mut gp, &r);
        }
    }
}

/// The demo bot thinks every tick and walks to its pick.
pub fn think_and_walk(time: Res<Time>, crew: Res<CrewState>, gp: Res<Gameplay>, mut demo: ResMut<BotDemo>) {
    let tasks = demo_tasks();
    let demo = &mut *demo;
    let view = BotView { id: BotId(1), class: BOT_CLASS, pos: [demo.pos[0], demo.pos[1], 0.0], skill: BOT_SKILL, current: demo.current.clone() };
    let sit = Situation { alert: crew.alert.level(), order: crew.orders.for_role(BOT_CLASS), flags: &[], now_s: gp.clock() };
    let choice = choose(&view, &tasks, &demo.claims, &crew.table, &sit, &mut demo.rng);
    match &choice {
        Some(c) => demo.claims.commit(view.id, c),
        None => demo.claims.release_bot(view.id),
    }
    let goal = match choice.as_ref().map(|c| &c.pick) {
        Some(Pick::Task(id)) => tasks.iter().find(|t| t.id == *id).map(|t| [t.pos[0], t.pos[1]]),
        // Follow me and report: to the overview console. Other orders: stay (no positions yet).
        Some(Pick::Order { kind: crew_core::OrderKind::FollowMe | crew_core::OrderKind::Report, .. }) => Some(CAPTAIN.point([-21.0, CONSOLE_Y - 2.0])),
        _ => None,
    };
    if demo.current.as_ref().map(|c| &c.pick) != choice.as_ref().map(|c| &c.pick) {
        info!("demo bot: {:?} -> {:?}", demo.current.as_ref().map(|c| &c.pick), choice.as_ref().map(|c| (&c.pick, c.score)));
    }
    demo.current = choice;
    if let Some(g) = goal {
        let d = [g[0] - demo.pos[0], g[1] - demo.pos[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len > ARRIVE_M {
            let step = (BOT_SPEED * time.delta_secs_f64()).min(len - ARRIVE_M);
            demo.pos = [demo.pos[0] + d[0] / len * step, demo.pos[1] + d[1] / len * step];
        }
    }
}

pub fn write_panel(crew: Res<CrewState>, ui: Res<BotsUi>, demo: Res<BotDemo>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&crew, &ui, &demo));
}

/// The demo's distance to a task (m), for scenarios.
pub fn distance_to(demo: &BotDemo, local: [f64; 2]) -> f64 {
    let a = CAPTAIN.point(local);
    ((a[0] - demo.pos[0]).powi(2) + (a[1] - demo.pos[1]).powi(2)).sqrt()
}

#[derive(Component)]
struct DemoBotLook;

/// Window: the demo bot as an orange box on the ground.
fn draw_bot(mut commands: Commands, demo: Res<BotDemo>, area: Res<DevArea>, planet: Res<crate::env::PlanetRes>, mut q: Query<&mut WorldPose, With<DemoBotLook>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(frame) = area.frame else { return };
    let (pos, up) = frame.ground(&planet, demo.pos[0], demo.pos[1], 0.85);
    let rot = frame.rotation(up, 0.0);
    match q.single_mut() {
        Ok(mut p) => *p = WorldPose { pos, rot },
        Err(_) => {
            commands.spawn((
                DemoBotLook,
                Mesh3d(meshes.add(Cuboid::new(0.6, 1.7, 0.6))),
                MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.1), ..default() })),
                Transform::default(),
                WorldPose { pos, rot },
            ));
        }
    }
}
