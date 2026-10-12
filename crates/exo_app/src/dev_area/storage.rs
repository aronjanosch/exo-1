//! Storage in the dev area (#287): an ingredient shelf, a medical cabinet, a locker, a secure locker
//! that needs an ID card, and a supply crate, as boxes with a panel in the STORAGE zone. A panel
//! lists the stacks inside and in the player's `Hands`, with take and store per stack and for all,
//! the fill as a bar, the lock state and the last refusal. The rules are `container_core`.
use super::science::{first_aid_item, hands_line, ingredient_item, item_label, key_item, syringe_item, Hands, LabItem, ScienceLab};
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::gameplay::Gameplay;
use container_core::{move_between, presets, Access, Container, Item, Refusal, SlotRule, Stack};
use display_core::{bar, button, column, label, row, space, title, Panel, Tone};
use gameplay_core::{CommodityId, EffectId, ProductSpec};
use production_core::devices::Syringe;

pub const SHELF: PanelId = PanelId::new("store_shelf");
pub const MEDICAL: PanelId = PanelId::new("store_medical");
pub const LOCKER: PanelId = PanelId::new("store_locker");
pub const SECURE: PanelId = PanelId::new("store_secure");
pub const CRATE: PanelId = PanelId::new("store_crate");

/// Panel size in panel pixels (1 m wide).
pub const PANEL_W: f64 = 560.0;
pub const PANEL_H: f64 = 660.0;
/// Rows of stacks shown per side.
pub const ROWS: usize = 5;
/// The row of boxes, metres north of the storage zone's centre (they face south).
pub const ROW_Y: f64 = -4.0;

/// A cabinet: its panel, its name, the container, the last line.
#[derive(Debug, Clone)]
pub struct Cabinet {
    pub id: PanelId,
    pub name: &'static str,
    pub x: f64,
    pub colour: [f32; 3],
    pub size: [f64; 3],
    pub container: Container<LabItem>,
    pub line: (String, Tone),
    /// The page of stacks shown (`ROWS` per page).
    pub page: usize,
}

#[derive(Resource, Debug, Clone)]
pub struct Storage {
    pub cabinets: Vec<Cabinet>,
}

impl Storage {
    pub fn get(&self, id: &PanelId) -> Option<&Cabinet> {
        self.cabinets.iter().find(|c| &c.id == id)
    }
    pub fn get_mut(&mut self, id: &PanelId) -> Option<&mut Cabinet> {
        self.cabinets.iter_mut().find(|c| &c.id == id)
    }
}

/// The ingredient shelf: 12 slots that take only lab goods. TODO(initiator): its size.
pub fn ingredient_shelf() -> Container<LabItem> {
    Container::new(vec![SlotRule::tagged(64, &["ingredient"], &[]); 12], Access::Open)
}

/// A stock syringe (for the cabinets at the start). TODO(initiator): what is stocked.
fn stock_syringe(base: &str, effects: &[&str], quality: f64) -> Item<LabItem> {
    syringe_item(Syringe { product: ProductSpec { base: CommodityId::new(base), effects: effects.iter().map(|e| EffectId::new(*e)).collect(), quality }, doses: 3 })
}

fn put(c: &mut Container<LabItem>, item: Item<LabItem>, n: u32) {
    let key = [container_core::Tag::new(presets::ID_CARD)];
    let left = c.try_insert(&key, Stack::new(item, n));
    assert!(matches!(left, Ok(None)), "dev area storage: the stock fits");
}

/// The cabinets with their stock: every lab good on the shelf, syringes in the medical cabinet,
/// the ID card in the locker, a better syringe behind the key, spare goods in the crate.
pub fn stocked(lab: &ScienceLab) -> Storage {
    let mut shelf = ingredient_shelf();
    for id in lab.lc.bases.keys().chain(lab.lc.ingredients.keys()) {
        put(&mut shelf, ingredient_item(id), 6);
    }
    let mut medical: Container<LabItem> = presets::medical_cabinet();
    put(&mut medical, stock_syringe("dream_goo", &["sleepy"], 0.5), 2);
    put(&mut medical, first_aid_item("bandage"), 6);
    put(&mut medical, first_aid_item("medkit"), 2);
    let mut locker: Container<LabItem> = presets::locker();
    put(&mut locker, key_item(), 1);
    let mut secure: Container<LabItem> = presets::secure_locker();
    put(&mut secure, stock_syringe("blorp_paste", &["glowing", "giggly"], 0.9), 1);
    let mut crate_: Container<LabItem> = presets::supply_crate();
    for id in lab.lc.ingredients.keys().take(2) {
        put(&mut crate_, ingredient_item(id), 10);
    }
    let cab = |id, name, x, colour, size, container| Cabinet { id, name, x, colour, size, container, line: Default::default(), page: 0 };
    Storage {
        cabinets: vec![
            cab(SHELF, "Ingredient shelf", -30.0, [0.55, 0.45, 0.30], [1.6, 2.0, 0.5], shelf),
            cab(MEDICAL, "Medical cabinet", -26.0, [0.85, 0.88, 0.90], [1.2, 2.0, 0.5], medical),
            cab(LOCKER, "Locker", -22.0, [0.45, 0.50, 0.55], [1.0, 2.0, 0.5], locker),
            cab(SECURE, "Secure locker", -18.0, [0.30, 0.32, 0.38], [1.0, 2.0, 0.5], secure),
            cab(CRATE, "Supply crate", -14.0, [0.55, 0.60, 0.35], [1.2, 0.8, 0.8], crate_),
        ],
    }
}

/// Where a cabinet stands in the storage zone (for scenarios).
pub fn cabinet_pos(s: &Storage, id: &PanelId) -> Option<[f64; 2]> {
    s.get(id).map(|c| [c.x, ROW_Y])
}

pub fn plugin(app: &mut App) {
    let storage = stocked(app.world().resource::<ScienceLab>());
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        for c in &storage.cabinets {
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
    app.insert_resource(storage);
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().after(super::science::write_panels).in_set(crate::phases::Fx::Effects));
}

/// A refusal in words.
pub fn refusal_text(r: &Refusal) -> String {
    match r {
        Refusal::Locked => "locked".into(),
        Refusal::NeedsKey(k) => format!("needs an {}", k.as_str().replace('_', " ")),
        Refusal::NotAllowed => "does not take that".into(),
        Refusal::Full => "no room".into(),
        Refusal::NoSuchSlot | Refusal::EmptySlot => "nothing there".into(),
        Refusal::NothingToMove => "nothing to move".into(),
    }
}

/// Moves stacks: `take:<slot>`, `store:<hands slot>`, `take_all`, `store_all`; pages with
/// `prev` and `next`.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut storage: ResMut<Storage>, mut hands: ResMut<Hands>) {
    for c in clicks.read() {
        let Some(cab) = storage.get_mut(&c.panel) else { continue };
        let carried = hands.carried();
        let (verb, arg) = c.action.split_once(':').unwrap_or((c.action.as_str(), ""));
        let slot = arg.parse::<usize>().ok();
        match verb {
            "prev" => cab.page = shown_page(cab).saturating_sub(1),
            "next" => cab.page = shown_page(cab) + 1,
            _ => {}
        }
        let result: Result<u32, Refusal> = match (verb, slot) {
            ("take", Some(i)) => move_between(&mut cab.container, &mut hands.0, &carried, i, u32::MAX),
            ("store", Some(i)) => move_between(&mut hands.0, &mut cab.container, &carried, i, u32::MAX),
            ("take_all", _) => all(&mut cab.container, &mut hands.0, &carried),
            ("store_all", _) => all(&mut hands.0, &mut cab.container, &carried),
            _ => continue,
        };
        cab.line = match result {
            Ok(n) => (format!("{n} moved"), Tone::Normal),
            Err(r) => (format!("refused: {}", refusal_text(&r)), Tone::Warn),
        };
        info!("storage {}: {} -> {:?}", cab.id, c.action, cab.line.0);
    }
}

/// Moves every stack it can; the first refusal if nothing moved.
fn all(from: &mut Container<LabItem>, to: &mut Container<LabItem>, carried: &[container_core::Tag]) -> Result<u32, Refusal> {
    let (mut moved, mut first) = (0, None);
    for i in 0..from.len() {
        if from.slot(i).is_none() {
            continue;
        }
        match move_between(from, to, carried, i, u32::MAX) {
            Ok(n) => moved += n,
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

/// The page shown: the one asked for, within what there is.
pub fn shown_page(cab: &Cabinet) -> usize {
    let n = (0..cab.container.len()).filter(|&i| cab.container.slot(i).is_some()).count();
    cab.page.min(n.div_ceil(ROWS).max(1) - 1)
}

/// A cabinet's panel.
pub fn cabinet_panel(cab: &Cabinet, hands: &Hands, names: &dyn Fn(&CommodityId) -> String) -> Panel {
    let carried = hands.carried();
    let open = cab.container.check_access(&carried);
    let lock = match (cab.container.access(), &open) {
        (Access::Open, _) => ("open".to_string(), Tone::Dim),
        (_, Ok(())) => ("unlocked with your ID card".into(), Tone::Good),
        (_, Err(r)) => (format!("LOCKED: {}", refusal_text(r)), Tone::Bad),
    };
    let cap = cab.container.capacity();
    let mut items = vec![
        title(cab.name.to_uppercase()).fixed(36.0),
        label(lock.0).tone(lock.1).fixed(22.0),
        bar(cap.used_slots as f64 / cap.slots.max(1) as f64, format!("{} / {} SLOTS  {} ITEMS", cap.used_slots, cap.slots, cap.items)).thresholds(Some(0.8), Some(0.95)).fixed(28.0),
    ];
    let inside: Vec<usize> = (0..cab.container.len()).filter(|&i| cab.container.slot(i).is_some()).collect();
    let pages = inside.len().div_ceil(ROWS).max(1);
    let page = shown_page(cab);
    items.push(
        row(8.0, vec![
            label(if open.is_ok() { format!("INSIDE  page {}/{pages}", page + 1) } else { "INSIDE".into() }).tone(Tone::Dim),
            button("<", "prev").enabled(open.is_ok() && page > 0).fixed(50.0),
            button(">", "next").enabled(open.is_ok() && page + 1 < pages).fixed(50.0),
        ])
        .fixed(26.0),
    );
    if open.is_err() {
        items.push(label("contents hidden").tone(Tone::Dim).fixed(28.0));
    } else if inside.is_empty() {
        items.push(label("empty").tone(Tone::Dim).fixed(28.0));
    }
    for &i in inside.iter().skip(page * ROWS).take(if open.is_ok() { ROWS } else { 0 }) {
        let s = cab.container.slot(i).unwrap();
        items.push(row(8.0, vec![label(format!("{} x{}", item_label(&s.item, names), s.count)), button("take", format!("take:{i}")).fixed(90.0)]).fixed(28.0));
    }
    items.push(label("YOUR HANDS").tone(Tone::Dim).fixed(20.0));
    let held: Vec<usize> = (0..hands.0.len()).filter(|&i| hands.0.slot(i).is_some()).collect();
    if held.is_empty() {
        items.push(label("empty").tone(Tone::Dim).fixed(28.0));
    }
    for &i in held.iter().take(ROWS) {
        let s = hands.0.slot(i).unwrap();
        items.push(row(8.0, vec![label(format!("{} x{}", item_label(&s.item, names), s.count)), button("store", format!("store:{i}")).fixed(90.0)]).fixed(28.0));
    }
    items.push(row(8.0, vec![button("take all", "take_all"), button("store all", "store_all")]).fixed(36.0));
    items.push(label(cab.line.0.clone()).tone(cab.line.1).fixed(22.0));
    items.push(label(hands_line(hands)).tone(Tone::Dim).fixed(20.0));
    items.push(space());
    Panel::new(PANEL_W, PANEL_H, column(5.0, items))
}

pub fn write_panels(storage: Res<Storage>, hands: Res<Hands>, gp: Res<Gameplay>, mut panels: ResMut<Panels>) {
    let names = |id: &CommodityId| super::science::commodity_name(&gp, id);
    for c in &storage.cabinets {
        panels.set(c.id.clone(), cabinet_panel(c, &hands, &names));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab() -> ScienceLab {
        let files = crate::content_files("gameplay");
        let kernel = gameplay_core::Content::load(&files, &["small", "medium", "large"]).unwrap();
        ScienceLab::new(production_core::LabContent::load(&files, &kernel).unwrap(), production_core::DeviceTuning::load(&files, &kernel).unwrap())
    }

    #[test]
    fn every_cabinet_panel_fits_full_and_the_lock_hides_the_contents() {
        let s = stocked(&lab());
        let mut hands = Hands::default();
        let goods = s.get(&SHELF).unwrap().container.slot(0).unwrap().item.clone();
        for _ in 0..super::super::science::HAND_SLOTS {
            hands.0.try_insert(&[], Stack::new(goods.clone(), 64)).ok();
        }
        let names = |id: &CommodityId| id.to_string();
        for c in &s.cabinets {
            let p = cabinet_panel(c, &hands, &names);
            for w in p.layout() {
                assert!(w.rect.max().y <= p.size.y - p.padding + 1e-9, "{}: {:?} spills out", c.id, w.widget);
            }
        }
        let secure = cabinet_panel(s.get(&SECURE).unwrap(), &Hands::default(), &names);
        assert!(secure.find("take:0").is_none(), "locked: no take buttons");
        assert!(secure.layout().iter().any(|w| matches!(w.widget, display_core::Widget::Label { text, .. } if text.starts_with("LOCKED"))));
        let mut keyed = Hands::default();
        keyed.give(key_item());
        let secure = cabinet_panel(s.get(&SECURE).unwrap(), &keyed, &names);
        assert!(secure.find("take:0").is_some(), "with the card it opens");
    }
}
