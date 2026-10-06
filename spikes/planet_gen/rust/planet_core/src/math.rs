//! f64 vectors and the cube-sphere mapping. f64 so that border vertices of
//! neighbouring chunks agree to far below a millimetre.
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[inline(always)]
pub fn v3(x: f64, y: f64, z: f64) -> V3 {
    V3 { x, y, z }
}
impl Add for V3 {
    type Output = V3;
    #[inline(always)]
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    #[inline(always)]
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for V3 {
    type Output = V3;
    #[inline(always)]
    fn mul(self, s: f64) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for V3 {
    type Output = V3;
    #[inline(always)]
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}
impl V3 {
    #[inline(always)]
    pub fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[inline(always)]
    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[inline(always)]
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    #[inline(always)]
    pub fn normalized(self) -> V3 {
        let l = self.length();
        if l == 0.0 {
            self
        } else {
            self * (1.0 / l)
        }
    }
    pub fn from_arr(a: [f64; 3]) -> V3 {
        v3(a[0], a[1], a[2])
    }
    pub fn arr(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

pub const FACE_NORMALS: [V3; 6] = [
    V3 { x: 1.0, y: 0.0, z: 0.0 },
    V3 { x: -1.0, y: 0.0, z: 0.0 },
    V3 { x: 0.0, y: 1.0, z: 0.0 },
    V3 { x: 0.0, y: -1.0, z: 0.0 },
    V3 { x: 0.0, y: 0.0, z: 1.0 },
    V3 { x: 0.0, y: 0.0, z: -1.0 },
];

/// Same mapping as `terrain.gd::cube_to_sphere` (spherified cube, u x v = face normal).
#[inline]
pub fn cube_to_sphere(face: usize, a: f64, b: f64) -> V3 {
    let nrm = FACE_NORMALS[face];
    let u = v3(nrm.y, nrm.z, nrm.x);
    let v = nrm.cross(u);
    let p = nrm + u * a + v * b;
    let (x2, y2, z2) = (p.x * p.x, p.y * p.y, p.z * p.z);
    v3(
        p.x * (1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0).sqrt(),
        p.y * (1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0).sqrt(),
        p.z * (1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0).sqrt(),
    )
}

/// Face whose cube-map region contains the direction (dominant axis; ties are fine
/// because everything sampled per face is continuous across face edges).
pub fn face_of(d: V3) -> usize {
    let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
    if ax >= ay && ax >= az {
        if d.x >= 0.0 { 0 } else { 1 }
    } else if ay >= az {
        if d.y >= 0.0 { 2 } else { 3 }
    } else if d.z >= 0.0 {
        4
    } else {
        5
    }
}

/// Inverse of `cube_to_sphere` on the given face: Gauss-Newton from the gnomonic guess.
pub fn sphere_to_face_ab(face: usize, d: V3) -> (f64, f64) {
    let nrm = FACE_NORMALS[face];
    let u = v3(nrm.y, nrm.z, nrm.x);
    let v = nrm.cross(u);
    let dn = d.dot(nrm).max(1e-9);
    let (mut a, mut b) = (d.dot(u) / dn, d.dot(v) / dn);
    for _ in 0..8 {
        let r = cube_to_sphere(face, a, b) - d;
        let e = 1e-6;
        let ja = (cube_to_sphere(face, a + e, b) - cube_to_sphere(face, a - e, b)) * (0.5 / e);
        let jb = (cube_to_sphere(face, a, b + e) - cube_to_sphere(face, a, b - e)) * (0.5 / e);
        let (m00, m01, m11) = (ja.dot(ja), ja.dot(jb), jb.dot(jb));
        let (g0, g1) = (ja.dot(r), jb.dot(r));
        let det = m00 * m11 - m01 * m01;
        if det.abs() < 1e-18 {
            break;
        }
        let da = (m11 * g0 - m01 * g1) / det;
        let db = (m00 * g1 - m01 * g0) / det;
        a -= da;
        b -= db;
        if da.abs() < 1e-13 && db.abs() < 1e-13 {
            break;
        }
    }
    (a, b)
}

#[inline(always)]
pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---- hash (spike 6) ------------------------------------------------------

#[inline(always)]
pub fn lowbias32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}
#[inline(always)]
pub fn hash(key: u32, kind: u32, ci: u32, cj: u32, salt: u32) -> u32 {
    let inner = kind
        .wrapping_mul(0x85EBCA6B)
        .wrapping_add(ci.wrapping_mul(0xC2B2AE35))
        .wrapping_add(cj.wrapping_mul(0x27D4EB2F))
        .wrapping_add(salt.wrapping_mul(0x165667B1));
    lowbias32(key.wrapping_mul(0x9E3779B1) ^ lowbias32(inner))
}
#[inline(always)]
pub fn hash01(key: u32, kind: u32, ci: u32, cj: u32, salt: u32) -> f32 {
    (hash(key, kind, ci, cj, salt) >> 8) as f32 / 16777216.0
}
