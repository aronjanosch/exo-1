//! The extinguisher's supply: it wears down while it sprays, an empty one does not spray,
//! and a rack on the wall gives a full one for the empty one (Barotrauma's bracket). Two racks in
//! the dev area: one in the corridor outside the engine room, one inside by the door. The host
//! holds the stock (`item_core`, `Host::racks`); the clients get it as events.
//!
//! Research (`~/Work/exo-research`, 15 min):
//! - Barotrauma `barotrauma/content/Items/Tools/tools.xml`, item `extinguisher`: a RepairTool with
//!   `extinguishamount="60"` and an OnUse StatusEffect `Condition="-2.0"`, i.e. -2 condition per
//!   second while it sprays (`barotrauma/source/.../StatusEffects/StatusEffect.cs`, `GetModifiedValue`:
//!   `currValue + newValue * deltaTime`), so a full one lasts 50 s. Its tags are `fireextinguisher`
//!   and `PreferredContainer` `extinguisherholder`, plus `engcab`/`reactorcab` as secondary.
//! - Barotrauma `barotrauma/content/Items/Containers/containers.xml`, item `extinguisherbracket`:
//!   an ItemContainer of capacity 1 with `AutoInteractWithContained` and `Containable items="extinguisher"`.
//!   Barotrauma refills nothing: you hang the empty one and take a full one from the bracket
//!   (`barotrauma/content/ContainerTags.xml`, tag `extinguisherholder`, RecommendedAmount 1).
//! - Barotrauma `barotrauma/source/.../Items/Components/Holdable/RepairTool.cs`: a target item with
//!   Condition <= 0 is not hit (lines 443-447, 691-695); the extinguisher's spray goes through
//!   `ExtinguishAmount` and `hull.FireSources` (line 458 on).
//! - PULSAR `~/Work/exo-research/pulsar/assets/textassets/manual_plain.txt` line 133: repair raises
//!   a system's condition, the extinguisher puts out fires. The metadata
//!   (`pulsar/cpp2il/metadata-dump/Assembly-CSharp.txt`) has `GetNearestAmmoWithSupply`,
//!   `TickGoToNearestAmmoSupply`, `PLAmmoRefill`, `PlayerRequestsAmmoClip`: the refill is a supply
//!   point you walk to and take from, not a refill of the item itself. No per-extinguisher refill
//!   name exists; `FireExtinguisherPS` is only the spray's particle system.
//! So: wear while spraying, empty at 0, swap at a rack. Values in `content/gameplay/fire/extinguisher.json`
//! (`item_core::supply`).
use super::engine_room::{CORRIDOR, ROOM};
use super::*;
use crate::items::Items;
use crate::ship::Ship;
use item_core::supply::SupplyConfig;

/// The item's id and the slot tag its racks take.
pub const ITEM: &str = "extinguisher";
pub const TAG: &str = "extinguisher";

/// The two wall boxes: the corridor's west wall (faces east) and the engine room's east wall.
/// The supply's values and racks, as a resource.
#[derive(Resource, Clone, Debug)]
pub struct Supply(pub SupplyConfig);

pub fn boxes() -> [PlaceholderBox; 2] {
    [
        PlaceholderBox { id: "extinguisher_rack_corridor".into(), zone: CORRIDOR, local_pos: [-0.75, -1.0], size: [0.35, 1.2, 0.5], turn_deg: 90.0, colour: [0.85, 0.12, 0.08], panel: None },
        PlaceholderBox { id: "extinguisher_rack_engine_room".into(), zone: ROOM, local_pos: [4.6, 2.6], size: [0.35, 1.2, 0.5], turn_deg: 270.0, colour: [0.85, 0.12, 0.08], panel: None },
    ]
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let text = files.iter().find(|f| f.path == item_core::supply::FILE).map(|f| f.text.clone()).unwrap_or_default();
    let cfg = SupplyConfig::parse(&text).unwrap_or_else(|e| panic!("extinguisher: {e}"));
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        for b in boxes() {
            devices.add(b);
        }
    }
    app.insert_resource(Supply(cfg));
    app.add_systems(FixedUpdate, put_racks.in_set(crate::phases::Fx::Effects));
}

/// Once, when the area and the ship are there: each rack gets its stock (host only; the clients
/// hear it from the host).
fn put_racks(time: Res<Time>, area: Res<DevArea>, cfg: Res<Supply>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() || area.frame.is_none() {
        return;
    }
    *done = true;
    let now = time.elapsed_secs_f64();
    for r in &cfg.0.racks {
        let Some(d) = items.defs.get(ITEM).cloned() else { return };
        let stock = (0..r.stock).map(|_| d.stack_of(1)).collect();
        items.add_rack(now, r.id, TAG, r.slots, stock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_values_file_has_a_rack_for_every_box() {
        let text = std::fs::read_to_string(format!("{}/../../content/gameplay/fire/extinguisher.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let c = SupplyConfig::parse(&text).expect("the file is valid");
        for b in boxes() {
            assert!(c.rack_in(&b.id).is_some(), "box {} has a rack in the file", b.id);
        }
        assert_eq!(c.racks.len(), boxes().len());
    }
}
