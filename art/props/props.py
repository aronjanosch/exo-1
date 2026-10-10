"""Scatter props (#65): trees, a shrub, rocks, a ground-cover tuft and rare pieces.

This script is the only source of the props (concept repo DECISIONS.md, "Model source").
Blender units are metres; each prop stands on its origin, up is +Z (glTF +Y). Colours are
vertex colours: white parts take the biome tint in the game, darker parts stay darker
(the tint multiplies them). Goofy shapes on purpose. Triangle guide value for small props:
100-1,000 (DECISIONS.md, "Triangle budgets"); ground cover far below that, it is instanced
by the thousand.

Run headless (writes content/props/<id>.glb, renders optional):
    blender -b -P art/props/props.py -- --out content/props [--renders DIR]

The check report prints one line per prop with its triangle count, then a PROBLEM line for
each orientation problem or a prop over its triangle budget (BUDGETS). --renders writes one
three-quarter render (640x480, Workbench, vertex colours) per prop that has a budget.
"""

import argparse
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from mesh_checks import orientation_problems  # noqa: E402

SHARP_ANGLE = math.radians(40)
WHITE = (1.0, 1.0, 1.0)


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def colour_layer(bm):
    return bm.loops.layers.color.get("Col") or bm.loops.layers.color.new("Col")


def paint(bm, faces, rgb):
    layer = colour_layer(bm)
    for f in faces:
        for loop in f.loops:
            loop[layer] = (*rgb, 1.0)


def add_cone(bm, segments, r1, r2, depth, at, rgb, tilt=(0.0, 0.0)):
    m = Matrix.Translation(Vector(at)) @ Matrix.Rotation(tilt[0], 4, "X") @ Matrix.Rotation(tilt[1], 4, "Y") @ Matrix.Translation((0, 0, depth / 2))
    res = bmesh.ops.create_cone(bm, cap_ends=True, segments=segments, radius1=r1, radius2=r2, depth=depth, matrix=m)
    faces = {f for v in res["verts"] for f in v.link_faces}
    paint(bm, faces, rgb)
    return res["verts"]


def add_blob(bm, subdiv, radius, at, scale, rgb, jitter, rng):
    m = Matrix.Translation(Vector(at)) @ Matrix.Diagonal((*scale, 1.0))
    res = bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=radius, matrix=m)
    for v in res["verts"]:
        v.co += (v.co - Vector(at)).normalized() * rng.uniform(-jitter, jitter) * radius
    faces = {f for v in res["verts"] for f in v.link_faces}
    paint(bm, faces, rgb)
    return res["verts"]


def finish(bm, name):
    mesh = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    for p in mesh.polygons:
        p.use_smooth = True
    mod = obj.modifiers.new("hard edges", "EDGE_SPLIT")
    mod.split_angle = SHARP_ANGLE
    return obj


def add_cone_dir(bm, segments, r1, r2, length, at, direction, rgb, caps=True):
    """A cone from its base at `at`, pointing along `direction`."""
    rot = Vector((0, 0, 1)).rotation_difference(Vector(direction).normalized()).to_matrix().to_4x4()
    m = Matrix.Translation(Vector(at)) @ rot @ Matrix.Translation((0, 0, length / 2))
    res = bmesh.ops.create_cone(bm, cap_ends=caps, segments=segments, radius1=r1, radius2=r2, depth=length, matrix=m)
    paint(bm, {f for v in res["verts"] for f in v.link_faces}, rgb)
    return res["verts"]


def add_uvball(bm, segments, rings, radius, at, scale, rgb):
    """A UV sphere: fewer triangles than an icosphere of the same roundness."""
    m = Matrix.Translation(Vector(at)) @ Matrix.Diagonal((*scale, 1.0))
    res = bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=rings, radius=radius, matrix=m)
    paint(bm, {f for v in res["verts"] for f in v.link_faces}, rgb)
    return res["verts"]


def add_blade(bm, layer, pts, cols):
    """A thin two-sided blade: pts are base left, base right, mid left, mid right, tip."""
    front = [bm.verts.new(p) for p in pts]
    back = [bm.verts.new(p) for p in pts]
    colour = {v: c for v, c in zip(front + back, list(cols) + list(cols))}
    for vs in [(front[0], front[1], front[3], front[2]), (front[2], front[3], front[4]),
               (back[0], back[2], back[3], back[1]), (back[2], back[4], back[3])]:
        f = bm.faces.new(vs)
        for loop in f.loops:
            loop[layer] = (*colour[loop.vert], 1.0)


def add_lathe(bm, segments, profile):
    """Revolves an (r, z) profile around Z. Points on the axis (r == 0) share one vertex.
    Returns the faces per profile segment, profile[i] to profile[i + 1]."""
    rings = []
    for r, z in profile:
        if r == 0:
            rings.append([bm.verts.new((0.0, 0.0, z))] * segments)
        else:
            rings.append([bm.verts.new((r * math.cos(a), r * math.sin(a), z))
                          for a in (k / segments * math.tau for k in range(segments))])
    per_segment = []
    for i in range(len(profile) - 1):
        faces = []
        for k in range(segments):
            k2 = (k + 1) % segments
            corners = list(dict.fromkeys([rings[i][k], rings[i][k2], rings[i + 1][k2], rings[i + 1][k]]))
            faces.append(bm.faces.new(corners))
        per_segment.append(faces)
    bmesh.ops.recalc_face_normals(bm, faces=[f for fs in per_segment for f in fs])
    return per_segment


# ------------------------------------------------------------------ props

def tree_lolly(rng):
    """A lollipop tree: a bent stick with a squashed lumpy ball on top."""
    bm = bmesh.new()
    add_cone(bm, 6, 0.28, 0.16, 2.2, (0, 0, 0), (0.42, 0.33, 0.26))
    add_cone(bm, 6, 0.16, 0.12, 1.6, (0, 0, 2.1), (0.42, 0.33, 0.26), tilt=(0.25, 0.1))
    add_blob(bm, 2, 1.9, (0.0, -0.4, 4.6), (1.0, 1.0, 0.8), WHITE, 0.12, rng)
    add_blob(bm, 2, 0.9, (0.9, 0.2, 5.4), (1.0, 1.0, 0.9), WHITE, 0.1, rng)
    return finish(bm, "tree_lolly")


def tree_stack(rng):
    """Three cones stacked crooked, like a pine that lost an argument."""
    bm = bmesh.new()
    add_cone(bm, 5, 0.22, 0.18, 1.2, (0, 0, 0), (0.40, 0.31, 0.25))
    z = 1.0
    for i, (r, h) in enumerate([(1.7, 2.4), (1.25, 2.0), (0.8, 1.7)]):
        t = 0.12 if i % 2 == 0 else -0.14
        add_cone(bm, 7, r, 0.05, h, (0.0, 0.0, z), WHITE, tilt=(t, -t * 0.6))
        z += h * 0.62
    return finish(bm, "tree_stack")


def shrub_blob(rng):
    """Three lumps pushed together."""
    bm = bmesh.new()
    for at, r in [((0, 0, 0.45), 0.6), ((0.5, 0.2, 0.35), 0.45), ((-0.35, 0.4, 0.3), 0.4)]:
        add_blob(bm, 2, r, at, (1.0, 1.0, 0.85), WHITE, 0.15, rng)
    return finish(bm, "shrub_blob")


def rock_round(rng):
    """A lumpy boulder, flatter at the bottom (sinks into the ground a little)."""
    bm = bmesh.new()
    vs = add_blob(bm, 2, 1.0, (0, 0, 0.35), (1.2, 1.0, 0.75), (0.78, 0.78, 0.78), 0.22, rng)
    for v in vs:
        v.co.z = max(v.co.z, -0.1)
    return finish(bm, "rock_round")


def rock_slab(rng):
    """A tilted slab with chipped corners."""
    bm = bmesh.new()
    m = Matrix.Rotation(0.18, 4, "Y") @ Matrix.Translation((0, 0, 0.3)) @ Matrix.Diagonal((1.6, 1.0, 0.45, 1.0))
    res = bmesh.ops.create_cube(bm, size=1.0, matrix=m)
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=1, use_grid_fill=True)
    for v in bm.verts:
        v.co += Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-1, 1))) * 0.08
    paint(bm, bm.faces, (0.72, 0.72, 0.74))
    return finish(bm, "rock_slab")


def tuft(rng):
    """Ground cover: five fat blades fanned out."""
    bm = bmesh.new()
    layer = colour_layer(bm)
    for i in range(5):
        a = i / 5 * math.tau + rng.uniform(-0.3, 0.3)
        lean = rng.uniform(0.15, 0.4)
        h = rng.uniform(0.3, 0.5)
        d = Vector((math.cos(a), math.sin(a), 0))
        side = Vector((-d.y, d.x, 0)) * 0.05
        tip = d * lean + Vector((0, 0, h))
        v0 = bm.verts.new(-side)
        v1 = bm.verts.new(side)
        v2 = bm.verts.new(tip)
        f = bm.faces.new((v0, v1, v2))
        for loop, c in zip(f.loops, [(0.8, 0.8, 0.8), (0.8, 0.8, 0.8), WHITE]):
            loop[layer] = (*c, 1.0)
        # back face, so the blade shows from both sides without double-sided materials
        b = bm.faces.new((bm.verts.new(tip), bm.verts.new(side), bm.verts.new(-side)))
        for loop, c in zip(b.loops, [WHITE, (0.8, 0.8, 0.8), (0.8, 0.8, 0.8)]):
            loop[layer] = (*c, 1.0)
    return finish(bm, "tuft")


def mushroom_giant(rng):
    """Rare: a mushroom taller than a house, cap slightly askew."""
    bm = bmesh.new()
    add_cone(bm, 8, 0.7, 0.5, 5.0, (0, 0, 0), (0.92, 0.88, 0.80))
    cap = add_blob(bm, 3, 3.0, (0.3, 0.0, 5.6), (1.0, 1.0, 0.45), WHITE, 0.05, rng)
    for v in cap:
        v.co.z = max(v.co.z, 5.0)
    return finish(bm, "mushroom_giant")


def crystal(rng):
    """Rock variant: a cluster of leaning crystals."""
    bm = bmesh.new()
    for at, h, r, t in [((0, 0, 0), 2.2, 0.35, (0.0, 0.0)), ((0.45, 0.1, 0), 1.4, 0.25, (0.0, 0.5)), ((-0.3, 0.35, 0), 1.1, 0.22, (-0.45, -0.2))]:
        add_cone(bm, 6, r, r * 0.9, h * 0.75, at, WHITE, tilt=t)
        m = Matrix.Translation(Vector(at)) @ Matrix.Rotation(t[0], 4, "X") @ Matrix.Rotation(t[1], 4, "Y") @ Matrix.Translation((0, 0, h * 0.75 + h * 0.125))
        res = bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=r * 0.9, radius2=0.0, depth=h * 0.25, matrix=m)
        paint(bm, {f for v in res["verts"] for f in v.link_faces}, WHITE)
    return finish(bm, "crystal")


# ------------------------------------------------------------- site kits (#70)

def hull_broken(rng):
    """Crash site: the front half of a stubby hull, nose down, torn open at the back."""
    bm = bmesh.new()
    m = Matrix.Rotation(math.radians(-14), 4, "X") @ Matrix.Translation((0, 0, 1.4)) @ Matrix.Rotation(math.radians(90), 4, "X")
    res = bmesh.ops.create_cone(bm, cap_ends=False, segments=10, radius1=1.9, radius2=1.4, depth=5.5, matrix=m)
    paint(bm, {f for v in res["verts"] for f in v.link_faces}, (0.82, 0.80, 0.76))
    for v in res["verts"]:
        if v.co.y < -1.0:
            v.co += Vector((rng.uniform(-0.4, 0.4), rng.uniform(-0.6, 0.2), rng.uniform(-0.3, 0.3)))
    nose = add_blob(bm, 2, 1.45, (0.0, 2.9, 0.75), (1.0, 1.1, 0.9), (0.82, 0.80, 0.76), 0.04, rng)
    fin = add_cone(bm, 4, 0.9, 0.1, 1.8, (0.0, -1.2, 2.6), (0.95, 0.45, 0.12), tilt=(0.5, 0.0))
    return finish(bm, "hull_broken")


def debris_panel(rng):
    """A bent hull panel."""
    bm = bmesh.new()
    m = Matrix.Rotation(0.3, 4, "Y") @ Matrix.Translation((0, 0, 0.1)) @ Matrix.Diagonal((1.4, 0.9, 0.08, 1.0))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=1, use_grid_fill=True)
    for v in bm.verts:
        v.co.z += 0.25 * (v.co.x / 0.7) ** 2
    paint(bm, bm.faces, (0.80, 0.78, 0.74))
    return finish(bm, "debris_panel")


def debris_strut(rng):
    """A snapped strut."""
    bm = bmesh.new()
    add_cone(bm, 5, 0.12, 0.09, 2.4, (0, 0, 0), (0.55, 0.55, 0.58), tilt=(1.35, 0.2))
    return finish(bm, "debris_strut")


def wall_broken(rng):
    """Ruin: a wall piece with a ragged top."""
    bm = bmesh.new()
    m = Matrix.Translation((0, 0, 1.1)) @ Matrix.Diagonal((3.0, 0.45, 2.2, 1.0))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=3, use_grid_fill=True)
    for v in bm.verts:
        if v.co.z > 1.5:
            v.co.z -= rng.uniform(0.0, 1.1) * (0.5 + 0.5 * math.sin(v.co.x * 2.1))
        v.co.x += rng.uniform(-0.03, 0.03)
    paint(bm, bm.faces, (0.78, 0.74, 0.66))
    return finish(bm, "wall_broken")


def pillar_stub(rng):
    """Ruin: a broken round pillar."""
    bm = bmesh.new()
    vs = add_cone(bm, 8, 0.45, 0.4, 2.0, (0, 0, 0), (0.80, 0.76, 0.68))
    for v in vs:
        if v.co.z > 1.0:
            v.co.z -= rng.uniform(0.0, 0.6)
    return finish(bm, "pillar_stub")


def mast(rng):
    """Lookout: a tall mast with a lamp box on top and three stays."""
    bm = bmesh.new()
    add_cone(bm, 6, 0.18, 0.1, 9.0, (0, 0, 0), (0.62, 0.62, 0.66))
    m = Matrix.Translation((0, 0, 9.3)) @ Matrix.Diagonal((0.7, 0.7, 0.5, 1.0))
    res = bmesh.ops.create_cube(bm, size=1.0, matrix=m)
    paint(bm, {f for v in res["verts"] for f in v.link_faces}, (1.0, 0.75, 0.2))
    for i in range(3):
        a = i / 3 * math.tau
        add_cone(bm, 3, 0.04, 0.04, 6.3, (math.cos(a) * 3.0, math.sin(a) * 3.0, 0.0), (0.5, 0.5, 0.5), tilt=(math.sin(a) * -0.45, math.cos(a) * 0.45))
    return finish(bm, "mast")


def arch_giant(rng):
    """Landmark: a natural arch about 45 m tall, lumpy."""
    bm = bmesh.new()
    segs = 14
    for i in range(segs):
        a0 = math.pi * i / segs
        a1 = math.pi * (i + 1) / segs
        p0 = Vector((math.cos(a0) * 24.0, 0.0, math.sin(a0) * 40.0))
        p1 = Vector((math.cos(a1) * 24.0, 0.0, math.sin(a1) * 40.0))
        mid = (p0 + p1) * 0.5
        d = (p1 - p0)
        r = 5.5 - 2.0 * math.sin((a0 + a1) * 0.5)
        rot = Vector((0, 0, 1)).rotation_difference(d.normalized()).to_matrix().to_4x4()
        m = Matrix.Translation(mid - d * 0.5) @ rot @ Matrix.Translation((0, 0, d.length * 0.55))
        res = bmesh.ops.create_cone(bm, cap_ends=True, segments=7, radius1=r * 1.1, radius2=r, depth=d.length * 1.2, matrix=m)
        for v in res["verts"]:
            v.co += Vector((rng.uniform(-0.6, 0.6), rng.uniform(-0.6, 0.6), rng.uniform(-0.6, 0.6)))
        paint(bm, {f for v in res["verts"] for f in v.link_faces}, (0.82, 0.70, 0.60))
    # feet sink into the ground
    for v in bm.verts:
        v.co.z = max(v.co.z, -3.0)
    return finish(bm, "arch_giant")


# ------------------------------------------------------- field and town kit (#219)

BARK = (0.40, 0.31, 0.25)


def grass_stalk(rng):
    """Ground cover: a bundle of three to five thin bent blades, darker at the root."""
    bm = bmesh.new()
    layer = colour_layer(bm)
    for _ in range(rng.randint(3, 5)):
        a = rng.uniform(0, math.tau)
        lean = rng.uniform(0.08, 0.2)
        h = rng.uniform(0.4, 0.7)
        w = rng.uniform(0.025, 0.04)
        d = Vector((math.cos(a), math.sin(a), 0))
        side = Vector((-d.y, d.x, 0)) * w
        mid = d * lean * 0.4 + Vector((0, 0, h * 0.55))
        tip = d * lean + Vector((0, 0, h))
        pts = [-side, side, mid - side * 0.6, mid + side * 0.6, tip]
        add_blade(bm, layer, pts, [(0.7, 0.7, 0.7), (0.7, 0.7, 0.7), WHITE, WHITE, WHITE])
    return finish(bm, "grass_stalk")


def flower_bud(rng):
    """A short stem, one leaf and a fat round goofy bud, coloured so it reads under the tint."""
    bm = bmesh.new()
    add_cone(bm, 5, 0.035, 0.03, 0.3, (0, 0, 0), WHITE)
    add_cone_dir(bm, 3, 0.09, 0.0, 0.24, (0, 0, 0.12), (1.0, 0.2, 0.7), WHITE)
    add_uvball(bm, 8, 4, 0.17, (0.03, 0.0, 0.38), (1.0, 1.0, 1.1), (1.0, 0.42, 0.72))
    return finish(bm, "flower_bud")


def shrub_spiky(rng):
    """Desert shrub: a low lump bristling with five leaning spikes."""
    bm = bmesh.new()
    add_blob(bm, 2, 0.42, (0, 0, 0.32), (1.0, 1.0, 0.75), WHITE, 0.1, rng)
    for i in range(5):
        th = i / 5 * math.tau + rng.uniform(-0.3, 0.3)
        al = math.radians(rng.uniform(30, 50))
        d = (math.sin(al) * math.cos(th), math.sin(al) * math.sin(th), math.cos(al))
        at = (0.3 * math.cos(th), 0.3 * math.sin(th), 0.3)
        add_cone_dir(bm, 4, 0.07, 0.0, rng.uniform(0.45, 0.6), at, d, (0.5, 0.45, 0.3))
    return finish(bm, "shrub_spiky")


def wrack_pile(rng):
    """Beach pile: a flat heap of dried weed with five strands tangled over it."""
    bm = bmesh.new()
    add_blob(bm, 2, 0.75, (0, 0, 0.05), (1.0, 1.0, 0.25), (0.42, 0.38, 0.26), 0.1, rng)
    for _ in range(5):
        th = rng.uniform(0, math.tau)
        length = rng.uniform(1.0, 1.4)
        d = Vector((math.cos(th), math.sin(th), rng.uniform(0.0, 0.35)))
        centre = Vector((rng.uniform(-0.3, 0.3), rng.uniform(-0.3, 0.3), rng.uniform(0.12, 0.2)))
        at = centre - d.normalized() * length / 2
        add_cone_dir(bm, 4, 0.03, 0.02, length, at, d, (0.28, 0.36, 0.2))
    return finish(bm, "wrack_pile")


def crooked_tree(rng, name, far):
    """Cold-climate tree: a trunk bent in three, two or three lumpy crowns on top."""
    bm = bmesh.new()
    sides = 3 if far else 7
    at = Vector((0, 0, 0))
    dirs = [(0.25, 0.1, 1.0), (-0.35, -0.1, 1.0), (0.2, 0.3, 1.0)]
    lengths = [3.0, 2.6, 2.4]
    radii = [(0.42, 0.34), (0.34, 0.25), (0.25, 0.14)]
    for d, length, (r1, r2) in zip(dirs, lengths, radii):
        add_cone_dir(bm, sides, r1, r2, length, at, d, BARK, caps=not far)
        at = at + Vector(d).normalized() * length
    crowns = [((0, 0, -0.5), 1.9), ((1.5, 0.4, -1.6), 1.25), ((-1.2, -0.3, -2.2), 1.2)]
    for off, r in (crowns[:2] if far else crowns):
        add_blob(bm, 1 if far else 2, r, at + Vector(off), (1.0, 1.0, 0.8), WHITE, 0.12, rng)
    return finish(bm, name)


def tree_crooked(rng):
    return crooked_tree(rng, "tree_crooked", far=False)


def tree_crooked_far(rng):
    return crooked_tree(rng, "tree_crooked_far", far=True)


def round_tree(rng, name, far):
    """Crystal-biome lollipop: a stick, one big round crown, a few smaller balls."""
    bm = bmesh.new()
    add_cone(bm, 3 if far else 8, 0.22, 0.14, 5.2, (0, 0, 0), (0.60, 0.52, 0.66))
    add_uvball(bm, 6 if far else 16, 4 if far else 8, 2.3, (0, 0, 5.8), (1.0, 1.0, 0.95), WHITE)
    smalls = [((1.8, 0.5, 4.6), 0.9), ((-1.5, -0.9, 5.0), 0.75), ((0.3, -1.9, 4.2), 0.7)]
    for off, r in (smalls[:1] if far else smalls):
        add_uvball(bm, 4 if far else 8, 3 if far else 4, r, off, (1.0, 1.0, 1.0), WHITE)
    return finish(bm, name)


def tree_round(rng):
    return round_tree(rng, "tree_round", far=False)


def tree_round_far(rng):
    return round_tree(rng, "tree_round_far", far=True)


def landing_pad(rng):
    """Settlement landing pad: a 20 m disc with a raised rim, a darker ring, an orange H and four lights."""
    bm = bmesh.new()
    profile = [(0.0, 0.0), (10.0, 0.0), (10.0, 0.9), (9.4, 0.9), (9.4, 0.4), (7.6, 0.4), (6.6, 0.4), (0.0, 0.4)]
    paints = [(0.7, 0.7, 0.7), WHITE, WHITE, WHITE, WHITE, (0.55, 0.55, 0.6), WHITE]
    for faces, rgb in zip(add_lathe(bm, 48, profile), paints):
        paint(bm, faces, rgb)
    glyph = (1.0, 0.55, 0.18)
    turn = Matrix.Rotation(0.2, 4, "Z")
    for at, size in [((-1.25, 0.0, 0.415), (0.42, 3.0, 0.04)), ((1.25, 0.0, 0.415), (0.42, 3.0, 0.04)),
                     ((0.0, 0.0, 0.415), (2.5, 0.42, 0.04))]:
        # the glyph turns as one piece: bar centres turned about the pad centre, each bar turned too
        m = Matrix.Translation(turn.to_3x3() @ Vector(at)) @ turn @ Matrix.Diagonal((*size, 1.0))
        res = bmesh.ops.create_cube(bm, size=1.0, matrix=m)
        paint(bm, {f for v in res["verts"] for f in v.link_faces}, glyph)
    for k in range(4):
        a = k / 4 * math.tau
        x, y = 9.7 * math.cos(a), 9.7 * math.sin(a)
        add_cone(bm, 6, 0.12, 0.08, 1.2, (x, y, 0.8), (0.5, 0.5, 0.55))
        add_uvball(bm, 6, 4, 0.22, (x, y, 2.1), (1.0, 1.0, 1.0), (1.0, 0.9, 0.35))
    return finish(bm, "landing_pad")


PROPS = [tree_lolly, tree_stack, shrub_blob, rock_round, rock_slab, tuft, mushroom_giant, crystal,
         hull_broken, debris_panel, debris_strut, wall_broken, pillar_stub, mast, arch_giant,
         grass_stalk, flower_bud, shrub_spiky, wrack_pile, tree_crooked, tree_crooked_far,
         tree_round, tree_round_far, landing_pad]

# Triangle budgets (guide values, DECISIONS.md) for the #219 props; they also get review renders.
BUDGETS = {"grass_stalk": 40, "flower_bud": 80, "shrub_spiky": 150, "wrack_pile": 150,
           "tree_crooked": 500, "tree_crooked_far": 60, "tree_round": 450, "tree_round_far": 60,
           "landing_pad": 1500}


def problems_of(obj):
    bm = bmesh.new()
    try:
        bm.from_mesh(obj.data)
        return orientation_problems(bm)
    finally:
        bm.free()


def review(obj, path):
    """A three-quarter render of one prop, lit by Workbench, showing the vertex colours."""
    scene = bpy.context.scene
    obj.data.color_attributes.active_color = obj.data.color_attributes["Col"]
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.color_type = "VERTEX"
    scene.render.resolution_x, scene.render.resolution_y = 640, 480
    scene.render.filepath = path
    world = bpy.data.worlds.new("Backdrop")
    world.color = (0.36, 0.40, 0.46)
    scene.world = world
    corners = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    lo = Vector(tuple(min(c[i] for c in corners) for i in range(3)))
    hi = Vector(tuple(max(c[i] for c in corners) for i in range(3)))
    centre = (lo + hi) / 2
    size = max(hi[i] - lo[i] for i in range(3))
    location = centre + Vector((0.75, -1.0, 0.55)).normalized() * (size * 1.7 + 1.5)
    bpy.ops.object.camera_add(location=location)
    cam = bpy.context.active_object
    cam.rotation_euler = (centre - location).to_track_quat("-Z", "Y").to_euler()
    scene.camera = cam
    bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(cam)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="content/props")
    ap.add_argument("--renders", help="directory for one review render per prop with a budget")
    args = ap.parse_args(argv)
    for make in PROPS:
        clear()
        rng = random.Random(make.__name__)
        obj = make(rng)
        bpy.context.view_layer.objects.active = obj
        obj.select_set(True)
        dg = bpy.context.evaluated_depsgraph_get()
        tris = sum(len(p.vertices) - 2 for p in obj.evaluated_get(dg).data.polygons)
        problems = problems_of(obj)
        if obj.name in BUDGETS and tris > BUDGETS[obj.name]:
            problems.append(f"{tris} triangles, over the budget of {BUDGETS[obj.name]}")
        path = f"{args.out}/{obj.name}.glb"
        bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True, export_apply=True,
                                  export_vertex_color="ACTIVE", export_materials="NONE")
        print(f"prop {obj.name}: {tris} triangles -> {path}")
        for p in problems:
            print(f"  PROBLEM {obj.name}: {p}")
        if args.renders and obj.name in BUDGETS:
            os.makedirs(args.renders, exist_ok=True)
            review(obj, f"{args.renders.rstrip('/')}/{obj.name}.png")


main()
