//! The round-6 Bevy systems: stock crates (ingredients and products), the airspace the ship is
//! in, hailing, scan answers and confiscation, the kettles, handing products to buyers, the phone,
//! and filling `SmuggleHud`. All of it runs in the fixed step, headless too.
use super::systems::{SpawnStock, HIDDEN_HOLD_CRATES, LADDER, TAG_HIDDEN_HOLD, TAG_KETTLE_2};
use super::{BrewView, PhoneTab, SmuggleHud};
use crate::cargo::{crate_bundle, crate_world, Crate, Crates};
use crate::controls::{Actions, Tap};
use crate::feedback::{Anchor, Cue, Tone};
use crate::gameplay::{Gameplay, HOST};
use crate::grab::Grab;
use crate::interact::Interaction;
use crate::town::Town;
use crate::walker::Player;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::{CommodityId, LocationId, OrderId, ProductSpec, ScanChoice, Tag, TextKey, TownId, WorldEvent};
use walker_core::Frame;

/// A crate of the round-6 economy: an ingredient, a product base, or a made product.
#[derive(Component, Clone, Debug)]
pub struct Stock {
    pub commodity: CommodityId,
    pub product: Option<ProductSpec>,
}

/// How close the walker must stand to a kettle or a buyer to use them (m). TODO(initiator).
pub const STATION_RANGE: f64 = 2.4;
/// A buyer takes product crates lying this close to them (m).
pub const HANDOVER_RANGE: f64 = 8.0;
/// A counter-offer asks this much more (%). TODO(initiator): let the player pick.
pub const COUNTER_PCT: u32 = 20;
/// What paying the fixer costs per press (money). TODO(initiator).
pub const FIXER_STEP: i64 = 100;

/// What the walker can use right now besides crates and seats (it wins over `interact`).
#[derive(Clone, Debug, PartialEq)]
pub enum Station {
    Kettle(String),
    Buyer { order: OrderId, by: String },
}

#[derive(Resource, Default, Debug)]
pub struct StationTarget {
    pub target: Option<Station>,
    pub prompt: String,
}

/// When each kettle's brew finished (game clock), for the needle.
#[derive(Resource, Default, Debug)]
pub struct KettleClock(pub std::collections::BTreeMap<String, f64>);

/// Within `range` on the ground (horizontal) and at most 3 m above or below: stations stand on
/// uneven ground.
pub fn in_reach(a: DVec3, b: DVec3, up: DVec3, range: f64) -> bool {
    let d = b - a;
    let v = d.dot(up);
    (d - up * v).length() < range && v.abs() < 3.0
}

/// The kettles: in the city's back room beside the dealer (the plan's quest spot), a second one
/// once bought. Station id, world position on the ground, up.
pub fn kettles(town: &Town, gp: &Gameplay) -> Vec<(String, DVec3, DVec3)> {
    let Some(n) = town.npcs.iter().find(|n| n.id == "back_yard_dealer") else { return Vec::new() };
    let side = n.up.any_orthonormal_vector();
    let mut v = vec![("kettle_1".to_string(), n.pos + side * 2.5, n.up)];
    if gp.progress.has_tag(&Tag::new(TAG_KETTLE_2)) {
        v.push(("kettle_2".to_string(), n.pos + side * 5.0, n.up));
    }
    v
}

/// Where a buyer stands: at the edge of their handover pad, buyers of one pad side by side.
pub fn buyer_spot(gp: &Gameplay, at: &LocationId, index: usize) -> Option<(DVec3, DVec3)> {
    let pad = gp.pad_of(at)?;
    let east = pad.up.any_orthonormal_vector();
    let north = pad.up.cross(east);
    Some((pad.centre + north * (pad.radius + 1.5) + east * (index as f64 * 2.0), pad.up))
}

/// Buyers with taste, visible (not hidden), with their spot.
fn buyers(gp: &Gameplay) -> Vec<(String, LocationId)> {
    let mut v: Vec<(String, LocationId)> = gp
        .customer_content
        .customers
        .values()
        .filter_map(|c| {
            let r = &c.record;
            let at = r.at.clone()?;
            r.taste.as_ref().map(|_| (r.id.as_str().to_string(), at))
        })
        .filter(|(id, _)| !gp.customers.is_hidden(&customers_core::CustomerId::new(id)))
        .collect();
    v.sort();
    v
}

/// The town whose airspace a world point is in (the nearest whose radius it is inside).
pub fn airspace_at(gp: &Gameplay, p: DVec3) -> Option<TownId> {
    let mut best: Option<(f64, TownId)> = None;
    for t in gp.smuggling.traffic_c.towns.values().map(|t| &t.record) {
        let Some(centre) = t.pads.first().and_then(|l| gp.pad_of(l)).map(|p| p.centre) else { continue };
        let d = centre.distance(p);
        if d < t.airspace_m && best.as_ref().is_none_or(|(b, _)| d < *b) {
            best = Some((d, t.id.clone()));
        }
    }
    best.map(|(_, t)| t)
}

/// Airspace and speed to traffic, contraband aboard to the scans, hail and scan answers.
#[allow(clippy::too_many_arguments)]
pub fn smuggle_observe(
    time: Res<Time>,
    mut actions: ResMut<Actions>,
    mut gp: ResMut<Gameplay>,
    ships: Query<(Entity, &Position, &LinearVelocity), With<crate::ship::Ship>>,
    stock: Query<(&Stock, &Crate)>,
) {
    let dt = time.delta_secs_f64();
    let Some((ship_e, pos, vel)) = ships.iter().next() else { return };
    let inside = airspace_at(&gp, pos.0);
    let g = &mut *gp;
    let outs = g.smuggling.traffic.observe(&g.smuggling.traffic_c, HOST, inside.clone(), vel.0.length(), dt);
    for o in outs {
        match o {
            traffic_core::Outcome::Emit(e) => g.push_world(HOST, e),
            traffic_core::Outcome::Notice(n) => g.notify(n),
            traffic_core::Outcome::Say { town, key } => {
                let who = g.smuggling.town(&town).map(|t| t.name.clone()).unwrap_or_else(|| TextKey::new(town.as_str()));
                g.say(&who, key.as_str());
            }
        }
    }
    // 20 s before a clearance runs out, the tower reminds once (Star Citizen's ATC warns too).
    if let Some(c) = g.smuggling.traffic.clearance(HOST).cloned() {
        let key = (c.town.clone(), c.pad.clone());
        if c.time_left_s <= 20.0 && g.smuggling.reminded.as_ref() != Some(&key) {
            g.smuggling.reminded = Some(key);
            g.notify(gameplay_core::notice::Notice::new(gameplay_core::notice::NoticeKind::Warning, "notice.traffic.reminder"));
        }
    }
    // Air patrols (hot ships in a town's airspace): halt for a scan in the air, or run.
    let official = inside.as_ref().and_then(|t| g.smuggling.town(t)).filter(|t| !t.free_port).map(|t| (t.id.clone(), t.inspection_pad.clone().unwrap_or_else(|| t.pads[0].clone())));
    let stars = g.smuggling.heat.stars();
    let pouts = g.smuggling.patrol.step(&g.smuggling.heat_c.tuning.record, stars, official.as_ref().map(|(_, pad)| pad.clone()), vel.0.length(), dt);
    for o in pouts {
        match o {
            heat_core::PatrolOut::Ordered { .. } => {
                g.smuggling.log.push("patrol: halt ordered".into());
                g.say(&TextKey::new("patrol.name"), "patrol.ordered");
                let (at, text) = (g.clock(), g.line("stamp.halt"));
                g.feedback.push(at, Cue::Stamp { text, tone: Tone::Bad, hold: 0.0 });
                g.feedback.push(at, Cue::Kick(0.2));
            }
            heat_core::PatrolOut::Halted { at } => {
                g.smuggling.log.push("patrol: halted".into());
                g.say(&TextKey::new("patrol.name"), "patrol.halted");
                if g.smuggling.open_scan.is_none() {
                    g.smuggling.patrol_scan = true;
                    let e = g.smuggling.scan_at(at);
                    g.push_world(HOST, e);
                } else {
                    g.smuggling.patrol.scan_over(&g.smuggling.heat_c.tuning.record);
                }
            }
            heat_core::PatrolOut::Fled => {
                g.smuggling.log.push("patrol: fled".into());
                g.say(&TextKey::new("patrol.name"), "patrol.fled");
                if let Some((town, _)) = &official {
                    g.push_world(HOST, WorldEvent::Violation { town: town.clone(), kind: gameplay_core::ViolationKind::FledScan });
                } else if let Some(town) = g.smuggling.traffic_c.towns.keys().next().cloned() {
                    // Left the airspace: the violation still goes to the town's books.
                    g.push_world(HOST, WorldEvent::Violation { town, kind: gameplay_core::ViolationKind::FledScan });
                }
            }
        }
    }
    // Speeding off during a patrol's scan is fleeing it (on a pad, taking off is).
    if g.smuggling.patrol_scan && vel.0.length() > 3.0 * g.smuggling.heat_c.tuning.record.patrol_halt_speed_ms {
        if let Some((scan, _)) = g.smuggling.open_scan.clone() {
            g.smuggling.patrol_scan = false;
            g.push_world(HOST, WorldEvent::ScanAnswered { scan, choice: ScanChoice::Flee });
        }
    }
    // Contraband aboard: the hidden hold takes the first few, the scan sees the rest.
    let aboard = stock.iter().filter(|(s, c)| c.ship == Some(ship_e) && is_contraband(g, &s.commodity)).count() as u32;
    let hold = if g.progress.has_tag(&Tag::new(TAG_HIDDEN_HOLD)) { HIDDEN_HOLD_CRATES } else { 0 };
    g.smuggling.cargo.hidden = aboard.min(hold);
    g.smuggling.cargo.visible = aboard - g.smuggling.cargo.hidden;
    if actions.take_tap(Tap::Hail) {
        let town = inside.or_else(|| nearest_town(g, pos.0));
        if let Some(town) = town {
            g.push_world(HOST, WorldEvent::Hailed { town });
        }
    }
    if let Some((scan, _)) = g.smuggling.open_scan.clone() {
        for (tap, choice) in [(Tap::Choice1, ScanChoice::Comply), (Tap::Choice2, ScanChoice::Bribe), (Tap::Choice3, ScanChoice::Talk)] {
            if actions.take_tap(tap) {
                g.push_world(HOST, WorldEvent::ScanAnswered { scan, choice });
                break;
            }
        }
    }
}

fn nearest_town(gp: &Gameplay, p: DVec3) -> Option<TownId> {
    gp.smuggling
        .traffic_c
        .towns
        .values()
        .filter_map(|t| t.record.pads.first().and_then(|l| gp.pad_of(l)).map(|pad| (pad.centre.distance(p), t.record.id.clone())))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| t)
}

pub fn is_contraband(gp: &Gameplay, c: &CommodityId) -> bool {
    gp.kernel.commodities.get(c).is_some_and(|r| r.record.tags.iter().any(|t| t.as_str() == production_core::ingredient::CONTRABAND_TAG))
}

/// Spawns bought and brewed crates, takes confiscated ones.
pub fn smuggle_world(mut commands: Commands, table: Res<Crates>, mut gp: ResMut<Gameplay>, ships: Query<Entity, With<crate::ship::Ship>>, stock: Query<(Entity, &Stock, &Crate)>) {
    let spawns: Vec<SpawnStock> = std::mem::take(&mut gp.smuggling.spawn);
    for s in spawns {
        let Some(pad) = gp.pad_of(&s.at).cloned() else {
            gp.smuggling.log.push(format!("no pad for {} on this planet: {} crates wait", s.at, s.count));
            continue;
        };
        let east = pad.up.any_orthonormal_vector();
        let north = pad.up.cross(east);
        spawn_stock(&mut commands, &table, &gp, &s.commodity, s.product.as_ref(), s.count, pad.centre + north * (pad.radius * 0.5), pad.up, east);
    }
    if gp.smuggling.confiscate > 0 {
        gp.smuggling.confiscate = 0;
        let Some(ship_e) = ships.iter().next() else { return };
        let mut aboard: Vec<(Entity, u64)> = stock.iter().filter(|(_, s, c)| c.ship == Some(ship_e) && is_contraband(&gp, &s.commodity)).map(|(e, ..)| (e, e.to_bits())).collect();
        aboard.sort_by_key(|(_, b)| *b);
        let hidden = gp.smuggling.cargo.hidden as usize;
        let taken = aboard.len().saturating_sub(hidden);
        for (e, _) in aboard.into_iter().skip(hidden) {
            commands.entity(e).despawn();
        }
        gp.smuggling.log.push(format!("confiscated {taken} crates"));
    }
}

/// `count` stock crates in a row at `at` (on the ground), facing along `east`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_stock(commands: &mut Commands, table: &Crates, gp: &Gameplay, commodity: &CommodityId, product: Option<&ProductSpec>, count: u32, at: DVec3, up: DVec3, east: DVec3) {
    let size = gp.kernel.commodities.get(commodity).map(|c| c.record.crate_size.clone()).unwrap_or_else(|| "small".into());
    let half = table.0.get(&size).map_or(0.25, |s| s.extents[1] * 0.5);
    let fwd = DQuat::from_rotation_arc(DVec3::Y, up) * DVec3::NEG_Z;
    for i in 0..count {
        let off = (i as f64 - (count.max(1) - 1) as f64 * 0.5) * 1.2;
        let pos = at + up * (half + 0.3) + east * off;
        commands.spawn((crate_bundle(&table.0, &size, None, pos, fwd), Stock { commodity: commodity.clone(), product: product.cloned() }));
    }
}

/// The kettles and the buyers: what the walker can use, its prompt, and the tap. Runs before
/// `interact::interaction`, which leaves the tap and the prompt to it while it has a target.
#[allow(clippy::too_many_arguments)]
pub fn station_interact(
    mut commands: Commands,
    mut actions: ResMut<Actions>,
    bindings: Res<crate::controls::Bindings>,
    table: Res<Crates>,
    town: Res<Town>,
    mut gp: ResMut<Gameplay>,
    mut grab: ResMut<Grab>,
    mut st: ResMut<StationTarget>,
    mut clock: ResMut<KettleClock>,
    players: Query<&Player>,
    stock: Query<(Entity, &Stock, &Crate)>,
) {
    st.target = None;
    st.prompt.clear();
    let now = gp.clock();
    // Brews that just finished start the needle.
    for (id, ..) in kettles(&town, &gp) {
        let done = matches!(gp.smuggling.lab.kettle(&id), Some(production_core::Kettle::Done { .. }));
        if done {
            clock.0.entry(id).or_insert(now);
        } else {
            clock.0.remove(&id);
        }
    }
    let Ok(pl) = players.single() else { return };
    if pl.ship.is_some() || pl.seated || pl.fly {
        return;
    }
    let here = pl.world_pos(Frame::IDENTITY);
    let key = crate::interact::key_label(&bindings, Tap::Interact);
    let held = grab.held.and_then(|h| stock.get(h.crate_e).ok().map(|(e, s, _)| (e, s.clone())));
    // A kettle in reach.
    if let Some((id, ..)) = kettles(&town, &gp).into_iter().find(|(_, p, up)| in_reach(here, *p, *up, STATION_RANGE)) {
        let lc = &gp.smuggling.lab_c;
        let kettle = gp.smuggling.lab.kettle(&id).cloned().unwrap_or(production_core::Kettle::Empty);
        let name = |c: &CommodityId| gp.kernel.commodities.get(c).map(|r| gp.text(&r.record.name)).unwrap_or_else(|| c.to_string());
        let prompt = match (&held, &kettle) {
            (Some((_, s)), production_core::Kettle::Empty | production_core::Kettle::Loaded { .. }) if s.product.is_none() => format!("[{key}] put the {} into the kettle", name(&s.commodity)),
            (_, production_core::Kettle::Empty) => "Kettle: bring a base (blorp paste, dream goo) and up to 3 ingredients".to_string(),
            (_, production_core::Kettle::Loaded { base, ingredients }) => {
                let known = gp.smuggling.lab.preview(lc, &id).map(|e| effects_text(&gp, &e)).unwrap_or_else(|| "???".into());
                format!("[{key}] start the brew: {} + {} (gives {known})", name(base), ingredients.iter().map(&name).collect::<Vec<_>>().join(" + "))
            }
            (_, production_core::Kettle::Brewing { remaining_s, .. }) => format!("Brewing... {remaining_s:.0} s"),
            (_, production_core::Kettle::Done { .. }) => format!("[{key}] stop the needle in the green!"),
            (_, production_core::Kettle::Ready { crates, .. }) => format!("[{key}] take {crates} product crates"),
        };
        // A crate in reach in front of the walker is for picking up (more ingredients), not the kettle.
        let up = pl.world_up(Frame::IDENTITY);
        let flat = |v: DVec3| (v - up * v.dot(up)).normalize_or_zero();
        let look = flat(pl.world_look(Frame::IDENTITY));
        let crate_in_reach = stock.iter().any(|(_, _, c)| {
            let d = crate_world(c, &Frame::IDENTITY).0 - here;
            c.ship.is_none() && d.length() < 2.2 && flat(d).dot(look) > 0.5
        });
        let actionable = match (&held, &kettle) {
            (Some((_, s)), production_core::Kettle::Empty | production_core::Kettle::Loaded { .. }) => s.product.is_none(),
            (None, production_core::Kettle::Loaded { .. }) => !crate_in_reach,
            (None, production_core::Kettle::Done { .. } | production_core::Kettle::Ready { .. }) => true,
            _ => false,
        };
        st.target = actionable.then(|| Station::Kettle(id.clone()));
        st.prompt = prompt;
        if !actionable {
            return;
        }
        if actions.take_tap(Tap::Interact) {
            let g = &mut *gp;
            match kettle {
                production_core::Kettle::Empty | production_core::Kettle::Loaded { .. } if held.as_ref().is_some_and(|(_, s)| s.product.is_none()) => {
                    let (e, s) = held.clone().unwrap();
                    match g.smuggling.lab.insert(&g.smuggling.lab_c, &id, &s.commodity) {
                        Ok(()) => {
                            grab.held = None;
                            commands.entity(e).despawn();
                            g.smuggling.log.push(format!("{id}: in goes {}", s.commodity));
                            let at = g.clock();
                            g.feedback.push(at, Cue::Burst { at: Anchor::Kettle(id.clone()), tone: Tone::Info });
                        }
                        Err(r) => {
                            g.smuggling.log.push(format!("{id}: refused {}: {r:?}", s.commodity));
                            lab_refused(g, &r);
                        }
                    }
                }
                production_core::Kettle::Loaded { .. } => {
                    match g.smuggling.lab.start(&g.smuggling.lab_c, &id) {
                        Ok(_) => {
                            let at = g.clock();
                            g.feedback.push(at, Cue::Burst { at: Anchor::Kettle(id.clone()), tone: Tone::Gold });
                        }
                        Err(r) => {
                            g.smuggling.log.push(format!("{id}: start refused: {r:?}"));
                            lab_refused(g, &r);
                        }
                    }
                }
                production_core::Kettle::Done { .. } => {
                    let t = now - clock.0.get(&id).copied().unwrap_or(now);
                    match g.smuggling.lab.release(&g.smuggling.lab_c, &id, t) {
                        Ok(outs) => {
                            for o in outs {
                                match o {
                                    production_core::LabOutcome::Emit(e) => g.push_world(HOST, e),
                                    production_core::LabOutcome::Notice(n) => g.notify(n),
                                }
                            }
                        }
                        Err(r) => g.smuggling.log.push(format!("{id}: release refused: {r:?}")),
                    }
                }
                production_core::Kettle::Ready { .. } => {
                    if let Some((product, n)) = g.smuggling.lab.take(&id) {
                        let (_, p, up) = kettles(&town, g).into_iter().find(|(k, ..)| *k == id).expect("in reach above");
                        let side = up.any_orthonormal_vector();
                        let base = product.base.clone();
                        spawn_stock(&mut commands, &table, g, &base, Some(&product), n, p + up.cross(side) * 1.5, up, side);
                        g.smuggling.log.push(format!("{id}: took {n} crates of {} {:?} q{:.2}", product.base, product.effects, product.quality));
                    }
                }
                _ => {}
            }
        }
        return;
    }
    // A buyer in reach with an open order: hand over the product crates lying near them.
    let spots: Vec<(String, LocationId)> = buyers(&gp);
    let mut per_pad: std::collections::BTreeMap<LocationId, usize> = Default::default();
    for (by, at) in spots {
        let i = *per_pad.entry(at.clone()).and_modify(|n| *n += 1).or_insert(0);
        let Some((spot, _)) = buyer_spot(&gp, &at, i) else { continue };
        if !in_reach(here, spot, pl.world_up(Frame::IDENTITY), STATION_RANGE) {
            continue;
        }
        let order = gp.customers.product_orders().find(|(_, o)| o.customer.as_str() == by).map(|(id, o)| (id, o.clone()));
        let name = gp.customer_content.get(&customers_core::CustomerId::new(&by)).map(|c| gp.text(&c.name)).unwrap_or_else(|| by.clone());
        let Some((order, o)) = order else {
            st.prompt = format!("{name}: no order right now");
            return;
        };
        let near: Vec<(Entity, ProductSpec)> = stock
            .iter()
            .filter(|(_, s, c)| s.product.is_some() && c.ship.is_none() && crate_world(c, &Frame::IDENTITY).0.distance(spot) < HANDOVER_RANGE)
            .map(|(e, s, _)| (e, s.product.clone().unwrap()))
            .take(o.amount as usize)
            .collect();
        st.target = (!near.is_empty()).then(|| Station::Buyer { order, by: by.clone() });
        st.prompt = if near.is_empty() { format!("{name} wants {}× {} - set product crates down near them", o.amount, effects_text(&gp, &o.wants.iter().cloned().collect())) } else { format!("[{key}] hand {} of {} crates to {name}", near.len(), o.amount) };
        if !near.is_empty() && actions.take_tap(Tap::Interact) {
            for (e, _) in &near {
                commands.entity(*e).despawn();
            }
            let products = near.into_iter().map(|(_, p)| p).collect();
            gp.push_world(HOST, WorldEvent::ProductHandedOver { order, products });
            let at = gp.clock();
            gp.feedback.push(at, Cue::Burst { at: Anchor::Buyer(by.clone()), tone: Tone::Gold });
            gp.feedback.push(at, Cue::Hop(by.clone()));
        }
        return;
    }
}

pub fn effects_text(gp: &Gameplay, e: &std::collections::BTreeSet<gameplay_core::EffectId>) -> String {
    if e.is_empty() {
        return "no effects".into();
    }
    e.iter().map(|x| gp.kernel.effects.get(x).map(|r| gp.text(&r.record.name)).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", ")
}

#[derive(Resource, Default, Debug)]
pub struct PhoneState {
    pub open: bool,
    pub tab: usize,
    pub selected: usize,
}

/// One phone row and what F does on it.
#[derive(Clone, Debug)]
pub enum RowAction {
    None,
    PayTribute(i64),
    /// An order: F tracks it (the arrow), Z counters.
    Order(OrderId),
    PayFixer,
    Buy { shop: String, idx: usize },
}

/// The rows of a tab, with their actions.
pub fn phone_rows(gp: &Gameplay, tab: PhoneTab) -> Vec<(String, RowAction)> {
    let s = &gp.smuggling;
    let money = |m: i64| format!("{m} cr");
    match tab {
        PhoneTab::Goals => {
            let mut v = Vec::new();
            let rank = s.empire.rank(LADDER);
            if let Some(l) = s.ladders.ladders.get(LADDER).map(|l| &l.record) {
                v.push((format!("Family: {}   rank {}/{}", gp.text(&l.name), rank, l.ranks.len()), RowAction::None));
                if let Some(g) = s.empire.next_goal(&s.ladders, &gp.progress, &gp.kernel, LADDER) {
                    v.push((format!("Next rank: {} ({}/{} deeds)", gp.text(&g.rank_name), g.goals_done, g.goals_total), RowAction::None));
                    if let Some(r) = l.ranks.get(rank as usize) {
                        for goal in &r.requires {
                            let ok = goal.condition.holds(&gp.kernel, &gp.progress, None);
                            v.push((format!("  {} {}", if ok { "[x]" } else { "[ ]" }, gp.text(&goal.text)), RowAction::None));
                        }
                    }
                    let pay = if g.ready_to_pay { RowAction::PayTribute(g.tribute) } else { RowAction::None };
                    v.push((format!("Tribute {}{}", money(g.tribute), if g.ready_to_pay { "  - F: pay it" } else { "" }), pay));
                } else {
                    v.push(("Top of the family. (The break comes later.)".into(), RowAction::None));
                }
            }
            v
        }
        PhoneTab::Orders => {
            let mut v: Vec<(String, RowAction)> = gp
                .customers
                .product_orders()
                .map(|(id, o)| {
                    let who = gp.customer_content.get(&o.customer).map(|c| gp.text(&c.name)).unwrap_or_else(|| o.customer.to_string());
                    let town = s.town(&o.town).map(|t| gp.text(&t.name)).unwrap_or_else(|| o.town.to_string());
                    let wants = effects_text(gp, &o.wants.iter().cloned().collect());
                    let c = if o.countered { "" } else { "  Z: ask +20 %" };
                    let tracked = if gp.smuggling.tracked_order == Some(id) { "[tracked] " } else { "" };
                    (format!("{tracked}{who} @ {town}: {}× [{wants}] q>={:.0}% - {} - {:.0}:{:02.0} left{c}  F: track", o.amount, o.min_quality * 100.0, money(o.price), (o.left_s / 60.0).floor(), o.left_s % 60.0), RowAction::Order(id))
                })
                .collect();
            if v.is_empty() {
                v.push(("No orders. Buyers call when they want something.".into(), RowAction::None));
            }
            v
        }
        PhoneTab::Contacts => gp
            .customer_content
            .customers
            .values()
            .filter_map(|c| {
                let r = &c.record;
                let taste = r.taste.as_ref()?;
                let id = customers_core::CustomerId::new(r.id.as_str());
                if gp.customers.is_hidden(&id) {
                    return None;
                }
                let town = r.town.as_ref().and_then(|t| s.town(t)).map(|t| gp.text(&t.name)).unwrap_or_default();
                let rel = gp.customers.relationship(&id);
                let flag = if gp.customers.is_poached(&id) { "  (buys from the rival)" } else { "" };
                Some((format!("{} ({town}) {}{}  likes {}{flag}", gp.text(&r.name), "+".repeat(rel.round() as usize), ".".repeat(5 - rel.round().clamp(0.0, 5.0) as usize), effects_text(gp, &taste.wants.iter().cloned().collect())), RowAction::None))
            })
            .collect(),
        PhoneTab::Recipes => {
            let mut v: Vec<(String, RowAction)> = s
                .lab
                .book()
                .iter()
                .map(|(base, eff)| {
                    let b = gp.kernel.commodities.get(base).map(|r| gp.text(&r.record.name)).unwrap_or_else(|| base.to_string());
                    let p = ProductSpec { base: base.clone(), effects: eff.clone(), quality: 1.0 };
                    let price = production_core::price(&p, &gp.kernel).unwrap_or(0);
                    (format!("{b}: {} (up to {})", effects_text(gp, eff), money(price)), RowAction::None)
                })
                .collect();
            v.push(("- Ingredients -".into(), RowAction::None));
            for i in s.lab_c.ingredients.values().map(|i| &i.record) {
                let n = gp.kernel.commodities.get(&i.id).map(|r| gp.text(&r.record.name)).unwrap_or_else(|| i.id.to_string());
                let adds = i.adds.as_ref().map(|a| effects_text(gp, &[a.clone()].into())).unwrap_or_else(|| "-".into());
                let turns: Vec<String> = i.transforms.iter().map(|t| format!("{}->{}", effects_text(gp, &[t.from.clone()].into()), effects_text(gp, &[t.to.clone()].into()))).collect();
                v.push((format!("{n}: adds {adds}; turns {}", if turns.is_empty() { "-".into() } else { turns.join(", ") }), RowAction::None));
            }
            v
        }
        PhoneTab::Heat => {
            let t = &s.heat_c.tuning.record;
            vec![
                (format!("Heat {} ({:.0} of {:.0} points)", "*".repeat(s.heat.stars() as usize), s.heat.points(), t.max_points), RowAction::None),
                (format!("Official pads scan you {:.0} % of the time at this heat", heat_core::Heat::scan_chance(t, s.heat.stars(), false, gp.progress.has_tag(&Tag::new(super::systems::TAG_JAMMER))) * 100.0), RowAction::None),
                (format!("Cools {:.0}/min ({:.0}/min when clean)", t.decay_per_min, t.decay_per_min_clean), RowAction::None),
                (format!("Fixer: pay {} for -{:.0} heat  - F", money(FIXER_STEP), FIXER_STEP as f64 / t.paydown_per_point as f64), RowAction::PayFixer),
                (format!("Contraband aboard: {} seen, {} hidden", s.cargo.visible, s.cargo.hidden), RowAction::None),
            ]
        }
        PhoneTab::Shop => {
            let mut v = Vec::new();
            for (sid, shop) in &s.shops.shops {
                let shop = &shop.record;
                v.push((format!("- {} -", gp.text(&shop.name)), RowAction::None));
                for (idx, o) in shop.offers(&gp.progress, &gp.kernel).iter().enumerate() {
                    let state = if o.owned { "owned" } else if !o.available { "locked" } else if !o.affordable { "too expensive" } else { "F: buy" };
                    let amount = o.amount.map(|a| format!("{a}× ")).unwrap_or_default();
                    v.push((format!("{amount}{} - {}  ({state})", gp.text(&o.label), money(o.price)), RowAction::Buy { shop: sid.clone(), idx }));
                }
            }
            v
        }
    }
}

/// The phone: open, tabs, rows, F on a row, Z on an order.
pub fn phone_step(mut actions: ResMut<Actions>, mut phone: ResMut<PhoneState>, mut gp: ResMut<Gameplay>) {
    if actions.take_tap(Tap::Phone) {
        phone.open = !phone.open;
        phone.selected = 0;
    }
    if !phone.open {
        return;
    }
    let n = PhoneTab::ALL.len();
    if actions.take_tap(Tap::PhoneLeft) {
        phone.tab = (phone.tab + n - 1) % n;
        phone.selected = 0;
    }
    if actions.take_tap(Tap::PhoneRight) {
        phone.tab = (phone.tab + 1) % n;
        phone.selected = 0;
    }
    let rows = phone_rows(&gp, PhoneTab::ALL[phone.tab]);
    if actions.take_tap(Tap::PhoneUp) {
        phone.selected = phone.selected.saturating_sub(1);
    }
    if actions.take_tap(Tap::PhoneDown) {
        phone.selected = (phone.selected + 1).min(rows.len().saturating_sub(1));
    }
    let action = rows.get(phone.selected).map(|r| r.1.clone()).unwrap_or(RowAction::None);
    if actions.take_tap(Tap::Counter) {
        if let RowAction::Order(order) = action {
            gp.push_world(HOST, WorldEvent::OrderCountered { order, ask_pct: COUNTER_PCT });
        }
    }
    // F on a row with an action: the phone takes the tap (the world's F waits while it is open).
    if !matches!(action, RowAction::None) && actions.take_tap(Tap::Interact) {
        let g = &mut *gp;
        match action {
            RowAction::PayTribute(amount) => g.push_world(HOST, WorldEvent::TributePaid { ladder: LADDER.into(), amount }),
            RowAction::PayFixer => {
                if g.progress.wallet() >= FIXER_STEP {
                    g.push_world(HOST, WorldEvent::HeatPaidDown { amount: FIXER_STEP });
                } else {
                    g.notify(Notice::new(NoticeKind::Warning, "heat.fixer_short").arg("money", Arg::Number(FIXER_STEP)));
                }
            }
            RowAction::Buy { shop, idx } => {
                let Some(sh) = g.smuggling.shops.shops.get(&shop).map(|s| s.record.clone()) else { return };
                match sh.buy(idx, &g.progress, &g.kernel, sh.location.clone()) {
                    Ok(events) => events.into_iter().for_each(|e| g.push_world(HOST, e)),
                    Err(r) => {
                        g.smuggling.log.push(format!("shop {shop}: refused {r:?}"));
                        let why = match r {
                            gameplay_core::shop::Refusal::NotEnoughMoney { .. } => "shop.why.money",
                            gameplay_core::shop::Refusal::Owned => "shop.why.owned",
                            gameplay_core::shop::Refusal::WrongLocation => "shop.why.place",
                            _ => "shop.why.locked",
                        };
                        g.notify(Notice::new(NoticeKind::Warning, "shop.refused").arg("why", Arg::Key(TextKey::new(why))));
                    }
                }
            }
            RowAction::Order(order) => g.smuggling.tracked_order = if g.smuggling.tracked_order == Some(order) { None } else { Some(order) },
            RowAction::None => {}
        }
    }
}

/// Fills `SmuggleHud` (and the town's buyer characters) from the game state.
pub fn fill_hud(mut hud: ResMut<SmuggleHud>, mut gp: ResMut<Gameplay>, phone: Res<PhoneState>, town: Option<ResMut<Town>>, st: Res<StationTarget>, clock: Res<KettleClock>, players: Query<&Player>) {
    let s = &gp.smuggling;
    hud.clock = gp.clock();
    hud.heat_points = s.heat.points();
    hud.heat_stars = s.heat.stars();
    hud.goal_line = match s.empire.next_goal(&s.ladders, &gp.progress, &gp.kernel, LADDER) {
        Some(g) if g.ready_to_pay => format!("Next: pay the family {} cr (phone, Goals) -> {}", g.tribute, gp.text(&g.rank_name)),
        Some(g) => format!("Next: {} ({}/{}) -> {}", gp.text(&g.goal_text), g.goals_done, g.goals_total, gp.text(&g.rank_name)),
        None => String::new(),
    };
    hud.radio = s.radio.clone();
    let pad_name = |l: &LocationId| gp.kernel.locations.get(l).map(|r| gp.text(&r.record.name)).unwrap_or_else(|| l.to_string());
    hud.clearance = if let Some(left) = s.patrol.ordered_left() {
        format!("PATROL: halt! Slow below {:.0} m/s - {:.0} s (or run for it)", s.heat_c.tuning.record.patrol_halt_speed_ms, left.max(0.0))
    } else { match (s.traffic.clearance(HOST), s.traffic.pending(HOST)) {
        (Some(c), _) => format!("{}: {} - {:.0} s", if c.inspection { "INSPECTION" } else { "Cleared" }, pad_name(&c.pad), c.time_left_s.max(0.0)),
        (None, Some(p)) if p.second => format!("Holding... the tower answers in {:.0} s", p.due_s.max(0.0)),
        (None, Some(_)) => "Calling the tower...".into(),
        (None, None) => match s.traffic.inside(HOST).and_then(|t| s.town(t)) {
            Some(t) if !t.free_port => format!("{} airspace - L: ask for a pad, limit {:.0} m/s", gp.text(&t.name), t.speed_limit_ms),
            Some(t) => format!("{} - no tower, land anywhere", gp.text(&t.name)),
            None => String::new(),
        },
    } };
    hud.bar = if let Some(left) = s.patrol.ordered_left() {
        Some(super::HudBar { left: (left / s.heat_c.tuning.record.patrol_halt_s).clamp(0.0, 1.0), tone: Tone::Bad })
    } else {
        s.traffic.clearance(HOST).map(|c| {
            let left = (c.time_left_s / s.clear_window_s.max(1.0)).clamp(0.0, 1.0);
            super::HudBar { left, tone: if c.inspection || left < 0.2 { Tone::Bad } else { Tone::Info } }
        })
    };
    let tab = PhoneTab::ALL[phone.tab];
    let rows = if phone.open { phone_rows(&gp, tab) } else { Vec::new() };
    hud.phone = super::PhoneView {
        open: phone.open,
        tab,
        selected: phone.selected.min(rows.len().saturating_sub(1)),
        rows: rows.into_iter().map(|r| r.0).collect(),
        hint: "<- -> tabs   up/down rows   F act   Z counter-offer   ` close".into(),
    };
    hud.scan = match &s.open_scan {
        Some((_, at)) => {
            let t = &s.heat_c.tuning.record;
            let stars = s.heat.stars();
            let fine = t.fine_base + t.fine_per_crate * s.cargo.visible as i64;
            let place = gp.kernel.locations.get(at).map(|l| gp.text(&l.record.name)).unwrap_or_default();
            format!(
                "SCAN at {place}! They want to look in your hold.\n[1] Comply: lose the contraband, fine ~{fine} cr\n[2] Bribe: {} cr\n[3] Talk: {:.0} % chance\nOr take off now and flee (+heat)",
                t.bribe_base * (1 + stars as i64),
                t.talk_chance[stars.min(5) as usize] * 100.0
            )
        }
        None => String::new(),
    };
    hud.brew = match &st.target {
        Some(Station::Kettle(id)) if matches!(s.lab.kettle(id), Some(production_core::Kettle::Done { .. })) => {
            let t = &s.lab_c.tuning;
            let since = gp.clock() - clock.0.get(id).copied().unwrap_or(gp.clock());
            Some(BrewView { needle: production_core::needle(since, t.needle_period_s), sweet: t.sweet_spot, width: t.sweet_width })
        }
        _ => None,
    };
    let _ = players;
    // The buyers as characters at their pads.
    if let Some(mut town) = town {
        let spots: Vec<(String, LocationId)> = buyers(&gp);
        let mut per_pad: std::collections::BTreeMap<LocationId, usize> = Default::default();
        let mut npcs = Vec::new();
        for (by, at) in spots {
            let i = *per_pad.entry(at.clone()).and_modify(|n| *n += 1).or_insert(0);
            if let Some((pos, up)) = buyer_spot(&gp, &at, i) {
                npcs.push(crate::town::Npc { id: by, pos, up });
            }
        }
        if town.extra_npcs.len() != npcs.len() || town.extra_npcs.iter().zip(&npcs).any(|(a, b)| a.id != b.id || a.pos.distance(b.pos) > 0.1) {
            town.extra_npcs = npcs;
        }
    }
    gp.smuggling.radio.retain(|l| hud.clock - l.at < 120.0);
    let (at, line) = (gp.clock(), hud.goal_line.clone());
    gp.feedback.goal_line(at, &line);
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SmuggleHud>().init_resource::<StationTarget>().init_resource::<KettleClock>().init_resource::<PhoneState>();
    app.add_systems(
        FixedUpdate,
        (phone_step, station_interact).chain().before(crate::interact::interaction).in_set(crate::phases::Fx::Walker),
    );
    app.add_systems(FixedUpdate, (smuggle_observe, smuggle_world).chain().after(crate::cargo::budget_step).before(crate::gameplay::gameplay_step).in_set(crate::phases::Fx::Cargo));
    app.add_systems(FixedUpdate, fill_hud.after(crate::gameplay::gameplay_step).in_set(crate::phases::Fx::Effects));
}

/// Interaction's prompt while a station is targeted (the window shows one prompt line).
pub fn prompt_override(st: &StationTarget, inter: &mut Interaction) -> bool {
    if st.target.is_some() {
        inter.target = None;
        inter.prompt = st.prompt.clone();
        return true;
    }
    false
}

/// A placeholder vat at every kettle (window only): a fat cylinder with a glowing top.
#[derive(Component)]
pub struct KettleModel;

/// A kettle's brew surface: its material and where bubbles rise from.
#[derive(Component)]
pub struct KettleTop {
    pub id: String,
    pub mat: Handle<StandardMaterial>,
    pub top: DVec3,
    pub up: DVec3,
}

#[derive(Component)]
pub struct Bubble {
    born: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn kettle_view(
    mut commands: Commands,
    town: Res<Town>,
    gp: Res<Gameplay>,
    planet: Res<crate::env::PlanetRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shown: Query<Entity, With<KettleModel>>,
    mut count: Local<usize>,
) {
    let ks = kettles(&town, &gp);
    if ks.len() == *count {
        return;
    }
    *count = ks.len();
    for e in &shown {
        commands.entity(e).despawn();
    }
    let body = meshes.add(Cylinder::new(0.6, 1.1));
    let top = meshes.add(Cylinder::new(0.5, 0.08));
    let metal = mats.add(StandardMaterial { base_color: Color::srgb(0.45, 0.5, 0.55), metallic: 0.6, perceptual_roughness: 0.4, ..default() });
    for (id, p, up) in ks {
        // Each kettle's brew has its own material: its colour follows the mix (`kettle_fx`).
        let brew = mats.add(StandardMaterial { base_color: Color::srgb(0.2, 0.22, 0.2), emissive: LinearRgba::BLACK, ..default() });
        let ground = planet.centre + up * planet.surface(up);
        let ground = ground + (p - planet.centre - up * (p - planet.centre).dot(up));
        let rot = Quat::from_rotation_arc(Vec3::Y, up.as_vec3());
        commands.spawn((Mesh3d(body.clone()), MeshMaterial3d(metal.clone()), Transform::from_rotation(rot), crate::origin::WorldPos(ground + up * 0.55), crate::terrain::PlanetScene(planet.id), KettleModel));
        commands.spawn((Mesh3d(top.clone()), MeshMaterial3d(brew.clone()), Transform::from_rotation(rot), crate::origin::WorldPos(ground + up * 1.12), crate::terrain::PlanetScene(planet.id), KettleModel, KettleTop { id, mat: brew, top: ground + up * 1.16, up }));
    }
}

/// A kettle that refuses says why, in a toast.
fn lab_refused(g: &mut Gameplay, r: &production_core::LabRefusal) {
    let why = match r {
        production_core::LabRefusal::UnknownGoods => "lab.why.unknown",
        production_core::LabRefusal::SecondBase => "lab.why.second_base",
        production_core::LabRefusal::IngredientBeforeBase => "lab.why.base_first",
        production_core::LabRefusal::TooManyIngredients => "lab.why.full",
        production_core::LabRefusal::NoBase => "lab.why.no_base",
        _ => "lab.why.busy",
    };
    g.notify(Notice::new(NoticeKind::Warning, "lab.refused").arg("why", Arg::Key(TextKey::new(why))));
}

/// A colour for a mix: each name gives a hue, the mix is their blend. TODO(initiator): colours
/// per effect in the content.
pub fn mix_colour<'a>(names: impl Iterator<Item = &'a str>) -> Color {
    let (mut r, mut g, mut b, mut n) = (0.0, 0.0, 0.0, 0.0);
    for name in names {
        let h = name.bytes().fold(2166136261u32, |h, c| (h ^ c as u32).wrapping_mul(16777619));
        let c = Color::hsl((h % 360) as f32, 0.85, 0.55).to_srgba();
        (r, g, b, n) = (r + c.red, g + c.green, b + c.blue, n + 1.0);
    }
    if n == 0.0 { Color::srgb(0.2, 0.22, 0.2) } else { Color::srgb(r / n, g / n, b / n) }
}

/// The brew in each kettle (window): dull when empty, the mix's colour when loaded, bubbling
/// while it brews, pulsing gold when the needle waits, glowing when the crates are ready.
#[allow(clippy::too_many_arguments)]
pub fn kettle_fx(
    mut commands: Commands,
    time: Res<Time>,
    gp: Res<Gameplay>,
    planet: Res<crate::env::PlanetRes>,
    tops: Query<&KettleTop>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bubbles: Query<(Entity, &Bubble, &mut crate::origin::WorldPose, &mut Transform)>,
    mut next: Local<f64>,
    mut mesh: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    let now = time.elapsed_secs_f64();
    let lab = &gp.smuggling.lab;
    let mut spawn_at = Vec::new();
    for k in &tops {
        let Some(mut m) = mats.get_mut(&k.mat) else { continue };
        let (colour, glow) = match lab.kettle(&k.id) {
            None | Some(production_core::Kettle::Empty) => (Color::srgb(0.2, 0.22, 0.2), 0.0),
            Some(production_core::Kettle::Loaded { base, ingredients }) => (mix_colour(std::iter::once(base.as_str()).chain(ingredients.iter().map(|i| i.as_str()))), 0.6),
            Some(production_core::Kettle::Brewing { base, ingredients, .. }) => {
                spawn_at.push((k.top, k.up));
                (mix_colour(std::iter::once(base.as_str()).chain(ingredients.iter().map(|i| i.as_str()))), 1.2 + 0.8 * (now * 5.0).sin().abs() as f32)
            }
            Some(production_core::Kettle::Done { .. }) => (crate::ui_theme::GOLD, 1.0 + 2.5 * (now * 8.0).sin().abs() as f32),
            Some(production_core::Kettle::Ready { product, .. }) => (mix_colour(product.effects.iter().map(|e| e.as_str())), 2.0),
        };
        m.base_color = colour;
        m.emissive = colour.to_linear() * glow;
    }
    if now >= *next && !spawn_at.is_empty() {
        *next = now + 0.12;
        let (mesh, mat) = mesh.get_or_insert_with(|| (meshes.add(Sphere::new(0.07)), mats.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.7), emissive: LinearRgba::rgb(1.5, 1.5, 1.5), alpha_mode: AlphaMode::Blend, ..default() }))).clone();
        for (i, (top, up)) in spawn_at.into_iter().enumerate() {
            let e = up.any_orthonormal_vector();
            let a = (now * 7.3 + i as f64 * 2.1).sin() * std::f64::consts::PI;
            let off = (e * a.cos() + up.cross(e) * a.sin()) * 0.3;
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), crate::origin::WorldPose { pos: top + off, rot: DQuat::IDENTITY }, crate::terrain::PlanetScene(planet.id), Bubble { born: now }));
        }
    }
    let dt = time.delta_secs_f64();
    for (e, b, mut wp, mut t) in &mut bubbles {
        let age = now - b.born;
        if age > 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let up = planet.up(wp.pos);
        wp.pos += up * dt * 0.9;
        t.scale = Vec3::splat((1.0 + age as f32) * (1.0 - age as f32 * 0.8));
    }
}
