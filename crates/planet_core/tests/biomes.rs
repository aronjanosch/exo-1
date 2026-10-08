//! #68: biomes v2: nearest row, chunk and point query agree, quotas fail the bake.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn bake(text: &str) -> (Planet, Result<BakeStats, String>) {
    let mut p = Planet::new(Recipe::for_planet(text, 1337, 5000.0).unwrap());
    let st = p.bake_checked(0);
    (p, st)
}

#[test]
fn six_or_more_rows_have_area_and_walks_cross_biomes() {
    let (_, st) = bake(HEARTH);
    let st = st.unwrap();
    let with_area = st.biome_area_share.values().filter(|v| **v > 0.002).count();
    println!("shares {:?}, walks with 2+ biomes {:.0} %, median {}", st.biome_area_share, st.walks_two_biomes_share * 100.0, st.walk_biomes_median);
    assert!(with_area >= 6, "{with_area} rows with area");
    assert!(st.walks_two_biomes_share >= 0.6);
}

#[test]
fn chunk_build_and_point_query_give_the_same_biome() {
    let (p, _) = bake(HEARTH);
    let (mut same, mut all) = (0, 0);
    for (face, a0, b0, size) in [(2, -0.1, -0.1, 0.0625), (0, 0.3, -0.5, 0.125), (4, -1.0, -1.0, 0.03125), (5, 0.9, 0.9, 0.1)] {
        let c = p.build_chunk(face, a0, b0, size);
        for j in 1..=GRID + 1 {
            for i in 1..=GRID + 1 {
                let k = j * M + i;
                let world = V3::from_arr(c.center) + v3(c.verts[k][0] as f64, c.verts[k][1] as f64, c.verts[k][2] as f64);
                let q = p.sample(world.normalized()).biome as u8;
                all += 1;
                same += (q == c.biomes[k]) as usize;
            }
        }
    }
    // Only exact borders may differ (f32 vertex positions land on the other side).
    println!("{same} of {all} vertices agree");
    assert!(same as f64 >= all as f64 * 0.999, "{same} of {all}");
}

#[test]
fn a_missed_quota_fails_the_bake_with_the_row() {
    let text = HEARTH.replacen("\"min_share\": 0.005, \"color\": [0.62, 0.42, 0.72]", "\"min_share\": 0.6, \"color\": [0.62, 0.42, 0.72]", 1);
    assert_ne!(text, HEARTH, "fixture");
    let e = bake(&text).1.unwrap_err();
    assert!(e.contains("biome row 7") && e.contains("below its quota 60.00 %"), "{e}");
}

#[test]
fn nearest_row_wins() {
    let (p, _) = bake(HEARTH);
    let f = |moist: f64| planet_core::planet::Fields { elev: 0.0, temp: 1.0, moist, land: -0.5, weird: -0.5 };
    // 20 m above the sea, warm: wet, middle and dry moisture pick their rows.
    assert_eq!(p.biome_for(20.0, &f(0.6), None), 0);
    assert_eq!(p.biome_for(20.0, &f(0.0), None), 6);
    assert_eq!(p.biome_for(20.0, &f(-0.7), None), 1);
    // Under water: the sea floor; a forced landform: the rim.
    assert_eq!(p.biome_for(-5.0, &f(0.6), None), 5);
    assert_eq!(p.biome_for(20.0, &f(0.6), Some(1.0)), 2);
}
