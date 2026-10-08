"""Walker figures: one body plan, four blockouts (one human, three aliens).

This script is the only source of the figures (concept repo DECISIONS.md,
model source). Blender units are metres; each figure stands on its origin,
faces -Y and fits roughly into the walker capsule (1.8 m, radius 0.35 m).

Run headless:
    blender -b -P art/walker/walker.py -- --renders OUT_DIR [--blend FILE]
"""

import argparse
import math
import sys

import bmesh
import bpy
from mathutils import Vector

SHARP_ANGLE = math.radians(50)


# ---------------------------------------------------------------- materials

def material(name, rgb, roughness=0.6):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1)
    bsdf.inputs["Roughness"].default_value = roughness
    return m


def shared_materials():
    return {
        "eye": material("EyeWhite", (0.95, 0.95, 0.92), 0.3),
        "pupil": material("Pupil", (0.02, 0.02, 0.02), 0.3),
        "mouth": material("Mouth", (0.25, 0.04, 0.06)),
        "tongue": material("Tongue", (0.8, 0.32, 0.38)),
        "tooth": material("Tooth", (0.9, 0.88, 0.75)),
        "pants": material("Pants", (0.12, 0.13, 0.22)),
        "shoes": material("Shoes", (0.85, 0.85, 0.82)),
        "sole": material("Sole", (0.18, 0.18, 0.2)),
        "trim": material("Trim", (0.9, 0.88, 0.82)),
        "belt": material("Belt", (0.28, 0.16, 0.08)),
        "metal": material("Metal", (0.75, 0.75, 0.78), 0.3),
        "hair": material("Hair", (0.32, 0.2, 0.1)),
    }


# ---------------------------------------------------------------- body plan

class Figure:
    """Collects parts in local coordinates; build() joins them into one mesh."""

    def __init__(self, name, skin, suit):
        self.name = name
        self.parts = []
        self.m = shared_materials()
        self.m["skin"] = material(f"Skin_{name}", skin)
        self.m["skin_dark"] = material(f"SkinDark_{name}", tuple(c * 0.7 for c in skin))
        # Every figure has one material named Suit in its export; in the
        # blockout scene each one gets its own slot colour.
        self.m["suit"] = material(f"Suit_{name}", suit)

    def _add(self, obj, mat):
        obj.data.materials.append(self.m[mat])
        self.parts.append(obj)
        return obj

    # -- primitives

    def blob(self, mat, loc, size, segments=None, rings=None, rot=(0, 0, 0)):
        """Ellipsoid with half-axes `size` (x, y, z); small ones get fewer faces."""
        small = max(size) < 0.04
        segments = segments or (10 if small else 16)
        rings = rings or (6 if small else 10)
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, radius=1, location=loc)
        o = bpy.context.active_object
        o.scale = size
        o.rotation_euler = tuple(math.radians(a) for a in rot)
        return self._add(o, mat)

    def cap(self, mat, loc, size, keep_above, tilt=0.0, segments=16, rings=10):
        """Top of an ellipsoid shell (z above `keep_above` of its height), tilted
        forward by `tilt` degrees: eyelids, hair caps, hoods."""
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, radius=1, location=loc)
        o = bpy.context.active_object
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if v.co.z < keep_above - 1e-4], context="VERTS")
        bm.to_mesh(o.data)
        bm.free()
        o.scale = size
        o.rotation_euler = (math.radians(tilt), 0, 0)
        return self._add(o, mat)

    def tube(self, mat, points, radius, taper=None, sides=None):
        """Smooth noodle through `points`; `taper` scales the radius per point.

        Thick tubes get more sides: below 40 degrees per side they shade round.
        """
        sides = sides or (12 if radius >= 0.04 else 8)
        curve = bpy.data.curves.new("tube", "CURVE")
        curve.dimensions = "3D"
        curve.bevel_depth = radius
        curve.bevel_resolution = (sides - 4) // 2  # ring of 4 + 2 * resolution
        curve.resolution_u = 4
        curve.use_fill_caps = True
        spline = curve.splines.new("NURBS" if len(points) > 2 else "POLY")
        spline.points.add(len(points) - 1)
        for i, p in enumerate(points):
            spline.points[i].co = (*p, 1)
            spline.points[i].radius = taper[i] if taper else 1
        if len(points) > 2:
            spline.order_u = 3
            spline.use_endpoint_u = True
        o = bpy.data.objects.new("tube", curve)
        bpy.context.collection.objects.link(o)
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.convert(target="MESH")
        return self._add(bpy.context.active_object, mat)

    def torus(self, mat, loc, major, minor, squash=1.0, segments=24):
        bpy.ops.mesh.primitive_torus_add(major_segments=segments, minor_segments=8,
                                         major_radius=major, minor_radius=minor, location=loc)
        o = bpy.context.active_object
        o.scale = (1, squash, 1)
        return self._add(o, mat)

    def box(self, mat, loc, size):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
        o = bpy.context.active_object
        o.scale = size
        return self._add(o, mat)

    # -- features

    def eye(self, centre, radius, look=(0, 0), lid=0.3, lid_tilt=30, lid_mat="skin"):
        """Eyeball with a dot pupil and a heavy upper lid.

        look shifts the pupil (x, z) in fractions of the radius; lid is how far
        down the lid comes (0 open, 0.6 half shut), lid_tilt turns it forward.
        """
        centre = Vector(centre)
        seg, rings = (24, 14) if radius > 0.08 else (14, 8)
        self.blob("eye", centre, (radius,) * 3, segments=seg, rings=rings)
        pupil = centre + Vector((look[0] * radius, -radius * 0.92, look[1] * radius))
        self.blob("pupil", pupil, (radius * 0.24, radius * 0.12, radius * 0.24), segments=8, rings=4)
        if lid:
            self.cap(lid_mat, centre, (radius * 1.08,) * 3, keep_above=1 - 2 * lid, tilt=lid_tilt,
                     segments=seg, rings=rings)

    def eye_on(self, head, dx, dz, radius, **kw):
        self.eye(surface_front(head, dx, dz, inset=radius * 0.45), radius, **kw)

    def brow(self, head, dx, dz, width, angle=0.0, mat="hair"):
        """Short thick brow over an eye; angle in degrees, positive = outer end up."""
        a = math.radians(angle)
        side = 1 if dx >= 0 else -1
        pts = []
        for t in (-1, 0, 1):
            x = dx + t * width / 2
            z = dz + t * side * math.tan(a) * width / 2 + (0.006 if t == 0 else 0)
            pts.append(surface_front(head, x, z, inset=-0.006))
        self.tube(mat, pts, 0.011, taper=[0.6, 1, 0.6], sides=6)

    def ear(self, head, side, dz=0.0, size=1.0):
        c, (rx, ry, rz) = head.location, head.scale
        loc = (c.x + side * rx * 0.97, c.y + 0.01, c.z + dz)
        self.blob("skin", loc, (0.022 * size, 0.04 * size, 0.055 * size), rot=(0, side * -15, 0))
        self.blob("skin_dark", (loc[0] + side * 0.01, loc[1] - 0.004, loc[2]),
                  (0.012 * size, 0.026 * size, 0.038 * size))

    def mouth(self, head, dz, width, open_=0.015, lip=True):
        loc = surface_front(head, 0, dz, inset=0.012)
        self.blob("mouth", loc, (width, 0.015, open_))
        if lip:
            self.tube("skin_dark", [loc + Vector((-width * 1.05, -0.006, -open_ * 0.6)),
                                    loc + Vector((0, -0.012, -open_ * 1.3)),
                                    loc + Vector((width * 1.05, -0.006, -open_ * 0.6))], 0.008, sides=6)
        return loc

    def hand(self, wrist, side, fingers=3, size=1.0, mat="skin"):
        """Cartoon hand hanging down: palm, `fingers` fingers and a thumb."""
        w = Vector(wrist)
        s = size
        self.blob(mat, w + Vector((0, 0, -0.035 * s)), (0.02 * s, 0.035 * s, 0.04 * s))
        for i in range(fingers):
            y = (-0.022 + i * 0.044 / max(1, fingers - 1)) * s
            top = w + Vector((0, y, -0.065 * s))
            self.tube(mat, [top, top + Vector((side * 0.006, y * 0.2, -0.045 * s))], 0.01 * s, sides=6)
        t = w + Vector((side * -0.008, -0.03 * s, -0.03 * s))
        self.tube(mat, [t, t + Vector((0, -0.02 * s, -0.03 * s))], 0.01 * s, sides=6)

    def shoe(self, x, side, y=0.0, size=1.0):
        s = size
        self.blob("sole", (x, y - 0.035 * s, 0.018 * s), (0.07 * s, 0.13 * s, 0.018 * s))
        self.blob("shoes", (x, y - 0.03 * s, 0.05 * s), (0.065 * s, 0.12 * s, 0.045 * s))
        for k in (0, 1):
            self.tube("trim", [(x - 0.03 * s, y - (0.07 + k * 0.025) * s, 0.085 * s),
                               (x + 0.03 * s, y - (0.07 + k * 0.025) * s, 0.085 * s)], 0.005, sides=4)

    def spots(self, blob, where, mat="skin_dark"):
        """Flat bumps on an ellipsoid: where = [(azimuth deg, elevation deg, radius)]."""
        c, (rx, ry, rz) = blob.location, blob.scale
        for az, el, r in where:
            a, e = math.radians(az), math.radians(el)
            n = Vector((math.cos(e) * math.sin(a), -math.cos(e) * math.cos(a), math.sin(e)))
            p = c + Vector((n.x * rx, n.y * ry, n.z * rz)) * 0.985
            self.blob(mat, p, (r, r, r))

    def build(self, x=0.0):
        bpy.ops.object.select_all(action="DESELECT")
        for p in self.parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = self.parts[0]
        bpy.ops.object.join()
        o = bpy.context.active_object
        o.name = self.name
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
        o.data.shade_smooth()
        o.data.set_sharp_from_angle(angle=SHARP_ANGLE)
        o.location.x = x
        return o


def surface_front(blob, dx, dz, inset=0.0):
    """Point on the front (-Y) of an ellipsoid blob, pushed `inset` inwards."""
    c, (rx, ry, rz) = blob.location, blob.scale
    k = max(0.0, 1 - (dx / rx) ** 2 - (dz / rz) ** 2)
    return Vector((c.x + dx, c.y - ry * math.sqrt(k) + inset, c.z + dz))


def mirrored(f, fn):
    for s in (-1, 1):
        fn(f, s)


# ---------------------------------------------------------------- the four

def norb():
    """Human: lanky, big head, sleepy bulging eyes, T-shirt in the slot colour."""
    f = Figure("Norb", skin=(0.93, 0.72, 0.58), suit=(0.1, 0.55, 0.85))

    def leg(f, s):
        x = s * 0.095
        f.shoe(x, s)
        f.tube("pants", [(x, 0, 0.08), (x, 0.015, 0.45), (x, 0, 0.84)], 0.058, taper=[0.85, 0.9, 1.15])
        f.torus("pants", (x, 0, 0.1), 0.05, 0.012, segments=16)

    def arm(f, s):
        sh = (s * 0.2, 0, 1.15)
        f.tube("suit", [sh, (s * 0.24, 0.01, 1.04)], 0.052)
        f.torus("suit", (s * 0.245, 0.01, 1.03), 0.045, 0.01, segments=16)
        f.tube("skin", [(s * 0.23, 0.01, 1.08), (s * 0.27, 0.02, 0.95), (s * 0.27, -0.03, 0.8)], 0.032)
        f.hand((s * 0.27, -0.035, 0.8), s, fingers=3)

    mirrored(f, leg)
    mirrored(f, arm)
    # trousers and belt
    f.blob("pants", (0, 0, 0.86), (0.17, 0.12, 0.09))
    f.torus("belt", (0, 0, 0.9), 0.165, 0.016, squash=0.72)
    f.box("metal", (0, -0.125, 0.9), (0.04, 0.012, 0.03))
    # T-shirt: chest, belly, collar, hem
    f.blob("suit", (0, 0, 1.06), (0.2, 0.13, 0.15))
    f.blob("suit", (0, -0.005, 0.95), (0.18, 0.13, 0.11))
    f.torus("suit", (0, 0, 0.88), 0.17, 0.018, squash=0.74)
    f.torus("suit", (0, -0.005, 1.2), 0.055, 0.014, segments=16)
    # neck and head
    f.tube("skin", [(0, 0, 1.18), (0, -0.01, 1.26), (0, -0.02, 1.32)], 0.045)
    head = f.blob("skin", (0, -0.02, 1.49), (0.18, 0.17, 0.2), segments=20, rings=12)
    f.blob("skin", (0, -0.06, 1.35), (0.11, 0.1, 0.07))  # chin and jaw
    mirrored(f, lambda f, s: f.ear(head, s, dz=0.0))
    f.cap("hair", (0, 0.0, 1.5), (0.19, 0.185, 0.205), keep_above=0.15, tilt=-18, segments=20, rings=12)
    for x, z, r in ((-0.09, 1.66, 0.06), (0.0, 1.69, 0.07), (0.1, 1.66, 0.055), (0.05, 1.63, 0.05)):
        f.blob("hair", (x, -0.07, z), (r, r * 0.8, r * 0.7))
    f.eye_on(head, -0.072, 0.03, 0.055, look=(0.15, 0.0), lid=0.32, lid_tilt=25)
    f.eye_on(head, 0.072, 0.03, 0.06, look=(-0.1, -0.05), lid=0.25, lid_tilt=25)
    f.brow(head, -0.075, 0.1, 0.07, angle=-8)
    f.brow(head, 0.075, 0.105, 0.07, angle=12)
    f.blob("skin", surface_front(head, 0, -0.04, inset=0.008), (0.028, 0.045, 0.035))
    f.mouth(head, -0.12, 0.055, open_=0.014)
    f.torus("belt", (-0.27, -0.03, 0.82), 0.03, 0.008, segments=12)  # wristwatch
    return f


def glibbo():
    """Alien: mostly head on stilt legs, one huge eye, toothy mouth, scarf."""
    f = Figure("Glibbo", skin=(0.85, 0.68, 0.12), suit=(0.9, 0.2, 0.5))

    def leg(f, s):
        x = s * 0.12
        for t in (-1, 0, 1):
            f.blob("skin", (x + t * 0.03, -0.09, 0.02), (0.022, 0.06, 0.02), rot=(0, 0, t * -15))
        f.blob("skin", (x, -0.02, 0.03), (0.04, 0.05, 0.03))
        f.tube("skin", [(x, 0, 0.03), (x, 0.05, 0.55), (x, 0, 1.0)], 0.028)
        f.blob("skin", (x, 0.04, 0.55), (0.04, 0.04, 0.045))  # knobbly knee

    def arm(f, s):
        f.tube("skin", [(s * 0.3, 0, 1.25), (s * 0.4, -0.02, 1.08), (s * 0.38, -0.08, 0.95)], 0.022)
        f.blob("skin", (s * 0.4, -0.02, 1.08), (0.03, 0.03, 0.03))  # elbow
        f.hand((s * 0.38, -0.08, 0.95), s, fingers=3, size=0.9)

    mirrored(f, leg)
    mirrored(f, arm)
    # scarf: two knitted rings and a hanging end
    f.torus("suit", (0, 0, 1.06), 0.22, 0.06)
    f.torus("trim", (0, 0, 1.11), 0.21, 0.04)
    head = f.blob("skin", (0, 0, 1.33), (0.33, 0.29, 0.33), segments=24, rings=14)
    f.spots(head, [(55, 30, 0.02), (-45, 45, 0.015)])
    f.eye_on(head, 0, 0.1, 0.13, look=(0.1, -0.05), lid=0.22, lid_tilt=12)
    f.brow(head, 0, 0.27, 0.2, angle=0, mat="skin_dark")
    mouth = f.mouth(head, -0.13, 0.15, open_=0.05, lip=True)
    for dx, h in ((-0.07, 0.03), (-0.02, 0.022), (0.05, 0.034)):
        f.blob("tooth", mouth + Vector((dx, -0.035, 0.035 - h * 0.4)), (0.02, 0.012, h), segments=8, rings=4)
    f.blob("tongue", mouth + Vector((0.02, -0.03, -0.02)), (0.07, 0.03, 0.025))
    for x, curl in ((-0.05, -1), (0.0, 1), (0.06, 1)):
        base = (x, 0.02, 1.64)
        f.tube("skin_dark", [base, (x + curl * 0.02, 0.0, 1.72), (x + curl * 0.05, -0.02, 1.71)], 0.006, sides=4)
    return f


def zorp():
    """Alien: heavy pear body, no neck, three eyes on stalks, hoodie."""
    f = Figure("Zorp", skin=(0.5, 0.3, 0.62), suit=(0.95, 0.5, 0.1))

    def leg(f, s):
        x = s * 0.13
        f.shoe(x, s, size=1.15)
        f.tube("pants", [(x, 0, 0.08), (x, 0, 0.36)], 0.075)
        f.torus("pants", (x, 0, 0.11), 0.072, 0.014, segments=16)

    def arm(f, s):
        f.tube("suit", [(s * 0.26, 0, 1.0), (s * 0.34, -0.02, 0.88), (s * 0.35, -0.05, 0.8)], 0.055)
        f.torus("trim", (s * 0.35, -0.05, 0.79), 0.045, 0.012, segments=16)
        f.hand((s * 0.35, -0.06, 0.79), s, fingers=2, size=1.2)

    mirrored(f, leg)
    mirrored(f, arm)
    # hoodie: belly, chest, waistband, pocket, zip, hood and strings
    f.blob("suit", (0, 0, 0.62), (0.32, 0.27, 0.31), segments=20, rings=12)
    f.blob("suit", (0, 0, 0.95), (0.24, 0.21, 0.2))
    f.torus("trim", (0, 0, 0.38), 0.21, 0.025, squash=0.85)
    f.blob("suit", (0, -0.25, 0.55), (0.17, 0.05, 0.09))
    f.tube("metal", [surface_front(f.parts[-3], 0, 0.15, inset=-0.004),
                     surface_front(f.parts[-3], 0, 0.0, inset=-0.004)], 0.006, sides=4)
    f.blob("suit", (0, 0.13, 1.12), (0.2, 0.1, 0.12))
    for s in (-1, 1):
        top = (s * 0.06, -0.17, 1.08)
        f.tube("trim", [top, (s * 0.07, -0.21, 0.96), (s * 0.065, -0.22, 0.88)], 0.006, sides=4)
        f.blob("metal", (s * 0.065, -0.22, 0.87), (0.012, 0.012, 0.018))
    head = f.blob("skin", (0, -0.02, 1.16), (0.17, 0.16, 0.12))
    f.mouth(head, -0.03, 0.09, open_=0.016)
    for tip, r, look, lid in (((-0.13, -0.06, 1.52), 0.055, (0.2, 0), 0.35),
                              ((0.0, -0.09, 1.62), 0.06, (0, -0.15), 0.2),
                              ((0.14, -0.05, 1.48), 0.05, (-0.25, 0.1), 0.45)):
        base = (tip[0] * 0.4, -0.02, 1.25)
        mid = (tip[0] * 0.8, tip[1] * 0.5, (1.25 + tip[2]) / 2 + 0.03)
        f.tube("skin", [base, mid, (tip[0], tip[1] + 0.02, tip[2] - r * 0.8)], 0.02)
        f.eye(tip, r, look, lid=lid, lid_tilt=20)
    return f


def wobbel():
    """Alien: tentacles for legs, four arms, lumpy head, bathrobe."""
    f = Figure("Wobbel", skin=(0.5, 0.78, 0.25), suit=(0.25, 0.35, 0.9))

    for i in range(6):
        a = math.radians(30 + i * 60)
        c, s = math.cos(a), math.sin(a)
        curl = math.radians(30 + i * 60 + 35)
        f.tube("skin", [(0, 0, 0.55), (c * 0.12, s * 0.12, 0.3), (c * 0.28, s * 0.28, 0.05),
                        (c * 0.38, s * 0.38, 0.03), (math.cos(curl) * 0.4, math.sin(curl) * 0.4, 0.09)],
               0.065, taper=[1, 0.9, 0.55, 0.35, 0.2], sides=8)
        for t, r in ((0.2, 0.018), (0.28, 0.014), (0.35, 0.01)):
            f.blob("skin_dark", (c * t, s * t, 0.012 + (0.32 - t) * 0.1), (r, r, r * 0.5))
    # bathrobe: body, lapels, belt with knot and ends, pocket
    robe = f.blob("suit", (0, 0, 0.86), (0.23, 0.19, 0.36), segments=20, rings=12)
    for s in (-1, 1):
        f.blob("trim", (s * 0.045, -0.172, 1.0), (0.022, 0.02, 0.15), rot=(0, s * 22, 0))
    f.torus("trim", (0, 0, 0.78), 0.215, 0.022, squash=0.85)
    f.blob("trim", (0.05, -0.17, 0.78), (0.035, 0.025, 0.03))
    for dx in (0.03, 0.07):
        f.tube("trim", [(dx, -0.17, 0.77), (dx + 0.01, -0.175, 0.68), (dx, -0.17, 0.6)], 0.012, sides=6)
    f.blob("suit", surface_front(robe, -0.12, -0.12, inset=0.02), (0.06, 0.02, 0.05))

    def arms(f, s):
        for z, reach in ((1.1, 0.0), (0.95, 0.05)):
            f.blob("suit", (s * 0.19, 0, z), (0.06, 0.06, 0.05))
            f.torus("trim", (s * 0.22, -0.01, z - 0.04), 0.035, 0.014, segments=12)
            f.tube("skin", [(s * 0.2, 0, z), (s * (0.31 + reach), -0.03, z - 0.12),
                            (s * (0.3 + reach), -0.1, z - 0.24)], 0.025)
            f.hand((s * (0.3 + reach), -0.1, z - 0.24), s, fingers=2, size=0.9)

    mirrored(f, arms)
    f.tube("skin", [(0, 0, 1.14), (0, -0.02, 1.26), (0, -0.03, 1.38)], 0.05)
    head = f.blob("skin", (0, -0.03, 1.5), (0.17, 0.15, 0.14), segments=20, rings=12)
    f.blob("skin", (0.08, 0.0, 1.61), (0.12, 0.11, 0.1))
    f.blob("skin", (-0.09, 0.02, 1.58), (0.08, 0.08, 0.07))
    f.spots(head, [(-55, 25, 0.018)])
    f.eye_on(head, -0.07, 0.03, 0.065, look=(0.2, 0.1), lid=0.2, lid_tilt=25)
    f.eye_on(head, 0.07, 0.0, 0.042, look=(-0.1, -0.2), lid=0.42, lid_tilt=25)
    f.brow(head, -0.075, 0.1, 0.07, angle=-15, mat="skin_dark")
    f.brow(head, 0.07, 0.06, 0.05, angle=20, mat="skin_dark")
    f.blob("skin_dark", surface_front(head, 0, -0.03, inset=0.006), (0.012, 0.01, 0.008))
    f.mouth(head, -0.08, 0.07, open_=0.022)
    return f


FIGURES = [norb, glibbo, zorp, wobbel]


# ---------------------------------------------------------------- review renders

def stage(night=False):
    scene = bpy.context.scene
    bpy.ops.mesh.primitive_plane_add(size=400)
    ground = bpy.context.active_object
    ground.data.materials.append(material("Ground", (0.08, 0.07, 0.1) if night else (0.42, 0.38, 0.45), 0.9))
    bpy.ops.object.light_add(type="SUN", rotation=(math.radians(55), 0, math.radians(-30)))
    bpy.context.active_object.data.energy = 1.2 if night else 3.5
    world = bpy.data.worlds.new("Sky")
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (
        (0.004, 0.004, 0.015, 1) if night else (0.25, 0.22, 0.35, 1))
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    scene.world = world
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.device = "GPU"
    return ground


def shoot(path, location, target, lens=None, fov_deg=None, size=(1600, 900)):
    scene = bpy.context.scene
    bpy.ops.object.camera_add(location=location)
    cam = bpy.context.active_object
    cam.rotation_euler = (Vector(target) - Vector(location)).to_track_quat("-Z", "Y").to_euler()
    if fov_deg:
        cam.data.lens_unit = "FOV"
        cam.data.angle = math.radians(fov_deg)
    else:
        cam.data.lens = lens
    scene.camera = cam
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(cam)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--renders", help="directory for the review renders")
    ap.add_argument("--blend", help="save the lineup scene for a look in Blender")
    args = ap.parse_args(argv)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    spacing = 1.2
    xs = [(i - (len(FIGURES) - 1) / 2) * spacing for i in range(len(FIGURES))]
    figures = [make().build(x) for make, x in zip(FIGURES, xs)]
    for o in figures:
        tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
        top = max((o.matrix_world @ v.co).z for v in o.data.vertices)
        half = max(abs(v.co.x) for v in o.data.vertices)
        print(f"FIGURE {o.name}: {tris} triangles, {top:.2f} m tall, {half:.2f} m half width")

    ground = stage()
    if args.blend:
        bpy.ops.wm.save_as_mainfile(filepath=args.blend)
    if args.renders:
        d = args.renders.rstrip("/")
        shoot(f"{d}/lineup_three_quarter.png", (2.4, -6.0, 1.7), (0, 0, 0.9), lens=40)
        shoot(f"{d}/lineup_front.png", (0, -7.0, 0.95), (0, 0, 0.9), lens=45)
        for o in figures:
            top = max((o.matrix_world @ v.co).z for v in o.data.vertices)
            face = (o.location.x, 0, top - 0.25)
            shoot(f"{d}/face_{o.name.lower()}.png", (o.location.x + 0.45, -1.3, face[2] + 0.05), face,
                  lens=50, size=(800, 800))
        # A game-like view: 80 degree field of view, eye height, 30 m away, night.
        bpy.data.objects.remove(ground)
        for light in [o for o in bpy.data.objects if o.type == "LIGHT"]:
            bpy.data.objects.remove(light)
        stage(night=True)
        shoot(f"{d}/at_30m_night.png", (0, -30, 1.0), (0, 0, 1.0), fov_deg=80, size=(1920, 1080))


main()
