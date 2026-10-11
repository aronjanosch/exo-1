//! The first city and the town dressing (round 6): the city plan (`content/city/plan.json`,
//! exported from `art/city/city_plan.py`) placed on the place `city`, box colliders for its
//! buildings, landing-pad models on every pad, and placeholder characters (NPCs) where the plan
//! and the gameplay put someone to talk to.
//!
//! Buildings stand on the ground below their own centre (no flattening yet: TODO(initiator)).
//! Colliders exist headless too, so the walker and the ship meet the same walls in scenarios;
//! models are window-only.
use crate::env::{PlanetRes, from_v3, to_v3};
use crate::gameplay::Gameplay;
use crate::named_material::{NamedMaterials, WantMaterials};
use crate::origin::WorldPos;
use crate::terrain::PlanetScene;
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashSet};

/// The place the plan's origin sits on (`content/place/city.json`).
pub const CITY_PLACE: &str = "city";

#[derive(Clone, Debug)]
pub struct Plan {
    /// The place the plan's origin sits on. The city plan has none (it is `CITY_PLACE`); a dress
    /// plan in `content/city/dress/` names its own.
    pub place: Option<String>,
    pub placements: Vec<Placement>,
    pub models: BTreeMap<String, Bounds>,
    pub quests: Vec<Quest>,
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub model: String,
    /// Content folder of the model (`city` when absent). Dress placements may say `props`.
    pub folder: Option<String>,
    /// Metres in the place frame: x east, y north, z up (z is the model's lift).
    pub at: [f64; 3],
    pub turn_deg: f64,
    pub fit: Option<String>,
}

impl Placement {
    /// The model's file under `content/`, e.g. `city/row_slant.glb` or `props/x.glb`.
    pub fn model_path(&self) -> String {
        format!("{}/{}.glb", self.folder.as_deref().unwrap_or("city"), self.model)
    }
}

/// A model's box in glTF axes: x, y up, z towards the model's front.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, Debug)]
pub struct Quest {
    pub id: String,
    pub at: [f64; 2],
}

pub fn load_plan() -> Plan {
    let path = format!("{}/city/plan.json", crate::content_dir());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    parse_plan(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Reads the exporter's JSON (no serde derive in this crate: `serde_json::Value` by hand).
pub fn parse_plan(text: &str) -> Result<Plan, String> {
    use serde_json::Value;
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let num = |v: &Value, what: &str| v.as_f64().ok_or_else(|| format!("{what}: expected a number"));
    let arr = |v: &Value, what: &str, n: usize| -> Result<Vec<f64>, String> {
        let a = v.as_array().filter(|a| a.len() == n).ok_or_else(|| format!("{what}: expected {n} numbers"))?;
        a.iter().map(|x| num(x, what)).collect()
    };
    let mut placements = Vec::new();
    for (i, p) in v["placements"].as_array().ok_or("placements: expected a list")?.iter().enumerate() {
        let at = arr(&p["at"], &format!("placements[{i}].at"), 3)?;
        placements.push(Placement {
            model: p["model"].as_str().ok_or_else(|| format!("placements[{i}].model: expected text"))?.to_string(),
            folder: p["folder"].as_str().map(str::to_string),
            at: [at[0], at[1], at[2]],
            turn_deg: num(&p["turn_deg"], &format!("placements[{i}].turn_deg"))?,
            fit: p["fit"].as_str().map(str::to_string),
        });
    }
    let mut models = BTreeMap::new();
    for (id, b) in v["models"].as_object().ok_or("models: expected an object")? {
        let (lo, hi) = (arr(&b["min"], &format!("models.{id}.min"), 3)?, arr(&b["max"], &format!("models.{id}.max"), 3)?);
        models.insert(id.clone(), Bounds { min: [lo[0], lo[1], lo[2]], max: [hi[0], hi[1], hi[2]] });
    }
    let mut quests = Vec::new();
    for (i, q) in v["quests"].as_array().ok_or("quests: expected a list")?.iter().enumerate() {
        let at = arr(&q["at"], &format!("quests[{i}].at"), 2)?;
        quests.push(Quest { id: q["id"].as_str().ok_or_else(|| format!("quests[{i}].id: expected text"))?.to_string(), at: [at[0], at[1]] });
    }
    Ok(Plan { place: v["place"].as_str().map(str::to_string), placements, models, quests })
}

/// The dress plans: every `content/city/dress/*.json`, sorted by file name, as (place, plan).
/// A file that does not parse or names no place is skipped with a warning (a bad dress file must
/// not stop the game).
pub fn load_dress() -> Vec<(String, Plan)> {
    let dir = format!("{}/city/dress", crate::content_dir());
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warn!("dress {}: {e}", path.display());
                continue;
            }
        };
        match parse_plan(&text) {
            Ok(plan) => match plan.place.clone() {
                Some(place) => out.push((place, plan)),
                None => warn!("dress {}: no \"place\"", path.display()),
            },
            Err(e) => warn!("dress {}: {e}", path.display()),
        }
    }
    out
}

/// Models that get a box collider: buildings and big street furniture. Everything else (trees,
/// lanes, lamps, cars) is walked and flown through. TODO(initiator): which ones, and real shapes.
pub fn solid(model: &str) -> bool {
    const SOLID: [&str; 12] = ["row_", "bungalow_", "company_hq", "pod_tower", "corner_", "saucer_diner", "bubble_shop", "charge_stop", "beacon_spire", "booth_tele", "vending_tube", "roof_dish"];
    SOLID.iter().any(|p| model.starts_with(p))
}

/// The tangent frame of a place: origin on the ground, east, north, up (planet space directions).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub dir: DVec3,
    pub east: DVec3,
    pub north: DVec3,
}

impl Frame {
    pub fn of_place(planet: &PlanetRes, id: &str) -> Option<Frame> {
        let p = planet.pgen.places.iter().find(|p| p.id == id)?;
        let dir = from_v3(p.dir());
        let (e0, n0) = planet_core::look::tangent_frame(to_v3(dir));
        let yaw = p.turn_deg.to_radians();
        let (e0, n0) = (from_v3(e0), from_v3(n0));
        let east = e0 * yaw.cos() + n0 * yaw.sin();
        let north = dir.cross(east);
        Some(Frame { dir, east, north })
    }

    /// World position on the ground below the frame point (x east, y north) plus `lift` up, and
    /// the up there.
    pub fn ground(&self, planet: &PlanetRes, x: f64, y: f64, lift: f64) -> (DVec3, DVec3) {
        let d = (self.dir * planet.radius + self.east * x + self.north * y).normalize();
        (planet.centre + d * (planet.surface(d) + lift), d)
    }

    /// Rotation from glTF model axes (x, y up, z front) to the world, turned by `turn_deg`
    /// counter-clockwise seen from above (the plan's turn: 0 faces south, -north).
    pub fn rotation(&self, up: DVec3, turn_deg: f64) -> DQuat {
        // Re-orthogonalise east and north around the local up (the city spans a few hundred metres).
        let east = (self.east - up * self.east.dot(up)).normalize();
        let north = up.cross(east);
        let basis = DQuat::from_mat3(&bevy::math::DMat3::from_cols(east, up, -north));
        basis * DQuat::from_rotation_y(turn_deg.to_radians())
    }
}

/// A placeholder character: who, where (world, on the ground), and the up there.
#[derive(Clone, Debug)]
pub struct Npc {
    pub id: String,
    pub pos: DVec3,
    pub up: DVec3,
}

#[derive(Resource, Default)]
pub struct Town {
    for_planet: Option<warp_core::PlanetId>,
    colliders: Vec<Entity>,
    pub plan: Option<Plan>,
    /// The dress plans (`content/city/dress/`) with the place each one stands on.
    pub dress: Vec<(String, Plan)>,
    pub frame: Option<Frame>,
    /// Characters from the plan (quest spots); gameplay adds its own through `extra_npcs`.
    pub npcs: Vec<Npc>,
    /// Characters the gameplay asks for (customers at their pads), refreshed by the gameplay glue.
    pub extra_npcs: Vec<Npc>,
    pub solid_count: usize,
}

impl Town {
    pub fn all_npcs(&self) -> impl Iterator<Item = &Npc> {
        self.npcs.iter().chain(self.extra_npcs.iter())
    }
}

/// Builds the colliders and the character list once per planet (headless too).
pub fn update_town(mut commands: Commands, planet: Res<PlanetRes>, mut town: ResMut<Town>) {
    if town.for_planet == Some(planet.id) {
        return;
    }
    town.for_planet = Some(planet.id);
    for e in town.colliders.drain(..) {
        commands.entity(e).despawn();
    }
    town.npcs.clear();
    town.solid_count = 0;
    if town.plan.is_none() {
        town.plan = Some(load_plan());
        town.dress = load_dress();
    }
    let Some(frame) = Frame::of_place(&planet, CITY_PLACE) else {
        town.frame = None;
        return;
    };
    town.frame = Some(frame);
    let plan = town.plan.clone().expect("loaded above");
    let mut colliders = spawn_colliders(&mut commands, &planet, &frame, &plan);
    town.solid_count = colliders.len();
    for (place, dress) in &town.dress {
        match Frame::of_place(&planet, place) {
            Some(frame) => colliders.extend(spawn_colliders(&mut commands, &planet, &frame, dress)),
            None => warn!("dress: place '{place}' is not on the planet; its plan is skipped"),
        }
    }
    town.colliders = colliders;
    town.npcs = plan
        .quests
        .iter()
        .map(|q| {
            let (pos, up) = frame.ground(&planet, q.at[0], q.at[1], 0.0);
            Npc { id: q.id.clone(), pos, up }
        })
        .collect();
}

/// Spawns the box colliders of a plan's solid placements on `frame`, and returns them.
fn spawn_colliders(commands: &mut Commands, planet: &PlanetRes, frame: &Frame, plan: &Plan) -> Vec<Entity> {
    let mut colliders = Vec::new();
    for p in plan.placements.iter().filter(|p| solid(&p.model)) {
        let Some(b) = plan.models.get(&p.model) else { continue };
        let (ground, up) = frame.ground(planet, p.at[0], p.at[1], p.at[2]);
        let rot = frame.rotation(up, p.turn_deg);
        let half = DVec3::new(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) * 0.5;
        let centre_local = DVec3::new(b.max[0] + b.min[0], b.max[1] + b.min[1], b.max[2] + b.min[2]) * 0.5;
        // Sink the box a little so it meets sloped ground instead of hovering on its low side.
        let sink = 1.5;
        let e = commands
            .spawn((
                RigidBody::Static,
                Collider::cuboid(half.x * 2.0, half.y * 2.0 + sink, half.z * 2.0),
                Position(ground + rot * centre_local - up * (sink * 0.5)),
                Rotation(rot),
                CollisionLayers::new(Layer::World, LayerMask::ALL),
                TownCollider,
            ))
            .id();
        colliders.push(e);
    }
    colliders
}

/// Spawns the models of a plan's placements on `frame` (window only); returns how many.
fn spawn_models(commands: &mut Commands, planet: &PlanetRes, assets: &AssetServer, frame: &Frame, plan: &Plan) -> usize {
    for p in &plan.placements {
        let (ground, up) = frame.ground(planet, p.at[0], p.at[1], p.at[2]);
        let rot = frame.rotation(up, p.turn_deg);
        commands.spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(p.model_path()))),
            Transform::from_rotation(rot.as_quat()),
            WorldPos(ground),
            PlanetScene(planet.id),
        ));
    }
    plan.placements.len()
}

#[derive(Component)]
pub struct TownCollider;

/// Which planet the city models were spawned for.
#[derive(Resource, Default)]
pub struct TownView {
    for_planet: Option<warp_core::PlanetId>,
    npc_count: usize,
    pub models: usize,
}

/// A character's marker; `pos` and `up` are where it stands (the hop of a paid buyer starts there).
#[derive(Component)]
pub struct NpcModel {
    pub id: String,
    pub pos: bevy::math::DVec3,
    pub up: bevy::math::DVec3,
}

/// The model of a character: a quest giver keeps the plain marker; a buyer is one of three
/// figures, picked by a stable hash of its id (FNV-1a), so a buyer keeps its look.
pub fn npc_model(id: &str, buyer: bool) -> &'static str {
    const BUYERS: [&str; 3] = ["smuggle/buyer_a.glb", "smuggle/buyer_b.glb", "smuggle/buyer_c.glb"];
    if !buyer {
        return "city/npc_marker.glb";
    }
    let h = id.bytes().fold(2166136261u32, |h, c| (h ^ c as u32).wrapping_mul(16777619));
    BUYERS[(h % BUYERS.len() as u32) as usize]
}

/// The city's models and the characters' markers (window only).
fn update_town_view(mut commands: Commands, planet: Res<PlanetRes>, town: Res<Town>, assets: Res<AssetServer>, mut view: ResMut<TownView>, npcs: Query<Entity, With<NpcModel>>) {
    let npc_total = town.npcs.len() + town.extra_npcs.len();
    if town.for_planet != Some(planet.id) {
        return;
    }
    if view.for_planet != Some(planet.id) {
        view.for_planet = Some(planet.id);
        view.models = 0;
        if let (Some(plan), Some(frame)) = (&town.plan, town.frame) {
            view.models += spawn_models(&mut commands, &planet, &assets, &frame, plan);
        }
        for (place, dress) in &town.dress {
            // Unknown places were warned about in `update_town`.
            if let Some(frame) = Frame::of_place(&planet, place) {
                view.models += spawn_models(&mut commands, &planet, &assets, &frame, dress);
            }
        }
        view.npc_count = usize::MAX;
    }
    if view.npc_count != npc_total {
        view.npc_count = npc_total;
        for e in &npcs {
            commands.entity(e).despawn();
        }
        for (n, buyer) in town.npcs.iter().map(|n| (n, false)).chain(town.extra_npcs.iter().map(|n| (n, true))) {
            commands.spawn((
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(npc_model(&n.id, buyer)))),
                Transform::from_rotation(Quat::from_rotation_arc(Vec3::Y, n.up.as_vec3())),
                WorldPos(n.pos),
                PlanetScene(planet.id),
                NpcModel { id: n.id.clone(), pos: n.pos, up: n.up },
            ));
        }
    }
}

/// Height of an order marker's lowest tip above the buyer's ground (m).
const ORDER_MARKER_H: f32 = 2.3;

/// A buyer with an open product order shows a floating marker (window only). It bobs (about 1 Hz,
/// 0.15 m) and turns; the tracked order's marker is bigger and brighter.
#[derive(Component)]
pub struct OrderMarker;

fn order_markers(
    mut commands: Commands,
    time: Res<Time>,
    gp: Res<Gameplay>,
    assets: Res<AssetServer>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    buyers: Query<(Entity, &NpcModel)>,
    mut markers: Query<(Entity, &ChildOf, &mut Transform, Option<&NamedMaterials>), With<OrderMarker>>,
) {
    // Buyer id -> is its order the tracked one? (One entry per buyer with open product orders.)
    let mut wanted: Vec<(String, bool)> = Vec::new();
    for (id, o) in gp.customers.product_orders() {
        let tracked = gp.smuggling.tracked_order == Some(id);
        match wanted.iter_mut().find(|(b, _)| b == o.customer.as_str()) {
            Some(w) => w.1 |= tracked,
            None => wanted.push((o.customer.as_str().to_string(), tracked)),
        }
    }
    let t = time.elapsed_secs();
    let mut marked = HashSet::new();
    for (e, parent, mut tr, named) in &mut markers {
        let Ok((_, npc)) = buyers.get(parent.parent()) else {
            commands.entity(e).despawn();
            continue;
        };
        let Some(&(_, tracked)) = wanted.iter().find(|(b, _)| *b == npc.id) else {
            commands.entity(e).despawn();
            continue;
        };
        marked.insert(parent.parent());
        tr.translation = Vec3::new(0.0, ORDER_MARKER_H + 0.15 * (t * std::f32::consts::TAU).sin(), 0.0);
        tr.rotation = Quat::from_rotation_y(t * 0.8);
        tr.scale = Vec3::splat(if tracked { 1.3 } else { 1.0 });
        if let Some(h) = named.and_then(|n| n.0.get("glow")) {
            let glow = Color::srgb(1.0, 0.8, 0.3).to_linear() * if tracked { 4.0 } else { 1.2 };
            if mats.get(h).is_some_and(|m| m.emissive != glow) {
                if let Some(mut m) = mats.get_mut(h) {
                    m.emissive = glow;
                }
            }
        }
    }
    for (e, npc) in &buyers {
        if marked.contains(&e) || !wanted.iter().any(|(b, _)| *b == npc.id) {
            continue;
        }
        commands.entity(e).with_children(|c| {
            c.spawn((
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("smuggle/order_marker.glb"))),
                Transform::from_translation(Vec3::Y * ORDER_MARKER_H),
                WantMaterials(&["glow"]),
                OrderMarker,
            ));
        });
    }
}

/// Height and distance of the landmarks: a tower stands this far from its town's main pad (m).
const TOWER_OFF_M: f64 = 25.0;
/// The tower keeps this much clear of every pad's edge (m).
const PAD_CLEAR_M: f64 = 8.0;
/// Towers already in a town's dress plan (a town with one of these gets no `tower_small`).
const TOWERS: [&str; 3] = ["beacon_spire", "pod_tower", "tower_small"];

/// The towns that get a `tower_small`: the places with an inspect pad whose dress plan has no tower.
pub fn tower_places(inspect_places: &[&str], dress: &[(String, Plan)]) -> Vec<String> {
    inspect_places
        .iter()
        .filter(|place| !dress.iter().any(|(p, plan)| p == *place && plan.placements.iter().any(|x| TOWERS.contains(&x.model.as_str()))))
        .map(|p| p.to_string())
        .collect()
}

/// A tower's spot: 25 m from the main pad's centre, east, north, west or south (first that clears
/// every pad's edge by 8 m); None when no side does. `others` are (centre, radius) of all pads.
pub fn tower_spot(main: DVec3, up: DVec3, others: &[(DVec3, f64)]) -> Option<DVec3> {
    let east = up.any_orthonormal_vector();
    let north = up.cross(east);
    [east, north, -east, -north].into_iter().map(|d| main + d * TOWER_OFF_M).find(|spot| others.iter().all(|(c, r)| spot.distance(*c) > r + PAD_CLEAR_M))
}

/// The landmarks of the towns (window only), once per planet when the pads are known: a scan gate
/// over every inspect pad, a tower in each town that has none.
fn landmarks(mut commands: Commands, planet: Res<PlanetRes>, gp: Res<Gameplay>, town: Res<Town>, assets: Res<AssetServer>, mut done: Local<Option<warp_core::PlanetId>>) {
    if gp.pads_for != Some(planet.id) || *done == Some(planet.id) {
        return;
    }
    *done = Some(planet.id);
    for pad in gp.pads.iter().filter(|p| p.location.as_str().ends_with("_inspect")) {
        let east = pad.up.any_orthonormal_vector();
        let rot = DQuat::from_mat3(&bevy::math::DMat3::from_cols(east, pad.up, -pad.up.cross(east)));
        commands.spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("smuggle/scan_gate.glb"))),
            Transform::from_rotation(rot.as_quat()),
            WorldPos(pad.centre),
            PlanetScene(planet.id),
        ));
    }
    let inspect: Vec<String> = gp.pads.iter().filter(|p| p.location.as_str().ends_with("_inspect")).filter_map(|p| gp.kernel.locations.get(&p.location).map(|l| l.record.place.clone())).collect();
    let inspect: Vec<&str> = inspect.iter().map(String::as_str).collect();
    let others: Vec<(DVec3, f64)> = gp.pads.iter().map(|p| (p.centre, p.radius)).collect();
    for place in tower_places(&inspect, &town.dress) {
        let Some(main) = gp.pads.iter().find(|p| p.location.as_str() == place) else { continue };
        let Some(spot) = tower_spot(main.centre, main.up, &others) else {
            warn!("tower: no clear spot for '{place}' near its main pad");
            continue;
        };
        let up = planet.up(spot);
        let ground = planet.centre + up * planet.surface(up);
        commands.spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("smuggle/tower_small.glb"))),
            Transform::from_rotation(Quat::from_rotation_arc(Vec3::Y, up.as_vec3())),
            WorldPos(ground),
            PlanetScene(planet.id),
        ));
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Town>();
    app.add_systems(Update, update_town.in_set(crate::phases::Frame::World));
}

pub fn window_plugin(app: &mut App) {
    app.init_resource::<TownView>();
    app.add_systems(Update, update_town_view.after(update_town).in_set(crate::phases::Frame::World));
    app.add_systems(Update, order_markers.after(update_town_view).in_set(crate::phases::Frame::World));
    app.add_systems(Update, landmarks.after(update_town).in_set(crate::phases::Frame::World));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::model_fit;

    const DRESS: &str = r#"{
        "place": "bent_spoon",
        "placements": [
            {"model": "x", "folder": "props", "at": [1.0, 2.0, 0.0], "turn_deg": 0.0},
            {"model": "row_slant", "at": [3.0, 4.0, 0.5], "turn_deg": 90.0, "fit": "bar"}
        ],
        "models": {"x": {"min": [0, 0, 0], "max": [1, 1, 1]}},
        "quests": []
    }"#;

    #[test]
    fn parse_plan_reads_place_and_folder() {
        let plan = parse_plan(DRESS).unwrap();
        assert_eq!(plan.place.as_deref(), Some("bent_spoon"));
        assert_eq!(plan.placements[0].folder.as_deref(), Some("props"));
        assert_eq!(plan.placements[1].folder, None);
        assert_eq!(plan.placements[1].fit.as_deref(), Some("bar"));
    }

    #[test]
    fn plan_without_place_or_folder_still_parses() {
        let plan = parse_plan(r#"{"placements": [], "models": {}, "quests": []}"#).unwrap();
        assert_eq!(plan.place, None);
    }

    #[test]
    fn dress_props_map_to_props_folder() {
        let plan = parse_plan(DRESS).unwrap();
        assert_eq!(plan.placements[0].model_path(), "props/x.glb");
    }

    #[test]
    fn placements_without_folder_are_city_models() {
        let plan = parse_plan(DRESS).unwrap();
        assert_eq!(plan.placements[1].model_path(), "city/row_slant.glb");
    }

    #[test]
    fn npc_model_quest_givers_keep_the_marker_and_buyers_get_one_of_three() {
        assert_eq!(npc_model("quest_a", false), "city/npc_marker.glb");
        assert_eq!(npc_model("buyer_x", true), npc_model("buyer_x", true), "a buyer keeps its look");
        let seen: HashSet<&str> = (0..30).map(|i| npc_model(&format!("buyer_{i}"), true)).collect();
        assert_eq!(seen.len(), 3, "all three buyer figures are used");
        assert!(seen.iter().all(|m| m.starts_with("smuggle/buyer_")));
    }

    #[test]
    fn tower_goes_to_inspect_towns_without_a_tower() {
        let plan = |place: &str, model: &str| Plan {
            place: Some(place.into()),
            placements: vec![Placement { model: model.into(), folder: None, at: [0.0; 3], turn_deg: 0.0, fit: None }],
            models: BTreeMap::new(),
            quests: Vec::new(),
        };
        let dress = vec![("bent_spoon".to_string(), plan("bent_spoon", "beacon_spire")), ("noodle_town".to_string(), plan("noodle_town", "pod_tower")), ("gulch_freeport".to_string(), plan("gulch_freeport", "row_tank"))];
        assert_eq!(tower_places(&["bent_spoon", "drip_rock", "noodle_town"], &dress), vec!["drip_rock".to_string()]);
    }

    #[test]
    fn tower_spot_clears_every_pad_or_gives_none() {
        let (main, up) = (DVec3::ZERO, DVec3::Y);
        let spot = tower_spot(main, up, &[(main, 6.0)]).expect("a spot is clear");
        assert!((spot.length() - TOWER_OFF_M).abs() < 1e-9, "25 m from the main pad");
        assert!(spot.distance(main) > 6.0 + PAD_CLEAR_M);
        assert_eq!(tower_spot(main, up, &[(main, 100.0)]), None, "a pad that covers every side leaves no spot");
    }

    #[test]
    fn model_fit_puts_the_box_on_the_crate() {
        let t = model_fit(Vec3::new(2.0, 2.0, 2.0), [0.0, 0.0, 0.0], [1.0, 2.0, 1.0]);
        assert_eq!(t.transform_point(Vec3::ZERO), Vec3::splat(-1.0), "the bottom corner is the crate's low corner");
        assert_eq!(t.transform_point(Vec3::new(1.0, 2.0, 1.0)), Vec3::splat(1.0));
    }
}
