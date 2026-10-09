//! Object budget (#85): which loose objects go, per category. Pure rules; the game feeds the
//! objects in and despawns what comes back.
use serde::Deserialize;

/// One category's limits (`content/cargo/budget.json`).
#[derive(Deserialize, Copy, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BudgetRow {
    /// Most objects of the category alive at once.
    pub cap: usize,
    /// Loose objects kept on a planet the players left (frozen until they come back).
    pub persistence_cap: usize,
    /// A loose object resting on the players' planet, untouched this long (s), goes.
    pub timeout_s: f64,
    /// A loose object still moving (drifting in space) this far from every player and ship (m) goes.
    pub far_m: f64,
}

/// `content/cargo/budget.json`: one row per category; crates only for now.
#[derive(Deserialize, Copy, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(rename = "crate")]
    pub crates: BudgetRow,
}

impl Budget {
    pub fn from_json(s: &str) -> Result<Budget, String> {
        let b: Budget = crate::parse_tuning("budget.json", s)?;
        let r = &b.crates;
        if r.cap == 0 || r.persistence_cap > r.cap || !(r.timeout_s > 0.0) || !(r.far_m > 0.0) {
            return Err("budget.json: crate: cap > 0, persistence_cap <= cap, timeout_s > 0, far_m > 0".into());
        }
        Ok(b)
    }
}

/// What the budget needs to know about one object.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Obj {
    pub id: u64,
    /// Held, locked or in a cabin: never goes.
    pub protected: bool,
    /// Seconds since anyone last touched it (or since it appeared).
    pub idle: f64,
    /// Distance to the nearest player or ship (m).
    pub distance: f64,
    /// At rest (sleeping).
    pub resting: bool,
    /// On the planet the players are on (false: left behind on another one).
    pub here: bool,
}

/// Ids of the objects that go now, in the order they were chosen.
pub fn over_budget(row: &BudgetRow, objs: &[Obj]) -> Vec<u64> {
    let mut gone: Vec<u64> = Vec::new();
    // Timeout and distance, for loose objects where the players are.
    for o in objs.iter().filter(|o| !o.protected && o.here) {
        if o.resting && o.idle > row.timeout_s || !o.resting && o.distance > row.far_m {
            gone.push(o.id);
        }
    }
    // Left behind on another planet: keep the most recently touched up to the persistence cap.
    let mut away: Vec<&Obj> = objs.iter().filter(|o| !o.protected && !o.here && !gone.contains(&o.id)).collect();
    away.sort_by(|a, b| a.idle.total_cmp(&b.idle));
    gone.extend(away.iter().skip(row.persistence_cap).map(|o| o.id));
    // Over the cap: the loose ones go, those left behind first, then the longest untouched.
    let alive = objs.len() - gone.len();
    if alive > row.cap {
        let mut loose: Vec<&Obj> = objs.iter().filter(|o| !o.protected && !gone.contains(&o.id)).collect();
        loose.sort_by(|a, b| a.here.cmp(&b.here).then(b.idle.total_cmp(&a.idle)));
        gone.extend(loose.iter().take(alive - row.cap).map(|o| o.id));
    }
    gone
}
