//! Station state machine: Idle, Loading, Running, Done, Blocked.
use std::collections::BTreeMap;

use gameplay_core::CommodityId;
use serde::{Deserialize, Serialize};

use crate::recipe::{RecipeId, StationKind};

#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// The station has no recipe set.
    NoRecipe,
    /// The commodity is not an input of the recipe.
    NotAnInput,
    /// The amount given would exceed the input quota.
    ExceedsQuota,
    /// A cycle is running or waiting for outputs to be taken.
    Busy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum StationState {
    Idle,
    /// Inputs partially filled; progress is (filled, total).
    Loading { progress: (u32, u32) },
    /// Time remaining.
    Running { time_remaining_s: f64 },
    /// Outputs waiting to be taken.
    Done,
    /// Outputs not taken, second cycle requested: next recipe or input insert refused.
    Blocked,
}

/// A station: kind, current recipe, state, and inventories.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub kind: StationKind,
    recipe: Option<RecipeId>,
    state: StationState,
    inputs: BTreeMap<CommodityId, u32>,
    outputs: BTreeMap<CommodityId, u32>,
    /// Expected outputs from the recipe, so we can populate outputs without having recipe access in step()
    expected_outputs: Vec<crate::recipe::Commodity>,
}

impl Station {
    pub fn new(kind: StationKind) -> Station {
        Station {
            kind,
            recipe: None,
            state: StationState::Idle,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            expected_outputs: Vec::new(),
        }
    }

    pub fn recipe(&self) -> Option<&RecipeId> {
        self.recipe.as_ref()
    }

    pub fn state_is_idle(&self) -> bool {
        self.state == StationState::Idle
    }

    pub fn state_is_loading(&self) -> bool {
        matches!(self.state, StationState::Loading { .. })
    }

    pub fn state_is_running(&self) -> bool {
        matches!(self.state, StationState::Running { .. })
    }

    pub fn state_is_done(&self) -> bool {
        self.state == StationState::Done
    }

    pub fn state_is_blocked(&self) -> bool {
        self.state == StationState::Blocked
    }

    pub fn inputs(&self) -> &BTreeMap<CommodityId, u32> {
        &self.inputs
    }

    pub fn outputs(&self) -> &BTreeMap<CommodityId, u32> {
        &self.outputs
    }

    /// Set the recipe for the next cycle. Fails if the station is busy (Done with outputs, or Blocked).
    pub fn set_recipe(&mut self, recipe_id: RecipeId) -> Result<(), Refusal> {
        if self.state == StationState::Blocked || self.state == StationState::Done {
            return Err(Refusal::Busy);
        }
        self.recipe = Some(recipe_id);
        Ok(())
    }

    /// Insert an amount of a commodity. Fails if not an input of the recipe, exceeds quota, or station is busy.
    pub fn insert(&mut self, commodity: CommodityId, amount: u32, recipe: &crate::recipe::Recipe) -> Result<(), Refusal> {
        if self.recipe.as_ref() != Some(&recipe.id) {
            return Err(Refusal::NoRecipe);
        }
        if self.state == StationState::Blocked {
            return Err(Refusal::Busy);
        }
        if !self.state_is_idle() && !self.state_is_loading() {
            return Err(Refusal::Busy);
        }

        // Find the input in the recipe.
        let Some(input) = recipe.inputs.iter().find(|i| i.id == commodity) else {
            return Err(Refusal::NotAnInput);
        };

        // Check quota.
        let current = self.inputs.get(&commodity).copied().unwrap_or(0);
        if current + amount > input.amount {
            return Err(Refusal::ExceedsQuota);
        }

        // Insert and transition if loading completes.
        *self.inputs.entry(commodity).or_insert(0) += amount;

        // Check if all inputs are now satisfied.
        let all_filled = recipe.inputs.iter().all(|inp| {
            let filled = self.inputs.get(&inp.id).copied().unwrap_or(0);
            filled >= inp.amount
        });

        if all_filled {
            // Transition to Running.
            self.expected_outputs = recipe.outputs.clone();
            self.state = StationState::Running { time_remaining_s: recipe.time_s };
        } else if self.state == StationState::Idle {
            // Transition to Loading with progress.
            let total: u32 = recipe.inputs.iter().map(|i| i.amount).sum();
            let filled: u32 = self.inputs.values().sum();
            self.state = StationState::Loading { progress: (filled, total) };
        } else if let StationState::Loading { progress } = &mut self.state {
            // Update progress.
            let total: u32 = recipe.inputs.iter().map(|i| i.amount).sum();
            let filled: u32 = self.inputs.values().sum();
            *progress = (filled, total);
        }

        Ok(())
    }

    /// Advance time. Returns whether the cycle completed.
    pub fn step(&mut self, dt: f64) -> bool {
        match &mut self.state {
            StationState::Running { time_remaining_s } => {
                *time_remaining_s -= dt;
                if *time_remaining_s <= 0.0 {
                    // Populate outputs from the recipe
                    for output in &self.expected_outputs {
                        *self.outputs.entry(output.id.clone()).or_insert(0) += output.amount;
                    }
                    self.state = StationState::Done;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Take all outputs. Returns them and transitions to Idle or Blocked if inputs are present.
    /// Fails if not Done.
    pub fn take_outputs(&mut self, _recipe: &crate::recipe::Recipe) -> Result<BTreeMap<CommodityId, u32>, Refusal> {
        if self.state != StationState::Done {
            return Err(Refusal::NoRecipe); // Or a better refusal type.
        }

        let outputs = self.outputs.clone();
        self.outputs.clear();
        self.inputs.clear();

        // Transition: if recipe is set and there are outputs to place somewhere, go to Idle;
        // otherwise stay Idle. For now, always go to Idle.
        if self.recipe.is_some() {
            self.state = StationState::Idle;
        } else {
            self.state = StationState::Idle;
        }

        Ok(outputs)
    }
}
