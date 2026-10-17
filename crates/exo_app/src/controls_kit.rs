//! Glue for the physical controls kit (#315): binds `controls_core` parts to glTF node names and
//! animates them. Generic: the reactor (#316) and later machines only fill a `Machine` and a
//! `KitBindings` list on their scene root.
//!
//! Flow (host side): a F press on a part becomes a `PressPart` message; `apply_presses` runs it on
//! the machine's `Controls` and answers with a `PartSound` (the sound hook, also the place to
//! forward the result to clients). State is only read from `Controls`, so a client that gets the
//! host's `Controls` sees the same positions.
use bevy::prelude::*;
use controls_core::{Controls, Hand, ItemId, Outcome, PartId};
use std::collections::BTreeSet;

pub fn plugin(app: &mut App) {
    app.add_message::<PressPart>().add_message::<PartSound>();
    app.add_systems(FixedUpdate, (apply_presses, tick_buttons).chain().in_set(crate::phases::Fx::Walker));
    app.add_systems(Update, (bind_nodes, animate_nodes).chain().in_set(crate::phases::Frame::World));
}

/// One machine's controls, on its scene root.
#[derive(Component, Default)]
pub struct Machine {
    pub controls: Controls,
    /// Lever steps the machine refuses right now, as (part, target detent). The machine's own
    /// logic (e.g. START not latching in PRIME without its conditions) refreshes this each tick.
    pub refused: BTreeSet<(PartId, u8)>,
    /// Items a slot may not take; empty = any item (the machine pre-checks with its own rules).
    pub rejected_items: BTreeSet<ItemId>,
}

/// How a part moves, relative to its node's rest pose and local axis (`art/ship/reactor.md`).
#[derive(Clone, Debug)]
pub struct Binding {
    pub part: PartId,
    pub node: String,
    /// Local axis: rotation about it, or the direction of a slide.
    pub axis: Vec3,
    /// Degrees per unit of `Controls::position` (a lever: per detent), or metres when `slide`.
    pub per_step: f32,
    pub slide: bool,
}

impl Binding {
    pub fn rotate(part: &str, node: &str, axis: Vec3, deg_per_step: f32) -> Self {
        Self { part: part.into(), node: node.into(), axis, per_step: deg_per_step, slide: false }
    }
    pub fn slide(part: &str, node: &str, axis: Vec3, metres: f32) -> Self {
        Self { part: part.into(), node: node.into(), axis, per_step: metres, slide: true }
    }
}

/// Bindings to resolve under this root; emptied once every node is found.
#[derive(Component, Default)]
pub struct KitBindings(pub Vec<Binding>);

/// A bound node: what `animate_nodes` moves.
#[derive(Component)]
pub struct KitNode {
    pub machine: Entity,
    pub binding: Binding,
    pub rest: Transform,
    /// The position drawn now; eases toward `Controls::position`.
    pub shown: f32,
}

/// Positions per second a node eases at (a lever crosses a detent in about 0.1 s).
pub const EASE_PER_S: f32 = 10.0;

/// A player pressed F on a part (client to host).
#[derive(Message, Clone, Debug)]
pub struct PressPart {
    pub machine: Entity,
    pub part: PartId,
    pub hand: Hand,
}

/// What a press did: the sound hook (click, clunk, thud) and the state change to broadcast.
#[derive(Message, Clone, Debug)]
pub struct PartSound {
    pub machine: Entity,
    pub part: PartId,
    pub outcome: Outcome,
}

fn apply_presses(mut presses: MessageReader<PressPart>, mut machines: Query<&mut Machine>, mut out: MessageWriter<PartSound>) {
    for p in presses.read() {
        let Ok(mut m) = machines.get_mut(p.machine) else { continue };
        let m = &mut *m;
        let part = p.part.clone();
        let outcome = m.controls.press(&part, p.hand, |i| !m.refused.contains(&(part.clone(), i)), |it| !m.rejected_items.contains(&it));
        out.write(PartSound { machine: p.machine, part, outcome });
    }
}

fn tick_buttons(time: Res<Time>, mut machines: Query<&mut Machine>) {
    for mut m in &mut machines {
        m.controls.tick(time.delta_secs_f64());
    }
}

/// Finds the named nodes under each root once its scene has spawned.
fn bind_nodes(mut commands: Commands, mut roots: Query<(Entity, &mut KitBindings)>, children: Query<&Children>, names: Query<(&Name, &Transform)>) {
    for (root, mut pending) in &mut roots {
        if pending.0.is_empty() {
            continue;
        }
        pending.0.retain(|b| {
            let found = children.iter_descendants(root).find_map(|e| names.get(e).ok().filter(|(n, _)| n.as_str() == b.node).map(|(_, t)| (e, *t)));
            let Some((e, rest)) = found else { return true };
            commands.entity(e).insert(KitNode { machine: root, binding: b.clone(), rest, shown: 0.0 });
            false
        });
    }
}

/// Pure: moves `shown` toward `target` by at most `step`.
pub fn ease(shown: f32, target: f32, step: f32) -> f32 {
    shown + (target - shown).clamp(-step, step)
}

fn animate_nodes(time: Res<Time>, machines: Query<&Machine>, mut nodes: Query<(&mut KitNode, &mut Transform)>) {
    for (mut n, mut t) in &mut nodes {
        let Ok(m) = machines.get(n.machine) else { continue };
        let target = m.controls.position(&n.binding.part) as f32;
        n.shown = ease(n.shown, target, EASE_PER_S * time.delta_secs());
        let amount = n.shown * n.binding.per_step;
        let axis = n.binding.axis.normalize_or_zero();
        *t = if n.binding.slide {
            Transform { translation: n.rest.translation + n.rest.rotation * (axis * amount), ..n.rest }
        } else {
            Transform { rotation: n.rest.rotation * Quat::from_axis_angle(axis, amount.to_radians()), ..n.rest }
        };
    }
}
