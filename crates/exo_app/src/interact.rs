use crate::cargo::{crate_world, Crate, Crates};
use crate::dev_area::{hull::doors::{BridgeDoor, BridgeSlab, EngineDoor, EquipDoor, EquipSlab, Slab}, layout::DOOR_REACH};
use crate::dev_area::DevBox;
use crate::controls::{Actions, Bindings, Input, Tap};
use crate::displays::{ActiveDisplay, DisplayPanel, USE_RANGE};
use crate::grab::Grab;
use crate::items::{Items, LooseItem};
use crate::ship::{Ship, SEAT_POS};
use crate::walker::{cabin_frame, CabinFloor, Player, EYE_HEIGHT};
use avian3d::prelude::*;
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use display_core::hud::{PromptKind, PromptView};
use display_core::Tone;
use grab_core::{in_cone, Reach};
use walker_core::Frame;

pub fn plugin(app: &mut App) {
    app.init_resource::<Interaction>();
    app.add_systems(FixedUpdate, interaction.before(crate::walker::walker_step).in_set(crate::phases::Fx::Walker));
}

/// The walker sits down when its feet are this close to the seat (m).
pub const SEAT_RANGE: f64 = 1.8;

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Seat,
    /// The seat, but the player has no flight licence: the prompt says where to get one.
    SeatLocked,
    StandUp,
    Crate(Entity, Reach),
    Drop(Entity),
    Counter(jobs_core::GiverId),
    Accept(jobs_core::JobId),
    Panel(Entity),
    LeavePanel(Entity),
    Item(Entity),
    /// The engine room's door: a request to slide it to the other state (`open` is the current state).
    Door { open: bool },
    /// The bridge door (r44): the same request for the bridge's door, a request to the room's bridge door.
    BridgeDoor { open: bool },
    /// The equipment room's door to the corridor (#309): the same request.
    EquipDoor { open: bool },
    /// The airlock's inner door (#309): an ordinary door from the ship while the airlock rests inside.
    AirlockDoor { open: bool },
    /// The same door while it is locked (the airlock rests outside, or a cycle runs): F shows LOCKED and does nothing.
    AirlockLocked,
    Rack(item_core::RackId),
    /// A locker, box or crate: F opens its window (or closes it while open). `by` is the player
    /// who has it open, if it is not the walker itself: the prompt greys out and F does nothing.
    Container { rack: item_core::RackId, by: Option<item_core::PlayerId> },
}

#[derive(Resource, Default, Debug)]
pub struct Interaction {
    pub target: Option<Target>,
    /// What the HUD shows, e.g. "[F] sit". Empty when there is nothing to do.
    pub prompt: String,
    /// What F does there, for the prompt's look (blocked is drawn dashed with an amber line).
    pub kind: display_core::hud::PromptKind,
    pub view: PromptView,
    /// The target's place in the world, if it has one: the prompt hangs 16 px above it on screen.
    pub at: Option<DVec3>,
    /// A display's two bottom corners in the world, if the target is one: its prompt goes under them.
    pub edge: Option<[DVec3; 2]>,
    pub used: Vec<Target>,
}

/// Short label of the first keyboard key bound to a tap ("F").
pub fn key_label(bindings: &Bindings, t: Tap) -> String {
    bindings
        .taps
        .iter()
        .filter(|(x, _)| *x == t)
        .flat_map(|(_, ks)| ks.iter())
        .find_map(|i| if let Input::Key(k) = i { Some(format!("{k:?}")) } else { None })
        .map(|k| k.trim_start_matches("Key").trim_start_matches("Digit").to_string())
        .unwrap_or_else(|| "?".into())
}

pub fn key_cap(bindings: &Bindings, t: Tap) -> String {
    match key_label(bindings, t).as_str() {
        "Escape" => "ESC".into(),
        k => k.to_uppercase(),
    }
}

/// "  [key] verb" for a tap, or nothing when the tap has no key (`Tap::KEYLESS`).
pub fn key_hint(bindings: &Bindings, t: Tap, verb: &str) -> String {
    match key_label(bindings, t).as_str() {
        "?" => String::new(),
        k => format!("  [{k}] {verb}"),
    }
}

/// Whether a loose item's kind is the suit (worn in its own place, never collected into the bar).
fn is_suit_def(items: &Items, def: &str) -> bool {
    items.defs.get(def).is_some_and(|d| d.tags.iter().any(|t| t == item_core::SUIT_TAG))
}

fn hands_free(items: &Items, crates: &Query<(Entity, &Crate), Without<LooseItem>>, table: &Crates, e: Entity) -> bool {
    crates.get(e).is_ok_and(|(_, c)| items.client.view.may_carry(&items.defs, table.0.sizes[c.size].hands))
}

fn verb(t: &Target, size: &str) -> String {
    match t {
        Target::Seat => "sit".into(),
        Target::SeatLocked => "sit".into(),
        Target::StandUp => "stand up".into(),
        Target::Crate(_, Reach::Hands) => format!("pick up the {size} crate"),
        Target::Crate(_, Reach::Tool) => format!("pull the {size} crate (grab tool)"),
        Target::Drop(_) => format!("set the {size} crate down"),
        Target::Counter(_) => "talk to ".into(),
        Target::Accept(_) => "take the job: ".into(),
        Target::Panel(_) => "use ".into(),
        Target::LeavePanel(_) => "leave ".into(),
        Target::Item(_) => "pick up ".into(),
        Target::Rack(_) => "swap at the rack".into(),
        Target::Container { .. } => "open ".into(),
        Target::AirlockLocked => "locked".into(),
        Target::Door { open } | Target::BridgeDoor { open } | Target::EquipDoor { open } | Target::AirlockDoor { open } => if *open { "close ".into() } else { "open ".into() },
    }
}

#[allow(clippy::too_many_arguments)]
pub fn interaction(
    mut commands: Commands,
    mut actions: ResMut<Actions>,
    bindings: Res<Bindings>,
    tuning: Res<crate::tuning::Tuning>,
    table: Res<Crates>,
    mut inter: ResMut<Interaction>,
    mut grab: ResMut<Grab>,
    mut players: Query<&mut Player>,
    mut ships: Query<(Entity, &mut Ship, &Position, &Rotation)>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    remotes: Query<(&Position, &Rotation), With<crate::ship::RemoteShip>>,
    crates: Query<(Entity, &Crate), Without<LooseItem>>,
    mut gp: ResMut<crate::gameplay::Gameplay>,
    (mut active, displays): (ResMut<ActiveDisplay>, Query<(Entity, &DisplayPanel)>),
    (time, mut items, loose, door, doors, bridge_door, bridge_doors): (Res<Time>, ResMut<Items>, Query<(Entity, &Crate, &LooseItem)>, Option<Res<EngineDoor>>, Query<&Position, With<Slab>>, Option<Res<BridgeDoor>>, Query<&Position, With<BridgeSlab>>),
    (boxes, supply, airlock_doors, equip_door, equip_doors, mut cw, mut ui, specs): (Query<(&DevBox, &Position)>, Option<Res<crate::dev_area::fire::extinguisher::Supply>>, Query<(&crate::dev_area::hull::airlock::AirlockSlab, &Position)>, Option<Res<EquipDoor>>, Query<&Position, With<EquipSlab>>, ResMut<crate::container_window::ContainerWindow>, ResMut<crate::items::InventoryUi>, Res<crate::container_window::ContainerSpecs>),
) {
    let Ok(mut pl) = players.single_mut() else { return };
    let Some((ship_e, sp, sr)) = ships.iter().next().map(|(e, _, p, r)| (e, *p, *r)) else { return };
    let frame = cabin_frame(ship_e, (&sp, &sr), &floors);
    let cfg = &tuning.grab;
    if grab.held.is_some_and(|h| crates.get(h.crate_e).is_err()) {
        grab.held = None;
    }
    let size_of = |e: Entity| crates.get(e).map(|(_, c)| table.0.sizes[c.size].name.clone()).unwrap_or_default();

    // In another player's cabin only loose items are offered yet; the cabin is that ship's.
    let own_frame = match pl.ship {
        None => true,
        Some(e) => e == ship_e,
    };
    let cabin = match pl.ship {
        Some(e) if e != ship_e => remotes.get(e).map_or(frame, |(p, r)| cabin_frame(e, (p, r), &floors)),
        _ => frame,
    };
    let frame_of = |c: &Crate| match c.ship {
        Some(e) if e != ship_e => remotes.get(e).map_or(frame, |(p, r)| cabin_frame(e, (p, r), &floors)),
        _ => frame,
    };
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly;
    let here = pl.world_pos(Frame::IDENTITY);
    if gp.panel.as_ref().is_some_and(|p| !on_foot || gp.counter_at(here).as_ref() != Some(&p.giver)) {
        gp.close_counter();
    }
    if on_foot && gp.panel.is_some() {
        if actions.take_tap(Tap::Decline) {
            gp.close_counter();
        } else if actions.take_tap(Tap::NextOffer) {
            if let Some(p) = gp.panel.as_mut() {
                p.page += 1;
            }
        }
    }
    let panel_offer = if on_foot && grab.held.is_none() { gp.panel_offer() } else { None };
    let container_pos = |rack: item_core::RackId| specs.get(rack).and_then(|s| boxes.iter().find(|(b, _)| b.id == s.box_id)).map(|(_, p)| p.0);
    let target = if let Some(rack) = cw.rack {
        // The window is open: F closes it, wherever the walker looks.
        Some(Target::Container { rack, by: None })
    } else if let Some(e) = active.using() {
        Some(Target::LeavePanel(e))
    } else if let Some(job) = panel_offer {
        Some(Target::Accept(job))
    } else if pl.seated {
        Some(Target::StandUp)
    } else if let Some(h) = grab.held {
        Some(Target::Drop(h.crate_e))
    } else if pl.fly {
        None
    } else {
        let f = if pl.ship.is_some() { cabin } else { Frame::IDENTITY };
        let eye = pl.world_pos(f) + pl.world_up(f) * EYE_HEIGHT;
        let look = pl.world_look(f);
        let half = cfg.cone_half_angle_deg.to_radians();
        let mut best: Option<(f64, Entity, Reach)> = None;
        let mut panel_pick = None;
        let mut item_target = None;
        let mut item_angle = None;
        for (e, c) in crates.iter().filter(|_| own_frame) {
            let (centre, _) = crate_world(c, &frame);
            let Some(dist) = in_cone(eye, look, centre, half, cfg.tool_max_range) else { continue };
            let angle = (centre - eye).angle_between(look);
            let reach = if dist - c.body.half.max_element() <= cfg.hand_range { Reach::Hands } else { Reach::Tool };
            // The grab tool is parked (#307): beyond the hands a crate is not offered.
            if reach == Reach::Tool && !cfg.tool_enabled {
                continue;
            }
            if best.is_none_or(|(a, ..)| angle < a) {
                best = Some((angle, e, reach));
            }
        }
        // A loose item in reach of the hands, nearest the view's centre; it wins over a crate
        // nearer the edge of the view.
        let item_pick = loose
            .iter()
            .filter(|(_, _, li)| crate::items::visible(&items, li))
            .filter_map(|(e, c, _)| {
                let (centre, _) = crate_world(c, &frame_of(c));
                in_cone(eye, look, centre, half, cfg.hand_range).map(|_| ((centre - eye).angle_between(look), e))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((a, e)) = item_pick
            && best.is_none_or(|(c, ..)| a <= c)
        {
            best = None;
            item_target = Some(e);
            item_angle = Some(a);
        }
        let at_seat = pl.ship == Some(ship_e) && pl.w.pos.distance(SEAT_POS) < SEAT_RANGE;
        // A display: looked at straight (the view ray meets its quad) or inside the cone, in reach,
        // from the front. The nearer to the view's centre wins against a crate.
        let panel = crate::displays::eye_on_foot(&pl).filter(|_| own_frame).and_then(|(eye, look)| {
            displays
                .iter()
                .filter(|(_, d)| (eye - d.centre).dot(d.rot * DVec3::Z) > 0.0)
                .filter_map(|(e, d)| match display_core::ray_to_panel(eye, look, d.centre, d.rot, d.size_m) {
                    Some(h) if h.distance <= USE_RANGE => Some((0.0, e)),
                    _ => in_cone(eye, look, d.centre, half, USE_RANGE).map(|_| ((d.centre - eye).angle_between(look), e)),
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
        });
        if let Some((a, e)) = panel
            && best.is_none_or(|(c, ..)| a <= c)
            && item_angle.is_none_or(|i| a < i)
        {
            best = None;
            item_target = None;
            panel_pick = Some(e);
        }
        let door_pick = (pl.ship.is_none() && doors.iter().any(|p| in_cone(eye, look, p.0, half, DOOR_REACH).is_some())).then_some(Target::Door { open: door.is_some_and(|d| d.open) });
        let bridge_pick = (pl.ship.is_none() && bridge_doors.iter().any(|p| in_cone(eye, look, p.0, half, DOOR_REACH).is_some())).then_some(Target::BridgeDoor { open: bridge_door.is_some_and(|d| d.open) });
        let equip_pick = (pl.ship.is_none() && equip_doors.iter().any(|p| in_cone(eye, look, p.0, half, DOOR_REACH).is_some())).then_some(Target::EquipDoor { open: equip_door.is_some_and(|d| d.open) });
        // The airlock's inner door is the ship's door while the airlock rests inside and no cycle runs; else it is locked.
        let airlock_pick = items.reactor.and_then(|r| {
            let near = airlock_doors.iter().any(|(s, p)| s.door == reactor_core::Door::Inner && in_cone(eye, look, p.0, half, DOOR_REACH).is_some());
            let usable = r.airlock.inside && r.airlock.cycling == item_core::wire::AirlockCycle::Idle;
            (pl.ship.is_none() && near).then_some(if usable { Target::AirlockDoor { open: r.airlock.inner_open } } else { Target::AirlockLocked })
        });
        match best {
            // A crate in reach of the hands wins over the seat; the tool's reach does not.
            Some((_, e, Reach::Hands)) => Some(Target::Crate(e, Reach::Hands)),
            _ if at_seat => Some(if gp.may_pilot(crate::gameplay::HOST) { Target::Seat } else { Target::SeatLocked }),
            Some((_, e, r)) => Some(Target::Crate(e, r)),
            None => {
                let rack_pick = pl.ship.is_none().then(|| supply.as_ref().and_then(|sup| boxes.iter().find_map(|(b, p)| sup.0.rack_in(&b.id).filter(|_| in_cone(eye, look, p.0, half, USE_RANGE).is_some()).map(Target::Rack)))).flatten();
                // Lockers stand side by side (#311): of the containers in the cone the one nearest the view's centre wins.
                let container_pick = pl.ship.is_none().then(|| specs.0.iter().filter_map(|s| boxes.iter().find(|(b, p)| b.id == s.box_id && in_cone(eye, look, p.0, half, USE_RANGE).is_some()).map(|(_, p)| ((p.0 - eye).angle_between(look), s))).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, s)| Target::Container { rack: s.rack, by: items.client.holder(s.rack).filter(|p| *p != items.me()) })).flatten();
                item_target.map(Target::Item).or(panel_pick.map(Target::Panel)).or(door_pick).or(bridge_pick).or(equip_pick).or(airlock_pick).or(rack_pick).or(container_pick)
            }
        }
    };
    let target = target.or_else(|| {
        let giver = (on_foot && grab.held.is_none()).then(|| gp.counter_at(here)).flatten()?;
        (!gp.offers_of(&giver).is_empty()).then_some(Target::Counter(giver))
    });
    inter.target = target.clone();
    inter.kind = match &target {
        // A suit lying about is put on; while one is worn the prompt greys out (F does nothing).
        Some(Target::Item(e)) if loose.get(*e).is_ok_and(|(_, _, li)| is_suit_def(&items, &li.def)) && items.client.view.worn.is_some() => PromptKind::Blocked,
        Some(Target::Item(_)) => PromptKind::Collect,
        Some(Target::StandUp | Target::LeavePanel(_)) => PromptKind::Leave,
        Some(Target::SeatLocked | Target::AirlockLocked | Target::Container { by: Some(_), .. }) => PromptKind::Blocked,
        Some(Target::Crate(e, Reach::Hands)) if !hands_free(&items, &crates, &table, *e) => PromptKind::Blocked,
        _ => PromptKind::Use,
    };
    inter.edge = match &target {
        Some(Target::Panel(e)) => displays.get(*e).ok().map(|(_, d)| {
            let corner = |u: f64| display_core::panel_point(d.centre, d.rot, d.size_m, DVec2::new(u, 1.0));
            [corner(0.0), corner(1.0)]
        }),
        _ => None,
    };
    inter.at = match &target {
        Some(Target::Crate(e, _)) => crates.get(*e).ok().map(|(_, c)| crate_world(c, &frame).0),
        Some(Target::Item(e)) => loose.get(*e).ok().map(|(_, c, _)| crate_world(c, &frame).0),
        Some(Target::Panel(e)) => displays.get(*e).ok().map(|(_, d)| d.centre),
        Some(Target::Seat | Target::SeatLocked) => Some(frame.to_world(SEAT_POS)),
        Some(Target::Door { .. }) => doors.iter().next().map(|p| p.0),
        Some(Target::BridgeDoor { .. }) => bridge_doors.iter().next().map(|p| p.0),
        Some(Target::EquipDoor { .. }) => equip_doors.iter().next().map(|p| p.0),
        Some(Target::AirlockDoor { .. } | Target::AirlockLocked) => airlock_doors.iter().find(|(s, _)| s.door == reactor_core::Door::Inner).map(|(_, p)| p.0),
        Some(Target::Rack(r)) => supply.as_ref().and_then(|sup| boxes.iter().find(|(b, _)| sup.0.rack_in(&b.id) == Some(*r)).map(|(_, p)| p.0)),
        Some(Target::Container { rack, .. }) => container_pos(*rack),
        _ => None,
    };
    inter.prompt = match &target {
        None => String::new(),
        Some(t) => {
            let size = match t {
                Target::Crate(e, _) | Target::Drop(e) => size_of(*e),
                _ => String::new(),
            };
            let mut p = format!("[{}] {}", key_label(&bindings, Tap::Interact), verb(t, &size));
            match t {
                Target::SeatLocked => p.push_str(&format!(" ({})", gp.licence_hint())),
                Target::Counter(g) => p.push_str(&gp.jobs_content.givers.get(g).map(|g| gp.text(&g.record.name)).unwrap_or_default()),
                Target::Panel(e) => p.push_str(&displays.get(*e).map(|(_, d)| d.name.clone()).unwrap_or_default()),
                Target::Item(e) => {
                    if let Ok((_, _, li)) = loose.get(*e) {
                        p.push_str(&items.defs.get(&li.def).map_or(li.def.clone(), |d| d.name.clone()));
                        if li.count > 1 {
                            p.push_str(&format!(" ({})", li.count));
                        }
                        if is_suit_def(&items, &li.def) && items.client.view.worn.is_some() {
                            p.push_str("  (wearing a suit)");
                        }
                    }
                }
                Target::Crate(e, Reach::Hands) if !hands_free(&items, &crates, &table, *e) => p.push_str(" (hands full: put the item away, 1 to 8)"),
                Target::Container { rack, by } => {
                    p.push_str(&specs.get(*rack).map(|s| s.name.clone()).unwrap_or_default());
                    if let Some(by) = by {
                        p.push_str(&format!("  (in use: player {by})"));
                    }
                }
                Target::LeavePanel(e) => {
                    p.push_str(&displays.get(*e).map(|(_, d)| d.name.clone()).unwrap_or_default());
                    p.push_str(&format!("  [{}] leave", key_label(&bindings, Tap::DisplayLeave)));
                }
                Target::Accept(j) => {
                    p.push_str(&gp.offer_label(*j));
                    p.push_str(&format!(" {}{}", key_hint(&bindings, Tap::NextOffer, "next"), key_hint(&bindings, Tap::Decline, "decline")));
                }
                _ => {}
            }
            if matches!(t, Target::Drop(_)) {
                p.push_str(&format!("  [{}] throw", key_label(&bindings, Tap::Throw)));
            }
            p
        }
    };

    inter.view = match &target {
        None => PromptView::default(),
        Some(t) => {
            let size = match t {
                Target::Crate(e, _) | Target::Drop(e) => size_of(*e),
                _ => String::new(),
            };
            let (drop_key, throw_key) = (key_cap(&bindings, Tap::Drop), key_cap(&bindings, Tap::Throw));
            let mut v = PromptView::default();
            let mut set = |verb: &str, name: String| {
                v.verb = verb.into();
                v.name = name;
            };
            let mut note = (String::new(), Tone::Normal);
            match t {
                Target::Seat => set("Sit", "Pilot seat".into()),
                Target::SeatLocked => {
                    set("Sit", "Pilot seat".into());
                    note = (format!("▲ {}", gp.licence_hint()), Tone::Warn);
                }
                Target::StandUp => set("Leave", "Pilot seat".into()),
                Target::Crate(e, Reach::Hands) => {
                    set("Pick up", format!("{size} crate"));
                    if !hands_free(&items, &crates, &table, *e) {
                        note = (format!("▲ Hands full · {drop_key} drop"), Tone::Warn);
                    }
                }
                Target::Crate(_, Reach::Tool) => {
                    set("Pull", format!("{size} crate"));
                    note = ("Grab tool".into(), Tone::Normal);
                }
                Target::Drop(_) => {
                    set("Set down", format!("{size} crate"));
                    note = (format!("{throw_key} throw"), Tone::Normal);
                }
                Target::Door { open } => set(if *open { "Close" } else { "Open" }, "Engine room door".into()),
                Target::BridgeDoor { open } => set(if *open { "Close" } else { "Open" }, "Bridge door".into()),
                Target::EquipDoor { open } => set(if *open { "Close" } else { "Open" }, "Equipment room door".into()),
                Target::AirlockDoor { open } => set(if *open { "Close" } else { "Open" }, "Airlock door".into()),
                Target::AirlockLocked => {
                    set("Locked", "Airlock door".into());
                    note = ("▲ LOCKED".into(), Tone::Warn);
                }
                Target::Rack(r) => {
                    // The rack's own name (r41, "Suit rack" for the airlock's), else the extinguishers'.
                    let name = supply.as_ref().and_then(|sup| sup.0.racks.iter().find(|x| x.id == *r)).and_then(|x| x.name.clone());
                    set("Swap", name.unwrap_or_else(|| "Extinguisher rack".into()))
                }
                Target::Container { rack, by } => {
                    set("Open", specs.get(*rack).map(|s| s.name.clone()).unwrap_or_default());
                    if let Some(by) = by {
                        note = (format!("IN USE · PLAYER {by}"), Tone::Warn);
                    }
                }
                Target::Counter(g) => set("Talk to", gp.jobs_content.givers.get(g).map(|g| gp.text(&g.record.name)).unwrap_or_default()),
                Target::Accept(j) => {
                    set("Take job", gp.offer_label(*j));
                    let hints: Vec<String> = [(Tap::NextOffer, "next"), (Tap::Decline, "decline")]
                        .into_iter()
                        .filter(|(t, _)| key_label(&bindings, *t) != "?")
                        .map(|(t, verb)| format!("{} {verb}", key_cap(&bindings, t)))
                        .collect();
                    note = (hints.join(" · "), Tone::Normal);
                }
                Target::Panel(e) => set("Use", displays.get(*e).map(|(_, d)| d.name.clone()).unwrap_or_default()),
                Target::LeavePanel(e) => {
                    set("Leave", displays.get(*e).map(|(_, d)| d.name.clone()).unwrap_or_default());
                    note = (format!("or {}", key_cap(&bindings, Tap::DisplayLeave)), Tone::Normal);
                }
                Target::Item(e) => {
                    if let Ok((_, _, li)) = loose.get(*e) {
                        let name = items.defs.get(&li.def).map_or(li.def.clone(), |d| d.name.clone());
                        if is_suit_def(&items, &li.def) {
                            set("Put on", name);
                            if items.client.view.worn.is_some() {
                                note = ("WEARING A SUIT".into(), Tone::Warn);
                            }
                        } else {
                            set("Collect", if li.count > 1 { format!("{name} ×{}", li.count) } else { name });
                        }
                    }
                }
            }
            v.note = note.0;
            v.note_tone = note.1;
            v
        }
    };

    if !actions.take_tap(Tap::Interact) {
        return;
    }
    let Some(t) = target else { return };
    inter.used.push(t.clone());
    let (_, mut ship, ..) = ships.get_mut(ship_e).unwrap();
    match t {
        Target::StandUp => {
            pl.seated = false;
            ship.piloted = false; // hover assist now holds the ship
            pl.w.pos = DVec3::new(0.0, 0.32, SEAT_POS.z + 1.0);
            pl.w.halt();
        }
        Target::Seat => {
            pl.seated = true;
            ship.piloted = true;
            if ship.parked {
                ship.parked = false;
                commands.entity(ship_e).insert(RigidBody::Dynamic);
            }
            pl.w.pos = SEAT_POS - DVec3::new(0.0, 0.3, 0.0);
            pl.w.halt();
        }
        Target::Crate(e, Reach::Hands) if !hands_free(&items, &crates, &table, e) => {}
        Target::Crate(e, reach) => {
            if let Ok((_, c)) = crates.get(e) {
                let f = if pl.ship.is_some() { frame } else { Frame::IDENTITY };
                let eye = pl.world_pos(f) + pl.world_up(f) * EYE_HEIGHT;
                let fwd = f.rot * pl.w.forward;
                grab.start(cfg, e, c, &table.0.sizes[c.size], reach, eye, fwd, &frame);
            }
        }
        Target::Drop(_) => grab.release(),
        Target::AirlockLocked => {}
        Target::SeatLocked => gp.notify(gameplay_core::notice::Notice::new(gameplay_core::notice::NoticeKind::Warning, "notice.licence.needed")),
        Target::Counter(g) => gp.open_counter(g),
        Target::Accept(j) => {
            gp.push_job(crate::gameplay::HOST, jobs_core::JobEvent::OfferAccepted { job: j });
            gp.close_counter();
        }
        Target::Panel(e) => *active = ActiveDisplay::Using(e),
        Target::Item(e) => {
            if let Ok((_, _, li)) = loose.get(e)
                && let Some(item) = li.id
            {
                // A suit goes on (not into the bar); while one is worn F does nothing.
                let req = if is_suit_def(&items, &li.def) { item_core::Request::CollectWear { item } } else { item_core::Request::Collect { item } };
                let _ = items.request(time.elapsed_secs_f64(), req);
            }
        }
        Target::LeavePanel(_) => *active = ActiveDisplay::Idle,
        Target::Door { .. } => items.ask_room(item_core::wire::RoomAsk::ToggleDoor),
        Target::BridgeDoor { .. } => items.ask_room(item_core::wire::RoomAsk::ToggleBridgeDoor),
        Target::EquipDoor { .. } => items.ask_room(item_core::wire::RoomAsk::ToggleEquipDoor),
        Target::AirlockDoor { .. } => items.ask_reactor(item_core::wire::ReactorAsk::Airlock(item_core::wire::AirlockAsk::ToggleInner)),
        Target::Container { rack, by: None } => {
            let now = time.elapsed_secs_f64();
            if cw.rack == Some(rack) {
                cw.close(now, &mut items, &mut ui);
            } else {
                cw.open(now, rack, &mut items, &mut ui);
            }
        }
        Target::Container { by: Some(_), .. } => {}
        Target::Rack(rack) => {
            let slot = items.client.view.selected;
            let _ = items.request(time.elapsed_secs_f64(), item_core::Request::Swap { slot, rack });
        }
    }
}
