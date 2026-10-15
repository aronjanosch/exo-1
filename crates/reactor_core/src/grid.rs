//! The power grid: the junction box's supply shared between three consumer groups.
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Engines,
    LifeSupport,
    Lights,
    /// Idle until the r40 lanes drive them: they draw nothing (`from_tuning` starts their demand at 0).
    Shields,
    Weapons,
    JumpDrive,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::Engines, Group::LifeSupport, Group::Lights, Group::Shields, Group::Weapons, Group::JumpDrive];

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GroupState {
    pub demand_kw: f64,
    pub breaker: bool,
    /// Higher is served first.
    pub priority: u8,
    /// Below this share of its request the group switches off.
    pub min_ratio: f64,
    /// 0..1: the request is `demand × limit` (the breaker on); 0 is off.
    pub limit: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Supply {
    pub request_kw: f64,
    pub delivered_kw: f64,
    /// `delivered / request` (1 when nothing was asked for).
    pub ratio: f64,
    /// How hard the group works, 0..1: `delivered / demand` (with a limit of 1 and full delivery
    /// 1; a limit of 0.6 caps it at 0.6).
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

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTuning {
    pub demand_kw: f64,
    pub priority: u8,
    pub min_ratio: f64,
    /// Start value of the limit, 0..1 (optional in the file, 1 when left out).
    #[serde(default = "full_limit")]
    pub limit: f64,
}

fn full_limit() -> f64 {
    1.0
}

/// The grid's demands, priorities and minimum ratios. TODO(initiator): placeholder values; the
/// demands add up to the default 500 kW load.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GridTuning {
    pub engines: GroupTuning,
    pub life_support: GroupTuning,
    pub lights: GroupTuning,
    pub shields: GroupTuning,
    pub weapons: GroupTuning,
    pub jump_drive: GroupTuning,
}

impl Default for GridTuning {
    fn default() -> GridTuning {
        GridTuning {
            engines: GroupTuning { demand_kw: 800.0, priority: 2, min_ratio: 0.5, limit: 1.0 },
            life_support: GroupTuning { demand_kw: 400.0, priority: 3, min_ratio: 0.3, limit: 1.0 },
            lights: GroupTuning { demand_kw: 80.0, priority: 1, min_ratio: 0.3, limit: 1.0 },
            shields: GroupTuning { demand_kw: 300.0, priority: 2, min_ratio: 0.3, limit: 1.0 },
            weapons: GroupTuning { demand_kw: 400.0, priority: 2, min_ratio: 0.5, limit: 1.0 },
            jump_drive: GroupTuning { demand_kw: 400.0, priority: 1, min_ratio: 0.5, limit: 1.0 },
        }
    }
}

impl GridTuning {
    pub fn group(&self, g: Group) -> GroupTuning {
        match g {
            Group::Engines => self.engines,
            Group::LifeSupport => self.life_support,
            Group::Lights => self.lights,
            Group::Shields => self.shields,
            Group::Weapons => self.weapons,
            Group::JumpDrive => self.jump_drive,
        }
    }
}

impl Default for Grid {
    fn default() -> Grid {
        Grid::from_tuning(&GridTuning::default())
    }
}

impl Grid {
    pub fn from_tuning(t: &GridTuning) -> Grid {
        let g = |s: GroupTuning| GroupState { demand_kw: s.demand_kw, breaker: true, priority: s.priority, min_ratio: s.min_ratio, limit: s.limit };
        // The idle groups start at 0 kW: their tuned demand is what they ask for once r40 drives them,
        // and the existing loads (and their tests) stay as they were.
        let idle = |s: GroupTuning| GroupState { demand_kw: 0.0, ..g(s) };
        Grid {
            groups: [g(t.engines), g(t.life_support), g(t.lights), idle(t.shields), idle(t.weapons), idle(t.jump_drive)],
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

    pub fn set_breaker(&mut self, g: Group, on: bool) {
        self.groups[g.index()].breaker = on;
    }

    pub fn breaker(&self, g: Group) -> bool {
        self.groups[g.index()].breaker
    }

    pub fn set_priority(&mut self, g: Group, priority: u8) {
        self.groups[g.index()].priority = priority;
    }

    /// Sets a group's limit, clamped to 0..1: its request becomes `demand × limit`.
    pub fn set_limit(&mut self, g: Group, limit: f64) {
        self.groups[g.index()].limit = limit.clamp(0.0, 1.0);
    }

    pub fn limit(&self, g: Group) -> f64 {
        self.groups[g.index()].limit
    }

    /// kW all groups ask for together (the breakers' limited demands): the load on the reactor.
    pub fn request_kw(&self) -> f64 {
        self.requests().iter().sum()
    }

    /// kW one group asks for: its demand times its limit with the breaker on, 0 when off.
    pub fn request_of(&self, g: Group) -> f64 {
        request(&self.groups[g.index()])
    }

    pub fn requests(&self) -> [f64; N] {
        std::array::from_fn(|i| request(&self.groups[i]))
    }

    pub fn supply(&self, g: Group) -> Supply {
        self.supply[g.index()]
    }

    pub fn factor(&self, g: Group) -> f64 {
        self.supply[g.index()].factor
    }

    /// Shares `supply_kw` between the groups by their breakers; returns what is left over.
    pub fn distribute(&mut self, supply_kw: f64) -> f64 {
        let req = self.requests();
        self.distribute_with(supply_kw, req, [true; N])
    }

    /// Shares `supply_kw` between the groups asking `req` (kW each, in `Group::ALL` order), and
    /// writes the result only for the groups in `write`. The priorities and minimum ratios are the
    /// groups'. Returns what is left over. The engine room uses it for the aux reactor's groups.
    pub fn distribute_with(&mut self, supply_kw: f64, req: [f64; N], write: [bool; N]) -> f64 {
        let mut left = supply_kw.max(0.0);
        let mut delivered = [0.0; N];
        let mut on = [false; N];
        let mut prios: Vec<u8> = self.groups.iter().map(|g| g.priority).collect();
        prios.sort_unstable_by(|a, b| b.cmp(a));
        prios.dedup();
        for p in prios {
            let mut tier: Vec<usize> = (0..N).filter(|&i| self.groups[i].priority == p && req[i] > 0.0).collect();
            loop {
                let asked: f64 = tier.iter().map(|&i| req[i]).sum();
                if asked <= left {
                    for &i in &tier {
                        delivered[i] = req[i];
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
                    .max_by(|&a, &b| self.groups[a].min_ratio.total_cmp(&self.groups[b].min_ratio).then(req[a].total_cmp(&req[b])));
                match drop {
                    Some(d) => tier.retain(|&i| i != d),
                    None => {
                        for &i in &tier {
                            delivered[i] = req[i] * share;
                            on[i] = true;
                        }
                        left = 0.0;
                        break;
                    }
                }
            }
        }
        for i in 0..N {
            if !write[i] {
                continue;
            }
            let g = &self.groups[i];
            self.supply[i] = Supply {
                request_kw: req[i],
                delivered_kw: delivered[i],
                ratio: if req[i] > 0.0 { delivered[i] / req[i] } else { 1.0 },
                factor: if g.demand_kw > 0.0 { delivered[i] / g.demand_kw } else { 0.0 },
                on: on[i],
            };
        }
        left
    }
}

fn request(g: &GroupState) -> f64 {
    if g.breaker { g.demand_kw * g.limit } else { 0.0 }
}
