//! A small software vector raster for pictures the pen's boxes cannot draw (polygons, bezier
//! blades, dashed rings with alpha): `Prim`s in a canvas's own units become an RGBA buffer.
//! Fills are non-zero winding with 4 sub-scanlines per pixel and exact horizontal coverage, so
//! edges are smooth and overlapping strokes of one `Prim` merge into one shape.
use std::f64::consts::TAU;

pub type Pt = [f64; 2];

/// One filled shape: closed polygons (all filled together, non-zero winding) in one colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Prim {
    pub polys: Vec<Vec<Pt>>,
    pub rgb: [u8; 3],
    /// 0..1, quantised to 1/255 so equal drawings compare equal.
    pub alpha: u8,
}

impl Prim {
    pub fn with_alpha(mut self, a: f64) -> Prim {
        self.alpha = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        self
    }

    /// Turned about `(cx, cy)` by `deg` (positive turns clockwise on a y-down canvas).
    pub fn rotated(mut self, cx: f64, cy: f64, deg: f64) -> Prim {
        let (s, c) = deg.to_radians().sin_cos();
        for p in self.polys.iter_mut().flatten() {
            let (x, y) = (p[0] - cx, p[1] - cy);
            *p = [cx + x * c - y * s, cy + x * s + y * c];
        }
        self
    }
}

pub fn fill(pts: &[Pt], rgb: [u8; 3]) -> Prim {
    Prim { polys: vec![pts.to_vec()], rgb, alpha: 255 }
}

pub fn rect(x: f64, y: f64, w: f64, h: f64, rgb: [u8; 3]) -> Prim {
    fill(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], rgb)
}

pub fn circle_pts(cx: f64, cy: f64, r: f64) -> Vec<Pt> {
    let n = ((r * 1.5) as usize).clamp(12, 160);
    (0..n).map(|i| [cx + r * (TAU * i as f64 / n as f64).cos(), cy + r * (TAU * i as f64 / n as f64).sin()]).collect()
}

pub fn disc(cx: f64, cy: f64, r: f64, rgb: [u8; 3]) -> Prim {
    fill(&circle_pts(cx, cy, r), rgb)
}

/// A cubic bezier flattened to `n` segments (the points include both ends).
pub fn bezier(p0: Pt, p1: Pt, p2: Pt, p3: Pt, n: usize) -> Vec<Pt> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            [k[0] * p0[0] + k[1] * p1[0] + k[2] * p2[0] + k[3] * p3[0], k[0] * p0[1] + k[1] * p1[1] + k[2] * p2[1] + k[3] * p3[1]]
        })
        .collect()
}

fn area2(p: &[Pt]) -> f64 {
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        a[0] * b[1] - b[0] * a[1]
    }).sum()
}

/// A quad around a segment, always wound the same way so overlapping quads merge.
fn quad(a: Pt, b: Pt, width: f64) -> Option<Vec<Pt>> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-9 {
        return None;
    }
    let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
    let mut q = vec![[a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny], [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny]];
    if area2(&q) < 0.0 {
        q.reverse();
    }
    Some(q)
}

fn oriented(mut p: Vec<Pt>) -> Vec<Pt> {
    if area2(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// A stroke along a polyline: one quad per segment, a small disc at each sharp joint. `dash` is
/// (on, off) along the line; `phase` shifts the pattern (same unit).
pub fn stroke(path: &[Pt], closed: bool, width: f64, dash: Option<(f64, f64)>, phase: f64, rgb: [u8; 3]) -> Prim {
    let mut pts = path.to_vec();
    if closed && pts.len() > 1 {
        pts.push(pts[0]);
    }
    let mut polys = Vec::new();
    // Walk the line, cutting it into the dashes' runs.
    let mut run: Vec<Pt> = Vec::new();
    let mut at = phase;
    let flush = |run: &mut Vec<Pt>, polys: &mut Vec<Vec<Pt>>| {
        for w in run.windows(2) {
            polys.extend(quad(w[0], w[1], width));
        }
        for i in 1..run.len().saturating_sub(1) {
            let (a, b, c) = (run[i - 1], run[i], run[i + 1]);
            let (u, v) = ((b[0] - a[0], b[1] - a[1]), (c[0] - b[0], c[1] - b[1]));
            let cos = (u.0 * v.0 + u.1 * v.1) / (u.0.hypot(u.1) * v.0.hypot(v.1)).max(1e-12);
            if cos < 0.98 {
                polys.push(oriented(circle_pts(b[0], b[1], width * 0.5).into_iter().collect()));
            }
        }
        run.clear();
    };
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        if len < 1e-9 {
            continue;
        }
        match dash {
            None => {
                if run.is_empty() {
                    run.push(a);
                }
                run.push(b);
            }
            Some((on, off)) => {
                let period = (on + off).max(1e-9);
                let mut s = 0.0;
                while s < len {
                    let m = (at + s).rem_euclid(period);
                    let (is_on, left) = if m < on { (true, on - m) } else { (false, period - m) };
                    let e = (s + left).min(len);
                    if is_on {
                        let p = |t: f64| [a[0] + (b[0] - a[0]) * t / len, a[1] + (b[1] - a[1]) * t / len];
                        if run.is_empty() {
                            run.push(p(s));
                        }
                        run.push(p(e));
                    }
                    if !is_on || e < len {
                        flush(&mut run, &mut polys);
                    }
                    s = e;
                }
                at += len;
            }
        }
    }
    flush(&mut run, &mut polys);
    Prim { polys, rgb, alpha: 255 }
}

/// A ring: a closed circle stroke.
pub fn ring(cx: f64, cy: f64, r: f64, width: f64, dash: Option<(f64, f64)>, rgb: [u8; 3]) -> Prim {
    // Finer than a disc: strokes show every kink.
    let n = ((r * 2.5) as usize).clamp(24, 360);
    let pts: Vec<Pt> = (0..n).map(|i| [cx + r * (TAU * i as f64 / n as f64).cos(), cy + r * (TAU * i as f64 / n as f64).sin()]).collect();
    stroke(&pts, true, width, dash, 0.0, rgb)
}

/// Sutherland-Hodgman: `subject` cut to the convex polygon `clip` (any winding).
pub fn clip_convex(subject: &[Pt], clip: &[Pt]) -> Vec<Pt> {
    let clip = oriented(clip.to_vec());
    let mut out = subject.to_vec();
    for i in 0..clip.len() {
        let (a, b) = (clip[i], clip[(i + 1) % clip.len()]);
        let side = |p: Pt| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

const SUB: usize = 4;

/// Draws `prims` over a transparent `w` by `h` pixel buffer (RGBA8, straight alpha), the
/// prims' units scaled by `scale` pixels per unit.
pub fn render(w: usize, h: usize, scale: f64, prims: &[Prim]) -> Vec<u8> {
    // Premultiplied float accumulation, one pixel = [r, g, b, a].
    let mut buf = vec![0.0f32; w * h * 4];
    let mut cov = vec![0.0f32; w];
    for prim in prims {
        let mut rows: Vec<Vec<(f64, i32)>> = vec![Vec::new(); h * SUB];
        let (mut y0, mut y1) = (usize::MAX, 0usize);
        for poly in &prim.polys {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (ax, ay, bx, by) = (a[0] * scale, a[1] * scale * SUB as f64, b[0] * scale, b[1] * scale * SUB as f64);
                if (ay - by).abs() < 1e-12 {
                    continue;
                }
                let (dir, (x0, ys, x1, ye)) = if ay < by { (1, (ax, ay, bx, by)) } else { (-1, (bx, by, ax, ay)) };
                // Sub-scanline k samples at y = k + 0.5.
                let first = (ys - 0.5).ceil().max(0.0) as i64;
                let last = (ye - 0.5).ceil().min((h * SUB) as f64) as i64;
                for k in first..last {
                    let t = (k as f64 + 0.5 - ys) / (ye - ys);
                    rows[k as usize].push((x0 + (x1 - x0) * t, dir));
                    y0 = y0.min(k as usize / SUB);
                    y1 = y1.max(k as usize / SUB);
                }
            }
        }
        if y0 == usize::MAX {
            continue;
        }
        let a = prim.alpha as f32 / 255.0;
        let src = [prim.rgb[0] as f32 / 255.0, prim.rgb[1] as f32 / 255.0, prim.rgb[2] as f32 / 255.0];
        for py in y0..=y1.min(h - 1) {
            cov.iter_mut().for_each(|c| *c = 0.0);
            let (mut xmin, mut xmax) = (w, 0usize);
            for s in 0..SUB {
                let xs = &mut rows[py * SUB + s];
                if xs.is_empty() {
                    continue;
                }
                xs.sort_by(|p, q| p.0.total_cmp(&q.0));
                let mut wind = 0;
                for i in 0..xs.len() {
                    wind += xs[i].1;
                    if wind != 0 && i + 1 < xs.len() {
                        let (xa, xb) = (xs[i].0.clamp(0.0, w as f64), xs[i + 1].0.clamp(0.0, w as f64));
                        if xb <= xa {
                            continue;
                        }
                        let (ia, ib) = (xa as usize, (xb.ceil() as usize).min(w));
                        xmin = xmin.min(ia);
                        xmax = xmax.max(ib);
                        for px in ia..ib {
                            let l = xa.max(px as f64);
                            let r = xb.min(px as f64 + 1.0);
                            cov[px] += ((r - l).max(0.0) / SUB as f64) as f32;
                        }
                    }
                }
            }
            for px in xmin..xmax.min(w) {
                let c = cov[px].min(1.0) * a;
                if c <= 0.0 {
                    continue;
                }
                let o = (py * w + px) * 4;
                let keep = 1.0 - c;
                for k in 0..3 {
                    buf[o + k] = buf[o + k] * keep + src[k] * c;
                }
                buf[o + 3] = buf[o + 3] * keep + c;
            }
        }
    }
    let mut out = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let a = buf[i * 4 + 3];
        for k in 0..3 {
            out[i * 4 + k] = if a > 0.0 { (buf[i * 4 + k] / a * 255.0 + 0.5).clamp(0.0, 255.0) as u8 } else { 0 };
        }
        out[i * 4 + 3] = (a * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(b: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let o = (y * w + x) * 4;
        [b[o], b[o + 1], b[o + 2], b[o + 3]]
    }

    #[test]
    fn a_rect_fills_its_pixels_and_nothing_else() {
        let b = render(8, 8, 1.0, &[rect(2.0, 2.0, 4.0, 4.0, [255, 0, 0])]);
        assert_eq!(px(&b, 8, 3, 3), [255, 0, 0, 255]);
        assert_eq!(px(&b, 8, 1, 3)[3], 0);
        assert_eq!(px(&b, 8, 6, 3)[3], 0);
    }

    #[test]
    fn overlapping_stroke_segments_merge_without_holes_or_double_alpha() {
        let path = circle_pts(20.0, 20.0, 12.0);
        let p = stroke(&path, true, 4.0, None, 0.0, [0, 255, 0]).with_alpha(0.5);
        let b = render(40, 40, 1.0, &[p]);
        let on_ring = px(&b, 40, 31, 20);
        assert!((on_ring[3] as i32 - 128).abs() <= 2, "{on_ring:?}");
        assert_eq!(px(&b, 40, 20, 20)[3], 0, "the middle stays empty");
    }

    #[test]
    fn a_dashed_ring_has_gaps() {
        let p = ring(30.0, 30.0, 20.0, 3.0, Some((4.0, 14.0)), [255, 255, 255]);
        let b = render(60, 60, 1.0, &[p]);
        let lit = (0..360).filter(|d| px(&b, 60, (30.0 + 20.0 * (*d as f64).to_radians().cos()) as usize, (30.0 + 20.0 * (*d as f64).to_radians().sin()) as usize)[3] > 128).count();
        assert!(lit > 20 && lit < 180, "{lit} of 360 on the ring");
    }

    #[test]
    fn a_clipped_stripe_stays_in_the_clip() {
        let clip = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let c = clip_convex(&[[-20.0, -20.0], [30.0, -20.0], [30.0, 20.0], [-20.0, 20.0]], &clip);
        assert!(c.iter().all(|p| (0.0..=10.0).contains(&p[0]) && (0.0..=5.0).contains(&p[1])));
    }
}
