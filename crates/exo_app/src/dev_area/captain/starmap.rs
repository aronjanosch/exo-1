//! The starmap display (placeholder): the system's planets, the dev area and the tracked job's
//! next stop as a list with distances from the player, and a destination the captain sets. The
//! destination is stored in `Destination`; showing it on the pilot's HUD is not in this round.
use super::*;
use crate::displays::{eye_on_foot, DisplayAction, Panels};
use crate::walker::Player;
use crate::warp::SystemRes;
use bevy::math::DVec3;
use display_core::{button, column, label, row, space, title, Panel, Tone};

pub const ID: PanelId = PanelId::new("captain_starmap");
pub const NAME: &str = "Starmap";

/// The place the captain picked (a pin id: `planet:Hearth`, `dev_area`, `job:<location>`).
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct Destination(pub Option<String>);

/// One line of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub id: String,
    pub name: String,
    /// Metres from the player; none when unknown (not on foot, not on this planet).
    pub distance: Option<f64>,
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(console(ID, NAME, 0.0, [0.22, 0.3, 0.45]));
    app.init_resource::<Destination>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects).after(super::overview::read_clicks));
}

/// The pins from the game's state.
pub fn pins(gp: &Gameplay, system: &SystemRes, planet: &crate::env::PlanetRes, area: &DevArea, from: Option<DVec3>) -> Vec<Pin> {
    let dist = |p: DVec3| from.map(|f| f.distance(p));
    let mut out: Vec<Pin> = system
        .0
        .planets
        .iter()
        .map(|p| {
            let centre = DVec3::from(p.centre);
            Pin { id: format!("planet:{}", p.name), name: format!("Planet {}", p.name), distance: dist(centre).map(|d| (d - p.radius).max(0.0)) }
        })
        .collect();
    out.push(Pin { id: "dev_area".into(), name: "Dev area".into(), distance: area.ground(planet, [0.0, 0.0], 0.0).and_then(|(p, _)| dist(p)) });
    if let Some((loc, stop)) = gp.tracked.and_then(|t| gp.jobs.get(t)).and_then(jobs_core::map::next_stop) {
        let name = gp.kernel.locations.get(&loc).map(|l| gp.text(&l.record.name)).unwrap_or_else(|| loc.to_string());
        out.push(Pin { id: format!("job:{loc}"), name: format!("Job {stop:?}: {name}"), distance: gp.pad_of(&loc).and_then(|p| dist(p.centre)) });
    }
    out
}

fn distance_text(d: Option<f64>) -> String {
    match d {
        None => "-".into(),
        Some(d) if d < 50.0 => "here".into(),
        Some(d) if d < 10_000.0 => format!("{d:.0} m"),
        Some(d) => format!("{:.0} km", d / 1000.0),
    }
}

pub fn panel(pins: &[Pin], dest: &Destination) -> Panel {
    let mut items = vec![title("STARMAP").fixed(40.0), label("Placeholder: a list until the map is drawn here.").tone(Tone::Dim).fixed(24.0)];
    for p in pins {
        let chosen = dest.0.as_deref() == Some(p.id.as_str());
        items.push(row(8.0, vec![label(pick(&p.name, chosen)).flex(3.0), label(distance_text(p.distance)).flex(1.0), button("SET DESTINATION", format!("dest:{}", p.id)).flex(1.4)]).fixed(44.0));
    }
    let name = dest.0.as_ref().and_then(|d| pins.iter().find(|p| &p.id == d)).map(|p| p.name.clone()).or_else(|| dest.0.clone()).unwrap_or_else(|| "none".into());
    items.push(label(format!("DESTINATION  {name}")).tone(Tone::Good).fixed(30.0));
    items.push(space());
    Panel::new(PANEL_PX[0], PANEL_PX[1], column(8.0, items))
}

fn player_pos(players: &Query<&Player>) -> Option<DVec3> {
    players.single().ok().and_then(eye_on_foot).map(|(e, _)| e)
}

#[allow(clippy::too_many_arguments)]
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut dest: ResMut<Destination>, mut gp: ResMut<Gameplay>, role: Res<DevRole>, system: Res<SystemRes>, planet: Res<crate::env::PlanetRes>, area: Res<DevArea>, players: Query<&Player>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        let Some(id) = c.action.strip_prefix("dest:") else {
            warn!("starmap: unknown action {c:?}");
            continue;
        };
        if role.0 != Role::Captain {
            refuse(&mut gp, &Refusal::NotCaptain);
            continue;
        }
        let name = pins(&gp, &system, &planet, &area, player_pos(&players)).into_iter().find(|p| p.id == id).map_or_else(|| id.to_string(), |p| p.name);
        dest.0 = Some(id.to_string());
        say(&mut gp, Notice::new(NoticeKind::Updated, "notice.captain.destination").arg("place", Arg::Text(name)));
    }
}

pub fn write_panel(gp: Res<Gameplay>, dest: Res<Destination>, system: Res<SystemRes>, planet: Res<crate::env::PlanetRes>, area: Res<DevArea>, players: Query<&Player>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&pins(&gp, &system, &planet, &area, player_pos(&players)), &dest));
}
