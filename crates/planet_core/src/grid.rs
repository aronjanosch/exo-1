//! A hash grid over 3D space for things standing on the planet (#177, spike 14): stamps, sites and
//! the points the placement keeps apart. Density rules give a planet of any radius thousands of
//! features; a linear scan per height query or per candidate would not survive that. Items keep
//! their insertion order inside a cell, so a sum over a cell's items adds in the same order as a
//! scan over all of them (the results are identical, not just close).
use crate::math::*;
use std::collections::HashMap;

pub struct Grid {
    cell: f64,
    radius: f64,
    map: HashMap<u64, Vec<u32>>,
}

fn key(c: [i64; 3]) -> u64 {
    let k = |v: i64| ((v + (1 << 20)) & 0x1F_FFFF) as u64;
    k(c[0]) | k(c[1]) << 21 | k(c[2]) << 42
}

impl Grid {
    /// An empty grid of cells `cell_m` wide on a planet of `radius`.
    pub fn new(cell_m: f64, radius: f64) -> Grid {
        Grid { cell: cell_m.max(1.0), radius, map: HashMap::new() }
    }

    fn cell_of(&self, x: f64) -> i64 {
        (x / self.cell).floor() as i64
    }

    /// Item `i` standing at `dir`, reaching `reach_m` metres: in every cell its reach touches.
    pub fn insert_reach(&mut self, i: u32, dir: V3, reach_m: f64) {
        let p = dir * self.radius;
        let r = reach_m.max(0.0);
        for x in self.cell_of(p.x - r)..=self.cell_of(p.x + r) {
            for y in self.cell_of(p.y - r)..=self.cell_of(p.y + r) {
                for z in self.cell_of(p.z - r)..=self.cell_of(p.z + r) {
                    self.map.entry(key([x, y, z])).or_default().push(i);
                }
            }
        }
    }

    /// Item `i` as a point (in its own cell only).
    pub fn insert_point(&mut self, i: u32, dir: V3) {
        let p = dir * self.radius;
        self.map.entry(key([self.cell_of(p.x), self.cell_of(p.y), self.cell_of(p.z)])).or_default().push(i);
    }

    /// The items whose reach touches the cell of `dir` (a superset of those that reach `dir`).
    pub fn at(&self, dir: V3) -> &[u32] {
        let p = dir * self.radius;
        self.map.get(&key([self.cell_of(p.x), self.cell_of(p.y), self.cell_of(p.z)])).map_or(&[], |v| v.as_slice())
    }

    /// Calls `f` with every point item in the cells within `within_m` of `dir` (a superset of the
    /// items within that distance); stops when `f` returns true, and returns that.
    pub fn any_near(&self, dir: V3, within_m: f64, mut f: impl FnMut(u32) -> bool) -> bool {
        let p = dir * self.radius;
        for x in self.cell_of(p.x - within_m)..=self.cell_of(p.x + within_m) {
            for y in self.cell_of(p.y - within_m)..=self.cell_of(p.y + within_m) {
                for z in self.cell_of(p.z - within_m)..=self.cell_of(p.z + within_m) {
                    if let Some(v) = self.map.get(&key([x, y, z])) {
                        for &i in v {
                            if f(i) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub fn cell_m(&self) -> f64 {
        self.cell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(k: u32) -> V3 {
        let z = ((k * 7919 % 2000) as f64 / 1000.0) - 1.0;
        let phi = (k * 104729 % 6283) as f64 / 1000.0;
        let r = (1.0 - z * z).sqrt();
        v3(r * phi.cos(), z, r * phi.sin())
    }

    #[test]
    fn near_finds_everything_a_scan_finds_and_in_order() {
        let radius = 5000.0;
        let mut g = Grid::new(400.0, radius);
        let pts: Vec<V3> = (0..500).map(dir).collect();
        for (i, p) in pts.iter().enumerate() {
            g.insert_point(i as u32, *p);
        }
        for q in 0..200 {
            let d = dir(1000 + q);
            let mut seen = Vec::new();
            g.any_near(d, 400.0, |i| {
                seen.push(i);
                false
            });
            for (i, p) in pts.iter().enumerate() {
                if radius * p.dot(d).clamp(-1.0, 1.0).acos() <= 400.0 {
                    assert!(seen.contains(&(i as u32)), "item {i} within 400 m is found");
                }
            }
        }
    }

    #[test]
    fn reach_items_are_in_every_cell_they_touch() {
        let radius = 5000.0;
        let mut g = Grid::new(300.0, radius);
        let pts: Vec<V3> = (0..300).map(dir).collect();
        for (i, p) in pts.iter().enumerate() {
            g.insert_reach(i as u32, *p, 700.0);
        }
        for q in 0..300 {
            let d = dir(2000 + q);
            let cell = g.at(d);
            assert!(cell.windows(2).all(|w| w[0] < w[1]), "ascending order inside a cell");
            for (i, p) in pts.iter().enumerate() {
                if radius * p.dot(d).clamp(-1.0, 1.0).acos() <= 700.0 {
                    assert!(cell.contains(&(i as u32)), "item {i} reaching the point is in its cell");
                }
            }
        }
    }
}
