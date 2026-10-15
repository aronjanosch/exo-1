//! Storage in the dev area: cabinets in the STORAGE zone, each a container on the host's map
//! (item_core racks, so every move goes through the authority). A panel lists the stacks inside and
//! in the player's bar, with take and store per stack and for all, pages, and the last line. The
//! rules are `item_core::container` (Barotrauma's containers: stack caps, tag filters, locks); a
//! cabinet's slot rules are its own (a mask cabinet takes masks and suits, an extinguisher cabinet
//! takes extinguishers), and the supply crate is the shared preset.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::items::Items;
use crate::ship::Ship;
use display_core::{bar, button, column, label, row, space, title, Panel, Tone};
use item_core::container::{presets, Access, Container, SlotRule, Stack};
use item_core::inventory::{Inventory, BAR_SLOTS};
use item_core::{ItemDefs, Reject, Request, RackId};

pub const MASKS: RackId = 101;
pub const EXTINGUISHERS: RackId = 102;
pub const CRATE: RackId = 103;

pub const MASKS_ID: PanelId = PanelId::new("store_masks");
pub const EXTINGUISHERS_ID: PanelId = PanelId::new("store_extinguishers");
pub const CRATE_ID: PanelId = PanelId::new("store_crate");

/// Panel size in panel pixels (1 m wide).
pub const PANEL_W: f64 = 560.0;
pub const PANEL_H: f64 = 660.0;
/// Rows of stacks shown per side.
pub const ROWS: usize = 5;
/// The row of cabinets, metres north of the storage zone's centre (they face south). TODO(initiator):
/// the places.
pub const ROW_Y: f64 = -3.0;

/// A cabinet: its rack, its place and look, its panel's name and the page and last line of its panel.
#[derive(Debug, Clone)]
pub struct Cabinet {
    pub id: PanelId,
    pub rack: RackId,
    pub name: &'static str,
    pub x: f64,
    pub colour: [f32; 3],
    pub size: [f64; 3],
    pub line: (String, Tone),
    /// The page of stacks shown (`ROWS` per page).
    pub page: usize,
}

#[derive(Resource, Debug, Clone)]
pub struct Storage {
    pub cabinets: Vec<Cabinet>,
}

impl Storage {
    pub fn get_mut(&mut self, id: &PanelId) -> Option<&mut Cabinet> {
        self.cabinets.iter_mut().find(|c| &c.id == id)
    }
}

/// The cabinets as they stand: a tall one for masks and extinguishers, a low crate.
pub fn cabinets() -> Vec<Cabinet> {
    let cab = |id: PanelId, rack, name, x, colour, size| Cabinet { id, rack, name, x, colour, size, line: Default::default(), page: 0 };
    vec![
        cab(MASKS_ID, MASKS, "Mask cabinet", -9.0, [0.45, 0.50, 0.55], [1.0, 2.0, 0.5]),
        cab(EXTINGUISHERS_ID, EXTINGUISHERS, "Extinguisher cabinet", -5.0, [0.85, 0.12, 0.08], [1.0, 2.0, 0.5]),
        cab(CRATE_ID, CRATE, "Supply crate", -1.0, [0.55, 0.60, 0.35], [1.2, 0.8, 0.8]),
    ]
}

/// A cabinet's slot rules and its stock. TODO(initiator): the slot counts, the caps and what is
/// stocked.
pub fn contents(rack: RackId, defs: &ItemDefs) -> Container {
    let mut c: Container = match rack {
        MASKS => Container::new(vec![SlotRule::tagged(1, &["mask", "suit"], &[]); 6], Access::Open),
        EXTINGUISHERS => Container::new(vec![SlotRule::tagged(1, &["extinguisher"], &[]); 4], Access::Open),
        _ => presets::supply_crate(),
    };
    match rack {
        MASKS => {
            put(&mut c, defs, "o2_mask", 3);
            put(&mut c, defs, "suit", 2);
        }
        EXTINGUISHERS => put(&mut c, defs, "extinguisher", 2),
        _ => {
            put(&mut c, defs, "fuel_rod", 1);
            put(&mut c, defs, "bandage", 4);
            put(&mut c, defs, "oxy", 2);
        }
    }
    c
}

/// Puts `n` of an item into a container; the stock fits by construction.
fn put(c: &mut Container, defs: &ItemDefs, id: &str, n: u32) {
    let d = defs.get(id).unwrap_or_else(|| panic!("storage: no item '{id}'"));
    let left = c.try_insert(&[], d.stack_of(n));
    assert!(matches!(left, Ok(None)), "storage: the stock of {id} fits");
}

pub fn plugin(app: &mut App) {
    let cabs = cabinets();
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        for c in &cabs {
            let h = PANEL_H / PANEL_W;
            // A tall cabinet carries the panel on its front; the low crate above it, leaning back.
            let (at, tilt) = if c.size[1] > 1.5 { ([0.0, 1.4, c.size[2] * 0.5 + 0.02], 0.0) } else { ([0.0, c.size[1] + 0.5 * h, 0.2], 15.0) };
            devices.add(PlaceholderBox {
                id: c.id.to_string(),
                zone: STORAGE,
                local_pos: [c.x, ROW_Y],
                size: c.size,
                turn_deg: 0.0,
                colour: c.colour,
                panel: Some(PanelSpec { id: c.id.clone(), name: c.name.into(), size_m: [1.0, h], at, tilt_deg: tilt }),
            });
        }
    }
    app.insert_resource(Storage { cabinets: cabs });
    app.add_systems(FixedUpdate, put_cabinets.in_set(crate::phases::Fx::Effects));
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

/// Once, when the ship is there: each cabinet gets its stock (host only; the clients hear it from
/// the host).
fn put_cabinets(time: Res<Time>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    *done = true;
    let now = time.elapsed_secs_f64();
    for rack in [MASKS, EXTINGUISHERS, CRATE] {
        let stock = contents(rack, &items.defs);
        items.add_container(now, rack, stock);
    }
}

/// A refusal in words.
pub fn refusal_text(r: &Reject) -> String {
    match r {
        Reject::Locked => "locked".into(),
        Reject::NotAllowed | Reject::NotCollectable => "does not take that".into(),
        Reject::Full => "no room".into(),
        Reject::EmptySlot | Reject::NoSuchSlot => "nothing there".into(),
        Reject::NoRack => "no such cabinet".into(),
        Reject::Gone => "gone".into(),
        _ => "refused".into(),
    }
}

/// The page shown: the one asked for, within what there is.
pub fn shown_page(cab: &Cabinet, inside: usize) -> usize {
    cab.page.min(inside.div_ceil(ROWS).max(1) - 1)
}

fn occupied(c: &Container) -> Vec<usize> {
    (0..c.len()).filter(|&i| c.slot(i).is_some()).collect()
}

/// Moves stacks: `take:<slot>`, `store:<bar slot>`, `take_all`, `store_all`; pages with `prev` and
/// `next`. Each move goes to the host through the items' request path (shown at once here).
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut storage: ResMut<Storage>, mut items: ResMut<Items>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for c in clicks.read() {
        let Some(cab) = storage.get_mut(&c.panel) else { continue };
        let rack = cab.rack;
        let inside = items.client.racks.get(&rack).map(occupied).unwrap_or_default();
        let (verb, arg) = c.action.split_once(':').unwrap_or((c.action.as_str(), ""));
        let slot = arg.parse::<usize>().ok();
        match verb {
            "prev" => cab.page = shown_page(cab, inside.len()).saturating_sub(1),
            "next" => cab.page = shown_page(cab, inside.len()) + 1,
            _ => {}
        }
        let result: Result<u32, Reject> = match (verb, slot) {
            ("take", Some(i)) => items.request(now, Request::Fetch { rack, rack_slot: i }).map(|_| 1),
            ("store", Some(i)) => items.request(now, Request::Store { slot: i, rack }).map(|_| 1),
            ("take_all", _) => {
                let reqs: Vec<Request> = inside.iter().map(|&i| Request::Fetch { rack, rack_slot: i }).collect();
                all(&mut items, now, reqs)
            }
            ("store_all", _) => {
                let reqs: Vec<Request> = (0..BAR_SLOTS).filter(|&i| items.client.view.slot(i).is_some()).map(|slot| Request::Store { slot, rack }).collect();
                all(&mut items, now, reqs)
            }
            _ => continue,
        };
        cab.line = match result {
            Ok(n) => (format!("{n} moved"), Tone::Normal),
            Err(r) => (format!("refused: {}", refusal_text(&r)), Tone::Warn),
        };
        info!("storage {}: {} -> {:?}", c.panel, c.action, cab.line.0);
    }
}

/// Sends every request it can; the first refusal if nothing went.
fn all(items: &mut Items, now: f64, reqs: Vec<Request>) -> Result<u32, Reject> {
    let (mut moved, mut first) = (0, None);
    for req in reqs {
        match items.request(now, req) {
            Ok(_) => moved += 1,
            Err(r) => {
                first.get_or_insert(r);
            }
        }
    }
    match (moved, first) {
        (0, Some(r)) => Err(r),
        (n, _) => Ok(n),
    }
}

/// A cabinet's panel: its lock, its fill, its stacks by page, the bar's stacks, the last line.
pub fn cabinet_panel(cab: &Cabinet, contents: Option<&Container>, bar_inv: &Inventory, names: &dyn Fn(&Stack) -> String) -> Panel {
    let empty = Container::new(vec![], Access::Open);
    let c = contents.unwrap_or(&empty);
    let lock = match c.access() {
        Access::Open => ("open".to_string(), Tone::Dim),
        _ => ("locked".to_string(), Tone::Bad),
    };
    let cap = c.capacity();
    let mut items = vec![
        title(cab.name.to_uppercase()).fixed(36.0),
        label(lock.0).tone(lock.1).fixed(22.0),
        bar(cap.used_slots as f64 / cap.slots.max(1) as f64, format!("{} / {} SLOTS  {} ITEMS", cap.used_slots, cap.slots, cap.items)).thresholds(Some(0.8), Some(0.95)).fixed(28.0),
    ];
    let inside = occupied(c);
    let pages = inside.len().div_ceil(ROWS).max(1);
    let page = shown_page(cab, inside.len());
    items.push(
        row(8.0, vec![
            label(format!("INSIDE  page {}/{pages}", page + 1)).tone(Tone::Dim),
            button("<", "prev").enabled(page > 0).fixed(50.0),
            button(">", "next").enabled(page + 1 < pages).fixed(50.0),
        ])
        .fixed(26.0),
    );
    if inside.is_empty() {
        items.push(label("empty").tone(Tone::Dim).fixed(28.0));
    }
    for &i in inside.iter().skip(page * ROWS).take(ROWS) {
        let s = c.slot(i).unwrap();
        items.push(row(8.0, vec![label(format!("{} x{}", names(s), s.count)), button("take", format!("take:{i}")).fixed(90.0)]).fixed(28.0));
    }
    items.push(label("YOUR BAR").tone(Tone::Dim).fixed(20.0));
    let held: Vec<usize> = (0..BAR_SLOTS).filter(|&i| bar_inv.slot(i).is_some()).collect();
    if held.is_empty() {
        items.push(label("empty").tone(Tone::Dim).fixed(28.0));
    }
    for &i in held.iter().take(ROWS) {
        let s = bar_inv.slot(i).unwrap();
        items.push(row(8.0, vec![label(format!("{} x{}", names(s), s.count)), button("store", format!("store:{i}")).fixed(90.0)]).fixed(28.0));
    }
    items.push(row(8.0, vec![button("take all", "take_all"), button("store all", "store_all")]).fixed(36.0));
    items.push(label(cab.line.0.clone()).tone(cab.line.1).fixed(22.0));
    items.push(space());
    Panel::new(PANEL_W, PANEL_H, column(5.0, items))
}

pub fn write_panels(storage: Res<Storage>, items: Res<Items>, mut panels: ResMut<Panels>) {
    let names = |s: &Stack| items.name(s);
    for c in &storage.cabinets {
        panels.set(c.id.clone(), cabinet_panel(c, items.client.racks.get(&c.rack), &items.client.view, &names));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> ItemDefs {
        crate::items::load_defs()
    }

    #[test]
    fn every_cabinet_panel_fits_full_and_holds_its_stock() {
        let d = defs();
        let names = |s: &Stack| s.item.kind.to_string();
        // A full bar: every slot holds an extinguisher (one per slot, it does not stack).
        let mut bar_inv = Inventory::default();
        for _ in 0..BAR_SLOTS {
            let _ = bar_inv.bar.try_insert(&[], d.get("extinguisher").unwrap().stack_of(1));
        }
        for c in cabinets() {
            let stock = contents(c.rack, &d);
            let p = cabinet_panel(&c, Some(&stock), &bar_inv, &names);
            for w in p.layout() {
                assert!(w.rect.max().y <= p.size.y - p.padding + 1e-9, "{}: {:?} spills out", c.id, w.widget);
            }
        }
    }

    #[test]
    fn the_stock_is_where_the_cabinets_say() {
        let d = defs();
        assert_eq!(contents(MASKS, &d).count(&item_core::container::ItemKind::new("o2_mask")), 3);
        assert_eq!(contents(EXTINGUISHERS, &d).count(&item_core::container::ItemKind::new("extinguisher")), 2);
        assert_eq!(contents(CRATE, &d).count(&item_core::container::ItemKind::new("fuel_rod")), 1);
        assert!(!contents(MASKS, &d).rule(0).accepts(&d.get("wrench").unwrap().item()), "a mask cabinet takes no wrench");
    }
}
