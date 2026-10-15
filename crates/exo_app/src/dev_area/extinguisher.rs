//! So: wear while spraying, empty at 0, swap at a rack. Values in `content/gameplay/fire/extinguisher.json`
//! (`item_core::supply`).
use super::engine_room::{AIRLOCK, CORRIDOR, ROOM};
use super::*;
use crate::items::Items;
use crate::ship::Ship;
use item_core::supply::SupplyConfig;

pub const ITEM: &str = "extinguisher";
pub const TAG: &str = "extinguisher";

/// The two wall boxes: the corridor's west wall (faces east) and the engine room's east wall.
#[derive(Resource, Clone, Debug)]
pub struct Supply(pub SupplyConfig);

/// The wall boxes, and the airlock's suit rack (r40, holding space suits since r41; its rack is in
/// `extinguisher.json` with its own item, tag and name).
pub fn boxes() -> [PlaceholderBox; 3] {
    [
        PlaceholderBox { id: "extinguisher_rack_corridor".into(), zone: CORRIDOR, local_pos: [-0.75, -1.0], size: [0.35, 1.2, 0.5], turn_deg: 90.0, colour: [0.85, 0.12, 0.08], panel: None },
        PlaceholderBox { id: "extinguisher_rack_engine_room".into(), zone: ROOM, local_pos: [4.6, 2.6], size: [0.35, 1.2, 0.5], turn_deg: 270.0, colour: [0.85, 0.12, 0.08], panel: None },
        // The airlock's east wall (inner face x -13.0), facing west into the room.
        PlaceholderBox { id: "airlock_suit_rack".into(), zone: AIRLOCK, local_pos: [0.8, 0.0], size: [0.35, 1.2, 0.5], turn_deg: 270.0, colour: [0.25, 0.3, 0.36], panel: None },
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
        let Some(d) = items.defs.get(r.item.as_deref().unwrap_or(ITEM)).cloned() else { continue };
        let stock = (0..r.stock).map(|_| d.stack_of(1)).collect();
        items.add_rack(now, r.id, r.tag.as_deref().unwrap_or(TAG), r.slots, stock);
    }
}
