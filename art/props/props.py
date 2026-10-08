"""Scatter props (#65): trees, a shrub, rocks, a ground-cover tuft and rare pieces.

This script is the only source of the props (concept repo DECISIONS.md, "Model source").
Blender units are metres; each prop stands on its origin, up is +Z (glTF +Y). Colours are
vertex colours: white parts take the biome tint in the game, darker parts stay darker
(the tint multiplies them). Goofy shapes on purpose. Triangle guide value for small props:
100-1,000 (DECISIONS.md, "Triangle budgets"); ground cover far below that, it is instanced
by the thousand.

Run headless (writes content/props/<id>.glb):
    blender -b -P art/props/props.py -- --out content/props
"""

import argparse
import math
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

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


PROPS = [tree_lolly, tree_stack, shrub_blob, rock_round, rock_slab, tuft, mushroom_giant, crystal,
         hull_broken, debris_panel, debris_strut, wall_broken, pillar_stub, mast, arch_giant]


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="content/props")
    args = ap.parse_args(argv)
    for make in PROPS:
        clear()
        rng = random.Random(make.__name__)
        obj = make(rng)
        bpy.context.view_layer.objects.active = obj
        obj.select_set(True)
        dg = bpy.context.evaluated_depsgraph_get()
        tris = sum(len(p.vertices) - 2 for p in obj.evaluated_get(dg).data.polygons)
        path = f"{args.out}/{obj.name}.glb"
        bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True, export_apply=True,
                                  export_vertex_color="ACTIVE", export_materials="NONE")
        print(f"prop {obj.name}: {tris} triangles -> {path}")


main()
