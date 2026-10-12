//! The power grid: the reactor's output shared between five consumer groups.
//!
//! Each group has a demand (what its devices draw at full work, set by the glue), a limit 0..1
//! that the engineer sets on the power board (it asks for `demand * limit`), a priority and a
//! minimum supply ratio. Groups are served from the highest priority down; groups of equal
//! priority share a shortfall in proportion. A group whose share falls below its minimum ratio
//! switches off and leaves its power to the rest, so under a brownout low priorities go first.
//! A group works at `factor = delivered / demand`: a lower limit or less power makes it slower,
//! not off, down to its minimum ratio.
use crate::reactor::{Reactor, ReactorEvent};

#[cfg(test)]
mod tests;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Engines,
    Shields,
    Weapons,
    LifeSupport,
    /// Lights and other small things.
    Aux,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Engines, Group::Shields, Group::Weapons, Group::LifeSupport, Group::Aux];

    fn index(self) -> usize {
        self as usize
    }
}

/// Settings of one group.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GroupState {
    /// kW its devices draw at full work.
    pub demand_kw: f64,
    /// 0..1, set on the power board.
    pub limit: f64,
    /// Higher is served first.
    pub priority: u8,
    /// Below this share of its request the group switches off.
    pub min_ratio: f64,
}

/// What one group got in the last distribution.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Supply {
    /// kW asked for: `demand * limit`.
    pub request_kw: f64,
    pub delivered_kw: f64,
    /// `delivered / request` (1 when nothing was asked for).
    pub ratio: f64,
    /// How hard the group works, 0..1: `delivered / demand`.
    pub factor: f64,
    /// Asked for power and got at least its minimum ratio.
    pub on: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    groups: [GroupState; 5],
    supply: [Supply; 5],
}

impl Default for Grid {
    /// TODO(initiator): placeholder demands for the dev area (they add up to the default
    /// reactor's 1000 kW), priorities and minimum ratios (engines 0.5 and 0.3 for small
    /// things follow the original).
    fn default() -> Grid {
        let g = |demand_kw, priority, min_ratio| GroupState { demand_kw, limit: 1.0, priority, min_ratio };
        Grid {
            groups: [
                g(400.0, 3, 0.5), // engines
                g(250.0, 3, 0.5), // shields
                g(200.0, 2, 0.5), // weapons
                g(100.0, 4, 0.3), // life support
                g(50.0, 1, 0.3),  // aux
            ],
            supply: [Supply::default(); 5],
        }
    }
}

impl Grid {
    pub fn group(&self, g: Group) -> &GroupState {
        &self.groups[g.index()]
    }

    pub fn group_mut(&mut self, g: Group) -> &mut GroupState {
        &mut self.groups[g.index()]
    }

    pub fn set_demand(&mut self, g: Group, kw: f64) {
        self.groups[g.index()].demand_kw = kw.max(0.0);
    }

    /// The power board slider, 0..1.
    pub fn set_limit(&mut self, g: Group, limit: f64) {
        self.groups[g.index()].limit = limit.clamp(0.0, 1.0);
    }

    pub fn set_priority(&mut self, g: Group, priority: u8) {
        self.groups[g.index()].priority = priority;
    }

    /// kW all groups ask for together: the load on the reactor.
    pub fn request_kw(&self) -> f64 {
        self.groups.iter().map(request).sum()
    }

    /// Result of the last `distribute` for one group.
    pub fn supply(&self, g: Group) -> Supply {
        self.supply[g.index()]
    }

    /// How hard a group works, 0..1 (its `Supply::factor`).
    pub fn factor(&self, g: Group) -> f64 {
        self.supply[g.index()].factor
    }

    /// Shares `supply_kw` between the groups; returns what is left over.
    pub fn distribute(&mut self, supply_kw: f64) -> f64 {
        let mut left = supply_kw.max(0.0);
        let mut delivered = [0.0; 5];
        let mut on = [false; 5];
        let mut prios: Vec<u8> = self.groups.iter().map(|g| g.priority).collect();
        prios.sort_unstable_by(|a, b| b.cmp(a));
        prios.dedup();
        for p in prios {
            let mut tier: Vec<usize> = (0..5).filter(|&i| self.groups[i].priority == p && request(&self.groups[i]) > 0.0).collect();
            loop {
                let asked: f64 = tier.iter().map(|&i| request(&self.groups[i])).sum();
                if asked <= left {
                    for &i in &tier {
                        delivered[i] = request(&self.groups[i]);
                        on[i] = true;
                    }
                    left -= asked;
                    break;
                }
                let share = left / asked;
                // The group that needs the highest ratio drops out first (ties: the bigger one).
                let drop = tier
                    .iter()
                    .copied()
                    .filter(|&i| self.groups[i].min_ratio > share)
                    .max_by(|&a, &b| {
                        let (ga, gb) = (&self.groups[a], &self.groups[b]);
                        ga.min_ratio.total_cmp(&gb.min_ratio).then(request(ga).total_cmp(&request(gb)))
                    });
                match drop {
                    Some(d) => tier.retain(|&i| i != d),
                    None => {
                        for &i in &tier {
                            delivered[i] = request(&self.groups[i]) * share;
                            on[i] = true;
                        }
                        left = 0.0;
                        break;
                    }
                }
            }
        }
        for i in 0..5 {
            let g = &self.groups[i];
            let req = request(g);
            self.supply[i] = Supply {
                request_kw: req,
                delivered_kw: delivered[i],
                ratio: if req > 0.0 { delivered[i] / req } else { 1.0 },
                factor: if g.demand_kw > 0.0 { delivered[i] / g.demand_kw } else { 0.0 },
                on: on[i],
            };
        }
        left
    }
}

fn request(g: &GroupState) -> f64 {
    g.demand_kw * g.limit
}

/// One step of reactor and grid together: the grid's request is the reactor's load, the
/// reactor's output is shared out.
pub fn run(reactor: &mut Reactor, grid: &mut Grid, dt: f64) -> Vec<ReactorEvent> {
    let ev = reactor.step(dt, grid.request_kw());
    grid.distribute(reactor.output_kw());
    ev
}
