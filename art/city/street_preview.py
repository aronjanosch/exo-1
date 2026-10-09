"""Review scene: imports the exported city .glb files and lays out the first quarter with them.

Writes nothing into the repo; it renders only. Also proves the .glb files import cleanly.

Run headless after the model scripts:
    blender -b -P art/city/street_preview.py -- --renders DIR [--content content/city]

In a live Blender (runpy, see art/AGENTS.md) it builds the street with ground, sun and sky into the
collection `street` and renders nothing; a rerun replaces only that collection.
"""

import argparse
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402

# The first quarter. (id, x, y, z, turn); turn is the yaw in degrees. Buildings face -Y as built:
# turn 0 faces -Y, 180 faces +Y, 90 faces +X, -90 faces -X. Fronts stand on the walkway edge,
# 5 m from a lane's centre line. Lane tiles sit on the 10 m grid.
#   Main street: along X at y = 0, from x = -40 to 40, turnarounds at both ends.
#   Cross street: along Y at x = 0, from y = -30 to 30; in the north it curves east into a stub.
#   NW block: row houses and a corner house, back yards behind fences.
#   NE block: a plaza with the beacon spire, market stalls, the company building on its east side.
#   SW block: row houses, a corner turret, a garage on the cross street.
#   SE block: a row house, the saucer diner, parking pads.
# Pylons stand in rows along the lane's edge; cables run between neighbours in a row.
PYLON_ROWS = [((-35, -25, -15), 3.3), ((15, 25, 35), 3.3), ((15, 25, 35), -3.3)]
STOREY_TOP = {"row_tower": 4.5 + 7 * 3.5 + 0.4, "row_garage": 4.5 + 1 * 3.5 + 0.4}
STREET = [
    # Main street.
    ("lane_end", -40, 0, 0, 90), ("lane_straight", -30, 0, 0, 90), ("lane_straight", -20, 0, 0, 90),
    ("lane_straight", -10, 0, 0, 90), ("lane_crossing", 0, 0, 0, 0), ("lane_straight", 10, 0, 0, 90),
    ("lane_straight", 20, 0, 0, 90), ("lane_straight", 30, 0, 0, 90), ("lane_end", 40, 0, 0, -90),
    # Cross street, the curve and the northern stub.
    ("lane_straight", 0, 10, 0, 0), ("lane_straight", 0, 20, 0, 0), ("lane_curve", 0, 30, 0, 0),
    ("lane_straight", 10, 30, 0, 90), ("lane_end", 20, 30, 0, -90),
    ("lane_straight", 0, -10, 0, 0), ("lane_straight", 0, -20, 0, 0), ("lane_end", 0, -30, 0, 180),
    # NW block.
    ("corner_sign", -9, 5, 0, 0), ("row_step", -17, 5, 0, 0), ("row_fin", -23, 5, 0, 0),
    ("row_bulb", -29, 5, 0, 0), ("row_tank", -37, 5, 0, 0),
    ("row_saw", -5, 21, 0, 90), ("row_tower", -5, 29, 0, 90),
    # NE block: the plaza.
    ("plaza_tile", 10, 10, 0, 0), ("plaza_tile", 20, 10, 0, 0), ("plaza_tile", 10, 20, 0, 0),
    ("plaza_tile", 20, 20, 0, 0), ("beacon_spire", 15, 15, 0, 0), ("company_hq", 27, 15.3, 0, -90),
    ("market_stall", 7.5, 11, 0, 90), ("market_stall", 7.5, 16, 0, 90), ("market_stall", 7.5, 21, 0, 90),
    ("bench_float", 15, 8.5, 0, 180), ("bench_float", 15, 21.5, 0, 0),
    ("planter_blob", 11, 19, 0, 0), ("planter_blob", 19, 11, 0, 0), ("booth_tele", 22.5, 7.5, 0, 0),
    ("robot_sweeper", 12, 10, 0, 30), ("vending_tube", 24.5, 22.5, 0, -90),
    ("tree_bulb", 22, 22, 0, 0), ("tree_spiral", 11.5, 23, 0, 0), ("grass_tuft", 19.5, 19.5, 0, 0),
    ("grass_tuft", 10.5, 11.5, 0, 40), ("billboard", 12, 37, 0, 0),
    # SW block.
    ("corner_turret", -11, -5, 0, 180), ("row_twin", -21, -5, 0, 180), ("row_needle", -27, -5, 0, 180),
    ("row_arch", -35, -5, 0, 180), ("row_garage", -5, -23, 0, 90),
    # SE block.
    ("row_slant", 11, -5, 0, 180), ("saucer_diner", 25, -12, 0, 180),
    ("parking_pads", 37, -10, 0, 0), ("row_butterfly", 5, -21, 0, -90),
    # Back yards: fences along the block's back line, junk and plants behind the houses.
    *[("fence_panel", x, 25.6, 0, 0) for x in (-39, -35, -31, -27, -23, -19)],
    *[("fence_panel", x, -25.6, 0, 0) for x in (-39, -35, -31, -27, -23, -19)],
    ("crate_stack", -30, 18, 0, 10), ("tree_spiral", -36, 21, 0, 0), ("bush_puff", -24, 22, 0, 0),
    ("bin_bot", -20, 17.5, 0, 180), ("grass_tuft", -33, 23, 0, 0), ("grass_tuft", -27, 19, 0, 0),
    ("tree_bulb", -33, -21, 0, 0), ("crate_stack", -23, -19, 0, 80), ("bush_puff", -38, -19, 0, 0),
    ("roof_dish", -19, -22, 0, 150), ("grass_tuft", -28, -23, 0, 0),
    # Walkways.
    ("lamp_arc", -31, 4.6, 0, 0), ("lamp_arc", -19, 4.6, 0, 0), ("lamp_arc", 13, -4.6, 0, 180),
    ("lamp_arc", -23, -4.6, 0, 180), ("lamp_arc", -4.6, 15, 0, -90), ("lamp_arc", 4.6, -15, 0, 90),
    *[("lane_pylon", x, y, 0, 90) for xs, y in PYLON_ROWS for x in xs],
    ("bin_bot", -18.6, 4.4, 0, 0), ("vending_tube", -13.6, 4.4, 0, 0), ("mail_tube", -27.5, 4.4, 0, 0),
    ("bench_float", -33, -3.9, 0, 180), ("bin_bot", -15, -4.4, 0, 180), ("mail_tube", 9, -4.4, 0, 180),
    ("sign_post", 4.2, -4.2, 0, 0), ("sign_post", -4.2, 4.2, 0, 180), ("booth_tele", -4.2, -10, 0, 90),
    ("robot_sweeper", -12, 3.8, 0, 200), ("planter_blob", 30, -4.3, 0, 0), ("bush_puff", 33, -4.2, 0, 0),
    # Traffic: hovering on the lanes, parked on pads.
    ("hover_bus", 16, 1.5, 1.4, -90), ("hover_car_red", -9, -1.5, 1.2, 90), ("hover_car_teal", 8, 1.5, 1.6, -90),
    ("hover_van", -1.5, -23, 1.0, 0), ("hover_car_teal", 1.5, 14, 1.3, 180),
    ("hover_car_red", 34.5, -10, 0.4, 0), ("hover_van", 39.5, -10, 0.4, 180),
    ("hover_car_teal", -10.5, 29, STOREY_TOP["row_tower"] + 1.8 + 0.4, 30),
    ("hover_car_red", -10.5, -23, STOREY_TOP["row_garage"] + 1.8 + 0.4, -60),
]


def cables(col, sag=0.6, height=6.95, arm=1.3):
    """Sagging cables between neighbouring pylons, one per insulator; generated, not a model."""
    mat = bpy.data.materials.get("cable") or bpy.data.materials.new("cable")
    next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = (0.04, 0.04, 0.05, 1)
    for xs, y in PYLON_ROWS:
        for a, b in zip(xs, xs[1:]):
            for side in (-arm, arm):
                curve = bpy.data.curves.new("cable", "CURVE")
                curve.dimensions = "3D"
                curve.bevel_depth = 0.025
                spline = curve.splines.new("POLY")
                n = 12
                spline.points.add(n - 1)
                for k, pt in enumerate(spline.points):
                    t = k / (n - 1)
                    pt.co = (a + (b - a) * t, y + side, height - sag * 4 * t * (1 - t), 1)
                curve.materials.append(mat)
                obj = bpy.data.objects.new("cable", curve)
                col.objects.link(obj)


def live():
    content = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "content", "city")
    col = kit.fresh_collection("street")
    for row in STREET:
        kit.place(content, *row, col=col)
    cables(col)
    kit.stage(col, render=False)
    print(f"STREET {len(col.objects)} objects in collection 'street'")


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--renders", required=True)
    ap.add_argument("--content", default="content/city")
    args = ap.parse_args(argv)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for row in STREET:
        kit.place(args.content, *row)
    col = kit.fresh_collection("preview")
    cables(col)
    os.makedirs(args.renders, exist_ok=True)
    d = args.renders.rstrip("/")
    kit.stage(col)
    kit.shoot(col, f"{d}/street_overview.png", (70, -80, 60), (0, 2, 5), lens=30)
    kit.shoot(col, f"{d}/street_eye.png", (-24, -3.5, 1.7), (6, 1, 6), lens=18)
    for o in [o for o in col.objects if o.type == "LIGHT" or o.name.startswith("Plane")]:
        bpy.data.objects.remove(o)
    kit.stage(col, night=True)
    kit.shoot(col, f"{d}/street_night.png", (-24, -3.5, 1.7), (6, 1, 6), lens=18)


if bpy.app.background:
    main()
else:
    live()
