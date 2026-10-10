//! production_core: recipes, stations, state transitions, save round-trip.
use std::path::Path;

use production_core::*;
use gameplay_core::{Content, File};

fn files(dir: &str) -> Vec<File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    for d in std::fs::read_dir(&root).unwrap() {
        let d = d.unwrap().path();
        for f in std::fs::read_dir(&d).unwrap() {
            let f = f.unwrap().path();
            out.push(File::new(f.strip_prefix(&root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
        }
    }
    out
}

fn kernel() -> Content {
    Content::load(&files("../gameplay_core/tests/fixtures/valid"), &["small", "medium", "large"]).unwrap()
}

fn content() -> RecipeContent {
    let f = files("tests/fixtures");
    RecipeContent::load(&f, &kernel()).unwrap()
}

fn load_with(path: &str, text: &str) -> Vec<String> {
    let mut f: Vec<File> = files("tests/fixtures").into_iter().filter(|f| f.path != path).collect();
    f.push(File::new(path, text));
    RecipeContent::load(&f, &kernel()).unwrap_err()
}

// ---------- loader ----------

#[test]
fn recipes_load() {
    let rc = content();
    assert!(rc.recipes.len() >= 1, "at least one recipe loaded");
}

#[test]
fn loader_errors_name_file_and_field() {
    // Test unknown commodity error reporting
    let e = load_with(
        "recipe/test.json",
        r#"{"id": "test", "station": "mill", "inputs": [{"id": "unknown", "amount": 2}], "outputs": [{"id": "sock_dust", "amount": 1}], "time_s": 10.0}"#
    );
    assert!(!e.is_empty(), "unknown commodity should error: {e:?}");
    assert!(e.iter().any(|m| m.starts_with("recipe/test.json") && m.contains("unknown")), "error should name file and field: {e:?}");

    // Test zero amount in inputs
    let e = load_with(
        "recipe/test.json",
        r#"{"id": "test", "station": "mill", "inputs": [{"id": "sock_dust", "amount": 0}], "outputs": [{"id": "sock_dust", "amount": 1}], "time_s": 10.0}"#
    );
    assert!(!e.is_empty(), "zero amount should error: {e:?}");
    assert!(e.iter().any(|m| m.starts_with("recipe/test.json") && m.contains("amount")), "error should name field: {e:?}");

    // Test negative time
    let e = load_with(
        "recipe/test.json",
        r#"{"id": "test", "station": "mill", "inputs": [{"id": "sock_dust", "amount": 1}], "outputs": [{"id": "sock_dust", "amount": 1}], "time_s": -1.0}"#
    );
    assert!(!e.is_empty(), "negative time should error: {e:?}");
    assert!(e.iter().any(|m| m.starts_with("recipe/test.json") && m.contains("time_s")), "error should name field: {e:?}");

    // Test empty outputs
    let e = load_with(
        "recipe/test.json",
        r#"{"id": "test", "station": "mill", "inputs": [{"id": "sock_dust", "amount": 1}], "outputs": [], "time_s": 10.0}"#
    );
    assert!(!e.is_empty(), "empty outputs should error: {e:?}");
    assert!(e.iter().any(|m| m.starts_with("recipe/test.json") && m.contains("outputs")), "error should name field: {e:?}");
}

// ---------- state transitions ----------

#[test]
fn station_starts_idle() {
    let st = Station::new(StationKind::new("mill"));
    assert!(st.state_is_idle());
    assert!(st.recipe().is_none());
}

#[test]
fn set_recipe_succeeds_from_idle() {
    let mut st = Station::new(StationKind::new("mill"));
    assert!(st.set_recipe(RecipeId::new("grind")).is_ok());
    assert_eq!(st.recipe().map(|r| r.as_str()), Some("grind"));
}

#[test]
fn insert_transitions_idle_to_loading_when_partial() {
    let rc = content();
    let _k = kernel();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return; // Skip if no recipes
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    // Insert partial input
    if let Some(first_input) = recipe.inputs.first() {
        let result = st.insert(first_input.id.clone(), 1, recipe);
        if first_input.amount > 1 {
            assert!(result.is_ok(), "partial insert should succeed");
            assert!(st.state_is_loading(), "should be in Loading state");
        }
    }
}

#[test]
fn insert_transitions_loading_to_running_when_complete() {
    let rc = content();
    let _k = kernel();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return; // Skip if no recipes
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    // Insert all inputs
    for input in &recipe.inputs {
        let result = st.insert(input.id.clone(), input.amount, recipe);
        assert!(result.is_ok(), "insert full input should succeed");
    }

    assert!(st.state_is_running(), "should be in Running state after all inputs filled");
}

#[test]
fn insert_refuses_unknown_input() {
    let rc = content();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    let result = st.insert(gameplay_core::CommodityId::new("nonexistent"), 1, recipe);
    assert!(result == Err(Refusal::NotAnInput), "unknown input should be refused");
}

#[test]
fn insert_refuses_exceeds_quota() {
    let rc = content();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    if let Some(input) = recipe.inputs.first() {
        let result = st.insert(input.id.clone(), input.amount + 1, recipe);
        assert!(result == Err(Refusal::ExceedsQuota), "exceeding quota should be refused");
    }
}

#[test]
fn step_counts_down_time_and_completes() {
    let rc = content();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    // Fill all inputs
    for input in &recipe.inputs {
        st.insert(input.id.clone(), input.amount, recipe).unwrap();
    }

    assert!(st.state_is_running(), "should be running");

    // Step partway
    let half_time = recipe.time_s / 2.0;
    assert!(!st.step(half_time), "step during cycle should return false");
    assert!(st.state_is_running(), "should still be running");

    // Step to completion
    let remaining = recipe.time_s - half_time + 0.1;
    assert!(st.step(remaining), "step to completion should return true");
    assert!(st.state_is_done(), "should be in Done state");
}

#[test]
fn step_progress_sums_to_time() {
    let rc = content();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    for input in &recipe.inputs {
        st.insert(input.id.clone(), input.amount, recipe).unwrap();
    }

    let total_time = recipe.time_s;
    // Create steps that definitely exceed the recipe time
    let steps = vec![total_time * 0.2, total_time * 0.3, total_time * 0.25, total_time * 0.3];
    let mut accumulated = 0.0;
    let mut completed = false;

    for &step_time in &steps {
        let before = accumulated;
        accumulated += step_time;
        let did_complete = st.step(step_time);

        if before < total_time && accumulated >= total_time {
            assert!(did_complete, "should complete when crossing total_time");
            completed = true;
            break;
        } else if before < total_time {
            assert!(!did_complete, "should not complete before reaching total_time");
        }
    }

    assert!(completed, "test steps should eventually complete the cycle");
    assert!(st.state_is_done(), "should be in Done state after completion");
}

#[test]
fn take_outputs_returns_outputs_and_transitions_to_idle() {
    let rc = content();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    for input in &recipe.inputs {
        st.insert(input.id.clone(), input.amount, recipe).unwrap();
    }

    st.step(recipe.time_s + 1.0);
    assert!(st.state_is_done());

    let outputs = st.take_outputs(recipe).unwrap();
    assert!(!outputs.is_empty(), "outputs should not be empty");
    assert!(st.state_is_idle(), "should be Idle after taking outputs");
}

#[test]
fn blocked_when_outputs_not_taken_and_cycle_requested() {
    let rc = content();
    let mut st = Station::new(StationKind::new("mill"));

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;
    st.set_recipe(recipe.id.clone()).unwrap();

    for input in &recipe.inputs {
        st.insert(input.id.clone(), input.amount, recipe).unwrap();
    }

    st.step(recipe.time_s + 1.0);
    assert!(st.state_is_done());

    // Try to set new recipe without taking outputs
    let result = st.set_recipe(RecipeId::new("another"));
    assert!(result == Err(Refusal::Busy), "should refuse new recipe while Done: {result:?}");
}

// ---------- save ----------

#[test]
fn production_section_round_trips() {
    use gameplay_core::save::Envelope;

    let mut prod = Production::new();
    prod.add_station("mill", StationKind::new("mill"));
    prod.add_station("kiln", StationKind::new("kiln"));

    let mut env = Envelope::new();
    prod.save(&mut env);

    let back = Production::load(&Envelope::from_json(&env.to_json()).unwrap()).unwrap().unwrap();
    assert_eq!(back, prod, "state → JSON → state should be equal");
}

#[test]
fn production_save_handles_version_mismatch() {
    use gameplay_core::save::Envelope;

    let prod = Production::new();
    let mut env = Envelope::new();
    prod.save(&mut env);

    let mut bad = Envelope::new();
    bad.put("production", 99, &prod);
    assert!(Production::load(&bad).is_err(), "wrong version should error");

    assert_eq!(Production::load(&Envelope::new()).unwrap(), None, "missing section should be None");
}

// ---------- determinism ----------

#[test]
fn same_recipe_same_time_always_completes() {
    let rc = content();

    if rc.recipes.is_empty() {
        return;
    }

    let recipe = &rc.recipes.values().next().unwrap().record;

    for _ in 0..3 {
        let mut st = Station::new(StationKind::new("mill"));
        st.set_recipe(recipe.id.clone()).unwrap();

        for input in &recipe.inputs {
            st.insert(input.id.clone(), input.amount, recipe).unwrap();
        }

        st.step(recipe.time_s + 1.0);
        assert!(st.state_is_done(), "deterministic: same inputs always complete");
    }
}
