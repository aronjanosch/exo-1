//! The extinguisher wears down while it sprays, and an empty one does not spray. A wall rack gives a
//! full one for the empty one (Barotrauma's bracket); two racks stand in the dev area, one in the
//! corridor and one in the engine room. The host holds the stock (`item_core`, `Host::racks`), the
//! clients get it as events. Values: `content/gameplay/fire/extinguisher.json` (`item_core::supply`).
use crate::dev_area::layout::{CORRIDOR, ROOM};
use crate::dev_area::*;
use crate::items::Items;
use crate::ship::Ship;
use item_core::supply::SupplyConfig;

pub const ITEM: &str = "extinguisher";
pub const TAG: &str = "extinguisher";

/// The extinguisher racks, and every other rack by box too: gear adds its own so the prompt and the
/// swap find them. The drain is the extinguisher's.
#[derive(Resource, Clone, Debug)]
pub struct Supply(pub SupplyConfig);

/// The two wall boxes: the corridor's west end wall (faces east, off the doors and the walk) and the engine room's north wall east of the door (faces south).
pub fn boxes() -> [PlaceholderBox; 2] {
    [
        PlaceholderBox { id: "extinguisher_rack_corridor".into(), zone: CORRIDOR, local_pos: [-11.05, 0.0], size: [0.35, 1.2, 0.5], turn_deg: 90.0, colour: [0.85, 0.12, 0.08], panel: None },
        PlaceholderBox { id: "extinguisher_rack_engine_room".into(), zone: ROOM, local_pos: [3.8, 3.75], size: [0.35, 1.2, 0.5], turn_deg: 0.0, colour: [0.85, 0.12, 0.08], panel: None },
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
    let mine = boxes();
    for r in cfg.0.racks.iter().filter(|r| mine.iter().any(|b| b.id == r.box_id)) {
        let Some(d) = items.defs.get(ITEM).cloned() else { continue };
        let stock = (0..r.stock).map(|_| d.stack_of(1)).collect();
        items.add_rack(now, r.id, TAG, r.slots, stock);
    }
}
