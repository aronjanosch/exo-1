// Cloud shell (#224): a sphere around the planet, seen from inside and outside. Cheap on purpose:
// 3 octaves of value noise on the direction from the planet centre, drifting slowly, one more
// sample towards the sun for the shading of the thick parts, alpha blended. No volumetrics.
#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::{view, globals}}

struct CloudLook {
    // rgb cloud colour, w coverage 0..1
    color: vec4<f32>,
    // xyz towards the sun, w noise cells over a radian (shell radius / feature size)
    sun: vec4<f32>,
    // x drift in cells per second, y fade-out distance near the camera (m), z fade-in range (m)
    misc: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> look: CloudLook;

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.1031);
    let r = q + dot(q, q.yzx + 33.33);
    return fract((r.x + r.y) * r.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash3(i), hash3(i + vec3(1.0, 0.0, 0.0)), u.x), mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), u.x), mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

// Cloud thickness 0..1 at a point of the unit sphere.
fn thickness(p: vec3<f32>) -> f32 {
    let n = value_noise(p) * 0.56 + value_noise(p * 2.17 + 7.1) * 0.30 + value_noise(p * 4.63 + 13.7) * 0.14;
    let cov = look.color.w;
    let t = 0.74 - 0.42 * cov;
    return smoothstep(t, t + 0.16, n);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_normal);
    let drift = vec3(globals.time * look.misc.x, 0.0, globals.time * look.misc.x * 0.6);
    let p = dir * look.sun.w + drift;
    let d = thickness(p);
    if d < 0.01 { discard; }

    // Shading: thicker towards the sun means in shadow, the sunlit side of a puff is bright.
    let sun = look.sun.xyz;
    let towards = normalize(sun - dir * dot(sun, dir) + vec3(1e-4));
    let d_sun = thickness(p + towards * 0.09 * look.sun.w * 0.15);
    let lit = clamp(1.0 - (d_sun - d) * 2.2 - d * 0.25, 0.0, 1.0);
    let ndl = dot(dir, sun);
    let day = smoothstep(-0.12, 0.3, ndl);
    let shade = 0.10 + day * (0.35 + 0.75 * lit);
    var col = look.color.rgb * shade;

    // Near the camera (inside or next to the layer) the cloud thins out, no wall of white.
    let dist = distance(in.world_position.xyz, view.world_position);
    let near = smoothstep(look.misc.y, look.misc.y + look.misc.z, dist);
    return vec4(col, d * 0.9 * near);
}
