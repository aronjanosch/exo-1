//! The power grid: the junction box's supply shared between three consumer groups.
//!
//! Each group has a demand (what its devices draw at full work, set by the glue), a breaker (on or
//! off, set on the junction box: an off group asks for nothing), a priority and a minimum supply
//! ratio. Groups are served from the highest priority down; groups of equal
//! priority share a shortfall in proportion. A group whose share falls below its minimum ratio
//! switches off and leaves its power to the rest, so under a brownout low priorities go first.
//! A group works at `factor = delivered / demand`: less power makes it slower, not off, down to
//! its minimum ratio.
//!
//! Trimmed from the dev-area grid (five groups) to the three the engine room slice needs.
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Engines,
    LifeSupport,
    /// Lights and other small things.
    Lights,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Engines, Group::LifeSupport, Group::Lights];

    fn index(self) -> usize {
        self as usize
    }
}

/// Settings of one group.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GroupState {
    /// kW its devices draw at full work.
    pub demand_kw: f64,
    /// The breaker: an off group asks for nothing.
    pub breaker: bool,
    /// Higher is served first.
    pub priority: u8,
    /// Below this share of its request the group switches off.
    pub min_ratio: f64,
}

/// What one group got in the last distribution.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Supply {
    /// kW asked for: `demand` when the breaker is on, 0 when off.
    pub request_kw: f64,
    pub delivered_kw: f64,
    /// `delivered / request` (1 when nothing was asked for).
    pub ratio: f64,
    /// How hard the group works, 0..1: `delivered / demand`.
    pub factor: f64,
    /// Asked for power and got at least its minimum ratio.
    pub on: bool,
}

const N: usize = Group::ALL.len();

#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    groups: [GroupState; N],
    supply: [Supply; N],
}

/// One group's values in the content file.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTuning {
    pub demand_kw: f64,
    pub priority: u8,
    pub min_ratio: f64,
}

/// The grid's demands, priorities and minimum ratios. TODO(initiator): placeholder values; the
/// demands add up to the default 500 kW load.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GridTuning {
    pub engines: GroupTuning,
    pub life_support: GroupTuning,
    pub lights: GroupTuning,
}

impl Default for GridTuning {
    fn default() -> GridTuning {
        GridTuning {
            engines: GroupTuning { demand_kw: 350.0, priority: 2, min_ratio: 0.5 },
            life_support: GroupTuning { demand_kw: 100.0, priority: 3, min_ratio: 0.3 },
            lights: GroupTuning { demand_kw: 50.0, priority: 1, min_ratio: 0.3 },
        }
    }
}

impl GridTuning {
    pub fn group(&self, g: Group) -> GroupTuning {
        match g {
            Group::Engines => self.engines,
            Group::LifeSupport => self.life_support,
            Group::Lights => self.lights,
        }
    }
}

impl Default for Grid {
    fn default() -> Grid {
        Grid::from_tuning(&GridTuning::default())
    }
}

impl Grid {
    /// The grid with every breaker on and the tuning's demands.
    pub fn from_tuning(t: &GridTuning) -> Grid {
        let g = |s: GroupTuning| GroupState { demand_kw: s.demand_kw, breaker: true, priority: s.priority, min_ratio: s.min_ratio };
        Grid {
            groups: [g(t.engines), g(t.life_support), g(t.lights)],
            supply: [Supply::default(); N],
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

    /// Switches a group's breaker on the junction box.
    pub fn set_breaker(&mut self, g: Group, on: bool) {
        self.groups[g.index()].breaker = on;
    }

    pub fn breaker(&self, g: Group) -> bool {
        self.groups[g.index()].breaker
    }

    pub fn set_priority(&mut self, g: Group, priority: u8) {
        self.groups[g.index()].priority = priority;
    }

    /// kW all groups ask for together (the breakers' demands): the load on the reactor.
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
        let mut delivered = [0.0; N];
        let mut on = [false; N];
        let mut prios: Vec<u8> = self.groups.iter().map(|g| g.priority).collect();
        prios.sort_unstable_by(|a, b| b.cmp(a));
        prios.dedup();
        for p in prios {
            let mut tier: Vec<usize> = (0..N).filter(|&i| self.groups[i].priority == p && request(&self.groups[i]) > 0.0).collect();
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
        for i in 0..N {
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
    if g.breaker { g.demand_kw } else { 0.0 }
}
