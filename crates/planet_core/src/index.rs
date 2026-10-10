//! Spatial index (#177): a uniform bucket grid for fast nearest-neighbor queries.
//! The index uses the unit sphere cube-map; each of the 6 faces has an NxN bucket grid.
use crate::math::V3;

/// A simple spatial index using a cube-map grid over the sphere.
/// Each of the 6 faces has a grid of buckets; points are bucketed by projection.
pub struct SpatialIndex {
    /// Grid resolution per face side (N x N grid per face).
    resolution: usize,
    /// Buckets: one per face per grid cell.
    /// Layout: face * resolution^2 + grid_y * resolution + grid_x.
    buckets: Vec<Vec<usize>>,
    /// Positions of all indexed points.
    positions: Vec<V3>,
}

impl SpatialIndex {
    /// Build an index from a list of positions on the unit sphere.
    /// `resolution` is the grid cells per side of each cube face (typically 8..16).
    pub fn build(positions: Vec<V3>, resolution: usize) -> Self {
        let bucket_count = 6 * resolution * resolution;
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); bucket_count];

        for (idx, pos) in positions.iter().enumerate() {
            if let Some(bucket_idx) = Self::bucket_idx(*pos, resolution) {
                buckets[bucket_idx].push(idx);
            }
        }

        SpatialIndex { resolution, buckets, positions }
    }

    /// Find the cube face and grid cell for a position on the unit sphere.
    fn bucket_idx(pos: V3, resolution: usize) -> Option<usize> {
        let V3 { x, y, z } = pos;
        let abs_x = x.abs();
        let abs_y = y.abs();
        let abs_z = z.abs();

        let (face, u, v) = if abs_x >= abs_y && abs_x >= abs_z {
            // X face
            if x > 0.0 {
                (0, (-z / abs_x + 1.0) * 0.5, (y / abs_x + 1.0) * 0.5)
            } else {
                (1, (z / abs_x + 1.0) * 0.5, (y / abs_x + 1.0) * 0.5)
            }
        } else if abs_y >= abs_z {
            // Y face
            if y > 0.0 {
                (2, (x / abs_y + 1.0) * 0.5, (z / abs_y + 1.0) * 0.5)
            } else {
                (3, (x / abs_y + 1.0) * 0.5, (-z / abs_y + 1.0) * 0.5)
            }
        } else {
            // Z face
            if z > 0.0 {
                (4, (x / abs_z + 1.0) * 0.5, (-y / abs_z + 1.0) * 0.5)
            } else {
                (5, (-x / abs_z + 1.0) * 0.5, (-y / abs_z + 1.0) * 0.5)
            }
        };

        let grid_x = (u * resolution as f64) as usize;
        let grid_y = (v * resolution as f64) as usize;
        let grid_x = grid_x.min(resolution - 1);
        let grid_y = grid_y.min(resolution - 1);
        let bucket_id = face * resolution * resolution + grid_y * resolution + grid_x;

        Some(bucket_id)
    }

    /// Find all positions within `radius_rad` (great-circle distance in radians) of `pos`.
    pub fn nearby(&self, pos: V3, radius_rad: f64) -> Vec<usize> {
        let cos_radius = radius_rad.cos();
        let mut results = Vec::new();

        if let Some(center_bucket) = Self::bucket_idx(pos, self.resolution) {
            let face = center_bucket / (self.resolution * self.resolution);
            let search_radius = ((radius_rad * 1.5).sin().atan() * self.resolution as f64 / 1.57) as usize;

            let grid_y = (center_bucket % (self.resolution * self.resolution)) / self.resolution;
            let grid_x = (center_bucket % (self.resolution * self.resolution)) % self.resolution;

            // Search nearby grid cells on the same face.
            for dy in -(search_radius as i32)..=(search_radius as i32) {
                for dx in -(search_radius as i32)..=(search_radius as i32) {
                    let gy = (grid_y as i32 + dy).max(0).min(self.resolution as i32 - 1) as usize;
                    let gx = (grid_x as i32 + dx).max(0).min(self.resolution as i32 - 1) as usize;
                    let bucket_idx = face * self.resolution * self.resolution + gy * self.resolution + gx;

                    for &idx in &self.buckets[bucket_idx] {
                        if self.positions[idx].dot(pos) >= cos_radius {
                            results.push(idx);
                        }
                    }
                }
            }
        }

        results
    }

    /// Get the position of an indexed point.
    pub fn position(&self, idx: usize) -> Option<V3> {
        self.positions.get(idx).copied()
    }

    /// Number of indexed points.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_nearby_finds_close_points() {
        use crate::math::v3;
        let positions = vec![
            v3(1.0, 0.0, 0.0).normalized(),
            v3(0.999, 0.01, 0.0).normalized(),
            v3(0.0, 1.0, 0.0).normalized(),
            v3(-1.0, 0.0, 0.0).normalized(),
        ];
        let index = SpatialIndex::build(positions, 8);

        let query = v3(1.0, 0.0, 0.0).normalized();
        let nearby = index.nearby(query, 0.05);

        assert!(nearby.len() >= 2, "should find at least 2 points (query and close one)");
        assert!(nearby.contains(&0), "should find the center point");
        assert!(nearby.contains(&1), "should find the nearby point");
        assert!(!nearby.contains(&3), "should not find the opposite point");
    }
}
