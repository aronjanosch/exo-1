"""Review scene: builds the city plan (city_plan.py) from the exported .glb files.

Writes nothing into the repo; it renders only. Also proves the .glb files import cleanly.
Placements come from the plan; roads off the lane grid, paving, the pond and pylon cables are
generated here from the plan's data, not stored as models.

Run headless after the model scripts:
    blender -b -P art/city/street_preview.py -- --renders DIR [--content content/city]

In a live Blender (runpy, see art/AGENTS.md) it builds the city with ground, sun and sky into the
collection `street` and renders nothing; a rerun replaces only that collection.
"""

import argparse
import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.modules.pop("city_plan", None)
import city_plan as plan  # noqa: E402
import kit  # noqa: E402
from kit import Part, colour  # noqa: E402

PAVING = colour("#d9d3e6")
DISTRICT = colour("#cdc7da")
LANE_RGB = colour("#4b5a8a")
GLOW = colour("#5ef2e0")
WATER = colour("#3f7f9a")
STONE = colour("#a89a8c")


def roads(col):
    """Hover lanes along the plan's polylines: paving, the lane, glowing edges."""
    for i, road in enumerate(plan.ROADS):
        p = Part(f"road_{i}", kind="road")
        pts, closed = road["points"], road["closed"]
        dz = 0.004 * i
        p.ribbon(pts, -plan.WALK, plan.WALK, -0.3, dz, PAVING, closed=closed)
        p.ribbon(pts, -plan.LANE, plan.LANE, dz, 0.02 + dz, LANE_RGB, closed=closed)
        for side, s0, s1 in road["edges"]:
            o = side * plan.LANE
            p.ribbon(plan.cut(pts, s0, s1, closed), o - 0.1, o + 0.1, dz, 0.03 + dz, GLOW, "glow")
        p.build(col)


def ground(col):
    """Paved districts and the pond, flat on the grass."""
    p = Part("ground", kind="road")
    for poly in plan.PAVING:
        p.prism(poly, -0.004, -0.002, DISTRICT)
    cx, cy, r = plan.CUL_END
    p.cylinder((cx, cy, -0.3), r + 2.0, 0.3 + 0.008, PAVING, segments=32)
    p.cylinder((cx, cy, 0.008), r - 2.0, 0.02, LANE_RGB, segments=32)
    p.cylinder((cx, cy, 0.008), 1.2, 0.06, colour("#f4f1e8"), segments=16)
    p.prism(plan.POND, -0.004, 0.01, WATER, "glass")
    p.ribbon(plan.POND, 0.0, 0.7, -0.004, 0.18, STONE, closed=True)
    p.build(col)


def cables(col, sag=0.6, height=6.95, arm=1.3):
    """Sagging cables between neighbouring pylons, one per insulator; generated, not a model."""
    mat = bpy.data.materials.get("cable") or bpy.data.materials.new("cable")
    next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.04, 0.04, 0.05, 1)
    for run in plan.CABLE_RUNS:
        for (ax, ay, at), (bx, by, bt) in zip(run, run[1:]):
            for side in (-arm, arm):
                ox, oy = side * math.cos(math.radians(at)), side * math.sin(math.radians(at))
                qx, qy = side * math.cos(math.radians(bt)), side * math.sin(math.radians(bt))
                curve = bpy.data.curves.new("cable", "CURVE")
                curve.dimensions = "3D"
                curve.bevel_depth = 0.025
                spline = curve.splines.new("POLY")
                n = 12
                spline.points.add(n - 1)
                for k, pt in enumerate(spline.points):
                    t = k / (n - 1)
                    pt.co = (ax + ox + (bx + qx - ax - ox) * t, ay + oy + (by + qy - ay - oy) * t,
                             height - sag * 4 * t * (1 - t), 1)
                curve.materials.append(mat)
                obj = bpy.data.objects.new("cable", curve)
                col.objects.link(obj)


def room_light(col, at, w, d, h):
    """A stand-in for the game's room light: one soft panel under the ceiling. at: a matrix on the
    floor in the room's middle."""
    light = bpy.data.objects.new("room_light", bpy.data.lights.new("room_light", "AREA"))
    light.data.shape, light.data.size, light.data.size_y = "RECTANGLE", w * 0.8, d * 0.8
    light.data.energy = 7.0 * w * d
    light.data.color = (1.0, 0.9, 0.78)
    col.objects.link(light)
    light.matrix_world = at @ Matrix.Translation((0, 0, h - 0.1))


def furnish(content, col, new, spec):
    """Put the placement's fit-out on the building's room anchor. Returns the fit-out's objects."""
    room = next((o for o in new if o.get("kind") == "room"), None)
    if room is None:
        print(f"PROBLEM no room anchor for fit {spec}")
        return []
    fit = kit.place(content, f"fit_{spec['fit']}_{room['bays']}", 0, 0, 0, 0, col=col)
    next(o for o in fit if o.parent is None).matrix_world = room.matrix_world.copy()
    bpy.context.view_layer.update()
    w, d, h = room["size"]
    room_light(col, room.matrix_world @ Matrix.Translation((0, -d / 2, 0)), w, d, h)
    return fit


def populate(content, col, objs, has_room):
    """A marker on every npc anchor; rooms without a room anchor (one-offs, homes) get a smaller
    light over their character."""
    for o in objs:
        if o.get("kind") != "npc":
            continue
        marker = kit.place(content, "npc_marker", 0, 0, 0, 0, col=col)
        next(m for m in marker if m.parent is None).matrix_world = o.matrix_world.copy()
        if not has_room:
            room_light(col, o.matrix_world, 6.0, 6.0, 2.7)


def build(content, col):
    for row in plan.PLACEMENTS:
        new = kit.place(content, *row[:5], col=col)
        for o in new:
            if o.get("kind") == "leaf":
                o.location += Vector(o["slide"])   # doors open, so the review sees in
        bpy.context.view_layer.update()
        fit = furnish(content, col, new, row[5]) if len(row) > 5 else []
        populate(content, col, new + fit, bool(fit))
    roads(col)
    ground(col)
    cables(col)


def live():
    content = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "content", "city")
    col = kit.fresh_collection("street")
    build(content, col)
    kit.stage(col, render=False, ground_rgb=plan.GRASS)
    print(f"STREET {len(col.objects)} objects in collection 'street'")


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--renders", required=True)
    ap.add_argument("--content", default="content/city")
    ap.add_argument("--shots", help="comma-separated shot names (default: all)")
    args = ap.parse_args(argv)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    col = kit.fresh_collection("preview")
    build(args.content, col)
    os.makedirs(args.renders, exist_ok=True)
    d = args.renders.rstrip("/")
    shots = [
        ("city_overview", (150, -330, 210), (60, -45, 0), 30),
        ("downtown_overview", (70, -80, 60), (0, 2, 5), 30),
        ("downtown_eye", (-24, -3.5, 1.7), (6, 1, 6), 18),
        ("arterial_eye", (60, -2.0, 1.7), (130, -40, 4), 22),
        ("ringside_eye", (205, -58, 1.7), (250, -95, 3), 22),
        ("shop_street_eye", (235, -58, 1.7), (235, -10, 5), 20),
        # Into the ground floors: through a shop window, and from a door into the room.
        ("window_eye", (-29, 2.5, 1.7), (-29, 12, 1.4), 22),
        ("inside_bar", (-14.6, 6.5, 1.7), (-17, 14.0, 1.2), 16),
        ("inside_workshop", (-6.0, -24.5, 1.7), (-14.0, -23.0, 1.0), 16),
        ("diner_eye", (24.0, -1.5, 1.7), (25.0, -12.0, 2.0), 22),
        ("bubble_eye", (10.0, -44.5, 1.7), (12.0, -51.0, 1.5), 22),
    ]
    only = set(args.shots.split(",")) if args.shots else None
    shots = [s for s in shots if not only or s[0] in only]
    kit.stage(col, ground_rgb=plan.GRASS)
    for name, eye, target, lens in shots:
        kit.shoot(col, f"{d}/{name}.png", eye, target, lens=lens)
    for o in [o for o in col.objects if o.name.startswith(("Sun", "Plane"))]:   # rooms keep their lights
        bpy.data.objects.remove(o)
    kit.stage(col, night=True, ground_rgb=plan.GRASS)
    for name, eye, target, lens in (("downtown_night", (-24, -3.5, 1.7), (6, 1, 6), 18),
                                    ("city_night", (150, -330, 210), (60, -45, 0), 30)):
        if not only or name in only:
            kit.shoot(col, f"{d}/{name}.png", eye, target, lens=lens)


if bpy.app.background:
    main()
else:
    live()
