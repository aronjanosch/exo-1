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


def build(content, col):
    for row in plan.PLACEMENTS:
        kit.place(content, *row, col=col)
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
    ]
    kit.stage(col, ground_rgb=plan.GRASS)
    for name, eye, target, lens in shots:
        kit.shoot(col, f"{d}/{name}.png", eye, target, lens=lens)
    for o in [o for o in col.objects if o.type == "LIGHT" or o.name.startswith("Plane")]:
        bpy.data.objects.remove(o)
    kit.stage(col, night=True, ground_rgb=plan.GRASS)
    kit.shoot(col, f"{d}/downtown_night.png", (-24, -3.5, 1.7), (6, 1, 6), lens=18)
    kit.shoot(col, f"{d}/city_night.png", (150, -330, 210), (60, -45, 0), lens=30)


if bpy.app.background:
    main()
else:
    live()
