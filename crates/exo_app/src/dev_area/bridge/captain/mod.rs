//! The captain's console in the dev area (#288, part of #284, ported into the CAPTAIN zone in r44):
//! five displays in the captain zone, built on `crew_core` (the logic) and `display_core` (the
//! widgets). Each display is a module with the test console's two systems (one writes the panel,
//! one reads its clicks):
//!
//! - `overview`: crew roster, alert level, the order wheel with target and priority, special
//!   orders, the target ship, the budget per role, the alarm list.
//! - `missions`: the real jobs of `Gameplay` as cards, filters, accept, set active, abandon.
//! - `ship_state`: the ship as `live_status` builds it from `ReactorNews`.
//! - `starmap`: places with distances and a destination; `comms`: a hail with one canned dialogue.
//!
//! Not here (later lane, r44): the bot priority table and the bodied bots (`captain/bots.rs`,
//! `captain/bot_work.rs` in `origin/round/dev-area`). `CrewState::table` stays for them.
//!
//! The crew-wide state (`CrewState`) lives here: alert, orders, budget, the bot table, the comms
//! threat flag. Only the captain may change it; the dev area plays the captain by default, and
//! `DevRole` lets the overview's dev button play another role to see the refusals. Orders, alert
//! changes and refusals become notices (#165); the order that applies to the player's role shows
//! as a line under the interaction prompt.
use crate::dev_area::*;
use crate::gameplay::Gameplay;
use crew_core::orders::OrderKind;
use crew_core::{Alert, AlertEvent, Budget, Order, Orders, PriorityTable, Refusal, Role, Target};
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::TextKey;

pub mod comms;
pub mod live_status;
pub mod missions;
pub mod overview;
pub mod ship_state;
pub mod starmap;

/// The role the player plays in the dev area. Captain by default.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevRole(pub Role);

impl Default for DevRole {
    fn default() -> Self {
        DevRole(Role::Captain)
    }
}

/// What the captain runs for the whole crew.
#[derive(Resource)]
pub struct CrewState {
    pub alert: Alert,
    pub orders: Orders,
    pub budget: Budget,
    /// The bot priority table: no display here yet (the bots' lane). Saved with the crew.
    pub table: PriorityTable,
    /// A hostile answer on comms: the threat flag for the automatic yellow, with the ship's trouble
    /// (`live_status::threat`).
    pub hostile: bool,
}

impl Default for CrewState {
    fn default() -> Self {
        CrewState { alert: Alert::default(), orders: Orders::default(), budget: Budget::default(), table: PriorityTable::default(), hostile: false }
    }
}

/// Text keys this module shows; checked against the text table at start.
pub const KEYS: &[&str] = &[
    "notice.alert.green",
    "notice.alert.yellow",
    "notice.alert.red",
    "notice.captain.order",
    "notice.captain.order_cleared",
    "notice.captain.refused",
    "notice.captain.destination",
    "order.go_to",
    "order.operate",
    "order.repair",
    "order.extinguish",
    "order.fix_breach",
    "order.follow_me",
    "order.stay",
    "order.report",
    "order.custom",
    "refusal.not_captain",
    "refusal.wrong_target",
    "refusal.no_such_slot",
    "refusal.no_custom_slot",
    "refusal.text_too_long",
    "refusal.bad_amount",
    "refusal.not_enough_available",
    "refusal.over_allowance",
    "refusal.not_enough_credits",
    "refusal.unknown_job",
    "refusal.not_offered",
    "refusal.not_active",
    "refusal.too_many_active",
];

/// Panel sizes: pixels and metres keep the same aspect, so the world quad is not stretched.
pub const PANEL_PX: [f64; 2] = [1280.0, 760.0];
pub const PANEL_M: [f64; 2] = [1.6, 0.95];

/// A console box in the captain zone with a big display, facing south.
pub fn console(id: PanelId, name: &str, x: f64, colour: [f32; 3]) -> PlaceholderBox {
    PlaceholderBox {
        id: id.as_str().to_string(),
        zone: CAPTAIN,
        local_pos: [x, CONSOLE_Y],
        size: [1.8, 1.0, 0.8],
        turn_deg: 0.0,
        colour,
        panel: Some(PanelSpec { id, name: name.into(), size_m: PANEL_M, at: [0.0, 1.55, 0.25], tilt_deg: 15.0 }),
    }
}

/// The consoles stand in a row this far south of the zone's centre (m). The zone is 14 m wide, so six
/// boxes of 1.8 m at 2.3 m apart fit with the outer edges 0.35 m inside it; the captain zone's last
/// place (x 5.75) is free for the bots' lane.
pub const CONSOLE_Y: f64 = -3.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<DevRole>().init_resource::<CrewState>().init_resource::<live_status::LiveShip>();
    app.add_systems(Startup, check_keys);
    app.add_systems(FixedUpdate, (tick_crew, live_status::update).chain().in_set(crate::phases::Fx::Effects).before(overview::read_clicks));
    app.add_plugins((overview::plugin, missions::plugin, ship_state::plugin, starmap::plugin, comms::plugin));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_order_line);
    app.add_systems(Update, update_order_line.in_set(crate::phases::Frame::Hud));
}

/// Every key this module shows has a text (a missing one is a content mistake: stop).
fn check_keys(gp: Res<Gameplay>) {
    let missing: Vec<&str> = KEYS.iter().copied().filter(|k| gp.text(&TextKey::new(*k)) == *k).collect();
    assert!(missing.is_empty(), "content/gameplay/text/en.json: no text for {missing:?}");
}

/// Each tick: orders expire, the threat flags feed the alert.
fn tick_crew(mut crew: ResMut<CrewState>, mut gp: ResMut<Gameplay>, items: Res<crate::items::Items>) {
    let now = gp.clock();
    let crew = &mut *crew;
    for _ in crew.orders.tick(now) {
        say(&mut gp, Notice::new(NoticeKind::Updated, "notice.captain.order_cleared"));
    }
    let trouble = items.reactor.as_ref().is_some_and(live_status::threat);
    if let Some(e) = crew.alert.update(crew.hostile || trouble, now) {
        alert_notice(&mut gp, e);
    }
}

/// Queues a notice and notes its key in the gameplay log (scenario checks read it there).
pub fn say(gp: &mut Gameplay, n: Notice) {
    gp.log.push(format!("captain: {}", n.key));
    gp.notify(n);
}

pub fn alert_notice(gp: &mut Gameplay, e: AlertEvent) {
    let AlertEvent::Changed { to, .. } = e;
    say(gp, to.notice());
}

/// A refusal as a warning notice: "Refused: only the captain can do that".
pub fn refuse(gp: &mut Gameplay, r: &Refusal) {
    gp.log.push(format!("captain: refused {}", r.key()));
    say(gp, Notice::new(NoticeKind::Warning, "notice.captain.refused").arg("reason", Arg::Key(TextKey::new(r.key()))));
}

/// A target as a short name.
pub fn target_name(t: &Target) -> String {
    let id = match t {
        Target::Room(r) => r.as_str(),
        Target::Station(s) => s.as_str(),
        Target::Device(d) => d.as_str(),
    };
    id.replace('_', " ")
}

/// An order in words: "Fix the breach in cargo hold", or the special order's text.
pub fn order_text(gp: &Gameplay, orders: &Orders, o: &Order) -> String {
    if let OrderKind::Custom(i) = o.kind {
        return orders.custom().get(i as usize).cloned().flatten().unwrap_or_else(|| gp.text(&TextKey::new("order.custom")));
    }
    let verb = gp.text(&TextKey::new(format!("order.{}", o.kind.id())));
    match &o.target {
        Some(t) => format!("{verb} {}", target_name(t)),
        None => verb,
    }
}

/// The notice for a new order.
pub fn order_notice(gp: &mut Gameplay, orders: &Orders, o: &Order) {
    let text = order_text(gp, orders, o);
    say(gp, Notice::new(NoticeKind::Updated, "notice.captain.order").arg("order", Arg::Text(text)));
}

/// The HUD line of the order that applies to the player's role ("" for none).
pub fn hud_order_line(gp: &Gameplay, crew: &CrewState, role: Role) -> String {
    crew.orders.for_role(role).map(|o| format!("ORDER  {}", order_text(gp, &crew.orders, o))).unwrap_or_default()
}

/// Window: the order line under the interaction prompt.
#[derive(Component)]
struct OrderLine;

fn spawn_order_line(mut commands: Commands) {
    commands.spawn((
        OrderLine,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(18.0), ..default() },
        TextColor(Color::srgb(0.6, 0.85, 1.0)),
        Node { position_type: PositionType::Absolute, top: percent(62), width: percent(100), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center),
    ));
}

fn update_order_line(gp: Res<Gameplay>, crew: Res<CrewState>, role: Res<DevRole>, mut q: Query<&mut Text, With<OrderLine>>) {
    let line = hud_order_line(&gp, &crew, role.0);
    if let Ok(mut t) = q.single_mut()
        && **t != line
    {
        **t = line;
    }
}

/// "[HIGH]" for the chosen option of a button group, "HIGH" otherwise.
pub fn pick(label: &str, chosen: bool) -> String {
    if chosen { format!("[{label}]") } else { label.to_string() }
}
