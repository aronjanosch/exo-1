//! The fabricator (#326): a machine that turns ore into parts. It knows counts by item id and
//! nothing about containers: the glue reads what the input places hold, asks `start`, steps it
//! with the ship's power, and at `Finished` removes the inputs and adds the output in the rack.
//!
//! `Idle` is also "done": the finished product waits in the output place, the machine holds nothing.
//! No Bevy types. Recipes are the fabricator's own (`FabRecipe`), apart from the lab's.
use content_core::{File, parse_strict};
use serde::{Deserialize, Serialize};

pub const PATH: &str = "fabricator/fabricator.json";

/// An item id and a count.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Amount {
    pub item: String,
    pub count: u32,
}

impl Amount {
    pub fn new(item: &str, count: u32) -> Amount {
        Amount { item: item.into(), count }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FabRecipe {
    pub inputs: Vec<Amount>,
    pub output: Amount,
    /// Seconds of powered running.
    pub seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FabConfig {
    pub recipes: Vec<FabRecipe>,
}

impl FabConfig {
    pub fn load(files: &[File]) -> Result<FabConfig, Vec<String>> {
        let Some(f) = files.iter().find(|f| f.path == PATH) else { return Err(vec![format!("{PATH}: missing")]) };
        FabConfig::parse(&f.text)
    }

    pub fn parse(text: &str) -> Result<FabConfig, Vec<String>> {
        let c = parse_strict::<FabConfig>(PATH, text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        if c.recipes.is_empty() {
            e.push(format!("{PATH}: recipes: needs at least one"));
        }
        for (i, r) in c.recipes.iter().enumerate() {
            let mut need = |ok: bool, rule: &str| {
                if !ok {
                    e.push(format!("{PATH}: recipes[{i}]: {rule}"));
                }
            };
            need(r.seconds.is_finite() && r.seconds > 0.0, "seconds must be > 0");
            need(!r.inputs.is_empty(), "needs at least one input");
            need(r.inputs.iter().all(|a| a.count > 0 && !a.item.is_empty()) && r.output.count > 0 && !r.output.item.is_empty(), "every amount needs an item and a count > 0");
            let mut ids: Vec<&str> = r.inputs.iter().map(|a| a.item.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            need(ids.len() == r.inputs.len(), "an input item may appear once");
        }
        if e.is_empty() { Ok(c) } else { Err(e) }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum FabState {
    Idle,
    /// `recipe` is an index into the config's recipes; `left_s` is finite and never negative.
    Running { recipe: usize, left_s: f64 },
}

/// Why the button does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FabRefusal {
    NoRecipe,
    AlreadyRunning,
    /// The output place holds another item.
    OutputOtherItem,
    /// The output place has no room for the product.
    OutputFull,
}

/// A run that ended: what the glue removes from the input places and adds to the output place.
#[derive(Clone, Debug, PartialEq)]
pub struct Finished {
    pub remove: Vec<Amount>,
    pub add: Amount,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Fabricator {
    recipes: Vec<FabRecipe>,
    state: FabState,
}

impl Fabricator {
    pub fn new(cfg: &FabConfig) -> Fabricator {
        Fabricator { recipes: cfg.recipes.clone(), state: FabState::Idle }
    }

    pub fn state(&self) -> &FabState {
        &self.state
    }

    pub fn recipes(&self) -> &[FabRecipe] {
        &self.recipes
    }

    pub fn running(&self) -> bool {
        matches!(self.state, FabState::Running { .. })
    }

    /// The recipe being made and the seconds left.
    pub fn progress(&self) -> Option<(&FabRecipe, f64)> {
        match self.state {
            FabState::Running { recipe, left_s } => self.recipes.get(recipe).map(|r| (r, left_s)),
            FabState::Idle => None,
        }
    }

    /// The first recipe the load covers: every input item present in at least its count (more
    /// stays where it is).
    pub fn match_recipe(&self, inputs: &[Amount]) -> Option<usize> {
        let have = |item: &str| inputs.iter().filter(|a| a.item == item).map(|a| a.count as u64).sum::<u64>();
        self.recipes.iter().position(|r| r.inputs.iter().all(|need| have(&need.item) >= need.count as u64))
    }

    /// Starts the run for the load. `output` is what the output place holds now (item, count) and
    /// `cap` the most of an item one place takes.
    pub fn start(&mut self, inputs: &[Amount], output: Option<(&str, u32)>, cap: impl Fn(&str) -> u32) -> Result<(), FabRefusal> {
        if self.running() {
            return Err(FabRefusal::AlreadyRunning);
        }
        let i = self.match_recipe(inputs).ok_or(FabRefusal::NoRecipe)?;
        let out = &self.recipes[i].output;
        let held = match output {
            Some((item, _)) if item != out.item => return Err(FabRefusal::OutputOtherItem),
            Some((_, n)) => n,
            None => 0,
        };
        if held as u64 + out.count as u64 > cap(&out.item) as u64 {
            return Err(FabRefusal::OutputFull);
        }
        self.state = FabState::Running { recipe: i, left_s: self.recipes[i].seconds };
        Ok(())
    }

    /// Runs `dt` seconds. Without power nothing moves. At zero the run ends once, however large
    /// `dt` was, and the machine is idle again. A `dt` that is negative or not finite is no time.
    pub fn step(&mut self, dt: f64, powered: bool) -> Option<Finished> {
        let FabState::Running { recipe, left_s } = &mut self.state else { return None };
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        if !powered {
            return None;
        }
        *left_s = (*left_s - dt).max(0.0);
        if *left_s > 0.0 {
            return None;
        }
        let r = self.recipes.get(*recipe).cloned();
        self.state = FabState::Idle;
        r.map(|r| Finished { remove: r.inputs, add: r.output })
    }

    /// The machine lost its run (a reload, a recipe set that changed): idle again.
    pub fn reset(&mut self) {
        self.state = FabState::Idle;
    }

    /// Repairs a state that does not fit the recipes (a bad save): idle, and a `left_s` clamped to
    /// the recipe's time.
    pub fn sanitize(&mut self) {
        if let FabState::Running { recipe, left_s } = &mut self.state {
            match self.recipes.get(*recipe) {
                Some(r) if left_s.is_finite() => *left_s = left_s.clamp(0.0, r.seconds),
                _ => self.state = FabState::Idle,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{ "recipes": [
        { "inputs": [{ "item": "grumblite", "count": 2 }], "output": { "item": "fuse", "count": 1 }, "seconds": 12 },
        { "inputs": [{ "item": "wobble_quartz", "count": 3 }], "output": { "item": "antimatter_canister", "count": 1 }, "seconds": 30 }
    ] }"#;

    fn fab() -> Fabricator {
        Fabricator::new(&FabConfig::parse(JSON).unwrap())
    }

    fn cap(item: &str) -> u32 {
        if item == "fuse" { 4 } else { 1 }
    }

    fn load(item: &str, n: u32) -> Vec<Amount> {
        vec![Amount::new(item, n)]
    }

    #[test]
    fn nothing_is_made_without_a_full_recipe() {
        let mut f = fab();
        assert_eq!(f.start(&[], None, cap), Err(FabRefusal::NoRecipe));
        assert_eq!(f.start(&load("grumblite", 1), None, cap), Err(FabRefusal::NoRecipe));
        assert_eq!(f.start(&load("snoozium", 9), None, cap), Err(FabRefusal::NoRecipe));
        assert_eq!(f.start(&[Amount::new("grumblite", 1), Amount::new("wobble_quartz", 2)], None, cap), Err(FabRefusal::NoRecipe), "two half loads are no recipe");
        assert!(!f.running());
        assert_eq!(f.step(1e6, true), None, "an idle machine makes nothing");
    }

    #[test]
    fn a_full_load_matches_and_split_stacks_add_up() {
        let f = fab();
        assert_eq!(f.match_recipe(&load("grumblite", 2)), Some(0));
        assert_eq!(f.match_recipe(&[Amount::new("grumblite", 1), Amount::new("grumblite", 1)]), Some(0), "one in each place");
        assert_eq!(f.match_recipe(&load("wobble_quartz", 3)), Some(1));
        assert_eq!(f.match_recipe(&load("grumblite", 3)), Some(0), "more than needed still matches");
    }

    #[test]
    fn the_inputs_are_reported_once_and_the_machine_is_idle_after() {
        let mut f = fab();
        f.start(&load("grumblite", 2), None, cap).unwrap();
        assert_eq!(f.step(11.9, true), None);
        let done = f.step(0.2, true).expect("finished");
        assert_eq!(done.remove, vec![Amount::new("grumblite", 2)]);
        assert_eq!(done.add, Amount::new("fuse", 1));
        assert!(!f.running());
        assert_eq!(f.step(100.0, true), None, "reported once");
    }

    #[test]
    fn a_huge_step_finishes_once() {
        let mut f = fab();
        f.start(&load("grumblite", 2), None, cap).unwrap();
        assert!(f.step(1e12, true).is_some());
        assert_eq!(f.step(1e12, true), None);
    }

    #[test]
    fn no_power_no_progress() {
        let mut f = fab();
        f.start(&load("grumblite", 2), None, cap).unwrap();
        f.step(5.0, true);
        let left = f.progress().unwrap().1;
        assert_eq!(f.step(1000.0, false), None);
        assert_eq!(f.progress().unwrap().1, left, "paused");
        assert!(f.step(left + 0.01, true).is_some(), "resumes where it stopped");
    }

    #[test]
    fn the_time_left_stays_finite_and_never_negative() {
        let mut f = fab();
        f.start(&load("grumblite", 2), None, cap).unwrap();
        for dt in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -5.0, 0.0] {
            assert_eq!(f.step(dt, true), None, "{dt} is no time");
            let left = f.progress().unwrap().1;
            assert!(left.is_finite() && (0.0..=12.0).contains(&left), "{dt}: {left}");
        }
        assert_eq!(f.progress().unwrap().1, 12.0, "bad steps took nothing");
        for _ in 0..1000 {
            f.step(0.0133, true);
            if let Some((_, left)) = f.progress() {
                assert!(left.is_finite() && left >= 0.0);
            }
        }
    }

    #[test]
    fn a_second_start_while_running_changes_nothing() {
        let mut f = fab();
        f.start(&load("grumblite", 2), None, cap).unwrap();
        f.step(4.0, true);
        let before = f.clone();
        assert_eq!(f.start(&load("grumblite", 2), None, cap), Err(FabRefusal::AlreadyRunning));
        assert_eq!(f, before);
    }

    #[test]
    fn the_output_place_decides_whether_it_starts() {
        let mut f = fab();
        assert_eq!(f.start(&load("grumblite", 2), Some(("snoozium", 1)), cap), Err(FabRefusal::OutputOtherItem));
        assert_eq!(f.start(&load("grumblite", 2), Some(("fuse", 4)), cap), Err(FabRefusal::OutputFull));
        assert_eq!(f.start(&load("wobble_quartz", 3), Some(("antimatter_canister", 1)), cap), Err(FabRefusal::OutputFull));
        assert!(!f.running());
        assert_eq!(f.start(&load("grumblite", 2), Some(("fuse", 3)), cap), Ok(()), "room for one more");
    }

    #[test]
    fn a_run_survives_save_and_load() {
        let mut f = fab();
        f.start(&load("wobble_quartz", 3), None, cap).unwrap();
        f.step(7.5, true);
        let text = serde_json::to_string(&f).unwrap();
        let mut back: Fabricator = serde_json::from_str(&text).unwrap();
        assert_eq!(back, f);
        assert_eq!(back.progress().unwrap().1, 22.5);
        assert!(back.step(22.5, true).is_some());
        let mut bad: Fabricator = serde_json::from_str(&text.replace("22.5", "-3.0")).unwrap();
        bad.sanitize();
        assert!(bad.progress().is_none_or(|(_, l)| l >= 0.0));
        let mut gone = f.clone();
        gone.state = FabState::Running { recipe: 9, left_s: 1.0 };
        gone.sanitize();
        assert!(!gone.running(), "a recipe that is not there ends the run");
    }

    #[test]
    fn the_config_is_strict() {
        assert!(FabConfig::parse(r#"{ "recipes": [] }"#).is_err());
        assert!(FabConfig::parse(&JSON.replace("12", "0")).is_err(), "no zero time");
        assert!(FabConfig::parse(&JSON.replace("\"seconds\"", "\"secs\"")).is_err(), "unknown field");
        assert!(FabConfig::parse(&JSON.replace("\"count\": 2", "\"count\": 0")).is_err());
    }
}
