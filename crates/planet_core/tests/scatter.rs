//! #65: scatter is deterministic per cell, stands along the radius, and the recipe's references
//! are checked.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn hearth() -> Planet {
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, 5000.0).unwrap());
    p.bake(0);
    p
}

/// Cells of one storey at a depth around a point on face 2 (the spawn side).
fn cells(depth: u32, n: usize) -> Vec<(usize, f64, f64, f64)> {
    let size = 2.0 / (1u32 << depth) as f64;
    let mut v = Vec::new();
    for i in 0..n {
        for j in 0..n {
            v.push((2, -size * (n as f64 / 2.0) + i as f64 * size, -size * (n as f64 / 2.0) + j as f64 * size, size));
        }
    }
    v
}

#[test]
fn same_seed_same_instances_in_any_load_order() {
    let p = hearth();
    let storeys = p.storeys().len();
    for st in 0..storeys {
        let cs = cells(7, 3);
        let forward: Vec<ScatterCell> = cs.iter().map(|c| p.build_scatter(c.0, c.1, c.2, c.3, st)).collect();
        let mut backward: Vec<ScatterCell> = cs.iter().rev().map(|c| p.build_scatter(c.0, c.1, c.2, c.3, st)).collect();
        backward.reverse();
        // And from a second planet object, baked again (another "session").
        let q = hearth();
        for (k, c) in cs.iter().enumerate() {
            let again = q.build_scatter(c.0, c.1, c.2, c.3, st);
            assert_eq!(forward[k].instances, backward[k].instances);
            assert_eq!(forward[k].instances, again.instances);
        }
    }
    let total: usize = (0..storeys).map(|st| cells(7, 3).iter().map(|c| p.build_scatter(c.0, c.1, c.2, c.3, st).instances.len()).sum::<usize>()).sum();
    println!("instances in 9 cells x {storeys} storeys: {total}");
    assert!(total > 0);
}

#[test]
fn upright_entries_stand_along_the_radius_and_on_the_ground() {
    let p = hearth();
    let entries = p.scatter_entries();
    let tree = p.storeys().iter().position(|(k, _)| k == "tree").unwrap();
    let mut n = 0;
    for c in cells(5, 4) {
        let cell = p.build_scatter(c.0, c.1, c.2, c.3, tree);
        for inst in &cell.instances {
            let world = V3::from_arr(cell.center) + v3(inst.pos[0] as f64, inst.pos[1] as f64, inst.pos[2] as f64);
            let up = v3(inst.basis[1][0] as f64, inst.basis[1][1] as f64, inst.basis[1][2] as f64);
            assert!(up.dot(world.normalized()) > 0.9999, "{} leans", entries[inst.entry as usize].id);
            // On the ground, less the sink.
            let ground = p.radius + p.height_at(world.normalized());
            assert!((world.length() - ground).abs() < 0.6, "{} floats {:.2} m", entries[inst.entry as usize].id, world.length() - ground);
            n += 1;
        }
    }
    assert!(n > 50, "only {n} trees");
}

#[test]
fn recipe_rejects_missing_cluster_mesh_and_unknown_multiplier() {
    for (from, to, want) in [
        ("\"cluster\": \"grove\"", "\"cluster\": \"nowhere\"", "cluster preset 'nowhere'"),
        ("\"mesh\": \"tree_b\"", "\"mesh\": \"teapot\"", "mesh 'teapot'"),
        ("\"scatter\": { \"tree_round\": 1.5,", "\"scatter\": { \"palm\": 1.5,", "unknown entry or group 'palm'"),
    ] {
        assert!(HEARTH.contains(from), "fixture {from}");
        let e = Recipe::from_json(&HEARTH.replacen(from, to, 1)).unwrap_err();
        assert!(e.contains(want), "{e}");
    }
}
